//! The queue's runner: `custodian run` in agent mode puts every queued job folder on waker_protocol's scheduler, one
//! task per job, its quality of service its kind's (`JobKind::qos`: triage interactive, a folder of specs standard,
//! features batch). One job runs at a time (custodian_kit's rule), so a task whose turn has not come yields and the
//! scheduler picks by class each round. A job has two stages, custodian_kit's `start` (its steps) and `close`
//! (its closing receipt); after the first, the task asks to be checkpointed, the scheduler saves its bytes through
//! [`JournalCheckpoints`] into the journal's `checkpoints` table, and restores it from them for the second. A process
//! that stops between the two leaves the job in `jobs/running` with its checkpoint; the next `custodian run` or
//! `custodian job start` restores it from the journal and closes it, without running its steps again.

#![allow(unsafe_code)]

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::journal::Journal;
use crate::jobs::{self, StartAsk};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use waker_protocol::{derive_protocol_task_vtable, CheckpointReason, CheckpointStore, ProtocolTask, RetentionPolicy, TaskId, TaskPoll, TaskPolicy, TaskState, WakeReason, WakerProtocol};
use waker_tables::VTableKey;

/// A queued job as the runner holds it, and as its checkpoint is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JobState {
    pub root: PathBuf,
    pub exe: PathBuf,
    /// The folder's name in `jobs/queued`, then `jobs/running`.
    pub folder: String,
    pub kind: String,
    /// The job's id, once started.
    pub job: Option<String>,
    /// 0: to start; 1: to close; 2: done.
    pub stage: u8,
    pub again: bool,
    /// What each stage said.
    pub said: Vec<String>,
    pub outputs: Vec<String>,
    pub failed: Option<String>,
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
        j.save_checkpoint(st.job.as_deref().unwrap_or(&st.folder), st.stage as i64, &payload).map_err(|e| e.to_string())
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
}

fn ctx_of(st: &JobState) -> Result<Ctx> {
    let mut ctx = Ctx::at(&st.root)?;
    ctx.exe = st.exe.clone();
    ctx.live = Box::new(|_| {});
    Ok(ctx)
}

impl JobTask {
    pub fn new(state: JobState) -> JobTask {
        JobTask { state }
    }

    /// One stage; `Ok(false)` when another job holds the turn.
    fn stage(&mut self) -> Result<bool> {
        let mut ctx = ctx_of(&self.state)?;
        match self.state.stage {
            0 => {
                let layout = ctx.layout()?.clone();
                if !jobs::running_ids(&layout)?.is_empty() {
                    return Ok(false);
                }
                let ran = jobs::start_now(&mut ctx, &StartAsk { which: Some(self.state.folder.clone()), again: self.state.again, ..StartAsk::default() })?;
                self.state.job = ran.job.clone();
                self.state.said.extend(ran.said);
                self.state.outputs.extend(ran.outputs);
            }
            1 => {
                let ran = jobs::close(&mut ctx, self.state.job.as_deref())?;
                self.state.said.extend(ran.said);
                self.state.outputs.extend(ran.outputs);
            }
            _ => {}
        }
        self.state.stage += 1;
        Ok(true)
    }
}

// SAFETY: every function touches only `self`; `restore` accepts any bytes (bytes that are not a JobState make a
// task that ends at its first poll, failed).
unsafe impl ProtocolTask for JobTask {
    unsafe fn poll(&mut self) -> TaskPoll {
        if self.state.failed.is_some() || self.state.stage >= 2 {
            return TaskPoll::Ready;
        }
        match self.stage() {
            Ok(false) => TaskPoll::Yield,
            Ok(true) if self.state.stage >= 2 => {
                if let Ok(ctx) = ctx_of(&self.state) {
                    if let Some(j) = ctx.journal.as_ref() {
                        let _ = j.remove_checkpoints_of(self.state.job.as_deref().unwrap_or(&self.state.folder));
                    }
                }
                TaskPoll::Ready
            }
            Ok(true) => TaskPoll::Checkpoint(CheckpointReason::Manual),
            Err(e) => {
                self.state.failed = Some(e.to_string());
                write_report(&self.state);
                TaskPoll::Ready
            }
        }
    }

    unsafe fn checkpoint(&mut self) -> Vec<u8> {
        serde_json::to_vec(&self.state).unwrap_or_default()
    }

    unsafe fn restore(bytes: &[u8]) -> Self {
        match serde_json::from_slice::<JobState>(bytes) {
            Ok(state) => JobTask { state },
            Err(e) => JobTask { state: JobState { failed: Some(format!("a checkpoint that does not restore: {e}")), stage: 2, ..JobState::default() } },
        }
    }

    unsafe fn cleanup(&mut self) {
        write_report(&self.state);
    }
}

derive_protocol_task_vtable!(JobTask, JOB_TASK_VTABLE);

/// The scheduler's key for job tasks.
pub const JOB_KEY: VTableKey = VTableKey { segment: "custodian", kind: "job", version: 1 };

/// What the tasks said, collected as they end (a task's state is its own until then).
static REPORTS: Mutex<Vec<JobState>> = Mutex::new(Vec::new());

fn write_report(st: &JobState) {
    if let Ok(mut r) = REPORTS.lock() {
        r.retain(|x| x.folder != st.folder);
        r.push(st.clone());
    }
}

