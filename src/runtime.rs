//! The runner: a job's steps as structured tasks, the queue of jobs scheduled with a quality of service per job
//! kind, and a checkpoint after every completed stage so an interrupted job resumes where it stopped.
//!
//! - A job is a [`JobTask`] on waker_protocol's scheduler, its class the job kind's (`JobKind::qos`: triage is
//!   interactive, a folder of specs standard, tests and checks batch). Each poll runs one stage and asks to be
//!   checkpointed (`TaskPoll::Checkpoint`); the scheduler saves the task's bytes through [`JournalCheckpoints`]
//!   into the journal's `checkpoints` table, the task goes dormant, the runner wakes it, the scheduler restores it
//!   from those bytes and polls the next stage. A process that stops between stages leaves the job in
//!   `jobs/running` with its checkpoint; the next `custodian job start` or `custodian run` restores it from the
//!   journal and goes on from the stage after the last completed one.
//! - A stage is the steps that do not read each other's receipts. They run as scope tasks on one thread: each step
//!   is a declared custodian line in a child process (`custodian run <spec> --job <id>`, `custodian test`,
//!   `custodian check`), polled until it exits, so the stage's steps run at the same time and are recorded in step
//!   order, each with a receipt of its own.
//! - The command line runs this same runner in its own process; `custodian serve` runs it for a request.

#![allow(unsafe_code)]

use crate::codes::{Code, Result};
use crate::decide;
use crate::facade::{self, Ctx, External, ExternalNames};
use crate::journal::Journal;
use crate::surface::{Check, Run, Test};
use custodian_kit::jobs::From as StepFrom;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Mutex;
use std::time::Instant;
use waker_protocol::{derive_protocol_task_vtable, CheckpointReason, CheckpointStore, ProtocolTask, RetentionPolicy, TaskId, TaskPoll, TaskPolicy, TaskState, WakeReason, WakerProtocol};
use waker_tables::VTableKey;

/// One step of a job, as planned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepPlan {
    pub name: String,
    /// A reading kind (`diagnostics`, `reach`, ...), `test` or `check`.
    pub kind: String,
    pub target: Option<String>,
    /// One reading per value an earlier step's reading gave.
    pub for_each: Option<StepFrom>,
    /// The change an `affected_tests` step reads: the values an earlier step's reading gave.
    pub change_items: Option<StepFrom>,
    /// Extra spec lines (TOML).
    pub with: Option<String>,
    /// A spec file in the job's folder (a folder of specs), relative to the root.
    pub spec: Option<String>,
}

impl StepPlan {
    /// The step it reads, if any.
    pub fn reads(&self) -> Option<&str> {
        self.for_each.as_ref().or(self.change_items.as_ref()).map(|f| f.step.as_str())
    }
}

/// One step, done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepDone {
    pub step: String,
    /// The lines it ran, as declared.
    pub lines: Vec<String>,
    /// The receipts of those lines (this runner's, one per line).
    pub seqs: Vec<i64>,
    pub accepted: bool,
    /// Why it did not run (the step-input gate's reason).
    pub skipped: Option<String>,
    /// The spec names its readings went under.
    pub readings: Vec<String>,
}

/// A job as the runner holds it, and as its checkpoint is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JobState {
    pub job: String,
    pub kind: String,
    pub target: Option<String>,
    pub root: PathBuf,
    pub exe: PathBuf,
    /// The job's folder in `jobs/running`.
    pub folder: PathBuf,
    pub stages: Vec<Vec<StepPlan>>,
    /// The next stage to run.
    pub next: usize,
    pub done: Vec<StepDone>,
    /// Read again rather than reuse (the job was started with `--again`).
    pub again: bool,
}

