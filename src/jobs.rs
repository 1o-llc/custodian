//! Jobs as folders (agent mode). A job is a folder in `state/jobs/queued/`: spec files run in name order, or a
//! job file (`v1.job.1o.job`, or an older `job.toml`) naming a job kind whose template lists the steps. `job start` moves the folder to `jobs/running/`,
//! runs its steps on the runner under the job's id (each a declared line with its receipt, each reading kept in
//! the folder too) and leaves the job open: readings run while it is open are tagged with it, in order. `job close`
//! writes the closing receipt into the folder (`closing.md`, the facts page, and `closing.json`: the specs read,
//! the readings reused, what changed since each spec was last read, the tests run) and moves the folder to
//! `jobs/done/`. `job brief` prints the facts page. `job import` unbundles a bundled job into the queue and starts
//! it under a new id, recording the bundle's digest as its origin.

use crate::codes::{Code, Result};
use crate::decide;
use crate::digest;
use crate::facade::{Ctx, Ran};
use crate::journal::{now_utc, stamp, JobRow};
use crate::layout::{self, write_whole, Layout};
use crate::runtime::{self, JobState, Limit, StepDone, StepPlan};
use crate::words::JobKind;
use custodian_kit::jobs::{JobFile, Template};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A test job: the tests a change reaches, found and run.
pub const TEST: &str = r#"# test: the tests the change since a git ref reaches (the job's target, HEAD by default), found and run,
# each cargo line with its receipt and its times in the journal.
kind = "test"
summary = "the affected tests, run"

[[step]]
name = "affected"
kind = "test"
target = "affected"
"#;

/// A check job: every maintenance check over the workspace.
pub const CHECK: &str = r#"# check: every maintenance check (sensitivity, mechanism, boundary, supersedes, reads_env, golden) over the workspace.
kind = "check"
summary = "every maintenance check, as maintenance's own lines"

[[step]]
name = "all"
kind = "check"
target = "all"
"#;

/// The job templates custodian ships; `agent init` writes each to `config/jobs/<kind>.toml`, where they are the
/// user's to edit. triage's is custodian_kit's.
pub const SHIPPED: &[(&str, &str)] = &[("triage", custodian_kit::jobs::TRIAGE), ("test", TEST), ("check", CHECK)];

/// The template of a kind: the project's file, else the one custodian ships.
pub fn template(layout: &Layout, kind: &str) -> Result<Template> {
    let file = layout.path::<layout::JobTemplate>(&[("kind", kind)])?;
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(_) => SHIPPED.iter().find(|(k, _)| *k == kind).map(|(_, t)| t.to_string()).ok_or_else(|| Code::UnknownKind.refuse(kind))?,
    };
    toml::from_str(&text).map_err(|e| Code::JobFile.refuse(format!("{}: {e}", file.display())))
}

/// A folder's job file, as custodian_kit reads it (`v1.job.1o.job`, else an older `job.toml`, else empty).
fn job_file(dir: &Path) -> Result<JobFile> {
    Ok(custodian_kit::jobs::job_file(dir))
}

/// Writes a folder's job file in 1o, as custodian_kit writes it.
fn write_job_file(dir: &Path, jf: &JobFile) -> Result<()> {
    let text = custodian_kit::oneo::render(custodian_kit::oneo::Doc::Job, jf)?;
    write_whole(&dir.join(custodian_kit::jobs::job_file_name()), text.as_bytes())
}

/// The files custodian keeps in a job's folder besides its specs.
pub const KEPT: &[&str] = &["closing.json", COUNTS_FILE, PASSENGER_FILE];

/// A spec file in a job's folder (TOML, JSON or 1o), not its job file and not a file custodian keeps there.
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
        let jf = job_file(&p).unwrap_or_default();
        if p.file_name().is_some_and(|n| n == which) || jf.id.as_deref() == Some(which) || jf.name.as_deref() == Some(which) {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

fn mv(from: &Path, to: &Path) -> Result<()> {
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p).map_err(|e| Code::Write.refuse(format!("{}: {e}", p.display())))?;
    }
    if to.exists() {
        return Err(Code::Write.refuse(format!("{} is already there", to.display())));
    }
    std::fs::rename(from, to).map_err(|e| Code::Write.refuse(format!("{} → {}: {e}", from.display(), to.display())))
}

