//! The command line: the words read into a declared line, the line executed through the facade (or sent to the
//! resident when one answers on the project's socket), and the outcome said.

use crate::codes::{Code, Result};
use crate::facade::{self, Ctx, Outcome, Ran};
use crate::layout::{self, write_whole, Layout};
use crate::mode::{FolderMode, Mode, Settings};
use crate::readings::{self, Ask};
use crate::surface::{self, s, so, Line};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The program: the words after `custodian`, run; the process's exit status returned.
pub fn main_argv(args: &[OsString]) -> i32 {
    if args.is_empty() || args.first().is_some_and(|a| a == "help" || a == "--help" || a == "-h") {
        println!("{}", usage());
        return 0;
    }
    // the kit's reading line, run by its job code: read back into the kit's declared line, never journaled here
    // (the kit keeps its receipt)
    if args.first().is_some_and(|a| a == "exec-reading") {
        return exec_reading(&args[1..]);
    }
    let line = match surface::read(args) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("custodian with no words lists every line");
            return 2;
        }
    };
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Line::McpServe(m) = &line {
        return mcp_serve(&here, so(&m.root));
    }
    if let Some(outcome) = crate::serve::try_socket(&here, &line) {
        print!("{}", outcome.text());
        return outcome.code;
    }
    let mut ctx = match Ctx::at(&here) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if let Line::Serve(_) = line {
        return crate::serve::serve(ctx);
    }
    let outcome = run_line(&mut ctx, &line);
    print!("{}", outcome.text());
    outcome.code
}

/// Every line, for `custodian` with no words: the root's help lines, generated from the declarations.
pub fn usage() -> String {
    let mut s = String::from("custodian: readings of a Rust workspace, each run a declared line with a receipt\n\n");
    let lines = surface::usage_lines();
    let width = lines.iter().map(|l| l.words.chars().count()).max().unwrap_or(0).min(72);
    for l in lines {
        let about = l.about.split(". ").next().unwrap_or(l.about).trim_end_matches('.');
        if l.words.chars().count() > width {
            s.push_str(&format!("  {}\n  {:width$}  {about}\n", l.words, ""));
        } else {
            s.push_str(&format!("  {:width$}  {about}\n", l.words));
        }
    }
    s.push_str("\nexit status: 0 done, 1 findings, 2 refused. Inside an initialized folder (.custodian here or above) every run is journaled; elsewhere it is free mode and nothing is stored.\n");
    s
}

/// A line executed through the facade in this process, holding the process's one writer.
pub fn run_line(ctx: &mut Ctx, line: &Line) -> Outcome {
    let held = facade::writer().try_enter();
    match held {
        Ok(_guard) => facade::execute(ctx, line, handle),
        Err(e) => facade::execute(ctx, line, move |_, _| Err(Code::WriterHeld.refuse(e))),
    }
}

