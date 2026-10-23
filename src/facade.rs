//! The facade: every run is a declared line executed through xccute's runtime, so the executor begins the
//! execution and every run leaves a receipt.
//!
//! A custodian line runs in this process: it is validated and previewed (`CommandPreview`), made a
//! `RuntimeOperation` inside a `RuntimeOperationPlan` (the plan is the job's, or `custodian`'s), handed to its
//! handler, and its exit status decided by the statuses the line names (`ExitStatusPolicy::of`); the
//! `RuntimeOperationReceipt` (plan digest, operation digest, decision) goes into custodian_kit's `Receipt` with the
//! sequence number, the UTC time, the spec's digest, the code's index key, the outputs, the exit status and the job.
//! A program custodian runs (cargo, code-passenger, custodian itself for a job's step) goes the same way as an
//! [`External`] line in a child process, with a receipt of its own.
//!
//! Journaled, the receipt is kept in the journal and as `data/receipts/<seq>.json`; free, it is printed in one
//! line and nothing is stored. Every run ends by saying what it did and where.

use crate::codes::{Code, Refusal, Result};
use crate::journal::{now_utc, Journal, ReadingRow};
use crate::layout::{self, Layout};
use crate::mode::Mode;
use crate::surface::Line;
use custodian_kit::api::{IndexKey, Receipt, Resolving, Scope, Workspace, RECEIPT_SCHEMA};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Instant;
use uh_tom_mick::AtomicAdmission;
use xccute_contract::{CommandPreview, CompositeShellCommand, ExitStatusWord, ValidatedCommand};
use xccute_runtime::{ExitStatusPolicy, RuntimeExitStatus, RuntimeOperation, RuntimeOperationPlan, RuntimeOperationReceipt, ShellCommand};

/// What a handler did: its status, its answer, what it said, what it wrote, and what the receipt names.
#[derive(Debug, Default, Clone)]
pub struct Ran {
    /// 0 done, 1 findings (the line's statuses).
    pub code: i32,
    /// What the receipt names: the spec, the crate, the check, the job.
    pub name: String,
    /// The answer: a page, a table.
    pub printed: Vec<String>,
    /// What it did and where, one fact per line.
    pub said: Vec<String>,
    /// Files written, relative to the root.
    pub outputs: Vec<String>,
    pub spec: Option<String>,
    pub spec_digest: Option<String>,
    pub index_key: Option<String>,
    pub code_digest: Option<String>,
    /// For a reuse: the run whose reading answered.
    pub reused_seq: Option<i64>,
    /// The job the run is part of.
    pub job: Option<String>,
    /// A reading, indexed under this run's number.
    pub reading: Option<ReadingRow>,
    /// An install receipt: (action, bin, destination, sha256, the installation receipt's JSON).
    pub install: Option<(String, String, String, String, String)>,
    /// A line that did its work and is still refused (a bounded job closed with lines no transform covers): the
    /// code it is refused with, its output kept.
    pub refusal: Option<Refusal>,
    /// Where the parsed index the run read came from (`parsed (31204 ms)`, `from cache (812 ms)`, `from daemon`): its
    /// receipt's reason says it.
    pub index: Option<String>,
    /// A reading's cost, or a reuse's saving (custodian_kit's `cost`): kept under this run's receipt number.
    pub cost: Option<custodian_kit::cost::CostRecord>,
}

impl Ran {
    pub fn named(name: impl Into<String>) -> Ran {
        Ran { name: name.into(), ..Ran::default() }
    }

    pub fn say(&mut self, line: impl Into<String>) {
        self.said.push(line.into());
    }

    pub fn print(&mut self, text: impl Into<String>) {
        self.printed.push(text.into());
    }
}

/// A line run: its status and receipt, and the lines it ends with.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Outcome {
    pub code: i32,
    pub receipt: Receipt,
    /// Journaled: `data/receipts/<seq>.json`.
    pub receipt_file: Option<String>,
    pub printed: Vec<String>,
    pub said: Vec<String>,
    /// A refused run: the liaise code and what was refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<Refusal>,
}