/// A new job id: `j-<UTC stamp>-<6 hex of the digest of the name and this moment>`.
pub fn new_id(name: &str) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    format!("j-{}-{}", stamp(), digest::short(&digest::of("job", &(name, nanos, std::process::id())), 6))
}

/// The steps a job runs: its template's, or its folder's spec files in name order.
pub fn plan(layout: &Layout, folder: &Path, kind: &str) -> Result<(Vec<Vec<StepPlan>>, String)> {
    if kind == "specs" {
        let mut specs: Vec<PathBuf> = std::fs::read_dir(folder)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| is_spec_file(p)).collect())
            .unwrap_or_default();
        specs.sort();
        let steps = specs
            .iter()
            .map(|p| StepPlan {
                name: custodian_kit::api::ReadingSpec::from_path(p).map(|s| s.name).unwrap_or_else(|_| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
                kind: "spec".into(),
                target: None,
                for_each: None,
                change_items: None,
                with: None,
                spec: Some(layout.rel(p)),
            })
            .collect();
        return Ok((runtime::stages_of(steps, true), "the folder's specs, in name order".into()));
    }
    let t = template(layout, kind)?;
    let steps = t
        .step
        .into_iter()
        .map(|s| StepPlan { name: s.name, kind: s.kind, target: s.target, for_each: s.for_each, change_items: s.change_items, with: s.with, spec: None })
        .collect();
    Ok((runtime::stages_of(steps, false), t.summary))
}

/// The counts file in a job's folder: every count the job took, in order (custodian_kit's `Taken` records).
pub const COUNTS_FILE: &str = "counts.json";

/// The routes a job counts, when it counts: the job file's `routes`, else the template's `count_route` (its
/// `{target}` filled), else the workspace; none when neither the job file nor the template says `count = true`.
pub fn count_routes(layout: &Layout, jf: &JobFile, kind: &str) -> Vec<String> {
    let t = if kind == "specs" { None } else { template(layout, kind).ok() };
    if !jf.count.unwrap_or_else(|| t.as_ref().is_some_and(|t| t.count)) {
        return Vec::new();
    }
    if !jf.routes.is_empty() {
        return jf.routes.clone();
    }
    let target = jf.target.clone().unwrap_or_default();
    vec![t.and_then(|t| t.count_route).map(|r| r.replace("{target}", &target)).unwrap_or_else(|| "*".to_string())]
}

