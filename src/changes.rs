// Copyright © 2026 1o. All rights reserved.

//! The bounded and unbounded lines: `preflight` and `transform` (a change spec, decided then made), `plan make |
//! approve | show` (an intent's plan, and a step of it as a change spec), `decide` (the open decisions, and one
//! made), `linkage add | check` (why declarations are bound together).
//!
//! custodian_kit holds every rule (the change spec and its vocabulary, the preflight and its gate, the plan, the
//! decisions as task-history records, the linkage records); code_passenger is the writer. Here each line is read,
//! run through the kit or through code_passenger's declared line, and said; every run keeps its receipt.

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::layout::write_whole;
use custodian_kit::api::{Resolving, Scope};
use custodian_kit::runs::{InJob, Runner};
use custodian_kit::store::Store;
use std::path::PathBuf;

fn store(ctx: &Ctx) -> Result<Store> {
    Store::find(ctx.root()).ok_or_else(|| Code::NeedsFolder.refuse(ctx.root().display()))
}

/// The scope an impact reading reads (it reads the tests too).
fn impact_scope() -> Scope {
    Scope { tests: true, ..Default::default() }
}

/// The job a line runs in: the one it names, else the open one.
fn in_job(ctx: &Ctx, named: Option<String>) -> Result<Option<InJob>> {
    let asked = named.is_some();
    let Some(id) = ctx.current_job(named)? else { return Ok(None) };
    let folder = crate::readings::job_folder(ctx, &id)?;
    if asked && folder.is_none() {
        return Err(Code::NoSuchJob.refuse(format!("{id}: not in the journal")));
    }
    Ok(folder.map(|f| InJob { id, readings: f.join(custodian_kit::jobs::READINGS_DIR) }))
}

/// A change spec named by its file or by its name in `data/specs/changes/` (`v1.<name>.1o.change`, `<name>.toml`).
pub fn spec_path(ctx: &Ctx, spec: &str) -> Result<PathBuf> {
    let here = std::env::current_dir().unwrap_or_default().join(spec);
    if here.is_file() {
        return Ok(here);
    }
    if let Some(p) = ctx.mode.project() {
        let dir = p.layout.dir::<crate::layout::ChangeSpec>(&[("name", "x")])?;
        for f in [dir.join(custodian_kit::oneo::Doc::Change.file_name(spec)), dir.join(format!("{spec}.toml")), dir.join(spec)] {
            if f.is_file() {
                return Ok(f);
            }
        }
    }
    Err(Code::NoChangeSpec.refuse(format!("`{spec}`: not a file, not in .custodian/data/specs/changes/")))
}

/// `custodian preflight SPEC [--job ID]`.
pub fn preflight(ctx: &mut Ctx, spec: &str, job: Option<String>) -> Result<Ran> {
    let path = spec_path(ctx, spec)?;
    let job = in_job(ctx, job)?;
    crate::budget::before_step(ctx, job.as_ref().map(|j| j.id.as_str()), &format!("preflight {spec}"), custodian_kit::step_time::PREFLIGHT, 1)?;
    let k = crate::jobs::kit(ctx);
    let ran = {
        let ws = ctx.workspace(&impact_scope(), if job.is_some() { Resolving::Job } else { Resolving::Free })?;
        custodian_kit::preflight::run_preflight(&k, &path, job.as_ref(), Runner::InProcess(ws))?
    };
    let p = &ran.preflight;
    let mut out = Ran::named(&p.spec_name);
    out.job = job.map(|j| j.id);
    out.print(p.markdown());
    out.said = ran.said.clone();
    out.outputs = ran.outputs.clone();
    if !p.authorized {
        out.code = 1;
    }
    Ok(out)
}

/// The names a batch list gives, one a line (blank lines and `#` lines left out); read where the line runs, else at
/// the root.
fn list_names(ctx: &Ctx, list: &str) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(std::env::current_dir().unwrap_or_default().join(list)).or_else(|_| std::fs::read_to_string(ctx.root().join(list))).map_err(|e| Code::NoChangeSpec.refuse(format!("{list}: {e}")))?;
    Ok(text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect())
}

