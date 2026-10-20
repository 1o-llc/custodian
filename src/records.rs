// Copyright © 2026 1o. All rights reserved.

//! Three lines that only read the records: `custodian handoff [JOB]`, `custodian why ROUTE` and `custodian cost`.
//! What each says is custodian_kit's (`handoff`, `why`, `cost`); this module hands them the journal's receipts and
//! readings, runs the one program `why` needs (`git log`, custodian_kit's declared line, with its own receipt) and
//! prints the answer.

use crate::codes::{Code, Result};
use crate::facade::{self, Ctx, External, ExternalNames, Ran};
use crate::layout;
use custodian_kit::handoff::Reader;

fn store(ctx: &Ctx) -> Result<custodian_kit::store::Store> {
    ctx.mode.need_project()?;
    custodian_kit::store::Store::find(ctx.root()).ok_or_else(|| Code::NeedsFolder.refuse(ctx.root().display()))
}

/// `custodian handoff [JOB] [--for james|agent] [--since SEQ]`: findings (1) when something waits on a person.
pub fn handoff(ctx: &mut Ctx, job: Option<&str>, reader: Option<&str>, since: Option<i64>) -> Result<Ran> {
    let store = store(ctx)?;
    let reader = reader.and_then(Reader::of_word).unwrap_or(Reader::James);
    let receipts = ctx.journal()?.receipts()?;
    let folders = match (job, since) {
        (Some(j), _) => vec![custodian_kit::handoff::job_folder(&store, Some(j))?],
        (None, Some(s)) => custodian_kit::handoff::jobs_since(&store, &receipts, s)?,
        (None, None) => vec![custodian_kit::handoff::job_folder(&store, None)?],
    };
    let mut ran = Ran::named(job.unwrap_or("handoff"));
    if folders.is_empty() {
        ran.say(format!("no job ran after receipt #{}", since.unwrap_or_default()));
        return Ok(ran);
    }
    let mut texts = Vec::new();
    let mut waiting = 0;
    for f in &folders {
        let h = custodian_kit::handoff::of_job(ctx.root(), &store, f, &receipts, since)?;
        waiting += h.decisions_open.len() + h.refused.len() + h.verify.iter().filter(|(_, s)| s != "satisfied").count() + usize::from(h.close_refusal.is_some()) + h.uncovered.len();
        ran.say(format!("handoff of job {} ({}): made from its records in {}", h.name, h.id, h.folder));
        texts.push(custodian_kit::handoff::render(&h, reader));
    }
    ran.print(texts.join("\n"));
    if waiting > 0 {
        ran.say(format!("{waiting} waiting on a person"));
        ran.code = 1;
    }
    Ok(ran)
}

/// `custodian why ROUTE`: the records about a route, and git's first and last commit touching it.
pub fn why(ctx: &mut Ctx, route: &str) -> Result<Ran> {
    let store = store(ctx)?;
    let journal = ctx.journal()?;
    let utc: std::collections::BTreeMap<i64, String> = journal.receipts()?.into_iter().map(|r| (r.seq, r.utc)).collect();
    let readings: Vec<custodian_kit::why::ReadingAbout> = journal
        .readings()?
        .into_iter()
        .map(|r| custodian_kit::why::ReadingAbout { seq: r.seq, utc: utc.get(&r.seq).cloned().unwrap_or_default(), name: r.name, kind: r.kind, target: r.target, job: r.job })
        .collect();
    let mut w = custodian_kit::why::of(ctx.root(), &store, route, &readings)?;
    let mut ran = Ran::named(route);
    if w.paths.is_empty() {
        w.git_note = Some("the route has no files".into());
    } else {
        // git's own account, through custodian_kit's declared `git log` line, with its receipt
        let root = ctx.root().to_path_buf();
        let line = External::of("git.log", &custodian_kit::exec::git_log_paths(&root, &w.paths))?;
        match facade::run_external(ctx, line, ExternalNames { name: format!("log {route}"), job: None, outputs: Vec::new() }, |_| {}) {
            Ok(run) => {
                w.git_receipt = (run.receipt.seq > 0).then_some(run.receipt.seq);
                if run.accepted {
                    w.git = custodian_kit::why::ends(&custodian_kit::why::commits_of(&run.stdout));
                    if w.git.is_none() {
                        w.git_note = Some("git log names no commit touching it".into());
                    }
                } else {
                    w.git_note = Some(run.stderr.lines().next().unwrap_or("git log refused").trim().to_string());
                }
                ran.say(format!("git: {} → {} ({}), receipt #{}", run.receipt.line, run.code.map(|c| c.to_string()).unwrap_or_else(|| "no status".into()), run.receipt.reason, run.receipt.seq));
            }
            Err(e) => w.git_note = Some(e.to_string()),
        }
    }
    ran.print(custodian_kit::why::render(&w));
    ran.say(format!(
        "why {route}: {} task histories, {} linkage records, {} readings, {}",
        w.tasks.len(),
        w.linkage.len(),
        w.readings.len(),
        w.git.as_ref().map(|g| format!("{} commits", g.commits)).unwrap_or_else(|| "no commit".into())
    ));
    Ok(ran)
}

/// `custodian cost [--job ID] [--since SEQ]`: the cost records summed.
pub fn cost(ctx: &mut Ctx, job: Option<&str>, since: Option<i64>) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    let records = custodian_kit::cost::load_all(&layout.dir::<layout::CostFile>(&[("seq", "1")])?);
    let receipts = ctx.journal()?.receipts()?;
    let sums = custodian_kit::cost::sum(&records, &receipts, job, since);
    let mut ran = Ran::named("cost");
    ran.print(custodian_kit::cost::render(&sums, job, since));
    ran.say(custodian_kit::cost::brief_line(&sums));
    Ok(ran)
}
