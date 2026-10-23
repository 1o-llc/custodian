//! `custodian run`, `custodian diff`, `custodian list`: a spec answered through custodian_kit, the same-search
//! check before a search is spent, the dated reading beside the specs, what moved since the last reading, and the
//! inventory rendered from the journal.

use crate::codes::{Code, Result};
use crate::decide;
use crate::digest;
use crate::facade::{Ctx, Ran};
use crate::journal::ReadingRow;
use crate::layout::{self, write_whole};
use custodian_kit::api::{ReadingKind, ReadingReport, ReadingSpec, Resolving};
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
    custodian_kit::reading::spec::name_slug(s)
}

/// A search on the line as a spec.
pub fn inline_spec(kind: &str, target: &str) -> Result<ReadingSpec> {
    let k: ReadingKind = kind.parse().map_err(|e: String| Code::LineRefused.refuse(e))?;
    // the kit's one search spec: the same a preflight or a plan makes for the same target
    Ok(ReadingSpec::search(k, target)?)
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
        // the root itself, named as a path, is the requirements reading's `.` (every workspace under it)
        let t = if t.is_empty() && k == ReadingKind::Requirements.as_str() { ".".to_string() } else { t };
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

/// The one same-search key, custodian_kit's (`api::SearchKey`: the spec's hash and klepto's fingerprint of the
/// code its scope reads). A bundle's filters are part of its spec (`ReadingSpec.bundle`), so they are part of the
/// one hash; there is no second key of the command line's own.
pub fn search_key(ctx: &Ctx, spec: &ReadingSpec) -> Result<custodian_kit::api::SearchKey> {
    custodian_kit::api::SearchKey::of(ctx.root(), spec)
}

/// The digest of a spec and an option word as the command line took it before the one key (kept for a reading
/// made before: its row still carries it).
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
    let k = search_key(ctx, spec)?;
    let (sd, key) = (k.spec_hash.clone(), k.index_key.0.clone());
    ran.spec_digest = Some(sd.clone());
    ran.index_key = Some(key.clone());
    ran.code_digest = Some(k.code_digest.clone());
    if let Some(journal) = ctx.journal.as_ref() {
        let (decided, prev) = decide::same_search(journal, &sd, &k.code_digest, ask.again)?;
        ran.say(decided.said());
        if let (Some((row, _)), false) = (prev, ask.again) {
            ran.reused_seq = Some(row.seq);
            ran.outputs = vec![row.page.clone(), row.record.clone()];
            let page = ctx.root().join(&row.page);
            if let Ok(text) = std::fs::read_to_string(&page) {
                ran.print(text);
            }
            ran.say(format!("reused: {} (search #{})", row.page, row.seq));
            // the reuse saves what the reading it reused cost
            let reused = ctx.journal()?.receipt(row.seq)?;
            if let (Some(store), Some(r)) = (custodian_kit::store::Store::find(ctx.root()), reused) {
                ran.cost = custodian_kit::cost::reading_cost(ctx.root(), &store, &r).map(|c| custodian_kit::cost::reuse_record(0, "", ask.job.as_deref(), &c));
            }
            let report = custodian_kit::reading::load(&ctx.root().join(&row.record)).ok();
            return Ok((ran, report));
        }
    }
    let how = if ask.job.is_some() { Resolving::Job } else { Resolving::Free };
    // a kind that reads no scan (a sweep, a requirements reading) is answered at the root: no workspace is opened
    let scanned = spec.kind.needs_scan();
    let mut reading = if scanned {
        let ws = ctx.workspace(&spec.scope, how)?;
        custodian_kit::api::run(ws, spec)?
    } else {
        custodian_kit::api::run_at(ctx.root(), how, spec)?
    };
    // where the index came from: the kit's (parsed, from cache), or the resident's held one; a kind that walks the
    // syntax trees says its own (it parses when the index came from the cache)
    if scanned && (!spec.kind.walks_trees() || !matches!(ctx.index, Some(custodian_kit::reading::facts::IndexSource::FromCache { .. }))) {
        if let Some(i) = &ctx.index {
            reading.report.index = Some(i.line());
        }
    }
    ran.index = reading.report.index.clone();
    if scanned {
        ran.say(format!("index: {}", reading.report.index.clone().unwrap_or_else(|| "-".into())));
    }
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
                    let into = dir.join(custodian_kit::jobs::READINGS_DIR).join(&spec.name);
                    let stem = page.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    write_whole(&into.join(format!("{stem}.md")), reading.markdown().as_bytes())?;
                    write_whole(&into.join(format!("{stem}.json")), reading.json().as_bytes())?;
                    ran.say(format!("in job {job}: {}", ctx.rel(&into)));
                }
            }
            ran.outputs = vec![page_rel.clone(), record_rel.clone()];
            // what reading the same files by hand would have cost; the facade keeps it under the receipt's number
            ran.cost = Some(custodian_kit::cost::read_record(ctx.root(), 0, "", ask.job.as_deref(), &reading.report));
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