fn taken(folder: &Path) -> Vec<custodian_kit::jobs::Taken> {
    std::fs::read_to_string(folder.join(COUNTS_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// A count of each route at `label` (`open`, `close`), as readings of the job, recorded in the folder's counts file.
pub fn take_counts(ctx: &mut Ctx, job: &str, folder: &Path, label: &str, routes: &[String], ran: &mut Ran) -> Result<()> {
    if routes.is_empty() {
        return Ok(());
    }
    let mut all = taken(folder);
    for route in routes {
        // each count its own declared line, with its own receipt, tagged with the job
        let line = crate::surface::Line::Count(crate::surface::Count { args: vec![route.into()], tests: true, again: true, job: Some(job.into()), by: None });
        let c = crate::facade::execute(ctx, &line, crate::cli::handle);
        if c.code == 2 {
            return Err(Code::StepFailed.refuse(format!("the {label} count of {route}: {}", c.said.last().cloned().unwrap_or_default())));
        }
        let name = c.receipt.name.clone();
        let stem = c.receipt.outputs.first().and_then(|p| Path::new(p).file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let reading = format!("readings/{name}/{stem}.json");
        if !folder.join(&reading).is_file() {
            return Err(Code::ReadingUnreadable.refuse(format!("{}: the {label} count wrote no reading into the job", folder.join(&reading).display())));
        }
        ran.say(format!("{label} count #{}: {}", c.receipt.seq, c.said.iter().find(|l| l.starts_with("counted ")).cloned().unwrap_or_default()));
        all.push(custodian_kit::jobs::Taken { label: label.to_string(), route: route.clone(), reading });
    }
    write_whole(&folder.join(COUNTS_FILE), (serde_json::to_string_pretty(&all).map_err(|e| Code::Write.refuse(e))? + "\n").as_bytes())
}

/// The passenger checkpoints a job took, kept in its folder: `{ "open": id, "close": id }`.
const PASSENGER_FILE: &str = "passenger.json";

fn passenger_of(folder: &Path, at: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(folder.join(PASSENGER_FILE)).ok()?).ok()?;
    v.get(at)?.as_str().map(str::to_owned)
}

/// A passenger checkpoint at `at`, kept in the folder's passenger file.
fn checkpoint(ctx: &mut Ctx, job: &str, folder: &Path, at: &str, ran: &mut Ran) -> Result<()> {
    let Some(id) = crate::maint::passenger_checkpoint(ctx, job, &format!("custodian job {job} {at}"), ran)? else { return Ok(()) };
    let file = folder.join(PASSENGER_FILE);
    let mut v: serde_json::Value = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    v[at] = serde_json::Value::String(id);
    write_whole(&file, (serde_json::to_string_pretty(&v).map_err(|e| Code::Write.refuse(e))? + "\n").as_bytes())
}

/// The ids of the jobs in `jobs/running`.
fn running_ids(layout: &Layout) -> Result<Vec<String>> {
    Ok(folders(layout, "running")?.iter().map(|p| job_file(p).ok().and_then(|j| j.id).unwrap_or_else(|| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())).collect())
}

/// What `job start` was asked.
#[derive(Debug, Clone, Default)]
pub struct StartAsk {
    pub which: Option<String>,
    pub kind: Option<String>,
    pub target: Option<String>,
    pub again: bool,
    pub origin: Option<String>,
    /// For a test: stop the runner after this many stages.
    pub limit: Limit,
}

/// `custodian job start [NAME|ID]`: a queued folder (or a new one, for an agentic task), its steps run, the job
/// left open. A running job with a checkpoint is resumed instead.
pub fn start(ctx: &mut Ctx, ask: &StartAsk) -> Result<Ran> {
    let project = ctx.mode.need_project()?.clone();
    project.need_agent()?;
    let layout = project.layout.clone();
    if let Some(resumed) = resume(ctx, ask.limit)? {
        return Ok(resumed);
    }
    let running = running_ids(&layout)?;
    let d = decide::job_running(&running);
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
            let name = ask.which.clone().unwrap_or_else(|| new_id("job"));
            let f = layout.job_dir("queued", &crate::readings::slug(&name))?;
            std::fs::create_dir_all(&f).map_err(|e| Code::Write.refuse(format!("{}: {e}", f.display())))?;
            ran.say(format!("no queued folder named {name}: opened a new job folder for an agentic task"));
            f
        }
    };
    let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut jf = job_file(&folder)?;
    let id = jf.id.clone().filter(|_| ask.origin.is_none()).unwrap_or_else(|| new_id(&folder_name));
    let kind = ask.kind.clone().or(jf.kind.clone()).unwrap_or_else(|| "specs".into());
    JobKind::of_word(&kind)?;
    jf.id = Some(id.clone());
    jf.name = jf.name.or(Some(folder_name.clone()));
    jf.kind = Some(kind.clone());
    jf.target = ask.target.clone().or(jf.target);
    if ask.origin.is_some() {
        jf.origin = ask.origin.clone();
    }
    write_job_file(&folder, &jf)?;
    let to = layout.job_dir("running", &folder_name)?;
    mv(&folder, &to)?;
    let journal = ctx.journal()?;
    let row = JobRow {
        id: id.clone(),
        name: jf.name.clone().unwrap_or_default(),
        kind: kind.clone(),
        state: "running".into(),
        folder: layout.rel(&to),
        origin: jf.origin.clone(),
        opened_seq: None,
        closed_seq: None,
        opened_utc: now_utc(),
        closed_utc: None,
    };
    journal.put_job(&row)?;
    journal.event("job", Some(&id), &row.name, "running", &format!("{} → {}", layout.rel(&folder), layout.rel(&to)), None)?;
    ran.say(format!("started job {id} ({kind}): {} → {}", layout.rel(&folder), layout.rel(&to)));
    let (stages, summary) = plan(&layout, &to, &kind)?;
    let routes = count_routes(&layout, &jf, &kind);
    take_counts(ctx, &id, &to, "open", &routes, &mut ran)?;
    checkpoint(ctx, &id, &to, "open", &mut ran)?;
    ran.say(format!("steps: {} in {} stage(s): {summary}", stages.iter().map(|s| s.len()).sum::<usize>(), stages.len()));
    let st = JobState { job: id.clone(), kind, target: jf.target, root: layout.root.clone(), exe: ctx.exe.clone(), folder: to.clone(), stages, next: 0, done: Vec::new(), again: ask.again };
    run_and_say(ctx, vec![st], ask.limit, &mut ran)?;
    ran.say(format!("job {id} is open: readings run now are tagged with it; custodian job close {id} writes the closing receipt"));
    ran.name = id.clone();
    ran.job = Some(id);
    ran.outputs = vec![layout.rel(&to)];
    Ok(ran)
}

/// Runs jobs on the runner and says what each step did.
fn run_and_say(ctx: &mut Ctx, jobs: Vec<JobState>, limit: Limit, ran: &mut Ran) -> Result<runtime::RunnerReport> {
    let journal_path = ctx.journal()?.path.clone();
    let ids: Vec<String> = jobs.iter().map(|j| j.job.clone()).collect();
    let report = runtime::run_jobs(&journal_path, jobs, limit)?;
    let journal = ctx.journal()?;
    for id in &ids {
        for e in journal.events_since(0)?.into_iter().filter(|e| e.job.as_deref() == Some(id) && e.kind == "step" && e.state != "running") {
            ran.say(format!("  step {} {}: {}{}", e.subject, e.state, e.detail, e.seq.map(|s| format!(" (#{s})")).unwrap_or_default()));
        }
    }
    for (job, msg) in &report.failed {
        ran.say(format!("job {job} failed on the runner: {msg}"));
        ran.code = 1;
    }
    for job in &report.interrupted {
        ran.say(format!("job {job} stopped between stages; its checkpoint is in the journal (custodian job start resumes it)"));
    }
    if journal.events_since(0)?.iter().any(|e| ids.contains(e.job.as_ref().unwrap_or(&String::new())) && e.kind == "step" && e.state == "failed") {
        ran.code = 1;
    }
    Ok(report)
}

/// A running job whose checkpoint is in the journal: restored and run on from the stage after its last.
pub fn resume(ctx: &mut Ctx, limit: Limit) -> Result<Option<Ran>> {
    let mut states = Vec::new();
    let journal = ctx.journal()?;
    for row in journal.jobs_in("running")? {
        if let Some((_, step, bytes)) = journal.latest_checkpoint(&row.id)? {
            if let Ok(mut st) = serde_json::from_slice::<JobState>(&bytes) {
                st.exe = ctx.exe.clone();
                states.push((row.id.clone(), step, st));
            }
        }
    }
    if states.is_empty() {
        return Ok(None);
    }
    let mut ran = Ran::default();
    for (id, step, st) in &states {
        ran.say(format!("resumed job {id} at stage {} of {} (its checkpoint after stage {step})", st.next + 1, st.stages.len()));
        ctx.journal()?.event("job", Some(id), id, "resumed", &format!("from stage {}", st.next + 1), None)?;
    }
    let ids: Vec<String> = states.iter().map(|(id, _, _)| id.clone()).collect();
    run_and_say(ctx, states.into_iter().map(|(_, _, s)| s).collect(), limit, &mut ran)?;
    ran.name = ids.join(", ");
    ran.job = ids.first().cloned();
    Ok(Some(ran))
}

/// The running job a line names, or the one running.
fn running(ctx: &Ctx, which: Option<&str>) -> Result<(JobRow, PathBuf)> {
    let journal = ctx.journal()?;
    let row = match which {
        Some(w) => journal.job(w)?.filter(|r| r.state == "running"),
        None => journal.jobs_in("running")?.into_iter().next(),
    };
    let row = row.ok_or_else(|| Code::NoSuchJob.refuse(format!("{} in jobs/running", which.unwrap_or("(a running job)"))))?;
    let folder = ctx.root().join(&row.folder);
    Ok((row, folder))
}

/// One spec read in a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Read {
    pub name: String,
    pub kind: String,
    pub target: String,
    pub seq: i64,
    pub page: String,
}