/// `custodian preflight --batch LIST [--job ID]`: every change spec LIST names, preflighted as a batch by
/// custodian_kit's `run_preflight_batch` over one open workspace: the code read once (one scan, one code digest, one
/// code graph), each member checked by its own preflight's rules, each member's record kept, one receipt listing each
/// member and its verdict. A name that is no change spec is said and left out.
pub fn preflight_batch(ctx: &mut Ctx, list: &str, job: Option<String>) -> Result<Ran> {
    let names = list_names(ctx, list)?;
    let mut ran = Ran::named(list);
    let mut paths = Vec::new();
    for name in &names {
        match spec_path(ctx, name) {
            Ok(p) => paths.push(p),
            Err(e) => ran.say(format!("left out: {name}: {e}")),
        }
    }
    let job = in_job(ctx, job)?;
    ran.job = job.as_ref().map(|j| j.id.clone());
    crate::budget::before_step(ctx, ran.job.as_deref(), &format!("preflight --batch {list} ({} members)", paths.len()), custodian_kit::step_time::PREFLIGHT, paths.len())?;
    let k = crate::jobs::kit(ctx);
    let list_rel = ctx.rel(&std::env::current_dir().unwrap_or_default().join(list));
    let b = {
        let ws = ctx.workspace(&impact_scope(), if job.is_some() { Resolving::Job } else { Resolving::Free })?;
        custodian_kit::preflight::run_preflight_batch(&k, &list_rel, &paths, job.as_ref(), ws)?
    };
    ran.print(b.batch.markdown());
    ran.said.extend(b.said.clone());
    ran.outputs = b.outputs.clone();
    if b.batch.authorized < names.len() {
        ran.code = 1;
    }
    Ok(ran)
}

/// `custodian transform SPEC [--write] [--job ID]`, `custodian transform --revert RECORD`: code_passenger's
/// `transform` line, through the executor with its receipt (`transform`, the kit's word, so the job's close reads it),
/// on the preflight receipt of this spec over this code.
pub fn transform(ctx: &mut Ctx, spec: Option<&str>, write: bool, revert: Option<&str>, job: Option<String>) -> Result<Ran> {
    transform_with(ctx, spec, write, false, revert, job)
}

/// `custodian transform SPEC [--write] [--tests]`: as `transform`, the crate's tests run on the proof against the
/// same tests before the change with `tests`.
pub fn transform_with(ctx: &mut Ctx, spec: Option<&str>, write: bool, tests: bool, revert: Option<&str>, job: Option<String>) -> Result<Ran> {
    if !crate::maint::passenger_installed() {
        return Err(Code::ProgramMissing.refuse("code-passenger is the writer: cargo install code_passenger"));
    }
    let root = ctx.root().to_path_buf();
    let job = ctx.current_job(job)?;
    let mut ran = Ran::named(spec.or(revert).unwrap_or_default());
    ran.job = job.clone();
    let (words, name, outputs): (Vec<String>, String, Vec<String>) = match (spec, revert) {
        (_, Some(r)) => (vec!["transform".into(), "--revert".into(), r.into()], format!("revert {r}"), Vec::new()),
        (Some(s), None) => {
            crate::budget::before_step(ctx, job.as_deref(), &format!("transform {s}"), custodian_kit::step_time::TRANSFORM, 1)?;
            let path = spec_path(ctx, s)?;
            let change = custodian_kit::change::ChangeSpec::from_path(&path)?;
            let st = store(ctx)?;
            let code = custodian_kit::code_digest(&root, &impact_scope().scan_options()).map_err(|e| Code::LineFailed.refuse(e))?;
            let (d, prev) = custodian_kit::preflight::gate(&st, &change, &code)?;
            ran.say(d.said());
            if !d.go {
                return Err(custodian_kit::reading::refusal::ReadingRefusal::NoPreflight.refusal(format!("{}: {}", change.name, d.reason)));
            }
            // the record where the job keeps its own, else the project's
            let out = match job.as_deref().map(|j| crate::readings::job_folder(ctx, j)).transpose()?.flatten() {
                Some(folder) => folder.join(custodian_kit::jobs::JOB_DATA).join("transforms").join(format!("{}-{}.json", custodian_kit::jobs::slug_of(&change.name), crate::journal::stamp())),
                None => st.path(&custodian_kit::store::named::TRANSFORM, &[("name", &change.name), ("stamp", &crate::journal::stamp())])?,
            };
            let mut w: Vec<String> = vec!["transform".into(), "--spec".into(), path.display().to_string(), "--out".into(), out.display().to_string()];
            if write {
                w.push("--write".into());
            }
            if tests {
                w.push("--tests".into());
            }
            if let Some(p) = prev {
                w.push("--preflight".into());
                w.push(p.seq.to_string());
            }
            (w, change.name.clone(), vec![ctx.rel(&out)])
        }
        (None, None) => return Err(Code::LineRefused.refuse("transform SPEC [--write], or transform --revert RECORD")),
    };
    let line = crate::maint::passenger(&root, words.iter().map(String::as_str));
    let ext = crate::facade::External::of("code-passenger.transform", &line)?.in_dir(&root).env("CODE_PASSENGER_JOB", job.clone().unwrap_or_else(|| "custodian".into()));
    let run = crate::facade::run_external(ctx, ext, crate::facade::ExternalNames { name, job: job.clone(), outputs: outputs.clone() }, |_| {})?;
    ran.print(run.stdout.clone());
    ran.say(format!("{} → {:?} ({}) #{}", run.receipt.line, run.code, run.receipt.reason, run.receipt.seq));
    if !run.stderr.trim().is_empty() {
        ran.say(run.stderr.lines().filter(|l| !l.starts_with("code-passenger:")).collect::<Vec<_>>().join("\n"));
    }
    ran.outputs = outputs;
    ran.code = match run.code {
        Some(0) => 0,
        Some(2) => return Err(Code::StepFailed.refuse(run.stderr.lines().find(|l| l.starts_with('[')).unwrap_or("code-passenger refused the spec").to_string())),
        _ => 1,
    };
    Ok(ran)
}

