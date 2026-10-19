//! `custodian mcp serve`: an MCP server (the official Rust SDK, rmcp) on stdin and stdout, whose tools are
//! custodian's declared lines.
//!
//! The tool list is made, not written: each entry of [`TOOLS`] names a declared segment (and, for a leaf with
//! actions, the words it fixes), and the tool's input schema is that leaf's [`Declared`] table read field by field
//! (a flag is a boolean, a `Vec` an array of strings, anything else a string; a word-set check is the schema's
//! `enum`; `init_required` is `required`; the field's `///` is its description). Every declared segment is either
//! a tool's or one of [`A_PERSONS`]; `tests::the_tool_list_is_the_declared_surface` holds that, and the golden
//! `tests/golden/mcp_tools.json` is the list a host sees.
//!
//! A call is the command line's path: the arguments are written as the declared line's words, read back through
//! [`surface::read`] (so the declaration's own checks refuse a bad call), sent to the resident when one answers on
//! the project's socket, else run in this process through [`crate::cli::run_line`] (the facade, a receipt). The
//! result is the receipt as JSON, the reading's path and resource, what the run said and showed, and the reading's
//! markdown inline when it is at most [`INLINE_CAP`] bytes. Arguments that do not make the line are a protocol
//! error carrying the liaise code; a line that ran and was refused is a tool error carrying the code and its receipt.
//!
//! Resources: `custodian://readings/<seq>` is the page of the reading the journal indexes under receipt `seq`, and
//! `custodian://job/<id>/brief` a closed job's brief. One thread holds the project's [`Ctx`] (its journal and the
//! workspaces it keeps open) and runs every call in turn, as the resident does.

use crate::codes::{Code, Refusal, Result};
use crate::facade::{Ctx, Outcome};
use crate::surface::{self, Declared, Field, Line, Written};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData, Implementation, JsonObject, ListResourceTemplatesResult, ListResourcesResult,
    ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ResourceTemplate,
    ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

/// The most of a reading's markdown a tool result carries inline; a longer page is named by its path and resource.
pub const INLINE_CAP: usize = 64 * 1024;

/// The scheme of every resource.
pub const SCHEME: &str = "custodian://";

/// One tool: its name, the declared segment it runs, the words it fixes, the fields it leaves out, and what it adds
/// to the declaration's doc.
#[derive(Debug, Clone, Copy)]
pub struct ToolLine {
    pub name: &'static str,
    pub segment: &'static str,
    /// (field, word): a scalar field fixed is not the tool's; a `Vec` field's fixed word goes first.
    pub fixed: &'static [(&'static str, &'static str)],
    pub hidden: &'static [&'static str],
    pub says: &'static str,
}

const fn tool(name: &'static str, segment: &'static str, fixed: &'static [(&'static str, &'static str)], hidden: &'static [&'static str], says: &'static str) -> ToolLine {
    ToolLine { name, segment, fixed, hidden, says }
}

const JOB_ONLY_NAME: &[&str] = &["kind", "target", "intent", "again", "bundle"];
const JOB_BRIEF: &[&str] = &["kind", "target", "intent", "again"];
const COUNT_ONLY_ARGS: &[&str] = &["by", "tests", "again", "job"];

/// The lines an agent runs, as tools. `job import` (a bundle made elsewhere) is a person's.
pub const TOOLS: &[ToolLine] = &[
    tool("run", "run", &[], &[], "A search on the line is `target` with `kind` (`reach`, `readers_of_env`, ...)."),
    tool("list", "list", &[], &[], ""),
    tool("diff", "diff", &[], &[], ""),
    tool("job_start", "job", &[("action", "start")], &["bundle"], ""),
    tool("job_close", "job", &[("action", "close")], JOB_ONLY_NAME, ""),
    tool("job_brief", "job", &[("action", "brief")], JOB_BRIEF, "`bundle` packs the closed job as one bundle file too."),
    tool("job_list", "job", &[("action", "list")], &["name", "kind", "target", "intent", "again", "bundle"], ""),
    tool("test", "test", &[], &[], ""),
    tool("check", "check", &[], &[], ""),
    tool("annotate", "annotate", &[], &["apply"], "The plan only: placing it (`--apply`) is a person's."),
    tool("features", "features", &[], &["apply"], "The plan only: gating (`--apply`) is a person's."),
    tool("diagnostics_list", "diagnostics", &[("action", "list")], &[], "`arg` is a crate, every crate by default."),
    tool("diagnostics_where", "diagnostics", &[("action", "where")], &[], "`arg` is the diagnostic code."),
    tool("diagnostics_from", "diagnostics", &[("action", "from")], &[], "`arg` is the log file."),
    tool("triage", "triage", &[], &[], ""),
    tool("bundle", "bundle", &[], &[], ""),
    tool("count", "count", &[], &[], "`args` is the one route, the workspace by default."),
    tool("count_report", "count", &[("args", "report")], COUNT_ONLY_ARGS, "`args` names the two counts, A then B: a receipt number, a job id, a commit or a date."),
    tool("count_history", "count", &[("args", "history")], COUNT_ONLY_ARGS, "`args` is the route, every route by default."),
];

/// The lines that are a person's, never a tool, and why.
pub const A_PERSONS: &[(&str, &str)] = &[
    ("init", "makes the folder custodian keeps"),
    ("agent", "makes the folder in agent mode"),
    ("sweep", "offers routes for a person to declare"),
    ("serve", "the resident, in a person's terminal"),
    ("stop", "stops the resident"),
    ("watch", "follows events until stopped"),
    ("self", "installs and removes the program"),
    ("mcp", "this server"),
];

/// The tool of a name.
pub fn tool_line(name: &str) -> Option<&'static ToolLine> {
    TOOLS.iter().find(|t| t.name == name)
}