/// One reading a job reused from the cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reused {
    pub name: String,
    pub seq: i64,
    pub reused_seq: i64,
}

/// What moved for a spec since it was last read before the job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changed {
    pub name: String,
    pub before: i64,
    pub after: i64,
    pub moved: bool,
    pub diff: String,
}

/// One test line a job ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tested {
    pub package: String,
    pub target: String,
    pub seq: i64,
    pub passed: i64,
    pub failed: i64,
    pub ignored: i64,
    pub millis: i64,
}

/// The closing receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Closing {
    pub schema: String,
    pub job: String,
    pub name: String,
    pub kind: String,
    pub origin: Option<String>,
    pub opened_utc: String,
    pub closed_utc: Option<String>,
    pub steps: Vec<StepDone>,
    pub specs_read: Vec<Read>,
    pub reused: Vec<Reused>,
    pub changed: Vec<Changed>,
    pub tests: Vec<Tested>,
    /// From the job's counts (when it counted): each route from open to close, as custodian_kit reckons it.
    #[serde(default)]
    pub lines_touched: Vec<custodian_kit::jobs::Touched>,
    /// code_passenger's checkpoints of the tree at open and at close, when it is installed: a job's before and after
    /// as two versions in the passenger store (`code-passenger passenger checkout <id>` restores either).
    #[serde(default)]
    pub passenger_open: Option<String>,
    #[serde(default)]
    pub passenger_close: Option<String>,
    pub receipts: Vec<i64>,
}