/// `custodian transform --batch LIST [--after-proof N] [--write] [--tests] [--job ID]`: James's rule for a repeated
/// shape. Each spec of LIST (one a line: a file, or a spec's name) goes on its own preflight receipt over this code (a
/// spec with none, or not authorized, is left out and named); the rest are made by code_passenger's `transform
/// --batch` as one batch (one compile, one expansion per crate, one record per member); then one `transform` receipt
/// per member, its record its output, so the job's close covers each member's lines as it covers a single transform's.
pub fn transform_batch(ctx: &mut Ctx, list: &str, after_proof: Option<&str>, write: bool, tests: bool, job: Option<String>) -> Result<Ran> {
    transform_batch_with(ctx, list, after_proof, write, tests, false, job)
}

/// `transform --batch` with `--preflight`: each member preflighted first in this run, over the one workspace the
/// run holds open (`Ctx::workspace` reuses it while the code does not move), each with its own preflight receipt, so
/// a batch of hundreds parses the index once, not once per member.
pub fn transform_batch_with(ctx: &mut Ctx, list: &str, after_proof: Option<&str>, write: bool, tests: bool, preflight_here: bool, job: Option<String>) -> Result<Ran> {
    if !crate::maint::passenger_installed() {
        return Err(Code::ProgramMissing.refuse("code-passenger is the writer: cargo install code_passenger"));
    }
    let root = ctx.root().to_path_buf();
    let job = ctx.current_job(job)?;
    let names = list_names(ctx, list)?;
    let mut ran = Ran::named(list);
    ran.job = job.clone();
    if preflight_here {
        // the batch preflight: one reading of the code for every member, one receipt listing each verdict
        let started = std::time::Instant::now();
        let b = preflight_batch(ctx, list, job.clone())?;
        for l in b.said.iter().filter(|l| l.starts_with("left out: ")) {
            ran.say(format!("preflight {}", l.trim_start_matches("left out: ")));
        }
        let yes = b.said.iter().find_map(|l| l.strip_prefix("preflights: ").and_then(|r| r.split(' ').next()).and_then(|n| n.parse::<usize>().ok())).unwrap_or(0);
        ran.say(format!("preflights: {} of {} authorized, over one open workspace ({} ms)", yes, names.len(), started.elapsed().as_millis()));
        ran.said.extend(b.said.into_iter().filter(|l| l.starts_with("batch preflight record: ")));
    }
    let st = store(ctx)?;
    let code = custodian_kit::code_digest(&root, &impact_scope().scan_options()).map_err(|e| Code::LineFailed.refuse(e))?;
    let mut members: Vec<(String, std::path::PathBuf, i64)> = Vec::new();
    let mut left: Vec<String> = Vec::new();
    let mut found: Vec<(std::path::PathBuf, custodian_kit::change::ChangeSpec)> = Vec::new();
    for name in &names {
        match spec_path(ctx, name) {
            Ok(path) => {
                let change = custodian_kit::change::ChangeSpec::from_path(&path)?;
                found.push((path, change));
            }
            Err(e) => left.push(format!("{name}: {e}")),
        }
    }
    // each member's gate: its own preflight receipt, or its line in a batch preflight's, the journal read once
    let gates = custodian_kit::preflight::gate_all(&st, &found.iter().map(|(_, c)| c).collect::<Vec<_>>(), &code)?;
    for ((path, change), gate) in found.into_iter().zip(gates) {
        match gate {
            (d, Some(prev)) if d.go => members.push((change.name.clone(), path, prev.seq)),
            (d, _) => left.push(format!("{}: {}", change.name, d.reason)),
        }
    }
    let after = after_proof.map(|a| a.trim_start_matches('#').to_string());
    let label = format!("batch of {}{}", members.len(), after.as_deref().map(|a| format!(" after proof #{a}")).unwrap_or_default());
    ran.say(format!("{label}: {} on their preflights, {} left out", members.len(), left.len()));
    for l in &left {
        ran.say(format!("left out: {l}"));
    }
    if members.is_empty() {
        ran.code = 1;
        return Ok(ran);
    }
    crate::budget::before_step(ctx, job.as_deref(), &label, custodian_kit::step_time::COMPILE, 1)?;
    // the batch's folder: the list code_passenger reads (each spec with its preflight), the members' records
    let stamp = crate::journal::stamp();
    let dir = match job.as_deref().map(|j| crate::readings::job_folder(ctx, j)).transpose()?.flatten() {
        Some(folder) => folder.join(custodian_kit::jobs::JOB_DATA).join("transforms").join(format!("batch-{stamp}")),
        None => st.path(&custodian_kit::store::named::TRANSFORM, &[("name", "batch"), ("stamp", &stamp)])?.with_extension(""),
    };
    std::fs::create_dir_all(&dir).map_err(|e| Code::LineFailed.refuse(format!("{}: {e}", dir.display())))?;
    let list_file = dir.join("list");
    let lines: String = members.iter().map(|(_, p, seq)| format!("{} {seq}\n", p.display())).collect();
    std::fs::write(&list_file, lines).map_err(|e| Code::LineFailed.refuse(format!("{}: {e}", list_file.display())))?;
    let mut w: Vec<String> = vec!["transform".into(), "--batch".into(), list_file.display().to_string(), "--out".into(), dir.display().to_string()];
    if let Some(a) = &after {
        w.push("--after-proof".into());
        w.push(a.clone());
    }
    if write {
        w.push("--write".into());
    }
    if tests {
        w.push("--tests".into());
    }
    let line = crate::maint::passenger(&root, w.iter().map(String::as_str));
    let ext = crate::facade::External::of("code-passenger.transform-batch", &line)?.in_dir(&root).env("CODE_PASSENGER_JOB", job.clone().unwrap_or_else(|| "custodian".into()));
    let run = crate::facade::run_external(ctx, ext, crate::facade::ExternalNames { name: label.clone(), job: job.clone(), outputs: vec![ctx.rel(&list_file)] }, |_| {})?;
    ran.print(run.stdout.clone());
    ran.say(format!("{} → {:?} ({}) #{}", run.receipt.line, run.code, run.receipt.reason, run.receipt.seq));
    if !run.stderr.trim().is_empty() {
        ran.say(run.stderr.lines().filter(|l| !l.starts_with("code-passenger:")).collect::<Vec<_>>().join("\n"));
    }
    // one transform receipt per member, its record its output: the job's close covers each member's lines
    let mut outputs = vec![ctx.rel(&list_file)];
    for (name, _, _) in &members {
        let file = dir.join(format!("{name}.json"));
        let Some(rec) = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<custodian_kit::transform::TransformRecord>(&t).ok()) else { continue };
        let mut r = run.receipt.clone();
        r.seq = 0;
        r.command = "transform".into();
        r.name = format!("{name} ({label} #{})", run.receipt.seq);
        r.outputs = vec![ctx.rel(&file)];
        r.accepted = rec.written && rec.proved() || (!write && rec.proved());
        r.reason = if r.accepted { "done".into() } else { "not proved: left out of the batch".into() };
        r.receipt_digest = custodian_kit::digest::of_value(&(&run.receipt.receipt_digest, name, &rec.files));
        if let Some(f) = ctx.keep(&mut r)? {
            ran.say(format!("{name}: receipt #{} ({})", r.seq, f.display()));
        }
        outputs.push(ctx.rel(&file));
    }
    ran.outputs = outputs;
    ran.code = match run.code {
        Some(0) => 0,
        Some(2) => return Err(Code::StepFailed.refuse(run.stderr.lines().find(|l| l.starts_with('[')).unwrap_or("code-passenger refused the batch").to_string())),
        _ => 1,
    };
    Ok(ran)
}

