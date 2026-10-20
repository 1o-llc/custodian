// Copyright © 2026 1o. All rights reserved.

//! `custodian exercise run [--ops ...] [--units ...] [--n N] [--tests] [--crate R] [--intents FILE] [--seed S]` and
//! `custodian exercise report [JOB]`: custodian_kit's `exercise` job kind from the command line. The options are the
//! kit's word (`exercise::Options`, its rules checked by flaga_solver); the job is queued, started (its cases run on
//! copies in its data folder) and closed by the kit's job code, as `triage` is; the summary is printed. Agent mode.

use crate::codes::Result;
use crate::facade::{Ctx, Ran};
use crate::surface::{so, ExerciseRun};
use std::path::PathBuf;

/// `custodian exercise run ...`: the exercise job from queue to close; the summary printed; 1 when it found
/// anything.
pub fn run(ctx: &mut Ctx, l: &ExerciseRun) -> Result<Ran> {
    ctx.mode.need_agent_project()?;
    let intents = so(&l.intents).map(PathBuf::from);
    let options = custodian_kit::exercise::Options::from_words(so(&l.ops).as_deref(), so(&l.units).as_deref(), so(&l.n).as_deref(), l.tests, so(&l.krate).as_deref(), intents.as_deref(), so(&l.seed).as_deref())?;
    let (started, closing, done, said) = custodian_kit::exercise::run_job(&crate::jobs::kit(ctx), &options)?;
    let mut ran = Ran::named(&started.id);
    ran.job = Some(started.id.clone());
    ran.said.extend(started.said);
    ran.said.extend(said);
    match custodian_kit::exercise::summary_of(&done) {
        Some(s) => {
            ran.print(custodian_kit::exercise::markdown(&s));
            if !s.findings.is_empty() {
                ran.code = 1;
            }
            let page = done.join(custodian_kit::jobs::JOB_DATA).join(custodian_kit::exercise::DIR).join(custodian_kit::exercise::SUMMARY_PAGE);
            ran.say(format!("summary: {}", ctx.rel(&page)));
            ran.outputs = vec![ctx.rel(&page), ctx.rel(&done.join("closing.md"))];
        }
        None => ran.print(custodian_kit::jobs::render_closing(&closing)),
    }
    ran.say(format!("closing receipt: {}", ctx.rel(&done.join("closing.md"))));
    Ok(ran)
}

/// `custodian exercise report [JOB]`: an exercise job's summary (the newest when none is named).
pub fn report(ctx: &mut Ctx, job: Option<&str>) -> Result<Ran> {
    ctx.mode.need_agent_project()?;
    let (s, folder) = custodian_kit::exercise::report(&crate::jobs::kit(ctx), job)?;
    let mut ran = Ran::named(&s.job);
    ran.job = Some(s.job.clone());
    ran.print(custodian_kit::exercise::markdown(&s));
    if !s.findings.is_empty() {
        ran.code = 1;
    }
    let page = folder.join(custodian_kit::jobs::JOB_DATA).join(custodian_kit::exercise::DIR).join(custodian_kit::exercise::SUMMARY_PAGE);
    ran.outputs = vec![ctx.rel(&page)];
    Ok(ran)
}