pub const CLOSING_SCHEMA: &str = "custodian.closing.v1";

/// The closing receipt of a job, from the journal.
pub fn closing(ctx: &Ctx, row: &JobRow) -> Result<Closing> {
    let journal = ctx.journal()?;
    let receipts = journal.receipts_of_job(&row.id)?;
    let readings: Vec<_> = journal.readings()?.into_iter().filter(|r| r.job.as_deref() == Some(&row.id)).collect();
    let specs_read: Vec<Read> = readings.iter().map(|r| Read { name: r.name.clone(), kind: r.kind.clone(), target: r.target.clone(), seq: r.seq, page: r.page.clone() }).collect();
    let reused: Vec<Reused> = receipts.iter().filter_map(|r| r.reused_seq.map(|n| Reused { name: r.name.clone(), seq: r.seq, reused_seq: n })).collect();
    let mut changed = Vec::new();
    let first = receipts.first().map(|r| r.seq).unwrap_or(i64::MAX);
    for r in &readings {
        let before = journal.readings_of(&r.name)?.into_iter().rev().find(|b| b.seq < first && b.seq < r.seq);
        if let Some(b) = before {
            let (Ok(a), Ok(c)) = (custodian_kit::reading::load(&ctx.root().join(&b.record)), custodian_kit::reading::load(&ctx.root().join(&r.record))) else { continue };
            let d = custodian_kit::diff::compare(&a, &c);
            changed.push(Changed { name: r.name.clone(), before: b.seq, after: r.seq, moved: !d.is_empty(), diff: custodian_kit::diff::render(&d) });
        }
    }
    let tests = journal
        .test_runs_of_job(&row.id)?
        .into_iter()
        .map(|t| Tested { package: t.package, target: t.target, seq: t.seq, passed: t.passed, failed: t.failed, ignored: t.ignored, millis: t.millis })
        .collect();
    let mut steps: Vec<StepDone> = Vec::new();
    for e in journal.events_since(0)?.into_iter().filter(|e| e.job.as_deref() == Some(&row.id) && e.kind == "step" && e.state != "running") {
        match steps.iter_mut().find(|s| s.step == e.subject) {
            Some(s) => {
                s.lines.push(e.detail.clone());
                s.seqs.extend(e.seq);
                s.accepted &= e.state != "failed";
            }
            None => steps.push(StepDone {
                step: e.subject.clone(),
                lines: if e.state == "skipped" { Vec::new() } else { vec![e.detail.clone()] },
                seqs: e.seq.into_iter().collect(),
                accepted: e.state != "failed",
                skipped: (e.state == "skipped").then(|| e.detail.clone()),
                readings: Vec::new(),
            }),
        }
    }
    Ok(Closing {
        schema: CLOSING_SCHEMA.into(),
        job: row.id.clone(),
        name: row.name.clone(),
        kind: row.kind.clone(),
        origin: row.origin.clone(),
        opened_utc: row.opened_utc.clone(),
        closed_utc: row.closed_utc.clone(),
        steps,
        specs_read,
        reused,
        changed,
        tests,
        lines_touched: {
            let folder = ctx.root().join(&row.folder);
            let t = taken(&folder);
            if t.len() >= 2 {
                custodian_kit::jobs::lines_touched_of(&folder, &t)?
            } else {
                Vec::new()
            }
        },
        passenger_open: passenger_of(&ctx.root().join(&row.folder), "open"),
        passenger_close: passenger_of(&ctx.root().join(&row.folder), "close"),
        receipts: receipts.iter().map(|r| r.seq).collect(),
    })
}

