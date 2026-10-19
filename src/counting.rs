//! `custodian count [ROUTE] [--by crate|language] [--tests]`, `custodian count report A B` and
//! `custodian count history [ROUTE]`.
//!
//! A count is custodian_kit's `count` reading kind (tokei for files, lines, code, comments and blanks; klepto for
//! the part that is tests), a reading with a receipt like every other, reused while the code has not moved. `report`
//! compares two counts with custodian_kit's `count::compare_counts` and keeps the comparison as a reading of its
//! own; a count is named by its receipt number, a job id, a commit or a date, resolved through the journal, and a
//! name that fits more than one count is refused with the counts it fits. `history` lists every count of a route
//! the journal holds, newest first.

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::journal::ReadingRow;
use crate::layout::{self, write_whole};
use crate::readings::{self, Ask};
use crate::surface::{so, CountOf};
use custodian_kit::reading::count::{compare_counts, CountDiff, CountReport, Lines};

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
        ran.say(format!("counted {}: {} files, {} lines ({} code), tests {}‰, digest {}{}", c.route, c.files, c.lines.lines, c.lines.code, c.test_share, c.digest, c.commit.as_ref().map(|x| format!(", commit {}", short(x))).unwrap_or_default()));
    }
    Ok(ran)
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

fn cells(l: &Lines) -> String {
    format!("{} | {} | {} | {}", l.lines, l.code, l.comments, l.blanks)
}

/// A count as a table, by crate or by language, with the tests' share when asked.
pub fn table(c: &CountReport, by: &str, tests: bool) -> String {
    let mut out = format!("# count of {}\n\n- digest: {}\n- commit: {}\n- files: {}\n- lines: {} (code {}, comments {}, blanks {})\n", c.route, c.digest, c.commit.as_deref().unwrap_or("-"), c.files, c.lines.lines, c.lines.code, c.lines.comments, c.lines.blanks);
    if tests {
        out.push_str(&format!("- tests: {} lines, {}‰ of all\n", c.tests.lines, c.test_share));
    }
    let head = if tests { "| files | lines | code | comments | blanks | test lines |\n" } else { "| files | lines | code | comments | blanks |\n" };
    let rule = if tests { "|---|---|---|---|---|---|---|\n" } else { "|---|---|---|---|---|---|\n" };
    let row = |name: &str, files: usize, l: &Lines, t: &Lines| {
        if tests {
            format!("| {name} | {files} | {} | {} |\n", cells(l), t.lines)
        } else {
            format!("| {name} | {files} | {} |\n", cells(l))
        }
    };
    if by == "language" {
        out.push_str(&format!("\n## by language\n\n| language {head}{rule}"));
        for l in &c.languages {
            out.push_str(&row(&l.language, l.files, &l.lines, &l.tests));
        }
    } else {
        out.push_str(&format!("\n## by crate\n\n| crate {head}{rule}"));
        for k in &c.crates {
            out.push_str(&row(&k.name, k.files, &k.lines, &k.tests));
        }
    }
    out
}

/// A count reading the journal holds, with its count.
struct Taken {
    row: ReadingRow,
    utc: String,
    count: CountReport,
}

/// Every count the journal holds, oldest first: the command line's own (`custodian count`) and the ones
/// custodian_kit's job code took (its reading lines), each read back from its record. A reuse is not a count.
fn counts(ctx: &Ctx) -> Result<Vec<Taken>> {
    let j = ctx.journal()?;
    let mut out = Vec::new();
    for r in j.receipts()?.into_iter().filter(|r| r.reused_seq.is_none()) {
        let Some(json) = r.outputs.iter().find(|o| o.ends_with(".json")) else { continue };
        let Ok(rep) = custodian_kit::reading::load(&ctx.root().join(json)) else { continue };
        let Some(count) = rep.count else { continue };
        let row = ReadingRow {
            seq: r.seq,
            name: r.name.clone(),
            kind: "count".into(),
            target: rep.target.clone(),
            spec_digest: r.spec_hash.clone().unwrap_or_default(),
            index_key: r.index_key.clone().unwrap_or_default(),
            word: "none".into(),
            stamp: rep.stamp.clone(),
            page: r.outputs.first().cloned().unwrap_or_default(),
            record: json.clone(),
            reading_digest: count.digest.clone(),
            job: r.job.clone(),
        };
        out.push(Taken { row, utc: r.utc.clone(), count });
    }
    Ok(out)
}