/// The handler of each line.
pub fn handle(ctx: &mut Ctx, line: &Line) -> Result<Ran> {
    use crate::install::SelfAsk;
    match line {
        Line::Init(l) => init_with(ctx, FolderMode::Plain, l.macros, so(&l.job_mode)),
        Line::AgentInit(l) => init_with(ctx, FolderMode::Agent, l.macros, so(&l.job_mode)),
        Line::AgentBrief(l) => crate::agent::brief(ctx, l.head),
        Line::AgentGuard(l) => crate::agent::guard(ctx, &s(&l.file)),
        Line::Run(l) => run(ctx, l),
        Line::List(_) => readings::list(ctx),
        Line::Diff(l) => readings::diff(ctx, &s(&l.spec), so(&l.from).and_then(|v| v.parse().ok()), so(&l.to).and_then(|v| v.parse().ok())),
        Line::JobStart(l) => job_start(ctx, l),
        Line::JobClose(l) => need_agent(ctx).and_then(|_| crate::jobs::close(ctx, so(&l.name).as_deref())),
        Line::JobBrief(l) => need_agent(ctx).and_then(|_| crate::jobs::brief(ctx, so(&l.name).as_deref(), l.bundle)),
        Line::JobImport(l) => need_agent(ctx).and_then(|_| crate::jobs::import(ctx, Path::new(&s(&l.bundle)), crate::runtime::Limit::default())),
        Line::JobList(_) => need_agent(ctx).and_then(|_| crate::jobs::list(ctx)),
        Line::JobVerify(l) => need_agent(ctx).and_then(|_| crate::jobs::verify(ctx, so(&l.job).as_deref(), &s(&l.line))),
        Line::TestRun(l) => crate::testing::test(ctx, l),
        Line::TestReport(_) => crate::testing::report(ctx),
        Line::Check(l) => crate::maint::check(ctx, &s(&l.which), so(&l.job)),
        Line::Annotate(l) => crate::maint::annotate(ctx, so(&l.from).as_deref(), l.apply),
        Line::Features(l) => crate::maint::features(ctx, l.apply),
        Line::DiagnosticsList(l) => crate::diag::diagnostics(ctx, "list", so(&l.krate).as_deref()),
        Line::DiagnosticsWhere(l) => crate::diag::diagnostics(ctx, "where", Some(&s(&l.code))),
        Line::DiagnosticsFrom(l) => crate::diag::diagnostics(ctx, "from", Some(&s(&l.log))),
        Line::Triage(l) => crate::diag::triage(ctx, &s(&l.target)),
        Line::Bundle(l) => crate::bundling::bundle(ctx, l),
        Line::Sweep(l) => readings_sweep(ctx, &s(&l.dir)),
        Line::CountOf(l) => crate::counting::count(ctx, l),
        Line::CountReport(l) => crate::counting::report(ctx, &s(&l.a), &s(&l.b)),
        Line::CountHistory(l) => crate::counting::history(ctx, so(&l.route).as_deref()),
        Line::Serve(_) => Err(Code::ServeRunning.refuse("serve runs in the foreground: custodian serve")),
        Line::Stop(_) => Ok(Ran { said: vec!["no resident answers on this project's socket: nothing to stop".into()], ..Ran::named("stop") }),
        Line::Watch(l) => crate::observe::watch(ctx, l),
        Line::SelfInstall(l) => crate::install::self_line(ctx, &SelfAsk { action: "install", from: so(&l.from), bin_dir: so(&l.bin_dir), state_dir: so(&l.state_dir), dry_run: l.dry_run, force: l.force }),
        Line::SelfUninstall(l) => crate::install::self_line(ctx, &SelfAsk { action: "uninstall", bin_dir: so(&l.bin_dir), state_dir: so(&l.state_dir), force: l.force, ..SelfAsk::default() }),
        Line::SelfStatus(l) => crate::install::self_line(ctx, &SelfAsk { action: "status", bin_dir: so(&l.bin_dir), state_dir: so(&l.state_dir), ..SelfAsk::default() }),
        Line::McpServe(_) => Err(Code::McpForeground.refuse("custodian mcp serve")),
        Line::Preflight(l) => crate::changes::preflight(ctx, &s(&l.spec), so(&l.job)),
        Line::Transform(l) => crate::changes::transform(ctx, so(&l.spec).as_deref(), l.write, so(&l.revert).as_deref(), so(&l.job)),
        Line::PlanMake(l) => crate::changes::plan_in(ctx, &s(&l.intent), so(&l.name), so(&l.job)),
        Line::PlanApprove(l) => crate::changes::approve_in(ctx, &s(&l.plan), &s(&l.step), so(&l.operation).as_deref(), so(&l.job)),
        Line::PlanShow(l) => crate::changes::show_in(ctx, &s(&l.plan), so(&l.job)),
        Line::Decide(l) => crate::changes::decide(ctx, so(&l.id).as_deref(), so(&l.answer).as_deref(), so(&l.why).as_deref()),
        Line::LinkageAdd(l) => crate::changes::linkage_add(ctx, &s(&l.name), so(&l.kind).as_deref(), so(&l.why).as_deref(), &l.bind.iter().map(s).collect::<Vec<_>>()),
        Line::LinkageCheck(l) => crate::changes::linkage_check(ctx, so(&l.target).as_deref()),
        Line::ExerciseRun(l) => crate::exercise::run(ctx, l),
        Line::ExerciseReport(l) => crate::exercise::report(ctx, so(&l.job).as_deref()),
    }
}

