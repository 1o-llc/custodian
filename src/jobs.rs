//! Jobs as folders (agent mode), run by custodian_kit's job code: the command line declares the lines and keeps
//! their receipts; `custodian_kit::jobs` owns the lifecycle.
//!
//! - `job start [NAME|ID] [--intent TEXT]`: the kit's `jobs::start`: the folder moved from `jobs/queued` to
//!   `jobs/running`, the task history's task and intent records written (a job name that already has a task opens
//!   its next revision), the job counted when its template or job file says `count = true`, its specs or template
//!   steps run as the kit's reading lines (each `custodian exec-reading`, with a receipt). A name with no queued
//!   folder opens an empty one, for an agentic task. `--intent` replaces the kit's generated intent with the asker's.
//!   With code_passenger installed, a passenger checkpoint of the tree is taken at open.
//! - `job close [ID]`: a passenger checkpoint at close, then the kit's `jobs::close`: the closing count, the lines
//!   touched, the task history's solutions, changes, verifications and result, the closing receipt in the folder,
//!   the folder moved to `jobs/done`.
//! - `job brief [ID] [--bundle]`: the kit's brief (its task history read back through `load_task_history`), with
//!   the lines touched and the passenger checkpoints; `--bundle` packs the closed job as `custodian bundle <job>`
//!   does, so the brief and the one file an agent picks the job up from come from one line.
//! - `job import BUNDLE`: custodian_kit's job bundle unbundled into the queue and started, its origin the bundle's
//!   digest. `job list`: every folder by state, with its status from its task record.

use crate::codes::{Code, Result};
use crate::decide;
use crate::facade::{Ctx, Ran};
use crate::layout::{write_whole, Layout};
use crate::runtime::Limit;
use custodian_kit::jobs::JobFile;
use std::path::{Path, PathBuf};

/// The kit's view of the same project.
pub fn kit(ctx: &Ctx) -> custodian_kit::runs::Ctx {
    custodian_kit::runs::Ctx::find(ctx.root(), ctx.exe.clone())
}

fn job_file(dir: &Path) -> JobFile {
    custodian_kit::jobs::job_file(dir)
}

fn write_job_file(dir: &Path, jf: &JobFile) -> Result<()> {
    let text = custodian_kit::oneo::render(custodian_kit::oneo::Doc::Job, jf)?;
    write_whole(&dir.join(custodian_kit::jobs::job_file_name()), text.as_bytes())
}

/// The files custodian keeps in a job's folder besides its specs.
pub const KEPT: &[&str] = &["closing.json", "counts.json", PASSENGER_FILE];

/// A spec file in a job's folder (TOML, JSON or 1o), not its job file and not a file kept there.
pub fn is_spec_file(p: &Path) -> bool {
    p.is_file() && custodian_kit::reading::spec::SpecForm::of(p).is_some() && !custodian_kit::jobs::is_job_file(p) && p.file_name().and_then(|n| n.to_str()).is_none_or(|n| !KEPT.contains(&n))
}

fn folders(layout: &Layout, state: &str) -> Result<Vec<PathBuf>> {
    let dir = layout.jobs_dir(state)?;
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    v.sort();
    Ok(v)
}