/// Stages from steps: a step runs in the stage after the step it reads; a folder of specs runs one spec a stage.
pub fn stages_of(steps: Vec<StepPlan>, in_order: bool) -> Vec<Vec<StepPlan>> {
    if in_order {
        return steps.into_iter().map(|s| vec![s]).collect();
    }
    let mut level: Vec<(usize, StepPlan)> = Vec::new();
    for s in steps {
        let l = s.reads().and_then(|dep| level.iter().find(|(_, p)| p.name == dep).map(|(l, _)| l + 1)).unwrap_or(0);
        level.push((l, s));
    }
    let max = level.iter().map(|(l, _)| *l).max().map_or(0, |m| m + 1);
    let mut out: Vec<Vec<StepPlan>> = vec![Vec::new(); max];
    for (l, s) in level {
        out[l].push(s);
    }
    out
}

/// The job queue's checkpoint store: the journal's `checkpoints` table.
pub struct JournalCheckpoints {
    journal: Mutex<Journal>,
}

impl JournalCheckpoints {
    pub fn open(path: &Path) -> Result<JournalCheckpoints> {
        Ok(JournalCheckpoints { journal: Mutex::new(Journal::open(path)?) })
    }
}

impl CheckpointStore for JournalCheckpoints {
    type Handle = i64;

    fn save(&self, _task_id: TaskId, payload: Vec<u8>) -> std::result::Result<i64, String> {
        let st: JobState = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
        let j = self.journal.lock().map_err(|e| e.to_string())?;
        j.save_checkpoint(&st.job, st.next as i64, &payload).map_err(|e| e.to_string())
    }

    fn load(&self, handle: &i64) -> std::result::Result<Vec<u8>, String> {
        let j = self.journal.lock().map_err(|e| e.to_string())?;
        j.load_checkpoint(*handle).map_err(|e| e.to_string())?.ok_or_else(|| format!("checkpoint {handle} is not in the journal"))
    }

    fn remove(&self, handle: &i64) -> std::result::Result<(), String> {
        let j = self.journal.lock().map_err(|e| e.to_string())?;
        j.remove_checkpoint(*handle).map_err(|e| e.to_string())
    }
}

/// A job on the scheduler.
#[derive(Debug)]
pub struct JobTask {
    state: JobState,
    /// Bytes that did not restore: the task ends at its next poll.
    broken: Option<String>,
}

impl JobTask {
    pub fn new(state: JobState) -> JobTask {
        JobTask { state, broken: None }
    }
}

// SAFETY: every function touches only `self`; `restore` accepts any bytes (bytes that are not a JobState make a
// task that ends at its first poll).
unsafe impl ProtocolTask for JobTask {
    unsafe fn poll(&mut self) -> TaskPoll {
        if self.broken.is_some() {
            return TaskPoll::Ready;
        }
        if self.state.next < self.state.stages.len() {
            let stage = self.state.stages[self.state.next].clone();
            let done = run_stage(&self.state, &stage);
            self.state.done.extend(done);
            self.state.next += 1;
        }
        if self.state.next >= self.state.stages.len() {
            finished(&self.state);
            TaskPoll::Ready
        } else {
            TaskPoll::Checkpoint(CheckpointReason::Manual)
        }
    }

    unsafe fn checkpoint(&mut self) -> Vec<u8> {
        serde_json::to_vec(&self.state).unwrap_or_default()
    }

    unsafe fn restore(bytes: &[u8]) -> Self {
        match serde_json::from_slice::<JobState>(bytes) {
            Ok(state) => JobTask { state, broken: None },
            Err(e) => JobTask { state: JobState::default(), broken: Some(e.to_string()) },
        }
    }

    fn footprint_estimate(&self) -> usize {
        std::mem::size_of::<Self>() + self.state.stages.iter().map(|s| s.len() * 256).sum::<usize>()
    }
}

derive_protocol_task_vtable!(JobTask, JOB_TASK_VTABLE);

/// The scheduler's key for job tasks.
pub const JOB_KEY: VTableKey = VTableKey { segment: "custodian", kind: "job", version: 1 };

/// A finished job's steps are in the journal (its `step` events); its checkpoints are no longer needed.
fn finished(st: &JobState) {
    if let Ok(ctx) = Ctx::at(&st.root) {
        if let Some(j) = ctx.journal.as_ref() {
            let _ = j.remove_checkpoints_of(&st.job);
            let _ = j.event("job", Some(&st.job), &st.job, "steps-done", &format!("{} steps", st.done.len()), None);
        }
    }
}