/// The plan's name from its intent's first words.
fn plan_name(intent: &str) -> String {
    let words: Vec<String> = intent.split_whitespace().take(5).map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).filter(|w| !w.is_empty()).collect();
    custodian_kit::jobs::slug_of(&words.join("-"))
}

/// `custodian plan make INTENT [--name NAME]`, in the running job.
pub fn plan(ctx: &mut Ctx, intent: &str, name: Option<String>) -> Result<Ran> {
    plan_in(ctx, intent, name, None)
}

/// `custodian plan make INTENT [--name NAME] [--job ID]`: the plan made in the job named (the running one by
/// default), so its impact readings are the job's and its decisions are in the job's list.
pub fn plan_in(ctx: &mut Ctx, intent: &str, name: Option<String>, job: Option<String>) -> Result<Ran> {
    let name = name.unwrap_or_else(|| plan_name(intent));
    let job = in_job(ctx, job)?;
    let k = crate::jobs::kit(ctx);
    let ran = {
        let ws = ctx.workspace(&impact_scope(), if job.is_some() { Resolving::Job } else { Resolving::Free })?;
        custodian_kit::plan::run_plan(&k, &name, intent, job.as_ref(), Runner::InProcess(ws))?
    };
    let mut out = Ran::named(&name);
    out.job = job.map(|j| j.id);
    out.print(custodian_kit::plan::markdown(&ran.plan));
    out.said = ran.said.clone();
    out.outputs = ran.outputs.clone();
    if !ran.plan.decisions.is_empty() {
        out.say(format!("{} decisions to make before any step: custodian decide", ran.plan.decisions.len()));
    }
    Ok(out)
}

