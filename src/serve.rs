//! `custodian serve`: the resident. It holds the project's open workspaces (one klepto scan, many readings, reopened
//! only when the code's index key moves) and the journal's writer, and answers on an owner-only Unix socket
//! (`mine`: the path guarded, mode 0600, the peer's uid checked before a byte is read) in length-prefixed frames
//! (`abut`: a little-endian u32 length, then the bytes). A frame holds JSON.
//!
//! The wire, one exchange per connection:
//!
//! ```text
//! client → server   [u32 LE length][Request JSON]
//! server → client   [u32 LE length][Reply JSON]
//! Request { schema: "custodian.serve.v1", kind: run_spec | run_job | import_bundle | status | stop, argv: [..] }
//! Reply   { schema: "custodian.serve.v1", outcome: Outcome { code, receipt, receipt_file, printed, said, refusal? }, live: [..] }
//! ```
//!
//! `argv` is the declared line's words after `custodian`; the resident reads them back through the same surface and
//! runs the line through the same facade, so a run over the socket leaves the same receipt a run on the command
//! line does. The command line tries the socket first for the lines a resident answers (`run`, `job start`,
//! `job import`, `stop`) and does the work itself when nothing answers.
//!
//! The socket is `.custodian/runtime/serve.sock`; when that path is longer than a Unix socket path may be
//! (`SUN_PATH`, 104 bytes on macOS), it is `<runtime dir>/custodian/<16 hex of the root's digest>.sock`, a rule
//! the client and the resident both apply.

use crate::codes::{Code, Result};
use crate::digest;
use crate::facade::{Ctx, Outcome, Ran};
use crate::layout;
use crate::mode::Mode;
use crate::surface::{self, Line};
use custodian_kit::api::{Receipt, RECEIPT_SCHEMA};
use mine::{framed, Admit, FramedReader, StreamBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const WIRE_SCHEMA: &str = "custodian.serve.v1";

/// The longest frame either side reads: a reading's page and record fit.
pub const MAX_FRAME: usize = 64 << 20;

/// The one request kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    RunSpec,
    RunJob,
    ImportBundle,
    Status,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub kind: RequestKind,
    /// The declared line's words after `custodian`.
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub schema: String,
    pub outcome: Outcome,
    /// What the run showed while it ran.
    pub live: Vec<String>,
}

/// A request as the bytes of one frame: the length, then the JSON.
pub fn frame_of<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(value).map_err(|e| Code::Frame.refuse(e))?;
    let mut w = mine::FramedWriter::new(Vec::new());
    w.write_frame(&json).map_err(|e| Code::Frame.refuse(e))?;
    Ok(w.into_inner())
}

/// The kind a line is sent as, if a resident answers it.
pub fn kind_of(line: &Line) -> Option<RequestKind> {
    match line {
        Line::Run(_) => Some(RequestKind::RunSpec),
        Line::JobStart(_) => Some(RequestKind::RunJob),
        Line::JobImport(_) => Some(RequestKind::ImportBundle),
        Line::Stop(_) => Some(RequestKind::Stop),
        Line::Serve(_) => Some(RequestKind::Status),
        _ => None,
    }
}

/// The project's socket.
pub fn socket_path(layout: &layout::Layout) -> Result<PathBuf> {
    let p = layout.path::<layout::Socket>(&[])?;
    if p.as_os_str().len() < 100 {
        return Ok(p);
    }
    let dir = mine::runtime_dir().join("custodian");
    Ok(dir.join(format!("{}.sock", digest::short(&digest::of("socket", &layout.root.to_string_lossy()), 16))))
}

/// A line's words, with a file it names made absolute (the resident runs elsewhere).
pub fn absolute(line: &Line, here: &Path) -> Line {
    let abs = |v: &Option<std::ffi::OsString>| {
        v.as_ref().map(|t| {
            let p = here.join(t);
            if p.exists() {
                p.into_os_string()
            } else {
                t.clone()
            }
        })
    };
    match line {
        Line::Run(r) => Line::Run(surface::Run { target: abs(&r.target), ..r.clone() }),
        Line::JobImport(j) => Line::JobImport(surface::JobImport { bundle: abs(&Some(j.bundle.clone())).unwrap_or_else(|| j.bundle.clone()) }),
        other => other.clone(),
    }
}