/// The one count a name names: a receipt number (a reuse names the count it reused), a job id or name (a job that
/// counted at open and at close fits two: `<job>:open` and `<job>:close` name one), a commit (a prefix of at least
/// 7), or a date (`2026-10-19`, or a stamp's prefix).
fn resolve<'a>(ctx: &Ctx, all: &'a [Taken], name: &str) -> Result<&'a Taken> {
    let j = ctx.journal()?;
    let mut fits: Vec<&Taken> = Vec::new();
    if let Ok(n) = name.parse::<i64>() {
        let n = j.receipt(n)?.and_then(|r| r.reused_seq).unwrap_or(n);
        fits.extend(all.iter().filter(|t| t.row.seq == n));
    }
    // a job, or its first (`:open`) or last (`:close`) count
    let (job_name, end) = match name.rsplit_once(':') {
        Some((j, e)) if e == "open" || e == "close" => (j, Some(e)),
        _ => (name, None),
    };
    if fits.is_empty() && j.job(job_name)?.is_some() {
        let id = j.job(job_name)?.map(|r| r.id).unwrap_or_default();
        let of: Vec<&Taken> = all.iter().filter(|t| t.row.job.as_deref() == Some(&id)).collect();
        match end {
            Some("open") => fits.extend(of.first().copied()),
            Some(_) => fits.extend(of.last().copied()),
            None => fits.extend(of),
        }
    }
    if fits.is_empty() && name.len() >= 7 && name.chars().all(|c| c.is_ascii_hexdigit()) {
        fits.extend(all.iter().filter(|t| t.count.commit.as_deref().is_some_and(|c| c.starts_with(name))));
    }
    if fits.is_empty() && name.len() >= 4 && name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        let compact = name.replace('-', "");
        fits.extend(all.iter().filter(|t| t.utc.starts_with(name) || t.row.stamp.starts_with(&compact)));
    }
    match fits.as_slice() {
        [one] => Ok(one),
        [] => Err(Code::CountNotFound.refuse(format!("`{name}`: no count by that receipt number, job, commit or date"))),
        many => Err(Code::CountAmbiguous.refuse(format!("`{name}` fits {} counts: {}; name one by its receipt number (or <job>:open, <job>:close)", many.len(), many.iter().map(|t| format!("#{} {} {}", t.row.seq, t.count.route, t.utc)).collect::<Vec<_>>().join(", ")))),
    }
}

/// A count diff as a table: both sides in the header, then what moved by crate.
pub fn diff_table(d: &CountDiff, a: i64, b: i64) -> String {
    let mut out = format!(
        "# count report: #{a} → #{b} ({})\n\n| | before #{a} | after #{b} |\n|---|---|---|\n| digest | {} | {} |\n| commit | {} | {} |\n| tests ‰ | {} | {} |\n\n- diff digest: {}\n- lines: +{} −{} (code +{} −{})\n- files: {} added, {} removed, {} changed\n- tests' share: {:+}‰\n",
        d.route,
        d.before.digest,
        d.after.digest,
        d.before.commit.as_deref().unwrap_or("-"),
        d.after.commit.as_deref().unwrap_or("-"),
        d.test_share_before,
        d.test_share_after,
        d.digest,
        d.moved.added.lines,
        d.moved.removed.lines,
        d.moved.added.code,
        d.moved.removed.code,
        d.files_added.len(),
        d.files_removed.len(),
        d.files_changed.len(),
        d.test_share_delta
    );
    out.push_str("\n## by crate\n\n| crate | lines + | lines − | code + | code − | files added | removed | changed | tests ‰ |\n|---|---|---|---|---|---|---|---|---|\n");
    for c in &d.crates {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} → {} |\n",
            c.name,
            c.moved.added.lines,
            c.moved.removed.lines,
            c.moved.added.code,
            c.moved.removed.code,
            c.files_added.len(),
            c.files_removed.len(),
            c.files_changed.len(),
            c.test_share_before,
            c.test_share_after
        ));
    }
    if d.crates.is_empty() {
        out.push_str("\nnothing moved.\n");
    }
    out
}

/// `custodian count report A B`: what moved between two counts, kept as a reading.
pub fn report(ctx: &mut Ctx, a: &str, b: &str) -> Result<Ran> {
    let all = counts(ctx)?;
    let (x, y) = (resolve(ctx, &all, a)?, resolve(ctx, &all, b)?);
    let d = compare_counts(&x.count, &y.count);
    let text = diff_table(&d, x.row.seq, y.row.seq);
    let name = format!("count-report-{}-{}", x.row.seq, y.row.seq);
    let mut ran = Ran::named(&name);
    ran.print(text.clone());
    ran.say(format!("compared count #{} ({}) with #{} ({}): {}", x.row.seq, x.utc, y.row.seq, y.utc, d.line("moved")));
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
            target: format!("{} {}", x.row.seq, y.row.seq),
            spec_digest: crate::digest::of("count_report", &(&d.before.digest, &d.after.digest)),
            index_key: y.row.index_key.clone(),
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

/// `custodian count history [ROUTE]`: every count of the route (all routes when none), newest first.
pub fn history(ctx: &mut Ctx, route: Option<&str>) -> Result<Ran> {
    let all = counts(ctx)?;
    let want = route.map(|r| if r.contains(':') || r == "*" { r.to_string() } else { format!("crate:{r}") });
    let mut rows: Vec<&Taken> = all.iter().filter(|t| want.as_deref().is_none_or(|w| t.count.route == w || t.row.target == w)).collect();
    rows.reverse();
    let mut out = format!("# count history{}\n\n| seq | utc | route | commit | digest | files | lines | code | tests ‰ |\n|---|---|---|---|---|---|---|---|---|\n", want.as_ref().map(|w| format!(" of {w}")).unwrap_or_default());
    for t in &rows {
        out.push_str(&format!(
            "| #{} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            t.row.seq,
            t.utc,
            t.count.route,
            t.count.commit.as_deref().map(short).unwrap_or("-"),
            crate::digest::short(t.count.digest.trim_start_matches("sha256:"), 12),
            t.count.files,
            t.count.lines.lines,
            t.count.lines.code,
            t.count.test_share
        ));
    }
    let mut ran = Ran::named("count history");
    ran.print(out);
    ran.say(format!("{} count(s) in the journal{}", rows.len(), want.map(|w| format!(" of {w}")).unwrap_or_default()));
    Ok(ran)
}
