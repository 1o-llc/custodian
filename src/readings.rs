//! `custodian run`, `custodian diff`, `custodian list`: a spec answered through custodian_kit, the same-search
//! check before a search is spent, the dated reading beside the specs, what moved since the last reading, and the
//! inventory rendered from the journal.

use crate::codes::{Code, Result};
use crate::decide;
use crate::digest;
use crate::facade::{Ctx, Ran};
use crate::journal::ReadingRow;
use crate::layout::{self, write_whole};
use custodian_kit::api::{IndexKey, ReadingKind, ReadingReport, ReadingSpec, Resolving};
use std::path::{Path, PathBuf};

/// Where a spec came from.
#[derive(Debug, Clone)]
pub enum From {
    /// A spec file named on the line.
    File(PathBuf),
    /// A spec in `specs/queued/` (it moves to `specs/done/` once it ran).
    Queued(PathBuf),
    /// A spec in `specs/done/`, run again.
    Done(PathBuf),
    /// A search on the line: `--kind` and a target.
    Line,
}

impl From {
    pub fn file(&self) -> Option<&Path> {
        match self {
            From::File(p) | From::Queued(p) | From::Done(p) => Some(p),
            From::Line => None,
        }
    }
}

/// A target as a folder name: `crate:core_lib/src/x.rs` → `crate_core_lib_src_x_rs`.
pub fn slug(s: &str) -> String {
    let mut out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_string();
    let out: String = out.chars().take(48).collect();
    if out.is_empty() {
        "all".into()
    } else {
        out
    }
}

/// A search on the line as a spec.
pub fn inline_spec(kind: &str, target: &str) -> Result<ReadingSpec> {
    let k: ReadingKind = kind.parse().map_err(|e: String| Code::LineRefused.refuse(e))?;
    let text = format!(
        "name = {}\nquestion = \"a search on the command line\"\nkind = \"{}\"\ntarget = {}\n",
        toml::Value::String(format!("{}-{}", k.as_str(), slug(target))),
        k.as_str(),
        toml::Value::String(target.to_string())
    );
    Ok(ReadingSpec::from_toml(&text)?)
}

/// The spec a line names: `--kind` with a target, a spec file, or (journaled) a queued or done spec's name.
pub fn resolve(ctx: &Ctx, target: Option<&str>, kind: Option<&str>) -> Result<(ReadingSpec, From)> {
    if let Some(k) = kind {
        let t = target.ok_or_else(|| Code::LineRefused.refuse("--kind needs a target: custodian run TARGET --kind KIND"))?;
        // a file named from anywhere is named from the root, as the kit reads a path target
        let file = std::env::current_dir().unwrap_or_default().join(t);
        let t = match (file.exists(), file.canonicalize().ok(), ctx.root().canonicalize().ok()) {
            (true, Some(f), Some(r)) if f.starts_with(&r) => f.strip_prefix(&r).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| t.to_string()),
            _ => t.to_string(),
        };
        return Ok((inline_spec(k, &t)?, From::Line));
    }
    let t = target.ok_or_else(|| Code::SpecNotFound.refuse("no spec named"))?;
    let as_path = Path::new(t);
    let as_file = if as_path.is_absolute() { as_path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(as_path) };
    if as_file.is_file() {
        let spec = ReadingSpec::from_path(&as_file)?;
        let queued = match ctx.mode.project() {
            Some(p) => p.layout.dir::<layout::QueuedSpec>(&[("name", "x")])?.canonicalize().ok() == as_file.parent().and_then(|d| d.canonicalize().ok()),
            None => false,
        };
        return Ok((spec, if queued { From::Queued(as_file) } else { From::File(as_file) }));
    }
    if let Some(p) = ctx.mode.project() {
        let name = t.trim_end_matches(".toml");
        let oneo = custodian_kit::oneo::Doc::Reading.file_name(name);
        for (dir, queued) in [(p.layout.dir::<layout::QueuedSpec>(&[("name", name)])?, true), (p.layout.dir::<layout::DoneSpec>(&[("name", name)])?, false)] {
            for file in [dir.join(format!("{name}.toml")), dir.join(&oneo), dir.join(format!("{name}.json"))] {
                if file.is_file() {
                    let spec = ReadingSpec::from_path(&file)?;
                    return Ok((spec, if queued { From::Queued(file) } else { From::Done(file) }));
                }
            }
        }
    }
    let hint = if t.contains(':') { "; a route needs a kind: custodian run ROUTE --kind reach" } else { "" };
    Err(Code::SpecNotFound.refuse(format!("`{t}`: not a file, not in specs/queued or specs/done{hint}")))
}