fn send(path: &Path, req: &Request) -> Result<Reply> {
    let stream = StreamBuilder::new().connect(path).map_err(|e| Code::Socket.refuse(format!("{}: {e}", path.display())))?;
    let (mut w, r) = framed(stream).map_err(|e| Code::Socket.refuse(e))?;
    let json = serde_json::to_vec(req).map_err(|e| Code::Frame.refuse(e))?;
    w.write_frame(&json).map_err(|e| Code::Frame.refuse(e))?;
    let mut r = FramedReader::with_max(r.into_inner(), MAX_FRAME);
    let mut buf = Vec::new();
    r.recv_into(&mut buf).map_err(|e| Code::Frame.refuse(e))?;
    let reply: Reply = serde_json::from_slice(&buf).map_err(|e| Code::Frame.refuse(e))?;
    if reply.schema != WIRE_SCHEMA {
        return Err(Code::Frame.refuse(format!("schema {}", reply.schema)));
    }
    Ok(reply)
}

/// The line sent to the resident, when one answers on the project's socket; what the run showed while it ran is
/// printed first.
pub fn try_socket(here: &Path, line: &Line) -> Option<Outcome> {
    let (outcome, live) = ask_resident(here, line)?;
    for l in &live {
        println!("{l}");
    }
    Some(outcome)
}

/// The line sent to the resident, when one answers on the project's socket: its outcome, and what it showed while
/// it ran (nothing printed: the MCP server's stdout is its protocol).
pub fn ask_resident(here: &Path, line: &Line) -> Option<(Outcome, Vec<String>)> {
    let kind = kind_of(line)?;
    if kind == RequestKind::Status {
        return None;
    }
    let Ok(Mode::Journaled(p)) = Mode::find(here) else { return None };
    let path = socket_path(&p.layout).ok()?;
    if !path.exists() {
        return None;
    }
    let line = absolute(line, here);
    let req = Request { schema: WIRE_SCHEMA.into(), kind, argv: line.argv().iter().map(|a| a.to_string_lossy().into_owned()).collect() };
    match send(&path, &req) {
        Ok(reply) => {
            let mut o = reply.outcome;
            o.said.push(format!("answered by the resident on {}", path.display()));
            Some((o, reply.live))
        }
        Err(_) => None,
    }
}

/// The resident's status, if one answers.
pub fn status(layout: &layout::Layout) -> Option<Reply> {
    let path = socket_path(layout).ok()?;
    if !path.exists() {
        return None;
    }
    send(&path, &Request { schema: WIRE_SCHEMA.into(), kind: RequestKind::Status, argv: vec!["serve".into()] }).ok()
}

/// An outcome with no run behind it (a refusal of the request itself, the status).
fn bare(code: i32, line: &str, said: Vec<String>) -> Outcome {
    Outcome {
        code,
        receipt: Receipt {
            schema: RECEIPT_SCHEMA.into(),
            seq: 0,
            utc: crate::journal::now_utc(),
            command: "serve".into(),
            name: "serve".into(),
            spec: None,
            spec_hash: None,
            code_digest: None,
            index_key: None,
            plan_id: "custodian.serve".into(),
            plan_digest: String::new(),
            operation_digest: String::new(),
            receipt_digest: String::new(),
            line: line.into(),
            exit_code: Some(code),
            accepted: code == 0,
            disposition: if code == 0 { "Continue".into() } else { "Stop".into() },
            reason: if code == 0 { "done".into() } else { "refused".into() },
            outputs: Vec::new(),
            job: None,
            reused_seq: None,
            millis: 0,
            runner: custodian_kit::receipts::RUNNER_IN_PROCESS.into(),
        },
        receipt_file: None,
        printed: Vec::new(),
        said,
        refusal: None,
    }
}

