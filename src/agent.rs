//! `custodian agent brief` and `custodian agent guard`: the first line an agent runs in a project, and the question
//! the pre-edit hook asks before an edit.
//!
//! The brief is made from the records, never from a conversation: the folder's settings (the job mode), the open
//! decisions in every task history (asked first, together), every job folder with its task state (custodian_kit's
//! `tasks::TaskState`, derived from the records), the latest reading of each spec with its age and whether the code
//! moved since (its receipt's code digest against the code's digest now), the test lines' last results, how much
//! rediscovery the journal saved, and the exact next lines to run. In free mode it says so, and what `agent init`
//! would give.
//!
//! The guard refuses a hand edit of code while a bounded job is open (its code is written through change specs:
//! `custodian preflight`, `custodian transform`); anything else may be edited.

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::journal::{now_utc, job_folders};
use custodian_kit::jobs::{JobMode, CODE_LANGUAGES};
use custodian_kit::questions::Question;
use custodian_kit::tasks::TaskState;
use std::collections::BTreeMap;
use std::path::Path;

/// The running job, as its folder says it now.
struct Open {
    id: String,
    name: String,
    mode: JobMode,
    target: Option<String>,
    target_known: bool,
    verify: Vec<String>,
    folder: String,
}

fn open_job(ctx: &Ctx) -> Option<Open> {
    let p = ctx.mode.project()?;
    let jobs = p.layout.jobs_dir("running").ok()?;
    let dir = std::fs::read_dir(&jobs).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).min()?;
    let jf = custodian_kit::jobs::job_file(&dir);
    Some(Open {
        id: jf.id.clone().unwrap_or_default(),
        name: jf.name.clone().unwrap_or_default(),
        mode: jf.mode.or(p.settings.job_mode).unwrap_or(JobMode::Unbounded),
        target: jf.target.clone(),
        target_known: jf.target_known.unwrap_or(false),
        verify: jf.verify.clone(),
        folder: ctx.rel(&dir),
    })
}

/// The workspace's job mode, and the mode of the job open now.
fn mode_line(ctx: &Ctx, open: Option<&Open>) -> String {
    let default = ctx.mode.project().and_then(|p| p.settings.job_mode).unwrap_or(JobMode::Unbounded);
    match open {
        Some(o) => format!("mode: {} (job {} is open; the workspace's default is {}) — {}", o.mode.as_str(), o.id, default.as_str(), o.mode.doc()),
        None => format!("mode: {} (the workspace's default; no job is open) — {}", default.as_str(), default.doc()),
    }
}

fn question_line(q: &Question) -> String {
    format!("decide {}: {}{}", q.id, q.question, if q.options.is_empty() { String::new() } else { format!(" — {}", q.options.join(" | ")) })
}

/// A stamp (`20261019T112814Z`) as an age from now.
fn age(stamp: &str) -> String {
    let Ok(t) = chrono::NaiveDateTime::parse_from_str(stamp.get(..16).unwrap_or(stamp), "%Y%m%dT%H%M%SZ") else {
        return "age unknown".into();
    };
    let secs = (chrono::Utc::now().naive_utc() - t).num_seconds().max(0);
    match secs {
        s if s < 120 => format!("{s} s ago"),
        s if s < 7200 => format!("{} min ago", s / 60),
        s if s < 172_800 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86_400),
    }
}