impl Outcome {
    /// The run's text: the answer, what it did and where, and the receipt line last.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for p in &self.printed {
            out.push_str(p.trim_end());
            out.push('\n');
        }
        for s in &self.said {
            out.push_str(s);
            out.push('\n');
        }
        let status = format!("{} ({})", self.receipt.exit_code.unwrap_or(-1), self.receipt.reason);
        match &self.receipt_file {
            Some(f) => out.push_str(&format!("receipt #{}: {f} · journal line {} · {} → {status}\n", self.receipt.seq, self.receipt.seq, self.receipt.line)),
            None => out.push_str(&format!("free mode (no .custodian here or above): nothing stored · {} → {status}\n", self.receipt.line)),
        }
        out
    }
}

/// The one writer in this process (uh-tom-mick's admission of one): a journaled run holds it.
pub fn writer() -> &'static AtomicAdmission {
    static W: OnceLock<AtomicAdmission> = OnceLock::new();
    W.get_or_init(|| AtomicAdmission::new(1).expect("an admission of one"))
}

/// A workspace held open, with the index key of what it read.
struct HeldWorkspace {
    root: PathBuf,
    scope: Scope,
    job: bool,
    key: IndexKey,
    ws: Workspace,
}

/// Where a run runs: the mode, the journal, the program to run for a job's step, the workspaces held open (the
/// resident keeps them across requests) and where live lines go.
pub struct Ctx {
    pub mode: Mode,
    pub journal: Option<Journal>,
    /// The running custodian: a job's steps run it.
    pub exe: PathBuf,
    held: Vec<HeldWorkspace>,
    /// Lines a run shows while it runs (a test's progress): stdout on the command line, the reply in the resident.
    pub live: Box<dyn FnMut(&str) + Send>,
    /// This run is the resident's (`custodian serve`): a job's readings run as the kit's declared lines.
    pub in_resident: bool,
    /// What opening the journal put back from the receipts and job folders (nothing, when it held them all).
    pub rebuilt: crate::journal::Rebuilt,
    /// Where the last workspace this run asked for came from: parsed, read from the cache, or held by the resident.
    pub index: Option<custodian_kit::reading::facts::IndexSource>,
}

impl Ctx {
    /// The run's context at `start`.
    pub fn at(start: &Path) -> Result<Ctx> {
        let mode = Mode::find(start)?;
        let mut rebuilt = crate::journal::Rebuilt::default();
        let journal = match &mode {
            Mode::Journaled(p) => {
                let mut j = Journal::open(&p.layout.path::<layout::Journal>(&[])?)?;
                // a fresh checkout: the journal rebuilt from the receipts and job folders it carries
                let receipts = p.layout.dir::<layout::ReceiptFile>(&[("seq", "1")])?;
                let jobs = p.layout.jobs_dir("x")?.parent().map(Path::to_path_buf).unwrap_or_default();
                rebuilt = j.rebuild_from(p.root(), &receipts, &jobs)?;
                Some(j)
            }
            Mode::Free { .. } => None,
        };
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("custodian"));
        Ok(Ctx { mode, journal, exe, held: Vec::new(), live: Box::new(|l| println!("{l}")), in_resident: false, rebuilt, index: None })
    }

    pub fn root(&self) -> &Path {
        self.mode.root()
    }

    pub fn layout(&self) -> Result<&Layout> {
        Ok(&self.mode.need_project()?.layout)
    }

    pub fn journal(&self) -> Result<&Journal> {
        self.journal.as_ref().ok_or_else(|| Code::NeedsFolder.refuse(self.root().display()))
    }

    pub fn journal_mut(&mut self) -> Result<&mut Journal> {
        let root = self.root().display().to_string();
        self.journal.as_mut().ok_or_else(|| Code::NeedsFolder.refuse(root))
    }

    /// `p` relative to the root.
    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(self.root()).unwrap_or(p).to_string_lossy().into_owned()
    }

    /// The job a run is part of: the one it names, else the one running (agent mode). A job named by its name
    /// (`--job toolkit-workspace`) is its id (`job-…`), the newest of that name, so its receipts carry the id its close
    /// reads them by.
    pub fn current_job(&self, named: Option<String>) -> Result<Option<String>> {
        if let Some(n) = named {
            return Ok(Some(match &self.journal {
                Some(j) => j.job(&n)?.map_or(n, |row| row.id),
                None => n,
            }));
        }
        match (&self.journal, self.mode.project()) {
            (Some(j), Some(p)) if p.agent() => Ok(j.jobs_in("running")?.into_iter().next().map(|r| r.id)),
            _ => Ok(None),
        }
    }

    /// The workspace for a scope, held open: reopened only when the code's index key moved.
    pub fn workspace(&mut self, scope: &Scope, how: Resolving) -> Result<&Workspace> {
        let root = self.root().to_path_buf();
        let job = how == Resolving::Job;
        let key = IndexKey::of_root(&root, scope)?;
        let at = self.held.iter().position(|h| h.root == root && &h.scope == scope && h.job == job);
        let reused = at.is_some_and(|i| self.held[i].key == key);
        match at {
            Some(_) if reused => {}
            Some(i) => {
                let ws = Workspace::open_with(&root, scope, how)?;
                self.held[i] = HeldWorkspace { root, scope: scope.clone(), job, key, ws };
            }
            None => {
                let ws = Workspace::open_with(&root, scope, how)?;
                self.held.push(HeldWorkspace { root, scope: scope.clone(), job, key, ws });
            }
        }
        let i = self.held.iter().position(|h| h.root == self.root() && &h.scope == scope && h.job == job).unwrap_or(0);
        // a workspace the resident held from a run before this one is the daemon's index
        self.index = Some(if reused && self.in_resident { custodian_kit::reading::facts::IndexSource::FromDaemon } else { self.held[i].ws.facts().index.clone() });
        Ok(&self.held[i].ws)
    }

    /// How many workspaces are held open (the resident's status).
    pub fn held_count(&self) -> usize {
        self.held.len()
    }

    /// Keeps a receipt (journaled), returning its number and file; free mode keeps nothing.
    pub fn keep(&mut self, r: &mut Receipt) -> Result<Option<PathBuf>> {
        let Some(layout) = self.mode.project().map(|p| p.layout.clone()) else { return Ok(None) };
        let journal = self.journal_mut()?;
        let (_, file) = journal.keep(r, |n| layout.path::<layout::ReceiptFile>(&[("seq", &n.to_string())]))?;
        Ok(Some(file))
    }
}