/// A segment's declaration table.
pub fn declared(segment: &str) -> Option<&'static Declared> {
    surface::DECLARED.iter().find(|d| d.segment() == segment)
}

impl ToolLine {
    fn declared(&self) -> &'static Declared {
        declared(self.segment).unwrap_or_else(|| panic!("tool {} names segment {}, which is not declared", self.name, self.segment))
    }

    fn fixed_of(&self, field: &str) -> Vec<&'static str> {
        self.fixed.iter().filter(|(f, _)| *f == field).map(|(_, w)| *w).collect()
    }

    /// The fields the tool takes: the declaration's, less the hidden and the fixed scalars.
    pub fn fields(&self) -> Vec<&'static Field> {
        self.declared().fields.iter().filter(|f| !self.hidden.contains(&f.name) && (f.ty == "Vec" || self.fixed_of(f.name).is_empty())).collect()
    }

    /// The line the tool runs, as the command line writes it: `custodian count report`.
    pub fn line(&self) -> String {
        let mut words = vec!["custodian".to_string(), self.segment.to_string()];
        for f in self.declared().fields {
            words.extend(self.fixed_of(f.name).into_iter().map(String::from));
        }
        words.join(" ")
    }

    /// The tool as a host lists it.
    pub fn tool(&self) -> Tool {
        let d = self.declared();
        let mut description = format!("Runs `{}`, a declared line; every run leaves a receipt.", self.line());
        if !self.says.is_empty() {
            description.push(' ');
            description.push_str(self.says);
        }
        description.push_str("\n\n");
        description.push_str(&d.doc());
        Tool::new(self.name, description, Arc::new(self.schema()))
    }

    /// The input schema, field by field from the declaration.
    pub fn schema(&self) -> JsonObject {
        let mut props = Map::new();
        let mut required = Vec::new();
        for f in self.fields() {
            let written = f.written().unwrap_or(Written::Positional);
            let mut p = Map::new();
            let word = |p: &mut Map<String, Value>| {
                p.insert("type".into(), json!("string"));
                if let Some(words) = f.words() {
                    p.insert("enum".into(), json!(words));
                }
            };
            match (written, f.ty) {
                (Written::Flag(_), _) => {
                    p.insert("type".into(), json!("boolean"));
                }
                (_, "Vec") => {
                    p.insert("type".into(), json!("array"));
                    let mut items = Map::new();
                    word(&mut items);
                    p.insert("items".into(), Value::Object(items));
                }
                _ => word(&mut p),
            }
            let how = match written {
                Written::Positional => "a word on the line".to_string(),
                Written::Flag(k) | Written::Kv(k) => format!("`{k}`"),
            };
            let doc = f.doc();
            p.insert("description".into(), json!(if doc.is_empty() { format!("({how})") } else { format!("{doc} ({how})") }));
            if f.required() {
                required.push(json!(f.name));
            }
            props.insert(f.name.into(), Value::Object(p));
        }
        let mut s = Map::new();
        s.insert("type".into(), json!("object"));
        s.insert("properties".into(), Value::Object(props));
        if !required.is_empty() {
            s.insert("required".into(), Value::Array(required));
        }
        s.insert("additionalProperties".into(), json!(false));
        s
    }

    /// The arguments, written as the declared line's words: the positionals in the declaration's order (a fixed word
    /// first), then the options.
    pub fn words(&self, args: &JsonObject) -> Result<Vec<OsString>> {
        let fields = self.fields();
        if let Some(k) = args.keys().find(|k| !fields.iter().any(|f| f.name == k.as_str())) {
            return Err(Code::ToolArguments.refuse(format!("`{k}` is not a field of {}: {}", self.name, fields.iter().map(|f| f.name).collect::<Vec<_>>().join(", "))));
        }
        let one = |f: &Field, v: &Value| -> Result<String> {
            match v {
                Value::String(s) => Ok(s.clone()),
                Value::Number(n) => Ok(n.to_string()),
                other => Err(Code::ToolArguments.refuse(format!("{}.{} is a string, not {other}", self.name, f.name))),
            }
        };
        let (mut positionals, mut options) = (Vec::new(), Vec::new());
        for f in self.declared().fields {
            positionals.extend(self.fixed_of(f.name).into_iter().map(String::from));
            let v = match args.get(f.name) {
                None | Some(Value::Null) => continue,
                Some(v) => v,
            };
            match f.written() {
                Some(Written::Flag(k)) => match v {
                    Value::Bool(true) => options.push(k.to_string()),
                    Value::Bool(false) => {}
                    other => return Err(Code::ToolArguments.refuse(format!("{}.{} is a boolean, not {other}", self.name, f.name))),
                },
                Some(Written::Kv(k)) => {
                    options.push(k.to_string());
                    options.push(one(f, v)?);
                }
                _ => match v {
                    Value::Array(items) if f.ty == "Vec" => {
                        for item in items {
                            positionals.push(one(f, item)?);
                        }
                    }
                    v => positionals.push(one(f, v)?),
                },
            }
        }
        let mut words = vec![OsString::from(self.segment)];
        words.extend(positionals.into_iter().chain(options).map(OsString::from));
        Ok(words)
    }

    /// The declared line the arguments make, read through the command line's own reader.
    pub fn line_of(&self, args: &JsonObject) -> Result<Line> {
        surface::read(&self.words(args)?)
    }
}