/// The closing receipt as the facts page.
pub fn page(c: &Closing) -> String {
    let mut s = format!("# job {} ({})\n\n- kind: {}\n- opened: {}\n- closed: {}\n", c.name, c.job, c.kind, c.opened_utc, c.closed_utc.clone().unwrap_or_else(|| "open".into()));
    if let Some(o) = &c.origin {
        s.push_str(&format!("- imported from a bundle: {o}\n"));
    }
    s.push_str(&format!("\n## steps ({})\n\n", c.steps.len()));
    for st in &c.steps {
        match &st.skipped {
            Some(why) => s.push_str(&format!("- {}: skipped, {why}\n", st.step)),
            None => s.push_str(&format!("- {}: {} ({})\n", st.step, if st.accepted { "done" } else { "failed" }, st.seqs.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", "))),
        }
    }
    s.push_str(&format!("\n## specs read ({})\n\n", c.specs_read.len()));
    for r in &c.specs_read {
        s.push_str(&format!("- {} ({} `{}`): #{} `{}`\n", r.name, r.kind, r.target.lines().next().unwrap_or(""), r.seq, r.page));
    }
    s.push_str(&format!("\n## readings reused from the cache ({})\n\n", c.reused.len()));
    for r in &c.reused {
        s.push_str(&format!("- {}: #{} reused search #{}\n", r.name, r.seq, r.reused_seq));
    }
    s.push_str(&format!("\n## what changed since each spec was last read ({})\n\n", c.changed.len()));
    for ch in &c.changed {
        s.push_str(&format!("- {}: #{} → #{}: {}\n", ch.name, ch.before, ch.after, if ch.moved { "moved" } else { "nothing moved" }));
        if ch.moved {
            for l in ch.diff.lines() {
                s.push_str(&format!("  {l}\n"));
            }
        }
    }
    s.push_str(&format!("\n## tests run ({})\n\n", c.tests.len()));
    for t in &c.tests {
        s.push_str(&format!("- {} {}: {} passed, {} failed, {} ignored, {} ms (#{})\n", t.package, t.target, t.passed, t.failed, t.ignored, t.millis, t.seq));
    }
    if !c.lines_touched.is_empty() {
        s.push_str(&format!("\n## lines touched ({})\n\n", c.lines_touched.len()));
        for t in &c.lines_touched {
            s.push_str(&format!("- {} ({}): before {} → after {} (`{}`)\n", t.line, t.route, t.before_digest, t.after_digest, t.diff));
        }
    }
    if c.passenger_open.is_some() || c.passenger_close.is_some() {
        s.push_str(&format!("\n## the tree, as code_passenger keeps it\n\n- at open: {}\n- at close: {}\n", c.passenger_open.as_deref().unwrap_or("-"), c.passenger_close.as_deref().unwrap_or("-")));
    }
    s.push_str(&format!("\n## receipts\n\n{}\n", c.receipts.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ")));
    s
}

/// `custodian job close [ID]`: the closing receipt written into the folder, the folder moved to `jobs/done`.
pub fn close(ctx: &mut Ctx, which: Option<&str>) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    let (mut row, folder) = running(ctx, which)?;
    let mut counted = Ran::default();
    let jf = job_file(&folder)?;
    let routes = count_routes(&layout, &jf, &row.kind);
    take_counts(ctx, &row.id, &folder, "close", &routes, &mut counted)?;
    checkpoint(ctx, &row.id, &folder, "close", &mut counted)?;
    row.closed_utc = Some(now_utc());
    let c = closing(ctx, &row)?;
    let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let to = layout.job_dir("done", &folder_name)?;
    let md = page(&c);
    write_whole(&folder.join("closing.md"), md.as_bytes())?;
    write_whole(&folder.join("closing.json"), (serde_json::to_string_pretty(&c).map_err(|e| Code::Write.refuse(e))? + "\n").as_bytes())?;
    mv(&folder, &to)?;
    row.state = "done".into();
    row.folder = layout.rel(&to);
    row.closed_seq = c.receipts.last().copied();
    let journal = ctx.journal()?;
    journal.put_job(&row)?;
    journal.event("job", Some(&row.id), &row.name, "done", &format!("{} → {}", layout.rel(&folder), layout.rel(&to)), None)?;
    let mut ran = Ran::named(&row.id);
    ran.said.extend(counted.said);
    ran.job = Some(row.id.clone());
    ran.print(md);
    let closing_md = layout.path::<layout::Closing>(&[("state", "done"), ("job", &folder_name), ("ext", "md")])?;
    let closing_json = layout.path::<layout::Closing>(&[("state", "done"), ("job", &folder_name), ("ext", "json")])?;
    ran.say(format!("closed job {}: {} specs read, {} reused, {} changed, {} test lines", row.id, c.specs_read.len(), c.reused.len(), c.changed.iter().filter(|x| x.moved).count(), c.tests.len()));
    for t in &c.lines_touched {
        ran.say(format!("lines touched: {} ({})", t.line, t.route));
    }
    ran.say(format!("closing receipt: {} (+ {})", layout.rel(&closing_md), layout.rel(&closing_json)));
    ran.say(format!("moved: {} → {}", layout.rel(&folder), layout.rel(&to)));
    ran.outputs = vec![layout.rel(&closing_md), layout.rel(&closing_json)];
    Ok(ran)
}

