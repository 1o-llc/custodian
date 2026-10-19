//! A job is a task history in James Hunt's typed 1o records (`task_history`, schema `nodeplan.1o.diff.v1`) under
//! `.custodian/data/history/<task>/<revision>/`. custodian_kit's job code writes them (task and intent at start, a
//! solution per step, the changes, the verifications, the result and the index at close); the command line adds
//! the intent the asker states (`job start --intent`) as an intent record of its own, and reads a job's status back
//! from its records for `job list`.

use crate::codes::{Code, Result};
use crate::facade::Ctx;
use custodian_kit::jobs::JobFile;
use custodian_kit::store::Store;
use custodian_kit::task_records;
use std::path::Path;
use task_history::{DiffDocument, DiffRecordKind};

fn store(ctx: &Ctx) -> Result<Store> {
    Store::find(ctx.root()).ok_or_else(|| Code::NeedsFolder.refuse(ctx.root().display()))
}

/// The intent stated on the line, written into the job's revision beside the one the kit wrote.
pub fn state_intent(ctx: &Ctx, folder: &Path, intent: &str) -> Result<String> {
    let jf = custodian_kit::jobs::job_file(folder);
    let t = task_records::of_job(&store(ctx)?, jf.task_id.as_deref(), jf.revision.as_deref())?.ok_or_else(|| Code::JobFile.refuse(format!("{}: no task history", folder.display())))?;
    t.write(DiffDocument::record(DiffRecordKind::Intent).with("intent", intent))?;
    Ok(format!("task history: {}/{} intent: {intent}", t.task_id, t.revision))
}

/// A job's status from its task record: the revision's result when it has one, else the task's.
pub fn status(ctx: &Ctx, jf: &JobFile) -> Option<String> {
    let task = task_records::read(&store(ctx).ok()?, jf.task_id.as_deref()?).ok()??;
    let rev = task.revisions.iter().find(|r| Some(r.name.as_str()) == jf.revision.as_deref()).or(task.revisions.last())?;
    let of = |k: DiffRecordKind| rev.documents.iter().rev().find(|d| d.kind().ok() == Some(k)).and_then(|d| d.one("status").ok().map(str::to_owned));
    of(DiffRecordKind::Result).or_else(|| of(DiffRecordKind::Task)).map(|s| format!("{s}, {}", rev.name))
}
