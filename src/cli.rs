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
    let line = match surface::read(args) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("custodian with no words lists every line");
            return 2;
        }
    };
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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

/// Every line, for `custodian` with no words.
pub fn usage() -> String {
    let mut s = String::from("custodian: readings of a Rust workspace, each run a declared line with a receipt\n\n");
    for (_, u) in surface::USAGE {
        s.push_str(&format!("  custodian {u}\n"));
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
    match line {
        Line::Init(l) => init(ctx, FolderMode::Plain, l.macros),
        Line::Agent(l) => init(ctx, FolderMode::Agent, l.macros),
        Line::Run(l) => run(ctx, l),
        Line::List(_) => readings::list(ctx),
        Line::Diff(l) => readings::diff(ctx, &s(&l.spec), so(&l.from).and_then(|v| v.parse().ok()), so(&l.to).and_then(|v| v.parse().ok())),
        Line::Job(l) => job(ctx, l),
        Line::Test(l) => crate::testing::test(ctx, l),
        Line::Check(l) => crate::maint::check(ctx, &s(&l.which), so(&l.job)),
        Line::Annotate(l) => crate::maint::annotate(ctx, so(&l.from).as_deref(), l.apply),
        Line::Features(l) => crate::maint::features(ctx, l.apply),
        Line::Diagnostics(l) => crate::diag::diagnostics(ctx, &s(&l.action), so(&l.arg).as_deref()),
        Line::Triage(l) => crate::diag::triage(ctx, &s(&l.target)),
        Line::Bundle(l) => crate::bundling::bundle(ctx, l),
        Line::Sweep(l) => readings_sweep(ctx, &s(&l.dir)),
        Line::Serve(_) => Err(Code::ServeRunning.refuse("serve runs in the foreground: custodian serve")),
        Line::Stop(_) => Ok(Ran { said: vec!["no resident answers on this project's socket: nothing to stop".into()], ..Ran::named("stop") }),
        Line::Watch(l) => crate::observe::watch(ctx, l),
        Line::SelfCmd(l) => crate::install::self_line(ctx, l),
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

fn job(ctx: &mut Ctx, l: &surface::Job) -> Result<Ran> {
    ctx.mode.need_project()?.need_agent()?;
    let name = so(&l.name);
    match s(&l.action).as_str() {
        "start" => crate::jobs::start(ctx, &crate::jobs::StartAsk { which: name, kind: so(&l.kind), target: so(&l.target), again: l.again, ..Default::default() }),
        "close" => crate::jobs::close(ctx, name.as_deref()),
        "brief" => crate::jobs::brief(ctx, name.as_deref()),
        "import" => crate::jobs::import(ctx, Path::new(name.as_deref().unwrap_or_default()), crate::runtime::Limit::default()),
        _ => crate::jobs::list(ctx),
    }
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
pub fn init(ctx: &mut Ctx, mode: FolderMode, macros: bool) -> Result<Ran> {
    let here = std::env::current_dir().map_err(|e| Code::Write.refuse(e))?;
    let root = match &ctx.mode {
        Mode::Journaled(p) => p.root().to_path_buf(),
        Mode::Free { .. } => here.canonicalize().map_err(|e| Code::Write.refuse(e))?,
    };
    let layout = Layout::at(&root)?;
    let mut ran = Ran::named(layout::FOLDER);
    let mut made: Vec<PathBuf> = Vec::new();
    let settings_file = layout.path::<layout::Settings>(&[])?;
    let mut settings: Settings = std::fs::read_to_string(&settings_file).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or(Settings { mode, exclude: Vec::new() });
    if mode == FolderMode::Agent {
        settings.mode = FolderMode::Agent;
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
    let old_paths = root.join(layout::FOLDER).join("config").join("paths.toml");
    if !old_paths.exists() {
        keep(layout.path::<layout::PathsTable>(&[])?, &custodian_kit::oneo::render(custodian_kit::oneo::Doc::Paths, &custodian_kit::paths::PathsFile::default())?)?;
    }
    keep(layout.path::<layout::ConfigFile>(&[("name", "features")])?, custodian_kit::features::SHIPPED_THRESHOLDS)?;
    keep(layout.path::<layout::ConfigFile>(&[("name", "annotate")])?, custodian_kit::annotate::SHIPPED_ANNOTATE)?;
    if settings.mode == FolderMode::Agent {
        for (kind, text) in crate::jobs::SHIPPED {
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
        ran.say(format!(
            "--macros: not added. code_passenger's cargo editing (gate --spec) marks existing dependencies optional and sets features; it has no line that adds a dependency, and custodian never edits a manifest itself. The line to add by hand: maintenance = {{ path = \"{}\" }}",
            {
                let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../maintenance_macros/crates/maintenance");
                p.canonicalize().unwrap_or(p).display().to_string()
            }
        ));
        ran.code = 1;
    }
    Ok(ran)
}