/// `custodian plan approve PLAN STEP [--operation OP]`, in the running job.
pub fn approve(ctx: &mut Ctx, plan: &str, step: &str, operation: Option<&str>) -> Result<Ran> {
    approve_in(ctx, plan, step, operation, None)
}

/// `custodian plan approve PLAN STEP [--operation OP] [--job ID]`: its receipt the job's (the running one by default).
pub fn approve_in(ctx: &mut Ctx, plan: &str, step: &str, operation: Option<&str>, job: Option<String>) -> Result<Ran> {
    let job = in_job(ctx, job)?.map(|j| j.id);
    let st = store(ctx)?;
    let n: usize = step.parse().map_err(|_| Code::LineRefused.refuse(format!("`{step}` is not a step's number")))?;
    let (spec, p) = custodian_kit::plan::approve(&st, plan, n, operation)?;
    let mut ran = Ran::named(plan);
    ran.job = job;
    ran.print(std::fs::read_to_string(&spec).unwrap_or_default());
    ran.say(format!("plan {plan} step {n} → {} (its TODO filled in, then: custodian preflight {})", ctx.rel(&spec), spec.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
    ran.say(format!("plan {}: {} of {} steps approved", p.name, p.steps.iter().filter(|s| s.status == "approved").count(), p.steps.len()));
    ran.outputs = vec![ctx.rel(&spec)];
    Ok(ran)
}

/// `custodian plan show PLAN`, in the running job.
pub fn show(ctx: &mut Ctx, plan: &str) -> Result<Ran> {
    show_in(ctx, plan, None)
}

/// `custodian plan show PLAN [--job ID]`: its receipt the job's (the running one by default).
pub fn show_in(ctx: &mut Ctx, plan: &str, job: Option<String>) -> Result<Ran> {
    let job = in_job(ctx, job)?.map(|j| j.id);
    let st = store(ctx)?;
    let (p, file) = custodian_kit::plan::load(&st, plan).map_err(|_| Code::NoPlan.refuse(plan))?;
    let mut ran = Ran::named(plan);
    ran.job = job;
    ran.print(custodian_kit::plan::markdown(&p));
    ran.outputs = vec![ctx.rel(&file)];
    Ok(ran)
}

/// `custodian decide`: the open decisions, together; `custodian decide ID ANSWER [--why TEXT]`: one made, and when it
/// is a job's "what does done look like", its answer declared as the job's verification.
pub fn decide(ctx: &mut Ctx, id: Option<&str>, answer: Option<&str>, why: Option<&str>) -> Result<Ran> {
    let st = store(ctx)?;
    let mut ran = Ran::named("decide");
    let (Some(id), Some(answer)) = (id, answer) else {
        let open = custodian_kit::questions::open(&st)?;
        let mut s = format!("# decisions open ({})\n\n", open.len());
        for q in &open {
            s.push_str(&format!("- `{}` {} ({}){}{}\n", q.id, q.question, q.source, if q.options.is_empty() { String::new() } else { format!("\n  - options: {}", q.options.join(" | ")) }, q.why.as_ref().map(|w| format!("\n  - why it matters: {w}")).unwrap_or_default()));
        }
        if open.is_empty() {
            s.push_str("none\n");
        }
        ran.print(s);
        ran.say(format!("{} decisions open; answer one: custodian decide ID ANSWER --why TEXT", open.len()));
        if !open.is_empty() {
            ran.code = 1;
        }
        return Ok(ran);
    };
    let q = custodian_kit::questions::open(&st)?.into_iter().find(|q| q.id == id);
    let d = custodian_kit::questions::answer(&st, id, answer, why, None)?;
    ran.say(format!("decided {}: {} → {} (task history {}); not asked again", d.id, d.question, d.answer, d.task));
    // a job's done question: its answer is the job's verification
    let done_question = q.is_some_and(|q| q.question.starts_with("what does done look like for ") && q.source.starts_with("job:"));
    if done_question && custodian_kit::jobs::check_verify(answer).is_ok() && ctx.current_job(None)?.is_some() {
        let (job, r) = custodian_kit::jobs::declare_verify(&crate::jobs::kit(ctx), None, answer)?;
        ran.say(format!("job {job}: done is {answer} (verify receipt #{})", r.seq));
        ran.job = Some(job);
    }
    Ok(ran)
}

/// `custodian linkage add NAME --kind K --why W --bind X --bind Y ...`.
pub fn linkage_add(ctx: &mut Ctx, name: &str, kind: Option<&str>, why: Option<&str>, binds: &[String]) -> Result<Ran> {
    let st = store(ctx)?;
    let (Some(kind), Some(why), true) = (kind, why, binds.len() >= 2) else {
        return Err(Code::LinkageArgs.refuse(format!("linkage add {name} --kind KIND --why TEXT --bind A --bind B")));
    };
    let kind: custodian_kit::linkage::LinkKind = kind.parse().map_err(|e: String| Code::WordRefused.refuse(e))?;
    let rec = {
        let ws = ctx.workspace(&Scope::default(), Resolving::Free)?;
        custodian_kit::linkage::record(ws.facts(), name, kind, why, binds, None)?
    };
    let file = custodian_kit::linkage::write(&st, &rec)?;
    let mut ran = Ran::named(name);
    ran.say(format!("linkage {name} ({}): {} bound, each resolved now; its evidence: {}", kind.as_str(), rec.binds.join(", "), rec.evidence.join(", ")));
    ran.say(format!("record: {} (an anchor in the code: a change spec with operation anchor)", ctx.rel(&file)));
    ran.outputs = vec![ctx.rel(&file)];
    Ok(ran)
}

/// `custodian linkage check [TARGET]`: the `linkage` reading.
pub fn linkage_check(ctx: &mut Ctx, target: Option<&str>) -> Result<Ran> {
    let ask = crate::readings::Ask { job: ctx.current_job(None)?, ..Default::default() };
    crate::readings::run_one(ctx, Some(target.unwrap_or("*")), Some("linkage"), &ask)
}

/// Writes a change spec into `data/specs/changes/` (in 1o, or its TOML twin when 1o cannot carry a value).
pub fn write_spec(ctx: &Ctx, spec: &custodian_kit::change::ChangeSpec) -> Result<PathBuf> {
    let dir = ctx.layout()?.dir::<crate::layout::ChangeSpec>(&[("name", "x")])?;
    let (file, text) = match spec.to_1o() {
        Ok(t) => (dir.join(custodian_kit::oneo::Doc::Change.file_name(&spec.name)), t),
        Err(_) => (dir.join(format!("{}.toml", spec.name)), spec.to_toml()?),
    };
    write_whole(&file, text.as_bytes())?;
    Ok(file)
}

