// Copyright © 2026 1o. All rights reserved.

//! What a job will cost before it runs, and its budget: custodian_kit's `step_time` over this workspace's receipts.
//! `job start`, `agent brief` and the handoff say the planned cost (each figure measured here, or unknown); a job's
//! `--budget hours=<n>,battery=<percent>` (else the workspace's `.custodian/config/budget.toml`) sets the battery floor
//! its cargo lines keep; its hours are information (the brief and the closing receipt say how far past them a job is),
//! never a refusal.

use crate::codes::Result;
use crate::facade::Ctx;
use custodian_kit::step_time;

/// The planned cost of what waits, from this workspace's receipts: in a running job, the specs written since the job
/// before it closed (the patterns reading's among them) and what it has spent, so the line says the whole job; else every spec
/// waiting.
pub fn planned_line(ctx: &Ctx) -> Result<Option<String>> {
    let (Some(store), Some(journal)) = (custodian_kit::store::Store::find(ctx.root()), ctx.journal.as_ref()) else { return Ok(None) };
    let receipts = journal.receipts()?;
    let running = ctx.current_job(None).ok().flatten();
    let planned = match running.as_deref().and_then(|j| journal.job(j).ok().flatten().map(|row| (j.to_string(), row))) {
        Some((id, row)) if !row.opened_utc.is_empty() => {
            // the job's plan is what was written since the job before it closed: a spec an earlier job left and
            // never made is not this job's, and a spec written for this job before it opened is
            let since = journal.jobs_in("done")?.into_iter().filter_map(|j| j.closed_utc).filter(|c| !c.is_empty() && c.as_str() <= row.opened_utc.as_str()).max().unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
            step_time::planned_in_job(&store, &receipts, &journal.receipts_of_job(&id)?, &since)
        }
        _ => step_time::planned_in(&store, &receipts),
    };
    Ok(Some(step_time::estimate(&step_time::times(&receipts), &planned).line()))
}

/// Before a step starts in `job`: its budget read (the job file's, else the workspace's) and its battery floor set for
/// the lines it starts. Nothing is refused by hours (James, 2026-10-23: the cost is information); the battery rule is
/// the one that stops a step, in custodian_kit's `power` before each cargo line.
pub fn before_step(ctx: &Ctx, job: Option<&str>, _step: &str, _kind: &str, _members: usize) -> Result<()> {
    let (Some(job), Some(store)) = (job, custodian_kit::store::Store::find(ctx.root())) else { return Ok(()) };
    let Some(folder) = crate::readings::job_folder(ctx, job)? else { return Ok(()) };
    let jf = custodian_kit::jobs::job_file(&folder);
    let Some(budget) = step_time::budget_of(&store, &jf)? else { return Ok(()) };
    if let Some(p) = budget.battery {
        std::env::set_var(custodian_kit::power::BATTERY_MIN_ENV, p.to_string());
    }
    Ok(())
}

/// How far past its budget's hours the running job is, when it is (`over the budget by 12.0 min (...)`): what the
/// brief says, never a refusal.
pub fn over_line(ctx: &Ctx) -> Result<Option<String>> {
    let (Some(store), Some(journal)) = (custodian_kit::store::Store::find(ctx.root()), ctx.journal.as_ref()) else { return Ok(None) };
    let Some(job) = ctx.current_job(None).ok().flatten() else { return Ok(None) };
    let Some(folder) = crate::readings::job_folder(ctx, &job)? else { return Ok(None) };
    let Some(budget) = step_time::budget_of(&store, &custodian_kit::jobs::job_file(&folder))? else { return Ok(None) };
    Ok(step_time::over_by(&budget, step_time::spent(&journal.receipts_of_job(&job)?)))
}