/// What ran, in custodian_kit's words for a receipt's `command` (`reading`, `test`, `check`, `git`, `gate`,
/// `job-close`): the journal's one vocabulary, which the kit's job close reads to list a job's readings, its tests
/// and its checks. A program's line whose kind the kit names is written with the kit's word.
pub const KIT_COMMANDS: &[(&str, &str)] = &[("cargo.test", "test"), ("git.diff", "git"), ("git.log", "git"), ("code-passenger.gate", "gate"), ("code-passenger.transform", "transform")];

/// The kit's word for a line that answered a spec, read or reused (`custodian run`, `count`, `diagnostics`,
/// `bundle`, `sweep`).
pub const READING: &str = "reading";

/// A receipt's `command`: `reading` for a run that answered a spec; the kit's word for a program's line, else the
/// line's id; the subcommand for every other custodian line.
pub fn command_word<'a>(id: &'a str, ran: &Ran) -> &'a str {
    if ran.spec_digest.is_some() {
        return READING;
    }
    KIT_COMMANDS.iter().find(|(line, _)| *line == id).map_or(id, |(_, word)| word)
}

/// The plan a run belongs to: its job's, or custodian's own.
pub fn plan_id(job: Option<&str>) -> String {
    match job {
        Some(j) => format!("custodian.job.{j}"),
        None => "custodian".into(),
    }
}

