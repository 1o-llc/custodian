// Copyright © 2026 1o. All rights reserved.

//! What a job will cost before it runs, and its budget: custodian_kit's `step_time` over this workspace's receipts.
//! `job start`, `agent brief` and the handoff say the planned cost (each figure measured here, or unknown); a job's
//! `--budget hours=<n>,battery=<percent>` (else the workspace's `.custodian/config/budget.toml`) refuses a step whose
//! measured time would take the job past its hours, before it starts, and sets the battery floor its cargo lines keep.

use crate::codes::Result;
use crate::facade::Ctx;
use custodian_kit::step_time;

/// The planned cost of what waits, from this workspace's receipts.
pub fn planned_line(ctx: &Ctx) -> Result<Option<String>> {
    let (Some(store), Some(journal)) = (custodian_kit::store::Store::find(ctx.root()), ctx.journal.as_ref()) else { return Ok(None) };
    let receipts = journal.receipts()?;
    Ok(Some(step_time::estimate(&step_time::times(&receipts), &step_time::planned_in(&store, &receipts)).line()))
}

/// Before a step of `kind` covering `members` starts in `job`: its budget read (the job file's, else the
/// workspace's), its battery floor set for the lines it starts, and the step refused when its measured time would
/// take the job past its hours. A step with no measured time here is not refused (nothing to refuse on).
pub fn before_step(ctx: &Ctx, job: Option<&str>, step: &str, kind: &str, members: usize) -> Result<()> {
    let (Some(job), Some(store)) = (job, custodian_kit::store::Store::find(ctx.root())) else { return Ok(()) };
    let Some(folder) = crate::readings::job_folder(ctx, job)? else { return Ok(()) };
    let jf = custodian_kit::jobs::job_file(&folder);
    let Some(budget) = step_time::budget_of(&store, &jf)? else { return Ok(()) };
    if let Some(p) = budget.battery {
        std::env::set_var(custodian_kit::power::BATTERY_MIN_ENV, p.to_string());
    }
    let journal = ctx.journal()?;
    let times = step_time::times(&journal.receipts()?);
    let t = times.iter().find(|t| t.kind == kind);
    let each = if kind == step_time::PREFLIGHT { t.and_then(step_time::StepTime::per_member) } else { t.and_then(step_time::StepTime::per_run) };
    let spent = step_time::spent(&journal.receipts_of_job(job)?);
    Ok(step_time::over_budget(&budget, spent, step, each.map(|e| e * members.max(1) as i64))?)
}