/// What a reading run needs besides the spec.
#[derive(Debug, Clone, Default)]
pub struct Ask {
    pub again: bool,
    pub job: Option<String>,
    /// The option word the reading runs with (`none`, or a bundle's filters): part of the same-search key.
    pub word: String,
}

/// The digest the same-search key takes of a spec and its option word.
pub fn spec_digest(spec: &ReadingSpec, word: &str) -> String {
    digest::of("spec", &(spec, word))
}

/// One spec answered: reused when the same spec ran over the same code (unless `--again`), else read through the
/// workspace held open; journaled, the reading is written beside the specs and indexed; free, it is printed.
pub fn read_spec(ctx: &mut Ctx, spec: &ReadingSpec, from: &From, ask: &Ask) -> Result<(Ran, Option<ReadingReport>)> {
    let mut ran = Ran::named(&spec.name);
    ran.job = ask.job.clone();
    ran.spec = from.file().map(|p| ctx.rel(p));
    let word = if ask.word.is_empty() { "none".to_string() } else { ask.word.clone() };
    let sd = spec_digest(spec, &word);
    let key = IndexKey::of_root(ctx.root(), &spec.scope)?.0;
    ran.spec_digest = Some(sd.clone());
    ran.index_key = Some(key.clone());
    ran.code_digest = Some(key.clone());
    if let Some(journal) = ctx.journal.as_ref() {
        let (decided, prev) = decide::same_search(journal, &sd, &key, ask.again)?;
        ran.say(decided.said());
        if let (Some((row, _)), false) = (prev, ask.again) {
            ran.reused_seq = Some(row.seq);
            ran.outputs = vec![row.page.clone(), row.record.clone()];
            let page = ctx.root().join(&row.page);
            if let Ok(text) = std::fs::read_to_string(&page) {
                ran.print(text);
            }
            ran.say(format!("reused: {} (search #{})", row.page, row.seq));
            let report = custodian_kit::reading::load(&ctx.root().join(&row.record)).ok();
            return Ok((ran, report));
        }
    }
    let how = if ask.job.is_some() { Resolving::Job } else { Resolving::Free };
    let reading = {
        let ws = ctx.workspace(&spec.scope, how)?;
        custodian_kit::api::run(ws, spec)?
    };
    let counts: Vec<String> = reading.counts().iter().map(|c| format!("{}: {}", c.what, c.n)).collect();
    match ctx.mode.project().map(|p| p.layout.clone()) {
        Some(layout) => {
            let (page, record, _latest) = reading.write(&layout.readings_dir()?)?;
            let (page_rel, record_rel) = (ctx.rel(&page), ctx.rel(&record));
            ran.print(reading.markdown());
            ran.say(format!("read {} ({} {}): {}", spec.name, spec.kind, spec.target, if counts.is_empty() { "nothing found".into() } else { counts.join(", ") }));
            ran.say(format!("reading: {page_rel} (+ {record_rel})"));
            if let Some(job) = &ask.job {
                if let Some(dir) = job_folder(ctx, job)? {
                    let into = dir.join("readings").join(&spec.name);
                    let stem = page.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    write_whole(&into.join(format!("{stem}.md")), reading.markdown().as_bytes())?;
                    write_whole(&into.join(format!("{stem}.json")), reading.json().as_bytes())?;
                    ran.say(format!("in job {job}: {}", ctx.rel(&into)));
                }
            }
            ran.outputs = vec![page_rel.clone(), record_rel.clone()];
            ran.reading = Some(ReadingRow {
                seq: 0,
                name: spec.name.clone(),
                kind: spec.kind.as_str().into(),
                target: spec.target.clone(),
                spec_digest: sd,
                index_key: key,
                word,
                stamp: reading.report.stamp.clone(),
                page: page_rel,
                record: record_rel,
                reading_digest: reading.digest(),
                job: ask.job.clone(),
            });
        }
        None => {
            ran.print(reading.markdown());
            ran.say(format!("read {} ({} {}): {}", spec.name, spec.kind, spec.target, if counts.is_empty() { "nothing found".into() } else { counts.join(", ") }));
        }
    }
    Ok((ran, Some(reading.report)))
}