/// A step's line, as declared, and the spec names its readings go under.
struct Planned {
    step: String,
    ext: External,
    readings: Vec<String>,
}

/// The values an earlier step's readings gave: rows of a kind, one field of each.
pub fn values_from(ctx: &Ctx, st: &JobState, from: &StepFrom) -> Vec<String> {
    let Some(journal) = ctx.journal.as_ref() else { return Vec::new() };
    let Some(prev) = st.done.iter().find(|d| d.step == from.step) else { return Vec::new() };
    let mut out = std::collections::BTreeSet::new();
    for name in &prev.readings {
        let Ok(Some(row)) = journal.step_reading(&st.job, name) else { continue };
        let Ok(report) = custodian_kit::reading::load(&st.root.join(&row.record)) else { continue };
        for r in report.query.results.iter().chain(&report.unresolved).filter(|r| r.kind == from.row) {
            let v = match from.field.as_str() {
                "label" => Some(r.label.clone()),
                "path" => r.path.clone(),
                "crate" => r.crate_name.clone(),
                f => r.attributes.get(f).cloned(),
            };
            if let Some(v) = v.filter(|v| !v.is_empty()) {
                out.insert(v);
            }
        }
    }
    out.into_iter().collect()
}

fn spec_text(name: &str, question: &str, kind: &str, target: &str, change: &[String], with: Option<&str>) -> String {
    let mut s = format!(
        "name = {}\nquestion = {}\nkind = {}\ntarget = {}\n",
        toml::Value::String(name.into()),
        toml::Value::String(question.into()),
        toml::Value::String(kind.into()),
        toml::Value::String(target.into())
    );
    if let Some(w) = with {
        s.push_str(w);
        s.push('\n');
    }
    if !change.is_empty() {
        s.push_str(&format!("\n[change]\nitems = {}\n", toml::Value::Array(change.iter().map(|v| toml::Value::String(v.clone())).collect())));
    }
    s
}