/// Runs a custodian line in this process through the runtime, keeps its receipt, and returns the outcome.
pub fn execute(ctx: &mut Ctx, line: &Line, handler: impl FnOnce(&mut Ctx, &Line) -> Result<Ran>) -> Outcome {
    let started = Instant::now();
    let preview = line.preview();
    let operation = RuntimeOperation::from_preview(format!("custodian.{}", line.segment()), &preview);
    let ran = match line.validate() {
        Err(e) => Err(Code::LineRefused.refuse(e)),
        Ok(()) => handler(ctx, line),
    };
    let (ran, refusal) = match ran {
        Ok(mut r) => {
            let refused = r.refusal.take();
            (r, refused)
        }
        Err(e) => (Ran::named(line.segment()), Some(e)),
    };
    let code = if refusal.is_some() { 2 } else { ran.code };
    let plan = RuntimeOperationPlan::new(plan_id(ran.job.as_deref())).then(operation.clone());
    let decision = line.decide(code);
    let rr = RuntimeOperationReceipt::new(&plan, &operation, decision);
    let mut receipt = receipt_of(&plan, &rr, &preview, &ran, started.elapsed().as_millis() as i64);
    receipt.command = command_word(line.segment(), &ran).to_string();
    if let Some(r) = &refusal {
        receipt.reason = format!("{} {r}", receipt.reason);
    }
    if let Some(i) = &ran.index {
        receipt.reason = format!("{} · index {i}", receipt.reason);
    }
    let mut said = ran.said.clone();
    if let Some(r) = &refusal {
        said.push(r.to_string());
    }
    let file = match ctx.keep(&mut receipt) {
        Ok(f) => f,
        Err(e) => {
            said.push(format!("the receipt was not kept: {e}"));
            None
        }
    };
    if let (Some(row), Some(j)) = (&ran.reading, ctx.journal.as_ref()) {
        let mut row = row.clone();
        row.seq = receipt.seq;
        if let Err(e) = j.index_reading(&row) {
            said.push(format!("the reading was not indexed: {e}"));
        }
    }
    if let (Some(rec), Some(layout)) = (&ran.cost, ctx.mode.project().map(|p| p.layout.clone())) {
        let rec = custodian_kit::cost::CostRecord { seq: receipt.seq, utc: receipt.utc.clone(), job: receipt.job.clone(), ..rec.clone() };
        match layout.dir::<layout::CostFile>(&[("seq", "1")]).map_err(|e| e.to_string()).and_then(|d| custodian_kit::cost::keep_in(&d, &rec).map_err(|e| e.to_string())) {
            Ok(_) => said.push(custodian_kit::cost::said(&rec)),
            Err(e) => said.push(format!("the cost record was not kept: {e}")),
        }
    }
    if let (Some((action, bin, dest, sha, json)), Some(j)) = (&ran.install, ctx.journal.as_ref()) {
        if let Err(e) = j.install(receipt.seq, action, bin, dest, sha, json) {
            said.push(format!("the install receipt was not journaled: {e}"));
        }
    }
    Outcome { code, receipt_file: file.map(|f| ctx.rel(&f)), receipt, printed: ran.printed, said, refusal }
}

fn receipt_of(plan: &RuntimeOperationPlan, rr: &RuntimeOperationReceipt, preview: &CommandPreview, ran: &Ran, millis: i64) -> Receipt {
    Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: now_utc(),
        command: preview.argv().first().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default(),
        name: ran.name.clone(),
        spec: ran.spec.clone(),
        spec_hash: ran.spec_digest.clone(),
        code_digest: ran.code_digest.clone(),
        index_key: ran.index_key.clone(),
        plan_id: plan.plan_id.clone(),
        plan_digest: rr.plan_digest.to_string(),
        operation_digest: rr.operation_digest.to_string(),
        receipt_digest: rr.digest().to_string(),
        line: preview.display().to_string(),
        exit_code: rr.decision.status.code,
        accepted: rr.decision.status.accepted,
        disposition: format!("{:?}", rr.decision.disposition),
        reason: rr.decision.reason.clone(),
        outputs: ran.outputs.clone(),
        job: ran.job.clone(),
        reused_seq: ran.reused_seq,
        millis,
        // a custodian line is answered in this process; finish_external marks a program's run as the line runner's
        runner: custodian_kit::receipts::RUNNER_IN_PROCESS.into(),
    }
}

/// A program custodian runs, as a declared line: its preview, the process, and the statuses it names.
pub struct External {
    /// The operation's id in the plan: `cargo.test`, `code-passenger.gate`, `custodian.run`.
    pub logical_id: String,
    pub preview: CommandPreview,
    pub command: Command,
    pub ok: &'static [i32],
    pub statuses: &'static [ExitStatusWord],
}

impl External {
    /// A declared argv line (`CompositeShellCommand`), validated before anything runs.
    pub fn of<C: CompositeShellCommand + ValidatedCommand>(logical_id: &str, line: &C) -> Result<External> {
        line.validate().map_err(|e| Code::LineRefused.refuse(e))?;
        Ok(External { logical_id: logical_id.into(), preview: line.preview(), command: line.to_std_command(), ok: line.ok_codes(), statuses: line.exit_statuses() })
    }

    /// A declared line run from a program at a path of its own (the running custodian), its words the line's.
    pub fn at<C: CompositeShellCommand + ValidatedCommand>(logical_id: &str, program: &Path, line: &C) -> Result<External> {
        let mut e = External::of(logical_id, line)?;
        let mut cmd = Command::new(program);
        cmd.args(line.argv());
        e.command = cmd;
        Ok(e)
    }