/// Every tool, as a host lists them.
pub fn tools() -> Vec<Tool> {
    TOOLS.iter().map(ToolLine::tool).collect()
}

// ---- a call ----

/// A call run: the outcome, what it showed, and where the reading is.
#[derive(Debug, Clone)]
pub struct Called {
    pub outcome: Outcome,
    pub live: Vec<String>,
    /// The reading's page, relative to the root.
    pub page: Option<String>,
    /// The receipt the journal indexes the reading under.
    pub reading_seq: Option<i64>,
    /// The reading's markdown (or, with no page, the run's answer).
    pub markdown: String,
}

/// A line run as the command line runs it: by the resident when one answers, else here through the facade.
pub fn call(ctx: &mut Ctx, line: &Line) -> Called {
    let root = ctx.root().to_path_buf();
    let line = crate::serve::absolute(line, &root);
    let (outcome, live) = match crate::serve::ask_resident(&root, &line) {
        Some(answered) => answered,
        None => {
            let shown = Arc::new(Mutex::new(Vec::<String>::new()));
            let sink = shown.clone();
            ctx.live = Box::new(move |l| {
                if let Ok(mut v) = sink.lock() {
                    v.push(l.to_string());
                }
            });
            let outcome = crate::cli::run_line(ctx, &line);
            let live = shown.lock().map(|v| v.clone()).unwrap_or_default();
            (outcome, live)
        }
    };
    let page = outcome.receipt.outputs.iter().find(|o| o.ends_with(".md")).cloned();
    let indexed = |seq: i64| ctx.journal.as_ref().and_then(|j| j.reading(seq).ok().flatten()).map(|r| r.seq);
    let reading_seq = page.as_ref().and_then(|_| outcome.receipt.reused_seq.and_then(indexed).or_else(|| indexed(outcome.receipt.seq)));
    let markdown = page.as_ref().and_then(|p| std::fs::read_to_string(root.join(p)).ok()).unwrap_or_else(|| outcome.printed.iter().map(|p| p.trim_end()).collect::<Vec<_>>().join("\n\n"));
    Called { outcome, live, page, reading_seq, markdown }
}