/// The lines a step runs, or why it does not run.
fn plan_step(ctx: &Ctx, st: &JobState, step: &StepPlan) -> Result<std::result::Result<Vec<Planned>, String>> {
    let job_target = st.target.clone().unwrap_or_default();
    let target = step.target.as_deref().map(|t| t.replace("{target}", &job_target));
    let at = |line: &dyn Fn() -> Result<External>| line().map(|e| e.in_dir(&st.root));
    match step.kind.as_str() {
        "test" => {
            let mut l = Test { job: Some(st.job.clone().into()), ..Test::default() };
            if target.as_deref() == Some("all") {
                l.all = true;
            } else {
                l.affected = true;
            }
            let ext = at(&|| External::at("custodian.test", &st.exe, &l.clone().rooted::<crate::surface::Custodian>()))?;
            return Ok(Ok(vec![Planned { step: step.name.clone(), ext, readings: Vec::new() }]));
        }
        "check" => {
            let l = Check { which: target.clone().unwrap_or_else(|| "all".into()).into(), job: Some(st.job.clone().into()) };
            let ext = at(&|| External::at("custodian.check", &st.exe, &l.clone().rooted::<crate::surface::Custodian>()))?;
            return Ok(Ok(vec![Planned { step: step.name.clone(), ext, readings: Vec::new() }]));
        }
        _ => {}
    }
    let run = |spec: &Path, name: &str| -> Result<Planned> {
        // the line runs at the root, so it names the spec from there
        let rel = spec.strip_prefix(&st.root).unwrap_or(spec);
        let l = Run { target: Some(rel.as_os_str().to_os_string()), job: Some(st.job.clone().into()), again: st.again, kind: None };
        let ext = External::at("custodian.run", &st.exe, &l.rooted::<crate::surface::Custodian>())?.in_dir(&st.root);
        Ok(Planned { step: step.name.clone(), ext, readings: vec![name.to_string()] })
    };
    if let Some(spec) = &step.spec {
        let path = st.root.join(spec);
        let name = custodian_kit::api::ReadingSpec::from_path(&path).map(|s| s.name).unwrap_or_else(|_| step.name.clone());
        return Ok(Ok(vec![run(&path, &name)?]));
    }
    let base = format!("{}-{}", st.kind, step.name);
    let question = format!("job {} step {}", st.job, step.name);
    let steps_dir = st.folder.join("steps");
    // a step's spec is written in 1o, as custodian_kit writes one; a value 1o cannot carry (a double quote) keeps it
    // in TOML, which is still read
    let write = |name: &str, text: &str| -> Result<PathBuf> {
        let spec = custodian_kit::api::ReadingSpec::from_toml(text)?;
        let unwritable = crate::codes::Refusal::from(custodian_kit::api::RefusalCode::OneoUnwritable.refuse("")).code;
        let (p, body) = match spec.to_1o() {
            Ok(o) => (steps_dir.join(custodian_kit::oneo::Doc::Reading.file_name(name)), o),
            Err(e) => {
                let r = crate::codes::Refusal::from(e);
                if r.code != unwritable {
                    return Err(r);
                }
                (steps_dir.join(format!("{name}.toml")), text.to_string())
            }
        };
        crate::layout::write_whole(&p, body.as_bytes())?;
        Ok(p)
    };
    if let Some(from) = &step.for_each {
        let values = values_from(ctx, st, from);
        let d = decide::step_input(&step.name, &from.step, &values);
        if !d.go {
            return Ok(Err(d.reason));
        }
        let mut out = Vec::new();
        for v in values {
            let name = format!("{base}-{}", crate::readings::slug(&v));
            let p = write(&name, &spec_text(&name, &question, &step.kind, &v, &[], step.with.as_deref()))?;
            out.push(run(&p, &name)?);
        }
        return Ok(Ok(out));
    }
    let mut change = Vec::new();
    if let Some(from) = &step.change_items {
        change = values_from(ctx, st, from);
        let d = decide::step_input(&step.name, &from.step, &change);
        if !d.go {
            return Ok(Err(d.reason));
        }
    }
    let target = target.unwrap_or_else(|| "*".into());
    let target = match target.strip_prefix('@') {
        Some(file) => std::fs::read_to_string(st.root.join(file)).or_else(|_| std::fs::read_to_string(file)).map_err(|e| Code::JobFile.refuse(format!("{file}: {e}")))?,
        None => target,
    };
    let p = write(&base, &spec_text(&base, &question, &step.kind, &target, &change, step.with.as_deref()))?;
    Ok(Ok(vec![run(&p, &base)?]))
}

/// A step's child process, polled by scope until it exits.
struct StepProc {
    slot: usize,
    child: Child,
    out: Option<std::thread::JoinHandle<String>>,
    err: Option<std::thread::JoinHandle<String>>,
    started: Instant,
}

/// What a child left: its status, its output, how long it took.
type Exited = (Option<i32>, String, String, i64);

impl<'env> scope::ScopedTask<'env, Vec<Option<Exited>>> for StepProc {
    fn poll(&mut self, cx: &mut scope::Cx<'_, 'env, Vec<Option<Exited>>>) -> scope::TaskPoll {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                let out = self.out.take().and_then(|h| h.join().ok()).unwrap_or_default();
                let err = self.err.take().and_then(|h| h.join().ok()).unwrap_or_default();
                cx.resources[self.slot] = Some((status.code(), out, err, self.started.elapsed().as_millis() as i64));
                scope::TaskPoll::Ready
            }
            Ok(None) => {
                std::thread::sleep(std::time::Duration::from_millis(5));
                cx.yield_now()
            }
            Err(e) => {
                cx.resources[self.slot] = Some((None, String::new(), e.to_string(), self.started.elapsed().as_millis() as i64));
                scope::TaskPoll::Ready
            }
        }
    }
}

fn drain(p: Option<impl Read + Send + 'static>) -> Option<std::thread::JoinHandle<String>> {
    p.map(|mut p| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = p.read_to_string(&mut s);
            s
        })
    })
}

