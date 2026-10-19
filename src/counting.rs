//! `custodian count [ROUTE] [--by crate|language] [--tests]`, `custodian count report A B` and
//! `custodian count history [ROUTE]`.
//!
//! A count is custodian_kit's `count` reading kind (tokei for files, lines, code, comments and blanks; klepto for
//! the part that is tests), a reading with a receipt like every other, reused while the code has not moved. `report`
//! compares two counts with tree_history's `count::compare_counts` and keeps the comparison as a reading of its
//! own; a count is named by its receipt number, a job id, a commit or a date (tree_history's `count::resolve`,
//! answered through the journal), and a name that fits more than one count is refused with the counts it fits.
//! `history` lists every count of a route the journal holds, newest first (differences_kit's `count_history`). The
//! tables are tree_history's.

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::journal::ReadingRow;
use crate::layout::{self, write_whole};
use crate::readings::{self, Ask};
use crate::surface::{so, CountOf};
use tree_history::count::{compare_counts, counts_in, Counted};

/// A count as a table, by crate or by language, with the tests' share when asked (tree_history's).
pub use tree_history::count::table;
/// A count diff as a table (tree_history's).
pub use tree_history::count::diff_table;

/// `custodian count of [ROUTE]`: one count of the route (the workspace by default).
pub fn count(ctx: &mut Ctx, l: &CountOf) -> Result<Ran> {
    take(ctx, &so(&l.route).unwrap_or_else(|| "*".into()), so(&l.by).as_deref().unwrap_or("crate"), l.tests, l.again, so(&l.job))
}

/// One count of `route`, as a reading.
pub fn take(ctx: &mut Ctx, route: &str, by: &str, tests: bool, again: bool, job: Option<String>) -> Result<Ran> {
    let mut spec = readings::inline_spec("count", route)?;
    spec.name = format!("count-{}", if route == "*" { "workspace".to_string() } else { readings::slug(route) });
    let job = match job {
        Some(j) => Some(j),
        None => ctx.current_job(None)?,
    };
    let (mut ran, report) = readings::read_spec(ctx, &spec, &readings::From::Line, &Ask { again, job, word: String::new() })?;
    // the table this line asked for, in place of the reading's page
    if let Some(c) = report.as_ref().and_then(|r| r.count.as_ref()) {
        ran.printed = vec![table(c, by, tests)];
        ran.say(format!("counted {}: {} files, {} lines ({} code), tests {}‰, digest {}{}", c.route, c.files, c.lines.lines, c.lines.code, c.test_share, c.digest, c.commit.as_ref().map(|x| format!(", commit {}", tree_history::count::short_commit(x))).unwrap_or_default()));
    }
    Ok(ran)
}

/// Every count the journal holds, oldest first: the command line's own (`custodian count`) and the ones
/// custodian_kit's job code took (its reading lines), each read back from its record. A reuse is not a count.
fn counts(ctx: &Ctx) -> Result<Vec<Counted>> {
    Ok(counts_in(ctx.root(), &ctx.journal()?.receipts()?))
}

/// The one count a name names (tree_history's `resolve`, answered through the journal).
fn resolve<'a>(ctx: &Ctx, all: &'a [Counted], name: &str) -> Result<&'a Counted> {
    let j = ctx.journal()?;
    let reused_of = |n: i64| j.receipt(n).ok().flatten().and_then(|r| r.reused_seq);
    let job_id = |name: &str| j.job(name).ok().flatten().map(|r| r.id);
    tree_history::count::resolve(all, name, reused_of, job_id).map_err(|e| match e.refusal {
        tree_history::Refusal::CountAmbiguous => Code::CountAmbiguous.refuse(e.detail),
        _ => Code::CountNotFound.refuse(format!("{}: no count by that receipt number, job, commit or date", e.detail)),
    })
}

/// `custodian count report A B`: what moved between two counts, kept as a reading.
pub fn report(ctx: &mut Ctx, a: &str, b: &str) -> Result<Ran> {
    let all = counts(ctx)?;
    let (x, y) = (resolve(ctx, &all, a)?, resolve(ctx, &all, b)?);
    let d = compare_counts(&x.count, &y.count);
    let text = diff_table(&d, x.seq, y.seq);
    let name = format!("count-report-{}-{}", x.seq, y.seq);
    let mut ran = Ran::named(&name);
    ran.print(text.clone());
    ran.say(format!("compared count #{} ({}) with #{} ({}): {}", x.seq, x.utc, y.seq, y.utc, d.line("moved")));
    if let Some(p) = ctx.mode.project() {
        let st = crate::journal::stamp();
        let page = p.layout.path::<layout::ReadingPage>(&[("name", &name), ("stamp", &st)])?;
        let record = p.layout.path::<layout::ReadingRecord>(&[("name", &name), ("stamp", &st)])?;
        write_whole(&page, text.as_bytes())?;
        write_whole(&record, (serde_json::to_string_pretty(&d).map_err(|e| Code::Write.refuse(e))? + "\n").as_bytes())?;
        let (pr, rr) = (p.layout.rel(&page), p.layout.rel(&record));
        ran.say(format!("report: {pr} (+ {rr})"));
        ran.outputs = vec![pr.clone(), rr.clone()];
        ran.reading = Some(ReadingRow {
            seq: 0,
            name,
            kind: "count_report".into(),
            target: format!("{} {}", x.seq, y.seq),
            spec_digest: crate::digest::of("count_report", &(&d.before.digest, &d.after.digest)),
            index_key: y.index_key.clone().unwrap_or_default(),
            word: "none".into(),
            stamp: st,
            page: pr,
            record: rr,
            reading_digest: d.digest.clone(),
            job: None,
        });
    }
    Ok(ran)
}

/// `custodian count history [ROUTE]`: every count of the route (all routes when none), newest first
/// (differences_kit's `count_history`).
pub fn history(ctx: &mut Ctx, route: Option<&str>) -> Result<Ran> {
    let p = differences_kit::Project::find(ctx.root()).map_err(|e| Code::NeedsFolder.refuse(e))?;
    let d = differences_kit::count_history(&p, route).map_err(|e| Code::Journal.refuse(e))?;
    let mut ran = Ran::named("count history");
    ran.print(d.page);
    for s in d.said {
        ran.say(s);
    }
    Ok(ran)
}