impl Called {
    /// The tool result: the JSON first, the markdown second when it fits.
    pub fn result(&self) -> CallToolResult {
        let o = &self.outcome;
        let mut said = o.said.clone();
        let inline = self.markdown.len() <= INLINE_CAP;
        if !inline {
            said.push(format!("the reading is {} bytes, over the {} inline: {}", self.markdown.len(), INLINE_CAP, self.page.as_deref().unwrap_or("(not kept: free mode)")));
        }
        let value = json!({
            "code": o.code,
            "status": o.receipt.reason,
            "receipt": o.receipt,
            "receipt_file": o.receipt_file,
            "reading": self.page.as_ref().map(|p| json!({
                "path": p,
                "resource": self.reading_seq.map(|n| format!("{SCHEME}readings/{n}")),
            })),
            "said": said,
            "live": self.live,
            "refusal": o.refusal,
        });
        let mut content = vec![ContentBlock::text(serde_json::to_string_pretty(&value).unwrap_or_default())];
        if inline && !self.markdown.is_empty() {
            content.push(ContentBlock::text(self.markdown.clone()));
        }
        let mut r = if o.code == 2 { CallToolResult::error(content) } else { CallToolResult::success(content) };
        r.structured_content = Some(value);
        r
    }
}

/// A refusal as the MCP error payload: the sentence, and the liaise code in `data`.
pub fn error_of(r: &Refusal) -> ErrorData {
    ErrorData::invalid_params(r.to_string(), Some(json!({ "code": r.code, "message": r.message })))
}

// ---- resources ----

/// A resource address, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    Reading(i64),
    Brief(String),
}

impl Address {
    pub fn parse(uri: &str) -> Result<Address> {
        let rest = uri.strip_prefix(SCHEME).ok_or_else(|| Code::NoSuchResource.refuse(uri))?;
        let parts: Vec<&str> = rest.split('/').collect();
        match parts.as_slice() {
            ["readings", seq] => seq.parse().map(Address::Reading).map_err(|_| Code::NoSuchResource.refuse(uri)),
            ["job", id, "brief"] if !id.is_empty() => Ok(Address::Brief(id.to_string())),
            _ => Err(Code::NoSuchResource.refuse(format!("{uri}: {SCHEME}readings/<seq> or {SCHEME}job/<id>/brief"))),
        }
    }

    pub fn uri(&self) -> String {
        match self {
            Address::Reading(n) => format!("{SCHEME}readings/{n}"),
            Address::Brief(id) => format!("{SCHEME}job/{id}/brief"),
        }
    }
}

/// Every reading the journal indexes and every closed job's brief (free mode keeps none).
pub fn resources(ctx: &Ctx) -> Result<Vec<Resource>> {
    let Some(j) = ctx.journal.as_ref() else { return Ok(Vec::new()) };
    let mut out: Vec<Resource> = j
        .readings()?
        .into_iter()
        .map(|r| Resource::new(Address::Reading(r.seq).uri(), format!("{} #{}", r.name, r.seq)).with_description(format!("{} {} ({}): {}", r.kind, r.target, r.stamp, r.page)).with_mime_type("text/markdown"))
        .collect();
    out.extend(j.jobs()?.into_iter().filter(|r| r.state == "done").map(|r| Resource::new(Address::Brief(r.id.clone()).uri(), format!("{} brief", r.name)).with_description(format!("job {} ({}), closed {}", r.id, r.kind, r.closed_utc.unwrap_or_default())).with_mime_type("text/markdown")));
    Ok(out)
}

/// A resource's text.
pub fn read(ctx: &mut Ctx, address: &Address) -> Result<String> {
    match address {
        Address::Reading(seq) => {
            let row = ctx.journal()?.reading(*seq)?.ok_or_else(|| Code::NoSuchResource.refuse(format!("{}: no reading under receipt #{seq}", address.uri())))?;
            std::fs::read_to_string(ctx.root().join(&row.page)).map_err(|e| Code::ReadingUnreadable.refuse(format!("{}: {e}", row.page)))
        }
        Address::Brief(id) => Ok(crate::jobs::brief(ctx, Some(id), false)?.printed.join("\n")),
    }
}