/// How far the runner goes before it returns: every stage, or (an interruption, for a test) this many.
#[derive(Debug, Clone, Copy, Default)]
pub struct Limit {
    pub stages: Option<usize>,
}

/// What the runner did.
#[derive(Debug, Clone, Default)]
pub struct RunnerReport {
    pub said: Vec<String>,
    pub outputs: Vec<String>,
    pub failed: Vec<(String, String)>,
    /// Jobs left with a checkpoint (the limit was reached).
    pub interrupted: Vec<String>,
}

fn run_states(ctx: &Ctx, states: Vec<JobState>, limit: Limit) -> Result<RunnerReport> {
    let store = JournalCheckpoints::open(&ctx.journal()?.path)?;
    let mut sched = WakerProtocol::new(store);
    sched.register_vtable(JOB_KEY, &JOB_TASK_VTABLE);
    if let Ok(mut r) = REPORTS.lock() {
        r.clear();
    }
    let mut handles = Vec::new();
    for st in states {
        let qos = crate::words::JobKind::of_word(&st.kind).map(|k| k.qos()).unwrap_or(waker_protocol::QoSClass::Standard);
        let policy = TaskPolicy { qos, retention: RetentionPolicy { keep_latest: 1, retain_on_completion: false, retain_on_failure: true, retain_on_cancel: true }, ..TaskPolicy::default() };
        let folder = st.folder.clone();
        let h = sched.spawn_box(JOB_KEY, Box::new(JobTask::new(st)), policy).map_err(|e| Code::StepFailed.refuse(e))?;
        h.wake(WakeReason::Manual).map_err(|e| Code::StepFailed.refuse(e))?;
        handles.push((folder, h));
    }
    let mut stages = 0usize;
    let mut report = RunnerReport::default();
    loop {
        sched.run_once().map_err(|e| Code::StepFailed.refuse(e))?;
        let mut live = 0;
        let mut stop = false;
        for (folder, h) in &handles {
            let Some(entry) = sched.tasks.get(h.id()) else { continue };
            live += 1;
            if entry.state == TaskState::Dormant {
                stages += 1;
                if limit.stages.is_some_and(|n| stages >= n) {
                    report.interrupted.push(folder.clone());
                    stop = true;
                    continue;
                }
                h.wake(WakeReason::Resume).map_err(|e| Code::StepFailed.refuse(e))?;
            }
        }
        if live == 0 || stop {
            break;
        }
    }
    for st in REPORTS.lock().map(|r| r.clone()).unwrap_or_default() {
        report.said.push(format!("job folder {} ({}, {:?}):", st.folder, st.kind, crate::words::JobKind::of_word(&st.kind).map(|k| k.qos()).unwrap_or(waker_protocol::QoSClass::Standard)));
        report.said.extend(st.said.iter().map(|s| format!("  {s}")));
        report.outputs.extend(st.outputs.clone());
        if let Some(f) = &st.failed {
            report.failed.push((st.folder.clone(), f.clone()));
            report.said.push(format!("  failed: {f}"));
        }
    }
    for f in &report.interrupted {
        report.said.push(format!("job folder {f} stopped between its stages; its checkpoint is in the journal (custodian run or custodian job start resumes it)"));
    }
    Ok(report)
}

/// The queued folders `names`, scheduled together.
pub fn run_folders(ctx: &Ctx, names: &[String], again: bool, limit: Limit) -> Result<RunnerReport> {
    let layout = ctx.layout()?.clone();
    let mut states = Vec::new();
    for n in names {
        let jf = custodian_kit::jobs::job_file(&layout.job_dir("queued", n)?);
        states.push(JobState { root: layout.root.clone(), exe: ctx.exe.clone(), folder: n.clone(), kind: jf.kind.unwrap_or_else(|| "specs".into()), job: None, stage: 0, again, ..JobState::default() });
    }
    run_states(ctx, states, limit)
}

/// The jobs in `jobs/running` whose checkpoint is in the journal: restored and run on from their next stage.
pub fn resume(ctx: &mut Ctx, limit: Limit) -> Result<Option<Ran>> {
    let journal = ctx.journal()?;
    let mut states = Vec::new();
    for id in jobs::running_ids(ctx.layout()?)? {
        if let Some((_, _, bytes)) = journal.latest_checkpoint(&id)? {
            if let Ok(mut st) = serde_json::from_slice::<JobState>(&bytes) {
                st.exe = ctx.exe.clone();
                st.said.clear();
                st.said.push(format!("resumed job {id} at its {} (its checkpoint after its start)", if st.stage == 1 { "close" } else { "start" }));
                states.push(st);
            }
        }
    }
    if states.is_empty() {
        return Ok(None);
    }
    let ids: Vec<String> = states.iter().filter_map(|s| s.job.clone()).collect();
    let report = run_states(ctx, states, limit)?;
    let mut ran = Ran::named(ids.join(", "));
    ran.job = ids.first().cloned();
    ran.said = report.said;
    ran.outputs = report.outputs;
    if !report.failed.is_empty() {
        ran.code = 1;
    }
    Ok(Some(ran))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_not_a_job_restore_to_a_task_that_ends() {
        let mut t = unsafe { JobTask::restore(b"not json") };
        assert!(t.state.failed.is_some());
        assert!(matches!(unsafe { t.poll() }, TaskPoll::Ready));
    }
}