/// `custodian job brief [ID]`: the facts page of a closed job, or of an open one so far.
pub fn brief(ctx: &mut Ctx, which: Option<&str>) -> Result<Ran> {
    let journal = ctx.journal()?;
    let row = match which {
        Some(w) => journal.job(w)?,
        None => journal.jobs()?.into_iter().last(),
    };
    let row = row.ok_or_else(|| Code::NoSuchJob.refuse(which.unwrap_or("(any)")))?;
    let folder = ctx.root().join(&row.folder);
    let mut ran = Ran::named(&row.id);
    ran.job = Some(row.id.clone());
    match std::fs::read_to_string(folder.join("closing.md")) {
        Ok(text) => {
            ran.print(text);
            ran.say(format!("the facts page of job {}: {}", row.id, ctx.rel(&folder.join("closing.md"))));
            ran.outputs = vec![ctx.rel(&folder.join("closing.md"))];
        }
        Err(_) => {
            let c = closing(ctx, &row)?;
            ran.print(page(&c));
            ran.say(format!("job {} is {}: the facts so far, from the journal (nothing written)", row.id, row.state));
        }
    }
    Ok(ran)
}

/// `custodian job import BUNDLE`: the bundled job unbundled into the queue, started under a new id with the
/// bundle's digest as its origin.
pub fn import(ctx: &mut Ctx, bundle: &Path, limit: Limit) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    ctx.mode.need_project()?.need_agent()?;
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
    let (folder, digest) = custodian_kit::job_bundle::unbundle_job(bundle, &queued)?;
    let mut jf = job_file(&folder)?;
    jf.id = None;
    jf.origin = Some(digest.clone());
    write_job_file(&folder, &jf)?;
    let mut ran = start(ctx, &StartAsk { which: Some(name.clone()), origin: Some(digest.clone()), limit, ..StartAsk::default() })?;
    ran.said.insert(0, format!("imported {} ({digest}) into {}", bundle.display(), layout.rel(&queued)));
    Ok(ran)
}