/// Runs one stage: every step's lines at once, recorded in step order.
fn run_stage(st: &JobState, stage: &[StepPlan]) -> Vec<StepDone> {
    let mut ctx = match Ctx::at(&st.root) {
        Ok(c) => c,
        Err(e) => return stage.iter().map(|s| StepDone { step: s.name.clone(), lines: Vec::new(), seqs: Vec::new(), accepted: false, skipped: Some(e.to_string()), readings: Vec::new() }).collect(),
    };
    let mut planned: Vec<Planned> = Vec::new();
    let mut done: Vec<StepDone> = Vec::new();
    for step in stage {
        match plan_step(&ctx, st, step) {
            Ok(Ok(p)) if !p.is_empty() => planned.extend(p),
            Ok(Ok(_)) => done.push(StepDone { step: step.name.clone(), lines: Vec::new(), seqs: Vec::new(), accepted: true, skipped: Some("nothing to run".into()), readings: Vec::new() }),
            Ok(Err(reason)) | Err(crate::codes::Refusal { message: reason, .. }) => {
                if let Some(j) = ctx.journal.as_ref() {
                    let _ = j.event("step", Some(&st.job), &step.name, "skipped", &reason, None);
                }
                done.push(StepDone { step: step.name.clone(), lines: Vec::new(), seqs: Vec::new(), accepted: true, skipped: Some(reason), readings: Vec::new() })
            }
        }
    }
    // start every line of the stage, then poll them together
    let mut exits: Vec<Option<Exited>> = vec![None; planned.len()];
    let mut procs = Vec::new();
    let mut kept: Vec<External> = Vec::new();
    for (slot, p) in planned.iter_mut().enumerate() {
        let ext = std::mem::replace(&mut p.ext, External::shell("none", &xccute_runtime::RawCommand(String::new())));
        if let Some(j) = ctx.journal.as_ref() {
            let _ = j.event("step", Some(&st.job), &p.step, "running", ext.preview.display(), None);
        }
        let started = Instant::now();
        match facade::spawn(ext) {
            Ok((mut child, ext)) => {
                let out = drain(child.stdout.take());
                let err = drain(child.stderr.take());
                procs.push(StepProc { slot, child, out, err, started });
                kept.push(ext);
            }
            Err(e) => {
                exits[slot] = Some((None, String::new(), e.to_string(), 0));
                kept.push(External::shell("refused", &xccute_runtime::RawCommand(e.to_string())));
            }
        }
    }
    let _report = scope::run_scoped(&mut exits, |s| {
        for p in procs {
            let name = format!("step-{}", p.slot);
            s.spawn_named(name, p);
        }
    });
    // record in step order
    let mut by_step: Vec<StepDone> = Vec::new();
    for (i, p) in planned.iter().enumerate() {
        let (code, out, err, millis) = exits[i].clone().unwrap_or((None, String::new(), "the step did not run".into(), 0));
        let names = ExternalNames { name: p.step.clone(), job: Some(st.job.clone()), outputs: p.readings.clone() };
        let r = facade::finish_external(&mut ctx, &kept[i], names, code, out, err, millis);
        let (seq, accepted, line) = match &r {
            Ok(run) => (run.receipt.seq, run.accepted, run.receipt.line.clone()),
            Err(_) => (0, false, kept[i].preview.display().to_string()),
        };
        if let Some(j) = ctx.journal.as_ref() {
            let _ = j.event("step", Some(&st.job), &p.step, if accepted { "done" } else { "failed" }, &line, Some(seq));
        }
        match by_step.iter_mut().find(|d| d.step == p.step) {
            Some(d) => {
                d.lines.push(line);
                d.seqs.push(seq);
                d.accepted &= accepted;
                d.readings.extend(p.readings.clone());
            }
            None => by_step.push(StepDone { step: p.step.clone(), lines: vec![line], seqs: vec![seq], accepted, skipped: None, readings: p.readings.clone() }),
        }
    }
    let mut ordered = Vec::new();
    for step in stage {
        if let Some(d) = by_step.iter().find(|d| d.step == step.name).or_else(|| done.iter().find(|d| d.step == step.name)) {
            ordered.push(d.clone());
        }
    }
    ordered
}