/// The folder of a running job.
pub fn job_folder(ctx: &Ctx, job: &str) -> Result<Option<PathBuf>> {
    let Some(j) = ctx.journal.as_ref() else { return Ok(None) };
    Ok(j.job(job)?.map(|row| ctx.root().join(row.folder)))
}

/// `custodian run SPEC|ROUTE`: one spec, and a queued spec moves to done.
pub fn run_one(ctx: &mut Ctx, target: Option<&str>, kind: Option<&str>, ask: &Ask) -> Result<Ran> {
    let (spec, from) = resolve(ctx, target, kind)?;
    let (mut ran, _) = read_spec(ctx, &spec, &from, ask)?;
    if let (From::Queued(q), Some(p)) = (&from, ctx.mode.project()) {
        let done = p.layout.dir::<layout::DoneSpec>(&[("name", &spec.name)])?.join(q.file_name().unwrap_or_default());
        if let Some(d) = done.parent() {
            std::fs::create_dir_all(d).map_err(|e| Code::Write.refuse(format!("{}: {e}", d.display())))?;
        }
        std::fs::rename(q, &done).map_err(|e| Code::Write.refuse(format!("{}: {e}", done.display())))?;
        ran.say(format!("moved to done: {} → {}", ctx.rel(q), ctx.rel(&done)));
        ran.spec = Some(ctx.rel(&done));
    }
    Ok(ran)
}

/// The queued specs, in name order.
pub fn queued_specs(ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let p = ctx.mode.need_project()?;
    let dir = p.layout.dir::<layout::QueuedSpec>(&[("name", "x")])?;
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| crate::jobs::is_spec_file(p)).collect()).unwrap_or_default();
    v.sort();
    Ok(v)
}

/// `custodian diff SPEC [--from SEQ] [--to SEQ]`: what moved between two readings of one spec.
pub fn diff(ctx: &mut Ctx, spec: &str, from: Option<i64>, to: Option<i64>) -> Result<Ran> {
    let journal = ctx.journal()?;
    let name = spec.trim_end_matches(".toml");
    let name = Path::new(name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string());
    let all = journal.readings_of(&name)?;
    let pick = |seq: Option<i64>, default: Option<&ReadingRow>| -> Result<ReadingRow> {
        match seq {
            Some(n) => all.iter().find(|r| r.seq == n).cloned().ok_or_else(|| Code::NothingToDiff.refuse(format!("{name} has no reading #{n}"))),
            None => default.cloned().ok_or_else(|| Code::NothingToDiff.refuse(format!("{name}: {} reading(s)", all.len()))),
        }
    };
    let after = pick(to, all.last())?;
    let before_default = all.iter().rev().find(|r| r.seq < after.seq);
    let before = pick(from, before_default)?;
    let load = |r: &ReadingRow| custodian_kit::reading::load(&ctx.root().join(&r.record)).map_err(|e| Code::ReadingUnreadable.refuse(format!("#{}: {e}", r.seq)));
    let (a, b) = (load(&before)?, load(&after)?);
    let mut ran = Ran::named(&name);
    // two counts: what grew, file by file (custodian_kit's count compare); any other reading: rows and counts
    if let (Some(x), Some(y)) = (&a.count, &b.count) {
        let d = custodian_kit::reading::count::compare_counts(x, y);
        ran.print(crate::counting::diff_table(&d, before.seq, after.seq));
        ran.say(format!("compared {name}: #{} ({}) → #{} ({}): {}", before.seq, before.stamp, after.seq, after.stamp, d.line("grew")));
    } else {
        let d = custodian_kit::diff::compare(&a, &b);
        ran.print(custodian_kit::diff::render(&d));
        ran.say(format!("compared {name}: #{} ({}) → #{} ({}): {}", before.seq, before.stamp, after.seq, after.stamp, if d.is_empty() { "nothing moved" } else { "moved" }));
    }
    ran.outputs = vec![before.record, after.record];
    Ok(ran)
}