/// `custodian agent brief [--head]`.
pub fn brief(ctx: &mut Ctx, head: bool) -> Result<Ran> {
    let mut ran = Ran::named("agent brief");
    let mut s = String::from("# agent brief\n\n");
    let Some(project) = ctx.mode.project().cloned() else {
        s.push_str(&format!(
            "free mode: no .custodian at {} or above. Readings print and nothing is stored: no journal, no receipts, no reuse, no jobs, no decisions kept.\n\n`custodian agent init` gives: specs and dated readings, the journal (every run a receipt; the same search reused), jobs as folders with task histories, the job lock, decisions asked once and kept, change specs with their preflight and transform, and this brief.\n\nnext: custodian agent init\n",
            ctx.root().display()
        ));
        ran.print(s);
        ran.say("free mode: custodian agent init makes the folder");
        return Ok(ran);
    };
    let store = custodian_kit::store::Store::find(ctx.root()).ok_or_else(|| Code::NeedsFolder.refuse(ctx.root().display()))?;
    let open = open_job(ctx);
    s.push_str(&format!("{}\n", mode_line(ctx, open.as_ref())));
    // the decisions first, together
    let decisions = custodian_kit::questions::open(&store)?;
    if decisions.is_empty() {
        s.push_str("\n## decisions: none open\n");
    } else {
        s.push_str(&format!("\n## decisions to make first ({})\n\n", decisions.len()));
        let mut by: BTreeMap<&str, Vec<&Question>> = BTreeMap::new();
        for q in &decisions {
            by.entry(q.source.as_str()).or_default().push(q);
        }
        for (src, qs) in by {
            s.push_str(&format!("- {src}\n"));
            for q in qs {
                s.push_str(&format!("  - `{}` {}{}\n", q.id, q.question, if q.options.is_empty() { String::new() } else { format!(" — {}", q.options.join(" | ")) }));
            }
        }
    }
    // the open job and the work outstanding
    let journal = ctx.journal()?;
    let jobs_root = project.layout.jobs_dir("x")?.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut outstanding = Vec::new();
    let mut done_n = (0usize, 0usize);
    let checkpointed: Vec<String> = journal.jobs()?.into_iter().filter(|j| journal.latest_checkpoint(&j.id).ok().flatten().is_some()).map(|j| j.id).collect();
    for (state, dir) in job_folders(&jobs_root) {
        let jf = custodian_kit::jobs::job_file(&dir);
        let closing: Option<custodian_kit::jobs::Closing> = std::fs::read_to_string(dir.join("closing.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
        let result = closing.as_ref().map(|c| c.result.clone()).filter(|r| !r.is_empty()).or_else(|| crate::history::status(ctx, &jf).map(|x| x.split(',').next().unwrap_or("").to_string()));
        let open_q = jf.task_id.as_deref().is_some_and(|t| decisions.iter().any(|q| q.task.as_deref().is_some_and(|x| x.starts_with(&format!("{t}/")))));
        let ts = TaskState::of_job(&state, result.as_deref(), open_q, jf.id.as_ref().is_some_and(|i| checkpointed.contains(i)));
        if state == "done" {
            done_n.0 += 1;
            if ts.word() == "completed" {
                done_n.1 += 1;
            }
        }
        if ts.word() != "completed" {
            outstanding.push(format!("- {} `{}` ({}): {}{}", ts.word(), jf.name.clone().unwrap_or_default(), jf.id.clone().unwrap_or_else(|| "not started".into()), ctx.rel(&dir), closing.as_ref().and_then(|c| c.result_reason.clone()).map(|r| format!(" — {r}")).unwrap_or_default()));
        }
    }
    match &open {
        Some(o) => {
            s.push_str(&format!("\n## the open job: {} ({})\n\n- folder: `{}`\n- target: {}\n- outcome: {}\n", o.id, o.name, o.folder, match (&o.target, o.target_known) {
                (Some(t), true) => format!("known: {t}"),
                (Some(t), false) => format!("unknown: {t} does not resolve"),
                (None, _) => "unknown: the job names none".into(),
            }, if o.verify.is_empty() { "unknown: no verification declared".to_string() } else { format!("known: done is {}", o.verify.join(", ")) }));
            let runs = journal.receipts_of_job(&o.id)?;
            let transforms = runs.iter().filter(|r| r.command == "transform").count();
            let preflights = runs.iter().filter(|r| r.command == "preflight").count();
            s.push_str(&format!("- runs in it: {} ({} readings, {preflights} preflights, {transforms} transforms)\n", runs.len(), runs.iter().filter(|r| r.command == "reading").count()));
        }
        None => s.push_str("\n## no job is open\n"),
    }
    s.push_str(&format!("\n## work not completed ({} of {} closed jobs completed)\n\n{}\n", done_n.1, done_n.0, if outstanding.is_empty() { "none".to_string() } else { outstanding.join("\n") }));
    if !head {
        // the latest reading of each spec, and whether the code moved since
        let readings = journal.readings()?;
        let mut latest: BTreeMap<&str, &crate::journal::ReadingRow> = BTreeMap::new();
        for r in &readings {
            latest.insert(r.name.as_str(), r);
        }
        let mut now_codes: BTreeMap<String, String> = BTreeMap::new();
        s.push_str("\n## the latest reading of each spec\n\n| spec | kind | target | # | age | the code since | headline |\n|---|---|---|---|---|---|---|\n");
        for (name, r) in &latest {
            let receipt = journal.receipt(r.seq)?;
            let then = receipt.as_ref().and_then(|x| x.code_digest.clone()).unwrap_or_default();
            let report = custodian_kit::reading::load(&ctx.root().join(&r.record)).ok();
            let scope_key = report.as_ref().map(|rep| if rep.kind.reads_tests() { "tests" } else { "src" }).unwrap_or("src");
            let now = match now_codes.get(scope_key) {
                Some(n) => n.clone(),
                None => {
                    let scope = custodian_kit::api::Scope { tests: scope_key == "tests", ..Default::default() };
                    let n = custodian_kit::code_digest(ctx.root(), &scope.scan_options()).unwrap_or_default();
                    now_codes.insert(scope_key.to_string(), n.clone());
                    n
                }
            };
            let moved = if then.is_empty() { "not recorded" } else if then == now { "unchanged" } else { "moved: read again" };
            let headline = report.as_ref().and_then(|rep| rep.headline.first().cloned()).or_else(|| report.as_ref().map(|rep| rep.counts.iter().take(2).map(|c| format!("{} {}", c.what, c.n)).collect::<Vec<_>>().join("; "))).unwrap_or_default();
            s.push_str(&format!("| {name} | {} | `{}` | #{} | {} | {moved} | {} |\n", r.kind, r.target, r.seq, age(&r.stamp), headline.replace('|', "/")));
        }
        if latest.is_empty() {
            s.push_str("none\n");
        }
        let tests = journal.test_runs()?;
        let mut last: BTreeMap<(String, String), &crate::journal::TestRun> = BTreeMap::new();
        for t in &tests {
            last.insert((t.package.clone(), t.target.clone()), t);
        }
        s.push_str("\n## test lines (the last run of each)\n\n");
        if last.is_empty() {
            s.push_str("none run here\n");
        }
        for ((p, t), r) in &last {
            s.push_str(&format!("- {p} {t}: #{} {} passed, {} failed, {} ignored ({} ms)\n", r.seq, r.passed, r.failed, r.ignored, r.millis));
        }
        // what the records saved, and what cannot be measured here
        let receipts = journal.receipts()?;
        let reused = receipts.iter().filter(|r| r.reused_seq.is_some()).count();
        let read = receipts.iter().filter(|r| r.command == "reading" && r.reused_seq.is_none()).count();
        let preflight_ms: i64 = receipts.iter().filter(|r| r.command == "preflight").map(|r| r.millis).sum();
        let asked = asked_records(&store)?;
        s.push_str(&format!(
            "\n## what the records saved\n\n- runs: {} (each a receipt)\n- searches reused rather than read again: {reused} (read: {read})\n- preflight time: {preflight_ms} ms over {} preflights\n- decisions asked: {} ({} answered, {} open); asked before a job changed code: {}, after: {}\n- jobs completed: {} of {} closed\n- tokens: not measured here (no source in the records)\n",
            receipts.len(),
            receipts.iter().filter(|r| r.command == "preflight").count(),
            asked.0,
            asked.0.saturating_sub(decisions.len()),
            decisions.len(),
            asked.1,
            asked.2,
            done_n.1,
            done_n.0
        ));
    }
    // the next lines, exactly
    let mut next: Vec<String> = decisions.iter().take(3).map(|q| format!("custodian decide {} <answer> --why \"<the reason>\"   # {}", q.id, q.question)).collect();
    match &open {
        None => {
            if journal.readings()?.is_empty() {
                next.push("custodian plan make \"<what the task is for>\"   # where the work is, what it costs, what to decide first".into());
            }
            next.push("custodian job start <name> --intent \"<what for>\" --verify test:<name> [--mode bounded]".into());
        }
        Some(o) => {
            if !o.target_known {
                next.push(format!("custodian plan make \"<what {} is for>\"   # the target is unknown: find it first", o.name));
            }
            if o.verify.is_empty() {
                next.push("custodian job verify test:<name>   # the outcome is unknown: what does done look like?".into());
            }
            if o.mode == JobMode::Bounded {
                next.push("custodian preflight .custodian/data/specs/changes/<spec>.toml   # bounded: the code is written through change specs".into());
                next.push("custodian transform .custodian/data/specs/changes/<spec>.toml --write".into());
            }
            next.push(format!("custodian job close {}", o.id));
        }
    }
    s.push_str("\n## next\n\n");
    for n in &next {
        s.push_str(&format!("- `{n}`\n"));
    }
    s.push_str(&format!("\n(made {} from the journal, the job folders and the task histories)\n", now_utc()));
    ran.print(s);
    ran.say(format!("{} · {} decisions open · {} jobs not completed", mode_line(ctx, open.as_ref()).split(" — ").next().unwrap_or(""), decisions.len(), outstanding.len()));
    if let Some(q) = decisions.first() {
        ran.say(question_line(q));
    }
    ran.job = open.map(|o| o.id);
    Ok(ran)
}

/// (asked, asked before the job changed code, asked after): every problem record with a decision id, against the
/// first transform record of its revision.
fn asked_records(store: &custodian_kit::store::Store) -> Result<(usize, usize, usize)> {
    let root = custodian_kit::task_records::root(store)?;
    if !root.exists() {
        return Ok((0, 0, 0));
    }
    let index = task_history::load_task_history(&root).map_err(|e| Code::Journal.refuse(e))?;
    let (mut all, mut before, mut after) = (0, 0, 0);
    for t in index.tasks {
        for r in t.revisions {
            let mut changed = false;
            for d in r.documents {
                match d.kind() {
                    Ok(task_history::DiffRecordKind::Solution) if d.one("solution").is_ok_and(|s| s.starts_with("transform")) => changed = true,
                    Ok(task_history::DiffRecordKind::Problem) if d.one("decision_id").is_ok() => {
                        all += 1;
                        if changed {
                            after += 1;
                        } else {
                            before += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok((all, before, after))
}

/// `custodian agent guard FILE`: refused (2) while a bounded job is open and the file is code; done (0) otherwise.
pub fn guard(ctx: &mut Ctx, file: &str) -> Result<Ran> {
    let mut ran = Ran::named(format!("guard {file}"));
    let Some(o) = open_job(ctx) else {
        ran.say(format!("{file}: no job is open: edit"));
        return Ok(ran);
    };
    ran.job = Some(o.id.clone());
    let path = std::env::current_dir().unwrap_or_default().join(file);
    let rel = path.strip_prefix(ctx.root()).map(Path::to_path_buf).ok();
    let is_code = match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => CODE_LANGUAGES.contains(&"Rust"),
        Some("toml") => CODE_LANGUAGES.contains(&"TOML"),
        _ => false,
    };
    let inside = rel.as_ref().is_some_and(|r| !r.starts_with(crate::layout::FOLDER));
    if o.mode == JobMode::Bounded && is_code && inside {
        return Err(Code::BoundedEdit.refuse(format!(
            "{file} in bounded job {}: write a change spec under .custodian/data/specs/changes/, then custodian preflight SPEC and custodian transform SPEC --write",
            o.id
        )));
    }
    ran.say(format!("{file}: job {} is {}: edit", o.id, o.mode.as_str()));
    Ok(ran)
}