    /// A shell-string builder from xccute_commands (`CargoBuilder`): run as xccute runs it (`to_command()`).
    pub fn shell<S: ShellCommand>(logical_id: &str, cmd: &S) -> External {
        let display = cmd.build();
        let preview = CommandPreview::new(OsString::from("sh"), vec![OsString::from("-c"), OsString::from(&display)]);
        External { logical_id: logical_id.into(), preview, command: cmd.to_command(), ok: cmd.ok_codes(), statuses: &[] }
    }

    /// Sets the directory the program runs in.
    pub fn in_dir(mut self, dir: &Path) -> External {
        self.command.current_dir(dir);
        self
    }

    /// Sets an environment variable for the program.
    pub fn env(mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) -> External {
        self.command.env(key, value);
        self
    }
}

/// An external line, run.
#[derive(Debug, Clone)]
pub struct ExternalRun {
    pub code: Option<i32>,
    pub accepted: bool,
    pub stdout: String,
    pub stderr: String,
    pub millis: i64,
    pub receipt: Receipt,
    pub receipt_file: Option<PathBuf>,
}

/// What an external run's receipt names besides the line.
#[derive(Debug, Default, Clone)]
pub struct ExternalNames {
    pub name: String,
    pub job: Option<String>,
    pub outputs: Vec<String>,
}

/// Starts an external line (its stdout and stderr piped).
pub fn spawn(mut ext: External) -> Result<(std::process::Child, External)> {
    ext.command.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    let child = ext.command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Code::ProgramMissing.refuse(format!("{}: {e}", ext.preview.display()))
        } else {
            Code::LineFailed.refuse(format!("{}: {e}", ext.preview.display()))
        }
    })?;
    Ok((child, ext))
}

/// Runs an external line to its end through the runtime, showing each stdout line to `each` as it comes, and keeps
/// its receipt.
pub fn run_external(ctx: &mut Ctx, ext: External, names: ExternalNames, mut each: impl FnMut(&str)) -> Result<ExternalRun> {
    // James's battery rule: a cargo line (declared, or a shell line that runs cargo) does not start on a dying battery
    let shown = ext.preview.argv().last().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
    if custodian_kit::power::is_cargo(ext.preview.program()) || ext.logical_id.starts_with("cargo.") || shown.starts_with("cargo ") {
        custodian_kit::power::cargo_may_start()?;
    }
    let started = Instant::now();
    let (mut child, ext) = spawn(ext)?;
    let mut err_pipe = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let mut stdout = String::new();
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(std::result::Result::ok) {
            each(&line);
            stdout.push_str(&line);
            stdout.push('\n');
        }
    }
    let status = child.wait().map_err(|e| Code::LineFailed.refuse(format!("{}: {e}", ext.preview.display())))?;
    let stderr = err_thread.join().unwrap_or_default();
    finish_external(ctx, &ext, names, status.code(), stdout, stderr, started.elapsed().as_millis() as i64)
}

/// The receipt of an external line that ended with `code`.
pub fn finish_external(ctx: &mut Ctx, ext: &External, names: ExternalNames, code: Option<i32>, stdout: String, stderr: String, millis: i64) -> Result<ExternalRun> {
    let accepted = code.is_some_and(|c| ext.ok.contains(&c));
    let operation = RuntimeOperation::from_preview(ext.logical_id.clone(), &ext.preview);
    let plan = RuntimeOperationPlan::new(plan_id(names.job.as_deref())).then(operation.clone());
    let decision = ExitStatusPolicy::named(ext.statuses).decide(RuntimeExitStatus::new(code, accepted));
    let rr = RuntimeOperationReceipt::new(&plan, &operation, decision);
    let ran = Ran { name: names.name, outputs: names.outputs, job: names.job, ..Ran::default() };
    let mut receipt = receipt_of(&plan, &rr, &ext.preview, &ran, millis);
    receipt.command = command_word(&ext.logical_id, &ran).to_string();
    receipt.runner = custodian_kit::receipts::RUNNER_LINE.into();
    let receipt_file = ctx.keep(&mut receipt)?;
    Ok(ExternalRun { code, accepted, stdout, stderr, millis, receipt, receipt_file })
}

/// The refusal of a line whose status was not accepted.
pub fn not_accepted(run: &ExternalRun) -> Refusal {
    let tail: String = run.stderr.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    Code::StepFailed.refuse(format!("{} → {:?} ({}){}", run.receipt.line, run.code, run.receipt.reason, if tail.is_empty() { String::new() } else { format!("\n{tail}") }))
}