/// `custodian requirements [ROUTE] [--intent TEXT] [--spec FILE] [--faces WORDS]`: what an intended change needs from
/// the workspace as it is, as custodian_kit's `requirements` reading: a search on the line (`run ROUTE --kind
/// requirements`) with the intent, the change spec's needs and the faces in its spec, so the one same-search key
/// (the manifests, the lock, the toolchain files and those) reuses it until one of them changes.
pub fn requirements(ctx: &mut Ctx, l: &crate::surface::Requirements) -> Result<Ran> {
    let so = |v: &Option<std::ffi::OsString>| v.as_ref().map(|x| x.to_string_lossy().into_owned());
    let job = ctx.current_job(so(&l.job))?;
    let mut target = so(&l.route).unwrap_or_else(|| ".".into());
    let change = match so(&l.spec) {
        Some(f) => {
            let file = std::env::current_dir().unwrap_or_default().join(&f);
            let cs = custodian_kit::change::ChangeSpec::from_path(&file)?;
            // the change spec's crate is the route when the line names none
            if so(&l.route).is_none() && cs.target.starts_with("crate:") && cs.target != "crate:*" {
                target = cs.target.split('/').next().unwrap_or(&cs.target).to_string();
            }
            Some((ctx.rel(&file.canonicalize().unwrap_or(file.clone())), cs))
        }
        None => None,
    };
    let (mut spec, from) = resolve(ctx, Some(&target), Some(ReadingKind::Requirements.as_str()))?;
    spec.requirements.intent = so(&l.intent);
    if let Some((rel, cs)) = change {
        spec.requirements.needs = custodian_kit::reading::requirements::needs::of_change(&cs);
        spec.requirements.change_spec = Some(rel);
    }
    if let Some(f) = so(&l.faces) {
        spec.requirements.faces = f.split(',').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect();
    }
    spec.requirements.word()?;
    let ask = Ask { again: l.again, job, word: String::new() };
    let (ran, _) = read_spec(ctx, &spec, &from, &ask)?;
    Ok(ran)
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

/// `custodian diff SPEC [--from SEQ] [--to SEQ]`: what moved between two readings of one spec. The two are picked
/// from the receipts, as `differences readings` picks them (differences_kit's `reading_pair`): the readings index is
/// a cache rebuilt from the receipts, never the source.
pub fn diff(ctx: &mut Ctx, spec: &str, from: Option<i64>, to: Option<i64>) -> Result<Ran> {
    ctx.journal()?;
    let name = spec.trim_end_matches(".toml");
    let name = Path::new(name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string());
    let project = differences_kit::Project::open(ctx.root()).map_err(|e| Code::NeedsFolder.refuse(e))?;
    let [before, after] = differences_kit::reading_pair(&project, &name, from, to).map_err(|e| match e.refusal {
        differences_kit::Refusal::NoReading | differences_kit::Refusal::NothingToCompare => Code::NothingToDiff.refuse(e.detail),
        _ => Code::Journal.refuse(e),
    })?;
    let side = |(r, record): &(custodian_kit::api::Receipt, String)| -> Result<(i64, custodian_kit::api::ReadingReport, String)> {
        let report = custodian_kit::reading::load(&project.at(record)).map_err(|e| Code::ReadingUnreadable.refuse(format!("#{}: {e}", r.seq)))?;
        Ok((r.seq, report, record.clone()))
    };
    let ((bs, a, br), (as_, b, ar)) = (side(&before)?, side(&after)?);
    let mut ran = Ran::named(&name);
    // two counts: what grew, file by file (custodian_kit's count compare); any other reading: rows and counts
    if let (Some(x), Some(y)) = (&a.count, &b.count) {
        let d = custodian_kit::reading::count::compare_counts(x, y);
        ran.print(tree_history::count::diff_table(&d, bs, as_));
        ran.say(format!("compared {name}: #{bs} ({}) → #{as_} ({}): {}", a.stamp, b.stamp, d.line("grew")));
    } else {
        let d = custodian_kit::diff::compare(&a, &b);
        ran.print(custodian_kit::diff::render(&d));
        ran.say(format!("compared {name}: #{bs} ({}) → #{as_} ({}): {}", a.stamp, b.stamp, if d.is_empty() { "nothing moved" } else { "moved" }));
    }
    ran.outputs = vec![br, ar];
    Ok(ran)
}

/// `custodian list`: the inventory, rendered from the journal into `data/inventory.md`.
pub fn list(ctx: &mut Ctx) -> Result<Ran> {
    let layout = ctx.layout()?.clone();
    let journal = ctx.journal()?;
    let mut s = format!("# inventory\n\n- root: `{}`\n- journal: {} receipts in `{}`\n", ctx.root().display(), journal.count()?, layout.rel(&journal.path));
    if ctx.rebuilt.any() {
        let r = ctx.rebuilt;
        s.push_str(&format!("- journal rebuilt from the records this checkout carries: {} receipts, {} readings, {} jobs put back; the sequence continues from #{}\n", r.receipts, r.readings, r.jobs, r.last));
    }
    let names = |dir: PathBuf| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.path().file_stem().map(|n| n.to_string_lossy().into_owned())).collect()).unwrap_or_default();
        v.sort();
        v
    };
    let queued = names(layout.dir::<layout::QueuedSpec>(&[("name", "x")])?);
    let done = names(layout.dir::<layout::DoneSpec>(&[("name", "x")])?);
    // a job's specs are specs too: the folder's own and the ones its steps wrote
    let jobs_root = layout.jobs_dir("x")?.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut in_jobs: Vec<(String, usize)> = Vec::new();
    for (_, dir) in crate::journal::job_folders(&jobs_root) {
        let n = custodian_kit::jobs::job_specs(&dir).len() + custodian_kit::jobs::job_specs(&dir.join("steps")).len();
        if n > 0 {
            in_jobs.push((dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(), n));
        }
    }
    let job_specs: usize = in_jobs.iter().map(|(_, n)| n).sum();
    s.push_str(&format!(
        "\n## specs ({} in all)\n\n- queued: {}\n- done: {}\n- in jobs: {}\n",
        queued.len() + done.len() + job_specs,
        list_or_none(&queued),
        list_or_none(&done),
        if in_jobs.is_empty() { "none".to_string() } else { in_jobs.iter().map(|(j, n)| format!("{j} ({n})")).collect::<Vec<_>>().join(", ") }
    ));
    let readings = journal.readings()?;
    let receipts = journal.receipts()?;
    s.push_str("\n## readings (the latest of each spec, in a job or not)\n\n| spec | kind | target | latest | runs | reuses | job | page |\n|---|---|---|---|---|---|---|---|\n");
    let mut by: std::collections::BTreeMap<&str, Vec<&ReadingRow>> = Default::default();
    for r in &readings {
        by.entry(r.name.as_str()).or_default().push(r);
    }
    let job_names: std::collections::BTreeMap<String, String> = journal.jobs()?.into_iter().map(|j| (j.id, j.name)).collect();
    for (name, rows) in &by {
        let last = rows.last().copied().unwrap_or(rows[0]);
        let reuses = receipts.iter().filter(|r| r.reused_seq.is_some_and(|n| rows.iter().any(|x| x.seq == n))).count();
        let job = last.job.as_ref().map(|j| job_names.get(j).map(|n| format!("{n} ({j})")).unwrap_or_else(|| j.clone())).unwrap_or_else(|| "-".into());
        s.push_str(&format!("| {name} | {} | `{}` | #{} {} | {} | {reuses} | {job} | `{}` |\n", last.kind, last.target, last.seq, last.stamp, rows.len(), last.page));
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