/// `custodian list`: the inventory, rendered from the journal into `data/inventory.md`.
pub fn list(ctx: &mut Ctx) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    let journal = ctx.journal()?;
    let mut s = format!("# inventory\n\n- root: `{}`\n- journal: {} receipts in `{}`\n", ctx.root().display(), journal.count()?, layout.rel(&journal.path));
    let names = |dir: PathBuf| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.path().file_stem().map(|n| n.to_string_lossy().into_owned())).collect()).unwrap_or_default();
        v.sort();
        v
    };
    let queued = names(layout.dir::<layout::QueuedSpec>(&[("name", "x")])?);
    let done = names(layout.dir::<layout::DoneSpec>(&[("name", "x")])?);
    s.push_str(&format!("\n## specs\n\n- queued: {}\n- done: {}\n", list_or_none(&queued), list_or_none(&done)));
    let readings = journal.readings()?;
    let receipts = journal.receipts()?;
    s.push_str("\n## readings (the latest of each spec)\n\n| spec | kind | target | latest | runs | reuses | page |\n|---|---|---|---|---|---|---|\n");
    let mut by: std::collections::BTreeMap<&str, Vec<&ReadingRow>> = Default::default();
    for r in &readings {
        by.entry(r.name.as_str()).or_default().push(r);
    }
    for (name, rows) in &by {
        let last = rows.last().copied().unwrap_or(rows[0]);
        let reuses = receipts.iter().filter(|r| r.reused_seq.is_some_and(|n| rows.iter().any(|x| x.seq == n))).count();
        s.push_str(&format!("| {name} | {} | `{}` | #{} {} | {} | {reuses} | `{}` |\n", last.kind, last.target, last.seq, last.stamp, rows.len(), last.page));
    }
    let jobs = journal.jobs()?;
    s.push_str("\n## jobs\n\n");
    if jobs.is_empty() {
        s.push_str("none\n");
    } else {
        s.push_str("| id | name | kind | state | opened | closed | folder |\n|---|---|---|---|---|---|---|\n");
        for j in &jobs {
            s.push_str(&format!("| {} | {} | {} | {} | {} | {} | `{}` |\n", j.id, j.name, j.kind, j.state, j.opened_utc, j.closed_utc.clone().unwrap_or_else(|| "-".into()), j.folder));
        }
    }
    let tests = journal.test_runs()?;
    s.push_str("\n## test lines (the last run of each)\n\n");
    if tests.is_empty() {
        s.push_str("none\n");
    } else {
        let mut last: std::collections::BTreeMap<(String, String), &crate::journal::TestRun> = Default::default();
        for t in &tests {
            last.insert((t.package.clone(), t.target.clone()), t);
        }
        s.push_str("| package | target | #seq | passed | failed | ignored | ms |\n|---|---|---|---|---|---|---|\n");
        for ((p, t), r) in last {
            s.push_str(&format!("| {p} | {t} | #{} | {} | {} | {} | {} |\n", r.seq, r.passed, r.failed, r.ignored, r.millis));
        }
    }
    let installs = journal.installs()?;
    if !installs.is_empty() {
        s.push_str("\n## installs\n\n");
        for (seq, action, bin, dest, sha) in installs {
            s.push_str(&format!("- #{seq} {action} {bin} → `{dest}` sha256 {}\n", digest::short(&sha, 12)));
        }
    }
    s.push_str("\n## the folder\n\n");
    for (route, path) in layout.parts() {
        s.push_str(&format!("- `{route}`: `{path}`\n"));
    }
    let file = layout.path::<layout::Inventory>(&[])?;
    write_whole(&file, s.as_bytes())?;
    let mut ran = Ran::named("inventory");
    ran.print(s);
    ran.say(format!("rendered the inventory from {} receipts: {}", receipts.len(), layout.rel(&file)));
    ran.outputs = vec![layout.rel(&file)];
    Ok(ran)
}

fn list_or_none(v: &[String]) -> String {
    if v.is_empty() {
        "none".into()
    } else {
        v.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_becomes_one_folder_name() {
        assert_eq!(slug("crate:core_lib/src/lib.rs"), "crate_core_lib_src_lib_rs");
        assert_eq!(slug("*"), "all");
        assert_eq!(inline_spec("reach", "crate:app").unwrap().name, "reach-crate_app");
        assert!(inline_spec("nope", "x").is_err());
    }
}