/// The folder in `state` a name or id names (the folder's name, or its job file's id or name).
fn find(layout: &Layout, state: &str, which: &str) -> Result<Option<PathBuf>> {
    for p in folders(layout, state)? {
        let jf = job_file(&p);
        if p.file_name().is_some_and(|n| n == which) || jf.id.as_deref() == Some(which) || jf.name.as_deref() == Some(which) {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

/// The workspace a job's readings are answered over in this process (custodian_kit's in-process runner), when no
/// resident answers for the project; with a resident answering, the job's readings run as the kit's declared reading
/// lines (the line runner). Either way each reading keeps its receipt, its `runner` field saying which.
fn runner_workspace(ctx: &mut Ctx) -> Result<Option<&custodian_kit::api::Workspace>> {
    let resident = ctx.layout().ok().and_then(crate::serve::status).is_some() || ctx.in_resident;
    if resident {
        return Ok(None);
    }
    Ok(Some(ctx.workspace(&custodian_kit::api::Scope::default(), custodian_kit::api::Resolving::Job)?))
}

/// The newest folder in `state` a name or id names: a job name run again has a folder per run, and its brief is
/// its latest run's (by when it closed, else by folder name).
fn find_latest(layout: &Layout, state: &str, which: &str) -> Result<Option<PathBuf>> {
    let mut all: Vec<PathBuf> = folders(layout, state)?
        .into_iter()
        .filter(|p| {
            let jf = job_file(p);
            p.file_name().is_some_and(|n| n == which) || jf.id.as_deref() == Some(which) || jf.name.as_deref() == Some(which)
        })
        .collect();
    let closed = |p: &PathBuf| std::fs::metadata(p.join("closing.json")).and_then(|m| m.modified()).ok();
    all.sort_by_key(|p| (closed(p), p.clone()));
    Ok(all.pop())
}

/// The ids of the jobs in `jobs/running`.
pub fn running_ids(layout: &Layout) -> Result<Vec<String>> {
    Ok(folders(layout, "running")?.iter().map(|p| job_file(p).id.unwrap_or_else(|| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())).collect())
}

/// What `job start` was asked.
#[derive(Debug, Clone, Default)]
pub struct StartAsk {
    pub which: Option<String>,
    pub kind: Option<String>,
    pub target: Option<String>,
    pub again: bool,
    pub origin: Option<String>,
    /// The intent record's text, in the asker's words.
    pub intent: Option<String>,
    /// For a test: stop the queue's runner after this many stages.
    pub limit: Limit,
    /// `bounded` or `unbounded` (the workspace's `job_mode` when unset).
    pub mode: Option<String>,
    /// What done looks like, declared at open.
    pub verify: Vec<String>,
    /// Run the tests the target reaches at open, as the job's baseline.
    pub baseline: bool,
    /// Files the job may touch that no operation writes, as globs.
    pub allow_file: Vec<String>,
    /// The job's budget, `hours=<n>,battery=<percent>`.
    pub budget: Option<String>,
}

/// The passenger checkpoints a job took, kept in its folder: `{ "open": id, "close": id }`.
pub const PASSENGER_FILE: &str = "passenger.json";

fn passenger_of(folder: &Path, at: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(folder.join(PASSENGER_FILE)).ok()?).ok()?;
    v.get(at)?.as_str().map(str::to_owned)
}

/// A passenger checkpoint at `at`, kept in the folder's passenger file. A checkpoint code_passenger cannot take (a
/// file it cannot parse) is said, with its receipt, and does not stop the job: the job's own records stand without it.
fn checkpoint(ctx: &mut Ctx, job: &str, folder: &Path, at: &str, ran: &mut Ran) -> Result<()> {
    let id = match crate::maint::passenger_checkpoint(ctx, job, &format!("custodian job {job} {at}"), ran) {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(()),
        Err(e) => {
            ran.say(format!("no passenger checkpoint at {at}: {}", e.message.lines().next().unwrap_or_default()));
            return Ok(());
        }
    };
    let file = folder.join(PASSENGER_FILE);
    let mut v: serde_json::Value = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    v[at] = serde_json::Value::String(id);
    write_whole(&file, (serde_json::to_string_pretty(&v).map_err(|e| Code::Write.refuse(e))? + "\n").as_bytes())
}

/// `custodian job start [NAME|ID]`: the kit's job start, the job left open.
pub fn start(ctx: &mut Ctx, ask: &StartAsk) -> Result<Ran> {
    let line = crate::surface::Line::JobStart(crate::surface::JobStart { name: ask.which.clone().map(Into::into), ..Default::default() });
    crate::runtime::under_job_lock(ctx, &line, |ctx| start_held(ctx, ask))
}

fn start_held(ctx: &mut Ctx, ask: &StartAsk) -> Result<Ran> {
    ctx.mode.need_agent_project()?;
    if let Some(resumed) = crate::runtime::resume(ctx, ask.limit)? {
        return Ok(resumed);
    }
    start_now(ctx, ask)
}

/// [`start`] without looking for a job to resume first (the queue's runner calls it).
pub fn start_now(ctx: &mut Ctx, ask: &StartAsk) -> Result<Ran> {
    let project = ctx.mode.need_agent_project()?.clone();
    let layout = project.layout.clone();
    let d = decide::job_running(&running_ids(&layout)?);
    if !d.go {
        return Err(Code::JobRunning.refuse(d.reason));
    }
    let mut ran = Ran::default();
    ran.say(d.said());
    let folder = match &ask.which {
        Some(w) => find(&layout, "queued", w)?,
        None => folders(&layout, "queued")?.into_iter().next(),
    };
    let folder = match folder {
        Some(f) => f,
        None => {
            // a new job is named by the one who opens it: nothing queued and no name is a question, never a job named by the time
            let name = ask.which.clone().ok_or_else(|| Code::JobNeedsName.refuse("nothing is queued in state/jobs/queued, and a new job is named by the one who opens it: custodian job start NAME"))?;
            let f = layout.job_dir("queued", &crate::readings::slug(&name))?;
            std::fs::create_dir_all(&f).map_err(|e| Code::Write.refuse(format!("{}: {e}", f.display())))?;
            write_job_file(&f, &JobFile { name: Some(name.clone()), kind: Some("specs".into()), ..JobFile::default() })?;
            ran.say(format!("no queued folder named {name}: opened a new job folder for an agentic task"));
            f
        }
    };
    let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // what the line says about the job goes into its job file before the kit reads it
    let mut jf = job_file(&folder);
    if let Some(k) = &ask.kind {
        crate::words::JobKind::of_word(k)?;
        jf.kind = Some(k.clone());
    }
    if ask.target.is_some() {
        jf.target = ask.target.clone();
    }
    if ask.origin.is_some() {
        jf.origin = ask.origin.clone();
        jf.id = None;
    }
    if let Some(m) = &ask.mode {
        jf.mode = Some(m.parse().map_err(|e: String| Code::WordRefused.refuse(e))?);
    }
    for v in &ask.verify {
        if !jf.verify.contains(v) {
            jf.verify.push(v.clone());
        }
    }
    if ask.baseline {
        jf.baseline = Some(true);
    }
    for g in &ask.allow_file {
        if !jf.allow_file.contains(g) {
            jf.allow_file.push(g.clone());
        }
    }
    if let Some(b) = &ask.budget {
        jf.budget = Some(custodian_kit::step_time::Budget::parse(b)?.word());
    }
    write_job_file(&folder, &jf)?;
    let started = {
        let k = kit(ctx);
        match runner_workspace(ctx)? {
            Some(ws) => custodian_kit::jobs::start_with(&k, Some(&folder_name), ask.again, Some(ws))?,
            None => custodian_kit::jobs::start_with(&k, Some(&folder_name), ask.again, None)?,
        }
    };
    ran.said.extend(started.said.iter().cloned());
    // the job's changes as events, for the observer set (watch)
    if let Some(j) = ctx.journal.as_ref() {
        j.event("job", Some(&started.id), &folder_name, "running", &ctx.rel(&started.folder), None)?;
        for r in started.runs.iter().filter_map(|r| r.receipt.as_ref()) {
            j.event("step", Some(&started.id), &r.name, if r.accepted { "done" } else { "failed" }, &r.line, Some(r.seq))?;
        }
    }
    if let Some(intent) = &ask.intent {
        let line = crate::history::state_intent(ctx, &started.folder, intent)?;
        ran.say(line);
    }
    checkpoint(ctx, &started.id, &started.folder, "open", &mut ran)?;
    if started.runs.iter().any(|r| r.receipt.as_ref().is_some_and(|x| !x.accepted)) {
        ran.code = 1;
    }
    // what it will cost, from this workspace's receipts, and the budget it runs under
    if let Some(p) = crate::budget::planned_line(ctx)? {
        ran.say(format!("planned cost: {p}"));
    }
    if let Some(store) = custodian_kit::store::Store::find(ctx.root()) {
        if let Some(b) = custodian_kit::step_time::budget_of(&store, &custodian_kit::jobs::job_file(&started.folder))? {
            ran.say(format!("budget: {} (information: the brief and the closing receipt say how far past its hours the job is; only the battery stops a step)", b.word()));
        }
    }
    ran.say(format!("job {} is open: readings run now are tagged with it; custodian job close {} writes the closing receipt", started.id, started.id));
    ran.name = started.id.clone();
    ran.job = Some(started.id.clone());
    // a file inside the folder, so the kit's move (jobs::move_job) rewrites it with the folder's other paths
    ran.outputs = vec![ctx.rel(&started.folder.join(custodian_kit::jobs::job_file_name()))];
    Ok(ran)
}

/// The running job a line names, or the one running.
fn running(ctx: &Ctx, which: Option<&str>) -> Result<(String, PathBuf)> {
    let layout = ctx.layout()?;
    let folder = match which {
        Some(w) => find(layout, "running", w)?,
        None => folders(layout, "running")?.into_iter().next(),
    };
    let folder = folder.ok_or_else(|| Code::NoSuchJob.refuse(format!("{} in jobs/running", which.unwrap_or("(a running job)"))))?;
    let id = job_file(&folder).id.ok_or_else(|| Code::NoSuchJob.refuse(format!("{} was never started", folder.display())))?;
    Ok((id, folder))
}

/// `custodian job close [ID]`: a passenger checkpoint, then the kit's job close.
pub fn close(ctx: &mut Ctx, which: Option<&str>) -> Result<Ran> {
    let line = crate::surface::Line::JobClose(crate::surface::JobClose { name: which.map(Into::into) });
    crate::runtime::under_job_lock(ctx, &line, |ctx| close_held(ctx, which))
}

fn close_held(ctx: &mut Ctx, which: Option<&str>) -> Result<Ran> {
    let (id, folder) = match running(ctx, which) {
        Ok(r) => r,
        // a done job whose close was refused is closed again over its receipts (the handoff's line for it)
        Err(e) => match which.and_then(|w| kit(ctx).store().ok().and_then(|s| custodian_kit::jobs::reopen_rejected(s, w))) {
            Some(reopened) => {
                reopened?;
                running(ctx, which)?
            }
            None => return Err(e),
        },
    };
    let mut ran = Ran::named(&id);
    ran.job = Some(id.clone());
    checkpoint(ctx, &id, &folder, "close", &mut ran)?;
    // a declared test no receipt satisfies yet: the close runs it before it decides, never `pending` for it
    crate::testing::verify_before_close(ctx, &id, &folder, &mut ran)?;
    let (closing, file, said) = {
        let k = kit(ctx);
        match runner_workspace(ctx)? {
            Some(ws) => custodian_kit::jobs::close_with(&k, &id, Some(ws))?,
            None => custodian_kit::jobs::close_with(&k, &id, None)?,
        }
    };
    ran.said.extend(said);
    if let Some(j) = ctx.journal.as_ref() {
        j.event("job", Some(&id), &closing.name, "done", &ctx.rel(&file), None)?;
    }
    ran.print(custodian_kit::jobs::render_closing(&closing));
    // the kit hands back the closed folder (or its closing file)
    let done = if file.is_dir() { file.clone() } else { file.parent().map(Path::to_path_buf).unwrap_or_default() };
    let file = done.join("closing.md");
    if let (Some(a), Some(b)) = (passenger_of(&done, "open"), passenger_of(&done, "close")) {
        ran.say(format!("the tree in code_passenger's store: {a} at open, {b} at close ({})", ctx.rel(&done.join(PASSENGER_FILE))));
    }
    ran.say(format!("closing receipt: {}", ctx.rel(&file)));
    ran.say(format!("result: {}{}", closing.result, closing.result_reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default()));
    if let Some(o) = &closing.over_budget {
        ran.say(format!("time: {o}"));
    }
    for b in &closing.verified_by {
        ran.say(format!("verified by {b}"));
    }
    ran.outputs = vec![ctx.rel(&file)];
    if let Some(r) = closing.refusal.clone() {
        // a bounded job with lines no transform covers: closed, its record terminal, and the line refused
        ran.code = 2;
        ran.refusal = Some(r);
    } else if closing.result == "failed_test" || (closing.result == "pending" && closing.mode == Some(custodian_kit::jobs::JobMode::Bounded)) {
        // a test that failed is a finding; an unbounded job's pending result is said, not a finding (it is help)
        ran.code = 1;
    }
    Ok(ran)
}

/// `custodian job brief [ID] [--bundle]`: the kit's brief of a closed job, with its lines touched and passenger
/// checkpoints; with `bundle`, the job packed as one bundle file too.
pub fn brief(ctx: &mut Ctx, which: Option<&str>, bundle: bool) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    let folder = match which {
        Some(w) => find_latest(&layout, "done", w)?,
        None => folders(&layout, "done")?.into_iter().last(),
    };
    let folder = folder.ok_or_else(|| Code::NoSuchJob.refuse(format!("{} in jobs/done (a job's brief is written when it closes)", which.unwrap_or("(any)"))))?;
    let jf = job_file(&folder);
    let which = jf.id.clone().unwrap_or_else(|| folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let mut text = String::new();
    let closing: Option<custodian_kit::jobs::Closing> = std::fs::read_to_string(folder.join("closing.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
    if let Some(c) = &closing {
        for t in c.lines_touched.iter().filter(|t| t.label == "job") {
            text.push_str(&format!("lines touched: {} ({})\n", t.line, t.route));
        }
    }
    if let Some(r) = &jf.revision {
        text.push_str(&format!("revision: {} of task {}\n", r, jf.task_id.clone().unwrap_or_default()));
    }
    text.push_str(&custodian_kit::jobs::brief(&kit(ctx), &which)?);
    if let Some(c) = &closing {
        for t in c.lines_touched.iter().filter(|t| t.label != "job") {
            text.push_str(&format!("lines touched at {}: {} ({})\n", t.label, t.line, t.route));
        }
    }
    if let (Some(a), Some(b)) = (passenger_of(&folder, "open"), passenger_of(&folder, "close")) {
        text.push_str(&format!("the tree, as code_passenger keeps it: {a} at open, {b} at close\n"));
    }
    let mut ran = Ran::named(&which);
    ran.job = jf.id.clone();
    ran.print(text);
    ran.say(format!("the facts page of job {which}: {}", ctx.rel(&folder.join("closing.md"))));
    ran.outputs = vec![ctx.rel(&folder.join("closing.md"))];
    if bundle {
        let row = ctx.journal()?.job(&which)?.ok_or_else(|| Code::NoSuchJob.refuse(format!("{which} is not in the journal")))?;
        let b = crate::bundling::bundle_job_by_default(ctx, &row)?;
        ran.said.extend(b.said);
        ran.outputs.extend(b.outputs);
    }
    Ok(ran)
}

/// `custodian job verify LINE [--job ID]`: what done looks like, declared on the running job (custodian_kit's
/// `jobs::declare_verify`, with its `verify` receipt).
pub fn verify(ctx: &mut Ctx, which: Option<&str>, line: &str) -> Result<Ran> {
    let (id, r) = custodian_kit::jobs::declare_verify(&kit(ctx), which, line)?;
    let mut ran = Ran::named(&id);
    ran.job = Some(id.clone());
    ran.say(format!("job {id}: done is {line} (verify receipt #{}); its close is accepted only when its runs satisfy it", r.seq));
    Ok(ran)
}

/// `custodian job allow GLOB [--job ID]`: a file the running job may touch that no operation writes (custodian_kit's
/// `jobs::declare_allow`, with its `allow` receipt).
pub fn allow(ctx: &mut Ctx, which: Option<&str>, glob: &str) -> Result<Ran> {
    let (id, r) = custodian_kit::jobs::declare_allow(&kit(ctx), which, glob)?;
    let mut ran = Ran::named(&id);
    ran.job = Some(id.clone());
    ran.say(format!("job {id}: may touch {glob} by hand (allow receipt #{}); its close names each such file", r.seq));
    Ok(ran)
}

/// `custodian job import BUNDLE`: the bundled job unbundled into the queue, started under a new id with the
/// bundle's digest as its origin.
pub fn import(ctx: &mut Ctx, bundle: &Path, limit: Limit) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    ctx.mode.need_agent_project()?;
    let stem = bundle.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "imported".into());
    let mut name = format!("{}-imported", crate::readings::slug(&stem));
    let mut n = 2;
    while layout.job_dir("queued", &name)?.exists() || layout.job_dir("running", &name)?.exists() || layout.job_dir("done", &name)?.exists() {
        name = format!("{}-imported-{n}", crate::readings::slug(&stem));
        n += 1;
    }
    let queued = layout.job_dir("queued", &name)?;
    if let Some(p) = queued.parent() {
        std::fs::create_dir_all(p).map_err(|e| Code::Write.refuse(format!("{}: {e}", p.display())))?;
    }
    let (_folder, digest) = custodian_kit::job_bundle::unbundle_job(bundle, &queued)?;
    let mut jf = job_file(&queued);
    // an imported job is a task of its own, under its new name
    jf.name = Some(name.clone());
    jf.task_id = None;
    jf.revision = None;
    write_job_file(&queued, &jf)?;
    let mut ran = start(ctx, &StartAsk { which: Some(name.clone()), origin: Some(digest.clone()), limit, ..StartAsk::default() })?;
    ran.said.insert(0, format!("imported {} ({digest}) into {}", bundle.display(), layout.rel(&queued)));
    Ok(ran)
}

/// `custodian job list`: every job folder by state, its status from its task record.
pub fn list(ctx: &mut Ctx) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    let journal = ctx.journal()?;
    let mut s = String::from("# jobs\n");
    for state in ["queued", "running", "done"] {
        let fs = folders(&layout, state)?;
        s.push_str(&format!("\n## {state} ({})\n\n", fs.len()));
        for f in fs {
            let jf = job_file(&f);
            let status = crate::history::status(ctx, &jf).map(|x| format!(" [{x}]")).unwrap_or_default();
            let row = jf.id.as_deref().map(|id| journal.job(id)).transpose()?.flatten();
            s.push_str(&format!(
                "- {} ({}){}{status}: `{}`{}\n",
                jf.name.clone().unwrap_or_else(|| f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                jf.kind.clone().unwrap_or_else(|| "specs".into()),
                jf.id.map(|i| format!(" {i}")).unwrap_or_default(),
                layout.rel(&f),
                row.map(|r| format!(" opened {}{}", r.opened_utc, r.closed_utc.map(|c| format!(", closed {c}")).unwrap_or_default())).unwrap_or_default()
            ));
        }
    }
    let mut ran = Ran::named("jobs");
    ran.print(s);
    ran.say(format!("listed {}", layout.rel(&layout.jobs_dir("x")?.parent().map(Path::to_path_buf).unwrap_or_default())));
    Ok(ran)
}

/// `custodian run` with nothing named, in agent mode: every queued job folder on the queue's runner.
pub fn run_queue(ctx: &mut Ctx, again: bool, ran: &mut Ran) -> Result<()> {
    let line = crate::surface::Line::Run(crate::surface::Run { again, ..Default::default() });
    crate::runtime::under_job_lock(ctx, &line, |ctx| run_queue_held(ctx, again, ran))
}

fn run_queue_held(ctx: &mut Ctx, again: bool, ran: &mut Ran) -> Result<()> {
    let layout = ctx.layout()?.clone();
    if let Some(r) = crate::runtime::resume(ctx, Limit::default())? {
        ran.said.extend(r.said);
    }
    let queued: Vec<String> = folders(&layout, "queued")?.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
    if queued.is_empty() {
        return Ok(());
    }
    let report = crate::runtime::run_folders(ctx, &queued, again, Limit::default())?;
    ran.said.extend(report.said);
    ran.outputs.extend(report.outputs);
    if !report.failed.is_empty() {
        ran.code = 1;
    }
    Ok(())
}