/// `custodian mcp serve [--root DIR]`: the process moves to the project (so a relative path in a tool's arguments
/// is the project's, as on the command line) and serves MCP on stdin and stdout until the host closes them.
fn mcp_serve(here: &Path, root: Option<String>) -> i32 {
    let root = root.map(|r| here.join(r)).unwrap_or_else(|| here.to_path_buf());
    if let Err(e) = std::env::set_current_dir(&root) {
        eprintln!("{}", Code::McpTransport.refuse(format!("{}: {e}", root.display())));
        return 2;
    }
    #[cfg(feature = "mcp")]
    {
        crate::mcp::serve_stdio(&root)
    }
    #[cfg(not(feature = "mcp"))]
    {
        eprintln!("{}", Code::McpBuiltWithout.refuse("cargo install custodian --features mcp"));
        2
    }
}

fn run(ctx: &mut Ctx, l: &surface::Run) -> Result<Ran> {
    let job = ctx.current_job(so(&l.job))?;
    let ask = Ask { again: l.again, job: job.clone(), word: String::new() };
    match (so(&l.target), so(&l.kind)) {
        (None, None) => {
            let project = ctx.mode.need_project()?.clone();
            let mut ran = Ran::named("queue");
            let specs = readings::queued_specs(ctx)?;
            for spec in &specs {
                let rel = ctx.rel(spec);
                let one = surface::Line::Run(surface::Run { target: Some(spec.as_os_str().to_os_string()), job: job.clone().map(Into::into), again: l.again, kind: None });
                let out = facade::execute(ctx, &one, handle);
                ran.say(format!("{rel}: receipt #{} → {} ({})", out.receipt.seq, out.code, out.receipt.reason));
                ran.said.extend(out.said.into_iter().map(|x| format!("  {x}")));
                ran.code = ran.code.max(if out.code == 2 { 1 } else { out.code });
            }
            if project.agent() {
                crate::jobs::run_queue(ctx, l.again, &mut ran)?;
            }
            if specs.is_empty() && ran.said.is_empty() {
                ran.say("nothing queued: no specs in data/specs/queued, no folders in state/jobs/queued");
            }
            Ok(ran)
        }
        (target, kind) => readings::run_one(ctx, target.as_deref(), kind.as_deref(), &ask),
    }
}

fn need_agent(ctx: &Ctx) -> Result<()> {
    ctx.mode.need_project()?.need_agent()
}

fn job_start(ctx: &mut Ctx, l: &surface::JobStart) -> Result<Ran> {
    need_agent(ctx)?;
    crate::jobs::start(
        ctx,
        &crate::jobs::StartAsk { which: so(&l.name), kind: so(&l.kind), target: so(&l.target), intent: so(&l.intent), again: l.again, mode: so(&l.mode), verify: l.verify.iter().map(s).collect(), ..Default::default() },
    )
}

fn readings_sweep(ctx: &mut Ctx, dir: &str) -> Result<Ran> {
    let spec = readings::inline_spec("sweep", dir)?;
    let job = ctx.current_job(None)?;
    if job.is_some() {
        return Err(custodian_kit::api::RefusalCode::WalkedPathInJob.refuse(format!("sweep {dir}: a sweep is never part of a job (close it first)")).into());
    }
    let (ran, _) = readings::read_spec(ctx, &spec, &readings::From::Line, &Ask::default())?;
    Ok(ran)
}

/// `custodian init` / `custodian agent init`: the folder, its settings, the declared routes file, the rules
/// custodian reads, and (agent) the job folders and templates. An existing folder is kept; its mode is raised to
/// agent when asked.
/// `custodian init` / `agent init` as 0.1.0 had it: no job mode named (the workspace's default stays unbounded).
pub fn init(ctx: &mut Ctx, mode: FolderMode, macros: bool) -> Result<Ran> {
    init_with(ctx, mode, macros, None)
}