/// One request, answered with one receipt.
pub fn answer(ctx: &mut Ctx, req: &Request) -> (Reply, bool) {
    let live = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = live.clone();
    ctx.live = Box::new(move |l| {
        if let Ok(mut v) = sink.lock() {
            v.push(l.to_string());
        }
    });
    let reply = |outcome: Outcome, live: &Arc<Mutex<Vec<String>>>| Reply { schema: WIRE_SCHEMA.into(), outcome, live: live.lock().map(|v| v.clone()).unwrap_or_default() };
    if req.schema != WIRE_SCHEMA {
        return (reply(bare(2, "?", vec![Code::Frame.refuse(format!("schema {}", req.schema)).to_string()]), &live), false);
    }
    if req.kind == RequestKind::Status {
        let journal = ctx.journal.as_ref().and_then(|j| j.count().ok()).unwrap_or(0);
        let running = ctx.journal.as_ref().and_then(|j| j.jobs_in("running").ok()).unwrap_or_default();
        let said = vec![
            format!("serving {} (pid {})", ctx.root().display(), std::process::id()),
            format!("workspaces held open: {}", ctx.held_count()),
            format!("journal: {journal} receipts"),
            format!("jobs running: {}", if running.is_empty() { "none".into() } else { running.iter().map(|r| r.id.clone()).collect::<Vec<_>>().join(", ") }),
        ];
        return (reply(bare(0, "custodian serve (status)", said), &live), false);
    }
    let argv: Vec<std::ffi::OsString> = req.argv.iter().map(Into::into).collect();
    let line = match surface::read(&argv) {
        Ok(l) => l,
        Err(e) => return (reply(bare(2, &req.argv.join(" "), vec![e.to_string()]), &live), false),
    };
    if kind_of(&line) != Some(req.kind) {
        return (reply(bare(2, &line.display(), vec![Code::Frame.refuse(format!("a {:?} request carries `{}`", req.kind, line.display())).to_string()]), &live), false);
    }
    let stop = req.kind == RequestKind::Stop;
    let outcome = if stop {
        crate::facade::execute(ctx, &line, |ctx, _| {
            let mut r = Ran::named("stop");
            r.say(format!("the resident on {} stops", ctx.root().display()));
            Ok(r)
        })
    } else {
        crate::cli::run_line(ctx, &line)
    };
    (reply(outcome, &live), stop)
}

/// `custodian serve`: binds the project's socket and answers until a stop request.
pub fn serve(mut ctx: Ctx) -> i32 {
    ctx.in_resident = true;
    let layout = match ctx.layout() {
        Ok(l) => l.clone(),
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if let Some(reply) = status(&layout) {
        eprintln!("{}", Code::ServeRunning.refuse(reply.outcome.said.join("; ")));
        return 2;
    }
    let path = match socket_path(&layout) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let bound = match StreamBuilder::new().bind_listener(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}", Code::Socket.refuse(format!("{}: {e}", path.display())));
            return 2;
        }
    };
    println!("serving {} on {} (owner-only, mode 0600); custodian stop ends it", ctx.root().display(), path.display());
    let admit = Admit::me();
    loop {
        let (stream, _peer) = match bound.accept_from(&admit) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("refused a connection: {e}");
                continue;
            }
        };
        let Ok((mut w, r)) = framed(stream) else { continue };
        let mut r = FramedReader::with_max(r.into_inner(), MAX_FRAME);
        let mut buf = Vec::new();
        if let Err(e) = r.recv_into(&mut buf) {
            eprintln!("a frame did not read: {e}");
            continue;
        }
        let (reply, stop) = match serde_json::from_slice::<Request>(&buf) {
            Ok(req) => answer(&mut ctx, &req),
            Err(e) => (Reply { schema: WIRE_SCHEMA.into(), outcome: bare(2, "?", vec![Code::Frame.refuse(e).to_string()]), live: Vec::new() }, false),
        };
        let line = reply.outcome.receipt.line.clone();
        let code = reply.outcome.code;
        if let Ok(bytes) = serde_json::to_vec(&reply) {
            let _ = w.write_frame(&bytes);
        }
        println!("answered: {line} → {code}");
        if stop {
            break;
        }
    }
    println!("stopped: {} is removed with the listener", path.display());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_one_frame_of_json() {
        let req = Request { schema: WIRE_SCHEMA.into(), kind: RequestKind::RunSpec, argv: vec!["run".into(), "env".into()] };
        let bytes = frame_of(&req).unwrap();
        let json = br#"{"schema":"custodian.serve.v1","kind":"run_spec","argv":["run","env"]}"#;
        assert_eq!(&bytes[..4], &(json.len() as u32).to_le_bytes());
        assert_eq!(&bytes[4..], json);
        assert_eq!(bytes, include_bytes!("../tests/golden/request_run_spec.frame").to_vec(), "the golden frame");
    }
}
