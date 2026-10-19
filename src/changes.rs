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
    let Some(id) = ctx.current_job(named)? else { return Ok(None) };
    let folder = crate::readings::job_folder(ctx, &id)?;
    Ok(folder.map(|f| InJob { id, readings: f.join("readings") }))
}

/// A change spec named by its file or by its name in `data/specs/changes/` (`v1.<name>.1o.change`, `<name>.toml`).
pub fn spec_path(ctx: &Ctx, spec: &str) -> Result<PathBuf> {
    let here = std::env::current_dir().unwrap_or_default().join(spec);
    if here.is_file() {
        return Ok(here);
    }
    if let Some(p) = ctx.mode.project() {
        let dir = p.layout.root.join(crate::layout::FOLDER).join("data/specs/changes");
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

/// `custodian transform SPEC [--write] [--job ID]`, `custodian transform --revert RECORD`: code_passenger's
/// `transform` line, through the executor with its receipt (`transform`, the kit's word, so the job's close reads it),
/// on the preflight receipt of this spec over this code.
pub fn transform(ctx: &mut Ctx, spec: Option<&str>, write: bool, revert: Option<&str>, job: Option<String>) -> Result<Ran> {
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
                Some(folder) => folder.join("data/transforms").join(format!("{}-{}.json", custodian_kit::jobs::slug_of(&change.name), crate::journal::stamp())),
                None => st.path(&custodian_kit::store::named::TRANSFORM, &[("name", &change.name), ("stamp", &crate::journal::stamp())])?,
            };
            let mut w: Vec<String> = vec!["transform".into(), "--spec".into(), path.display().to_string(), "--out".into(), out.display().to_string()];
            if write {
                w.push("--write".into());
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

/// The plan's name from its intent's first words.
fn plan_name(intent: &str) -> String {
    let words: Vec<String> = intent.split_whitespace().take(5).map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).filter(|w| !w.is_empty()).collect();
    custodian_kit::jobs::slug_of(&words.join("-"))
}

/// `custodian plan make INTENT [--name NAME]`.
pub fn plan(ctx: &mut Ctx, intent: &str, name: Option<String>) -> Result<Ran> {
    let name = name.unwrap_or_else(|| plan_name(intent));
    let job = in_job(ctx, None)?;
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

/// `custodian plan approve PLAN STEP [--operation OP]`.
pub fn approve(ctx: &mut Ctx, plan: &str, step: &str, operation: Option<&str>) -> Result<Ran> {
    let st = store(ctx)?;
    let n: usize = step.parse().map_err(|_| Code::LineRefused.refuse(format!("`{step}` is not a step's number")))?;
    let (spec, p) = custodian_kit::plan::approve(&st, plan, n, operation)?;
    let mut ran = Ran::named(plan);
    ran.print(std::fs::read_to_string(&spec).unwrap_or_default());
    ran.say(format!("plan {plan} step {n} → {} (its TODO filled in, then: custodian preflight {})", ctx.rel(&spec), spec.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
    ran.say(format!("plan {}: {} of {} steps approved", p.name, p.steps.iter().filter(|s| s.status == "approved").count(), p.steps.len()));
    ran.outputs = vec![ctx.rel(&spec)];
    Ok(ran)
}

/// `custodian plan show PLAN`.
pub fn show(ctx: &mut Ctx, plan: &str) -> Result<Ran> {
    let st = store(ctx)?;
    let (p, file) = custodian_kit::plan::load(&st, plan).map_err(|_| Code::NoPlan.refuse(plan))?;
    let mut ran = Ran::named(plan);
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
    let dir = ctx.layout()?.folder().join("data/specs/changes");
    let (file, text) = match spec.to_1o() {
        Ok(t) => (dir.join(custodian_kit::oneo::Doc::Change.file_name(&spec.name)), t),
        Err(_) => (dir.join(format!("{}.toml", spec.name)), spec.to_toml()?),
    };
    write_whole(&file, text.as_bytes())?;
    Ok(file)
}