/// How far the runner goes before it returns: every stage of every job, or (an interruption, for a test) this many.
#[derive(Debug, Clone, Copy, Default)]
pub struct Limit {
    pub stages: Option<usize>,
}

/// What the runner did with each job.
#[derive(Debug, Clone)]
pub struct RunnerReport {
    pub finished: Vec<String>,
    /// Jobs left dormant with a checkpoint (the limit was reached).
    pub interrupted: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Runs jobs on the scheduler until each has run every stage (or the limit is reached).
pub fn run_jobs(journal_path: &Path, jobs: Vec<JobState>, limit: Limit) -> Result<RunnerReport> {
    let store = JournalCheckpoints::open(journal_path)?;
    let mut sched = WakerProtocol::new(store);
    sched.register_vtable(JOB_KEY, &JOB_TASK_VTABLE);
    let mut handles = Vec::new();
    for st in jobs {
        let qos = crate::words::JobKind::of_word(&st.kind).map(|k| k.qos()).unwrap_or(waker_protocol::QoSClass::Standard);
        let policy = TaskPolicy { qos, retention: RetentionPolicy { keep_latest: 1, retain_on_completion: false, retain_on_failure: true, retain_on_cancel: true }, ..TaskPolicy::default() };
        let job = st.job.clone();
        let h = sched.spawn_box(JOB_KEY, Box::new(JobTask::new(st)), policy).map_err(|e| Code::StepFailed.refuse(e))?;
        h.wake(WakeReason::Manual).map_err(|e| Code::StepFailed.refuse(e))?;
        handles.push((job, h));
    }
    let mut stages = 0usize;
    let mut interrupted = Vec::new();
    loop {
        sched.run_once().map_err(|e| Code::StepFailed.refuse(e))?;
        let mut live = 0;
        for (job, h) in &handles {
            let Some(entry) = sched.tasks.get(h.id()) else { continue };
            live += 1;
            if entry.state == TaskState::Dormant {
                stages += 1;
                if limit.stages.is_some_and(|n| stages >= n) {
                    interrupted.push(job.clone());
                    continue;
                }
                h.wake(WakeReason::Resume).map_err(|e| Code::StepFailed.refuse(e))?;
            }
        }
        if live == 0 || !interrupted.is_empty() {
            break;
        }
    }
    let mut finished = Vec::new();
    let mut failed = Vec::new();
    for (id, state) in sched.finished.drain(..) {
        let job = handles.iter().find(|(_, h)| h.id() == id).map(|(j, _)| j.clone()).unwrap_or_default();
        match state {
            TaskState::Failed(msg) => failed.push((job, msg)),
            _ => finished.push(job),
        }
    }
    Ok(RunnerReport { finished, interrupted, failed })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(name: &str, reads: Option<&str>) -> StepPlan {
        StepPlan {
            name: name.into(),
            kind: "reach".into(),
            target: None,
            for_each: reads.map(|s| StepFrom { step: s.into(), row: "r".into(), field: "f".into() }),
            change_items: None,
            with: None,
            spec: None,
        }
    }

    #[test]
    fn a_step_runs_after_the_step_it_reads_and_beside_the_rest() {
        let stages = stages_of(vec![step("codes", None), step("reach", Some("codes")), step("tests", Some("codes")), step("marks", None)], false);
        let names: Vec<Vec<&str>> = stages.iter().map(|s| s.iter().map(|p| p.name.as_str()).collect()).collect();
        assert_eq!(names, [vec!["codes", "marks"], vec!["reach", "tests"]]);
        assert_eq!(stages_of(vec![step("a", None), step("b", None)], true).len(), 2, "a folder of specs runs in order");
    }

    #[test]
    fn bytes_that_are_not_a_job_restore_to_a_task_that_ends() {
        let mut t = unsafe { JobTask::restore(b"not json") };
        assert!(t.broken.is_some());
        assert!(matches!(unsafe { t.poll() }, TaskPoll::Ready));
    }
}