/// `custodian init [--job-mode MODE]` / `agent init [--job-mode MODE]`.
pub fn init_with(ctx: &mut Ctx, mode: FolderMode, macros: bool, job_mode: Option<String>) -> Result<Ran> {
    let here = std::env::current_dir().map_err(|e| Code::Write.refuse(e))?;
    let root = match &ctx.mode {
        Mode::Journaled(p) => p.root().to_path_buf(),
        Mode::Free { .. } => here.canonicalize().map_err(|e| Code::Write.refuse(e))?,
    };
    let layout = Layout::at(&root)?;
    let mut ran = Ran::named(layout::FOLDER);
    let mut made: Vec<PathBuf> = Vec::new();
    let settings_file = layout.path::<layout::Settings>(&[])?;
    let mut settings: Settings = std::fs::read_to_string(&settings_file).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or(Settings { mode, exclude: Vec::new(), job_mode: None });
    if mode == FolderMode::Agent {
        settings.mode = FolderMode::Agent;
    }
    if let Some(m) = &job_mode {
        settings.job_mode = Some(m.parse().map_err(|e: String| Code::WordRefused.refuse(e))?);
    }
    write_whole(&settings_file, toml::to_string(&settings).map_err(|e| Code::Write.refuse(e))?.as_bytes())?;
    made.push(settings_file);
    let mut keep = |p: PathBuf, text: &str| -> Result<()> {
        if !p.exists() {
            write_whole(&p, text.as_bytes())?;
            made.push(p);
        }
        Ok(())
    };
    // the routes table and the 1o schemas, as custodian_kit lays them out (the language server reads the schemas
    // through oneo.project.json at the root)
    let old_paths = layout.path::<layout::ConfigFile>(&[("name", "paths")])?;
    if !old_paths.exists() {
        keep(layout.path::<layout::PathsTable>(&[])?, &custodian_kit::oneo::render(custodian_kit::oneo::Doc::Paths, &custodian_kit::paths::PathsFile::default())?)?;
    }
    keep(layout.path::<layout::ConfigFile>(&[("name", "features")])?, custodian_kit::features::SHIPPED_THRESHOLDS)?;
    keep(layout.path::<layout::ConfigFile>(&[("name", "annotate")])?, custodian_kit::annotate::SHIPPED_ANNOTATE)?;
    if settings.mode == FolderMode::Agent {
        for (kind, text) in custodian_kit::jobs::SHIPPED_TEMPLATES {
            keep(layout.path::<layout::JobTemplate>(&[("kind", kind)])?, text)?;
        }
    }
    let schema_dir = layout.dir::<layout::Schema>(&[("modifier", "x")])?.parent().map(Path::to_path_buf).unwrap_or_default();
    if !schema_dir.exists() {
        made.extend(custodian_kit::oneo::write_schemas(&schema_dir)?);
        made.push(custodian_kit::oneo::declare_schema_dir(&root, &layout.rel(&schema_dir))?);
    }
    let mut dirs = vec![layout.dir::<layout::QueuedSpec>(&[("name", "x")])?, layout.dir::<layout::DoneSpec>(&[("name", "x")])?, layout.readings_dir()?, layout.dir::<layout::ReceiptFile>(&[("seq", "1")])?];
    if settings.mode == FolderMode::Agent {
        for state in ["queued", "running", "done"] {
            dirs.push(layout.jobs_dir(state)?);
        }
    }
    for d in dirs {
        if !d.exists() {
            std::fs::create_dir_all(&d).map_err(|e| Code::Write.refuse(format!("{}: {e}", d.display())))?;
            made.push(d);
        }
    }
    // the job lock a queue run holds (xccute's LockFile never makes it)
    let lock = layout.folder().join(crate::runtime::JOB_LOCK);
    if !lock.exists() {
        write_whole(&lock, b"")?;
        made.push(lock);
    }
    let journal_file = layout.path::<layout::Journal>(&[])?;
    let fresh = !journal_file.exists();
    let journal = crate::journal::Journal::open(&journal_file)?;
    if fresh {
        made.push(journal_file);
    }
    ran.say(format!("initialized {} ({}) at {}", layout::FOLDER, if settings.mode == FolderMode::Agent { "agent: specs, readings, journal, jobs as folders" } else { "plain: specs, readings, journal" }, root.display()));
    for m in &made {
        ran.say(format!("  {}", layout.rel(m)));
    }
    ran.outputs = made.iter().map(|m| layout.rel(m)).collect();
    // from here on the run is journaled: its own receipt is the folder's first line
    ctx.mode = Mode::Journaled(crate::mode::Project { layout: layout.clone(), settings });
    ctx.journal = Some(journal);
    if macros {
        macros_by_passenger(ctx, &mut ran)?;
    }
    Ok(ran)
}

/// The `maintenance` a project's crates get from crates.io: the one custodian itself builds against (Cargo.toml).
pub const MAINTENANCE: &str = "maintenance@0.1.0";