/// `custodian job list`: every job folder by state, with what the journal knows of it.
pub fn list(ctx: &mut Ctx) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    let journal = ctx.journal()?;
    let mut s = String::from("# jobs\n");
    for state in ["queued", "running", "done"] {
        let fs = folders(&layout, state)?;
        s.push_str(&format!("\n## {state} ({})\n\n", fs.len()));
        for f in fs {
            let jf = job_file(&f).unwrap_or_default();
            let row = jf.id.as_deref().map(|id| journal.job(id)).transpose()?.flatten();
            s.push_str(&format!(
                "- {} ({}){}: `{}`{}\n",
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

/// `custodian run` with nothing named, in agent mode: every queued job folder, scheduled together, each closed when
/// its steps are done.
pub fn run_queue(ctx: &mut Ctx, again: bool, ran: &mut Ran) -> Result<()> {
    let layout = ctx.layout()?.clone();
    if let Some(r) = resume(ctx, Limit::default())? {
        ran.said.extend(r.said);
    }
    let queued = folders(&layout, "queued")?;
    if queued.is_empty() {
        return Ok(());
    }
    let mut states = Vec::new();
    for folder in queued {
        let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut jf = job_file(&folder)?;
        let id = jf.id.clone().unwrap_or_else(|| new_id(&folder_name));
        let kind = jf.kind.clone().unwrap_or_else(|| "specs".into());
        JobKind::of_word(&kind)?;
        jf.id = Some(id.clone());
        jf.name = jf.name.or(Some(folder_name.clone()));
        jf.kind = Some(kind.clone());
        write_job_file(&folder, &jf)?;
        let to = layout.job_dir("running", &folder_name)?;
        mv(&folder, &to)?;
        let row = JobRow { id: id.clone(), name: jf.name.clone().unwrap_or_default(), kind: kind.clone(), state: "running".into(), folder: layout.rel(&to), origin: jf.origin.clone(), opened_seq: None, closed_seq: None, opened_utc: now_utc(), closed_utc: None };
        ctx.journal()?.put_job(&row)?;
        ctx.journal()?.event("job", Some(&id), &row.name, "running", &layout.rel(&to), None)?;
        let (stages, _) = plan(&layout, &to, &kind)?;
        let routes = count_routes(&layout, &jf, &kind);
        take_counts(ctx, &id, &to, "open", &routes, ran)?;
        ran.say(format!("queued job {id} ({kind}, {}): {} stage(s)", crate::words::JobKind::of_word(&kind).map(|k| format!("{:?}", k.qos())).unwrap_or_default(), stages.len()));
        states.push(JobState { job: id, kind, target: jf.target, root: layout.root.clone(), exe: ctx.exe.clone(), folder: to, stages, next: 0, done: Vec::new(), again });
    }
    let ids: Vec<String> = states.iter().map(|s| s.job.clone()).collect();
    let mut inner = Ran::default();
    run_and_say(ctx, states, Limit::default(), &mut inner)?;
    ran.said.extend(inner.said);
    ran.code = ran.code.max(inner.code);
    for id in ids {
        let closed = close(ctx, Some(&id))?;
        ran.said.extend(closed.said);
        ran.outputs.extend(closed.outputs);
    }
    Ok(())
}