// ---- the server ----

type Work = Box<dyn FnOnce(&mut Ctx) + Send>;

/// The server: the tool list, and the thread that holds the project's context.
#[derive(Clone)]
pub struct Server {
    root: PathBuf,
    work: mpsc::Sender<Work>,
    tools: Arc<Vec<Tool>>,
}

impl Server {
    /// The server for the project at `root`, its steps run by the running program.
    pub fn at(root: &Path) -> Result<Server> {
        Server::with_program(root, std::env::current_exe().unwrap_or_else(|_| PathBuf::from("custodian")))
    }

    /// The server for the project at `root`, a job's steps run by `program` (the tests' built custodian).
    pub fn with_program(root: &Path, program: PathBuf) -> Result<Server> {
        let (work, rx) = mpsc::channel::<Work>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        let at = root.to_path_buf();
        std::thread::Builder::new()
            .name("custodian-mcp".into())
            .spawn(move || {
                let mut ctx = match Ctx::at(&at) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                ctx.exe = program;
                let _ = ready_tx.send(Ok(()));
                while let Ok(w) = rx.recv() {
                    w(&mut ctx);
                }
            })
            .map_err(|e| Code::McpTransport.refuse(e))?;
        ready_rx.recv().map_err(|e| Code::McpTransport.refuse(e))??;
        Ok(Server { root: root.to_path_buf(), work, tools: Arc::new(tools()) })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `f` run on the context's thread, its answer awaited.
    async fn on_ctx<R: Send + 'static>(&self, f: impl FnOnce(&mut Ctx) -> R + Send + 'static) -> std::result::Result<R, ErrorData> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.work
            .send(Box::new(move |ctx| {
                let _ = tx.send(f(ctx));
            }))
            .map_err(|e| error_of(&Code::McpTransport.refuse(e)))?;
        rx.await.map_err(|e| error_of(&Code::McpTransport.refuse(e)))
    }
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_server_info(Implementation::new("custodian", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "custodian over {}: each tool runs one declared custodian line and leaves a receipt; readings are resources at {SCHEME}readings/<seq>, a closed job's brief at {SCHEME}job/<id>/brief.",
                self.root.display()
            ))
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.as_ref().clone()))
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> std::result::Result<CallToolResponse, ErrorData> {
        let t = tool_line(&request.name).ok_or_else(|| error_of(&Code::NoSuchTool.refuse(format!("`{}`: {}", request.name, TOOLS.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")))))?;
        let line = t.line_of(&request.arguments.unwrap_or_default()).map_err(|e| error_of(&e))?;
        let called = self.on_ctx(move |ctx| call(ctx, &line)).await?;
        Ok(called.result().into())
    }

    async fn list_resources(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> std::result::Result<ListResourcesResult, ErrorData> {
        let listed = self.on_ctx(|ctx| resources(ctx)).await?.map_err(|e| error_of(&e))?;
        Ok(ListResourcesResult::with_all_items(listed))
    }

    async fn list_resource_templates(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> std::result::Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(format!("{SCHEME}readings/{{seq}}"), "reading").with_description("the page of the reading indexed under receipt seq").with_mime_type("text/markdown"),
            ResourceTemplate::new(format!("{SCHEME}job/{{id}}/brief"), "job brief").with_description("a closed job's brief").with_mime_type("text/markdown"),
        ]))
    }

    async fn read_resource(&self, request: ReadResourceRequestParams, _context: RequestContext<RoleServer>) -> std::result::Result<ReadResourceResponse, ErrorData> {
        let address = Address::parse(&request.uri).map_err(|e| error_of(&e))?;
        let uri = request.uri.clone();
        let text = self.on_ctx(move |ctx| read(ctx, &address)).await?.map_err(|e| error_of(&e))?;
        let contents = match ResourceContents::text(text, uri) {
            ResourceContents::TextResourceContents { uri, text, meta, .. } => ResourceContents::TextResourceContents { uri, mime_type: Some("text/markdown".into()), text, meta },
            other => other,
        };
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}

/// `custodian mcp serve`: the server on stdin and stdout until the host closes them.
pub fn serve_stdio(root: &Path) -> i32 {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("{}", Code::McpTransport.refuse(e));
            return 2;
        }
    };
    rt.block_on(async {
        let server = match Server::at(root) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        };
        eprintln!("custodian mcp: serving {} on stdin and stdout ({} tools)", root.display(), TOOLS.len());
        match server.serve(rmcp::transport::stdio()).await {
            Ok(running) => match running.waiting().await {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("{}", Code::McpTransport.refuse(e));
                    2
                }
            },
            Err(e) => {
                eprintln!("{}", Code::McpTransport.refuse(e));
                2
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every declared segment is a tool's or a person's, never both; every name a tool fixes or hides is a field of
    /// its line; and the list a host sees is the golden one. A new subcommand fails here until it is declared one
    /// way or the other, and a changed declaration until the golden list is made again from it.
    #[test]
    fn the_tool_list_is_the_declared_surface() {
        for (seg, _) in surface::USAGE {
            let tool = TOOLS.iter().any(|t| t.segment == *seg);
            let persons = A_PERSONS.iter().any(|(p, _)| p == seg);
            assert!(tool != persons, "`custodian {seg}` is {}", if tool { "both a tool's and a person's" } else { "neither a tool's nor a person's: declare it in mcp::TOOLS or mcp::A_PERSONS" });
        }
        assert!(A_PERSONS.iter().all(|(p, _)| declared(p).is_some()));
        for t in TOOLS {
            let d = t.declared();
            for name in t.hidden.iter().chain(t.fixed.iter().map(|(f, _)| f)) {
                assert!(d.field(name).is_some(), "{}: `{name}` is not a field of `custodian {}`", t.name, t.segment);
            }
            for (f, w) in t.fixed {
                if let Some(words) = d.field(f).and_then(|f| f.words()) {
                    assert!(words.contains(w), "{}: {f} = {w} is not one of {words:?}", t.name);
                }
            }
        }
        let mut names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), TOOLS.len(), "tool names are one each");
        let listed = serde_json::to_string_pretty(&tools()).unwrap() + "\n";
        let golden = include_str!("../tests/golden/mcp_tools.json");
        assert!(listed == golden, "the tool list moved from tests/golden/mcp_tools.json; the list made now:\n{listed}");
    }

    #[test]
    fn arguments_are_written_as_the_declared_line() {
        let args = |v: Value| v.as_object().unwrap().clone();
        let run = tool_line("run").unwrap().line_of(&args(json!({"target": "crate:core_lib", "kind": "reach", "again": true}))).unwrap();
        assert_eq!(run.display(), "custodian run crate:core_lib --kind reach --again");
        let start = tool_line("job_start").unwrap().line_of(&args(json!({"name": "batch", "kind": "triage", "target": "CUST0004", "intent": "find it"}))).unwrap();
        assert_eq!(start.display(), "custodian job start batch --kind triage --target CUST0004 --intent find it");
        let report = tool_line("count_report").unwrap().line_of(&args(json!({"args": [3, "9"]}))).unwrap();
        assert_eq!(report.display(), "custodian count report 3 9");
        let diff = tool_line("diff").unwrap().line_of(&args(json!({"spec": "env", "from": 3}))).unwrap();
        assert_eq!(diff.display(), "custodian diff env --from 3");
        // a plan-only tool has no --apply, and a bad word is the declaration's own refusal
        let e = tool_line("annotate").unwrap().line_of(&args(json!({"apply": true}))).unwrap_err();
        assert_eq!(e.code, "CUST1904", "{e}");
        let e = tool_line("run").unwrap().line_of(&args(json!({"target": "x", "kind": "nonsense"}))).unwrap_err();
        assert_eq!(e.code, "CUST1001", "{e}");
        let e = tool_line("run").unwrap().line_of(&args(json!({"again": "yes"}))).unwrap_err();
        assert_eq!(e.code, "CUST1904", "{e}");
    }

    #[test]
    fn resource_addresses_read_back() {
        assert_eq!(Address::parse("custodian://readings/12").unwrap(), Address::Reading(12));
        assert_eq!(Address::parse("custodian://job/j-3/brief").unwrap().uri(), "custodian://job/j-3/brief");
        assert_eq!(Address::parse("custodian://readings/x").unwrap_err().code, "CUST1905");
        assert_eq!(Address::parse("file:///etc").unwrap_err().code, "CUST1905");
    }
}