/// `init --macros`: `maintenance` (from crates.io, [`MAINTENANCE`]) added to every workspace crate by
/// code_passenger's `dep add`, and `big-bad-bool` to each crate that declares a level word (`sensitivity_levels!`);
/// one declared line and one receipt each. custodian edits no manifest itself.
fn macros_by_passenger(ctx: &mut Ctx, ran: &mut Ran) -> Result<()> {
    if !crate::maint::passenger_installed() {
        ran.say("--macros: code-passenger is not on PATH; nothing added");
        ran.code = 1;
        return Ok(());
    }
    let root = ctx.root().to_path_buf();
    let (crates, words): (Vec<String>, Vec<String>) = {
        let ws = ctx.workspace(&custodian_kit::api::Scope::default(), custodian_kit::api::Resolving::Free)?;
        let k = ws.klepto();
        let mut crates: Vec<String> = k.packages.values().map(|p| p.cargo_name.clone()).collect();
        crates.sort();
        let mut words: Vec<String> = k
            .macros_inv
            .iter()
            .filter(|m| m.name == "sensitivity_levels")
            .filter_map(|m| k.packages.values().find(|p| ws.root.join(&m.location.path).starts_with(&p.root)).map(|p| p.cargo_name.clone()))
            .collect();
        words.sort();
        words.dedup();
        (crates, words)
    };
    let mut adds: Vec<(String, Vec<String>)> = crates.iter().map(|c| (c.clone(), vec!["dep".into(), "add".into(), c.clone(), MAINTENANCE.into()])).collect();
    adds.extend(words.iter().map(|c| (c.clone(), vec!["dep".into(), "add".into(), c.clone(), "big-bad-bool@0.1.1".into()])));
    for (krate, words) in adds {
        let line = crate::maint::passenger(&root, words.iter().map(String::as_str));
        let run = crate::maint::run_passenger(ctx, "code-passenger.dep", line, &format!("dep add {krate}"), None)?;
        ran.say(format!("--macros: {} → {:?} ({}) #{}", run.receipt.line, run.code, run.receipt.reason, run.receipt.seq));
        if !run.accepted {
            ran.code = 1;
        }
    }
    Ok(())
}

/// `custodian exec-reading --spec S --root R --out DIR|- --stamp T [--job ID]`: custodian_kit's declared reading
/// line (`exec::ExecReading`), as its job code runs it. The words are read back into that line and must be what
/// it writes; the reading is answered by `custodian_kit::reading::exec_reading`, its summary the last line.
fn exec_reading(rest: &[OsString]) -> i32 {
    let mut w = surface::Words::new(&rest.iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>());
    let read = (|| -> Result<(custodian_kit::exec::ExecReadingRooted<custodian_kit::exec::Custodian>, bool)> {
        let need = |w: &mut surface::Words, k: &str| -> Result<String> { w.kv(k)?.ok_or_else(|| Code::LineRefused.refuse(format!("exec-reading needs {k}"))) };
        let (spec, root, out, stamp) = (need(&mut w, "--spec")?, need(&mut w, "--root")?, need(&mut w, "--out")?, need(&mut w, "--stamp")?);
        let job = w.kv("--job")?;
        let mut line = custodian_kit::exec::Custodian::exec_reading(spec.as_str(), root.as_str(), out.as_str(), stamp.as_str());
        if let Some(j) = &job {
            line = line.job(j.as_str());
        }
        Ok((line, job.is_some()))
    })();
    let (line, in_job) = match read.and_then(|x| w.done("exec-reading").map(|_| x)) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    use xccute_contract::{CompositeShellCommand, ValidatedCommand};
    if line.argv()[1..] != *rest || line.validate().is_err() {
        eprintln!("{}", Code::LineRefused.refuse("exec-reading: the words are not the kit's declared line, in its order"));
        return 1;
    }
    let v = |k: &str| rest.iter().position(|a| a == k).and_then(|i| rest.get(i + 1)).map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
    match custodian_kit::reading::exec_reading(Path::new(&v("--spec")), Path::new(&v("--root")), &v("--out"), &v("--stamp"), in_job) {
        Ok(summary) => {
            println!("{}", serde_json::to_string(&summary).unwrap_or_default());
            0
        }
        Err(e) => {
            eprintln!("{}", crate::codes::Refusal::from(e));
            1
        }
    }
}
