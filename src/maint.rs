//! The maintenance loop: `custodian check` runs maintenance's own declared lines, `custodian annotate` builds the
//! plan of attributes the receipts call for, `custodian features` writes code_passenger's feature-gating spec from
//! custodian_kit's eligibility reading. custodian never edits source: what writes is code_passenger.

use crate::codes::{Code, Result};
use crate::facade::{self, Ctx, External, ExternalNames, Ran};
use crate::journal::stamp;
use crate::layout::{self, write_whole};
use custodian_kit::api::{Resolving, Scope};
use maintenance::runner::{CheckName, CheckRun, Maintenance};

/// `custodian check KIND|all`: maintenance's line `maintenance check <kind> --workspace <root>`, run as maintenance
/// runs it (its runner, in this process), its JSON kept beside the receipts.
pub fn check(ctx: &mut Ctx, which: &str, job: Option<String>) -> Result<Ran> {
    let name = CheckName::of_word(which).ok_or_else(|| Code::LineRefused.refuse(format!("{which} is not a check")))?;
    let job = ctx.current_job(job)?;
    let root = ctx.root().to_path_buf();
    let line = Maintenance::check_of(name, &root);
    let ext = External::of("maintenance.check", &line)?;
    let started = std::time::Instant::now();
    let out = maintenance::runner::execute(&line);
    let millis = started.elapsed().as_millis() as i64;
    let mut ran = Ran::named(format!("check {which}"));
    ran.job = job.clone();
    let mut outputs = Vec::new();
    if let Some(p) = ctx.mode.project() {
        let f = p.layout.path::<layout::CheckRunFile>(&[("stamp", &stamp())])?;
        write_whole(&f, out.stdout.as_bytes())?;
        outputs.push(p.layout.rel(&f));
    }
    let run = facade::finish_external(ctx, &ext, ExternalNames { name: format!("check {which}"), job, outputs: outputs.clone() }, Some(out.code), out.stdout.clone(), String::new(), millis)?;
    ran.say(format!("{} → {} ({}){}", run.receipt.line, out.code, out.word, run.receipt_file.as_ref().map(|f| format!(" · receipt #{} {}", run.receipt.seq, ctx.rel(f))).unwrap_or_default()));
    match serde_json::from_str::<CheckRun>(&out.stdout) {
        Ok(cr) => {
            let mut page = format!("# check {which}\n\n");
            for r in &cr.reports {
                page.push_str(&format!("## {} — {} ({} findings, {} notes)\n\n", r.check, if r.passed { "clean" } else { "findings" }, r.findings.len(), r.notes.len()));
                for f in r.findings.iter().chain(&r.notes) {
                    page.push_str(&format!("- {}{}\n", f.message, f.file.as_ref().map(|p| format!(" ({p}:{})", f.line.unwrap_or(0))).unwrap_or_default()));
                }
                page.push('\n');
            }
            ran.print(page);
            let findings: usize = cr.reports.iter().map(|r| r.findings.len()).sum();
            ran.say(format!("{} check(s): {} clean, {findings} findings", cr.reports.len(), cr.reports.iter().filter(|r| r.passed).count()));
        }
        Err(_) => ran.print(out.stdout.clone()),
    }
    for o in outputs {
        ran.say(format!("the check's JSON: {o}"));
    }
    ran.code = match out.code {
        0 => 0,
        1 => 1,
        _ => return Err(Code::CheckRefused.refuse(out.stdout.lines().find(|l| l.contains("MAINT")).unwrap_or("").trim())),
    };
    ran.outputs = run.receipt.outputs.clone();
    Ok(ran)
}

/// `custodian annotate [--from CHECK_JSON] [--apply]`: the plan of the maintenance attributes the receipts call for
/// (`#[timed_test(max)]` from measured times, `#[reads_env]` where a check found a bare read, candidates listed),
/// read from the latest check's receipt unless `--from` names another. `--apply` hands it to code_passenger, which
/// has no line that places an attribute yet, so it is refused with its code and the plan is kept.
pub fn annotate(ctx: &mut Ctx, from: Option<&str>, apply: bool) -> Result<Ran> {
    let mut ran = Ran::named("annotate");
    let rule_text = match ctx.mode.project() {
        Some(p) => std::fs::read_to_string(p.layout.path::<layout::ConfigFile>(&[("name", "annotate")])?).unwrap_or_else(|_| custodian_kit::annotate::SHIPPED_ANNOTATE.into()),
        None => custodian_kit::annotate::SHIPPED_ANNOTATE.into(),
    };
    let rule = custodian_kit::annotate::AnnotateRule::parse(&rule_text)?;
    let check_file = match from {
        Some(f) => Some(std::path::PathBuf::from(f)),
        None => latest_check(ctx)?,
    };
    let check_json = match &check_file {
        Some(f) => Some(std::fs::read_to_string(f).map_err(|e| Code::ReadingUnreadable.refuse(format!("{}: {e}", f.display())))?),
        None => None,
    };
    let plan = custodian_kit::annotate::plan(ctx.root(), &rule, check_json.as_deref())?;
    ran.print(custodian_kit::annotate::page(&plan));
    ran.say(format!(
        "planned {} insert(s) and {} candidate(s) from {} and maintenance's timings",
        plan.inserts.len(),
        plan.candidates.len(),
        check_file.as_ref().map(|f| ctx.rel(f)).unwrap_or_else(|| "no check receipt".into())
    ));
    if let Some(p) = ctx.mode.project() {
        let f = p.layout.path::<layout::PlanFile>(&[("name", "annotate.toml")])?;
        write_whole(&f, toml::to_string(&plan).map_err(|e| Code::Write.refuse(e))?.as_bytes())?;
        ran.say(format!("plan: {}", p.layout.rel(&f)));
        ran.outputs.push(p.layout.rel(&f));
    }
    if apply {
        return Err(Code::NoWriterLine.refuse("annotate --apply: code_passenger's writers are file headers (annotate), #[cfg] gates and manifest features (gate); none places an attribute from a plan. The plan is kept."));
    }
    Ok(ran)
}

/// The newest check JSON a receipt names.
fn latest_check(ctx: &Ctx) -> Result<Option<std::path::PathBuf>> {
    let Some(j) = ctx.journal.as_ref() else { return Ok(None) };
    Ok(j.receipts()?.into_iter().rev().filter(|r| r.command == "maintenance.check").find_map(|r| r.outputs.first().map(|o| ctx.root().join(o))))
}

/// `custodian features [--apply]`: each crate's dependencies judged primary or feature-eligible by the rule in
/// `config/features.toml` (custodian_kit's eligibility reading), code_passenger's `klepto.toml` spec written for the
/// eligible, and with `--apply` code_passenger's gate run on it as its own declared line, with `--write`.
pub fn features(ctx: &mut Ctx, apply: bool) -> Result<Ran> {
    let mut ran = Ran::named("features");
    let rule_text = match ctx.mode.project() {
        Some(p) => std::fs::read_to_string(p.layout.path::<layout::ConfigFile>(&[("name", "features")])?).unwrap_or_else(|_| custodian_kit::features::SHIPPED_THRESHOLDS.into()),
        None => custodian_kit::features::SHIPPED_THRESHOLDS.into(),
    };
    let rule = custodian_kit::features::Thresholds::parse(&rule_text)?;
    let facts = custodian_kit::reading::facts::Facts::build(ctx.root(), &Scope::default(), Resolving::Free)?;
    let judged = custodian_kit::features::judge(&facts, &rule);
    let spec = custodian_kit::features::spec_for(&facts, &judged);
    let mut page = String::from("# features — primary constituents and feature-eligible dependencies\n\n");
    let eligible = judged.iter().filter(|j| j.eligible).count();
    page.push_str(&format!("- judged: {}\n- feature-eligible: {eligible}\n\n", judged.len()));
    for j in &judged {
        page.push_str(&format!("- **{}** in {}: {} — {}\n", j.dependency, j.package, if j.eligible { "feature-eligible" } else { "primary or not judged" }, j.why));
    }
    let spec_text = toml::to_string(&spec).map_err(|e| Code::Write.refuse(e))?;
    page.push_str(&format!("\n## code_passenger's spec (klepto.toml)\n\n```toml\n{spec_text}```\n"));
    ran.print(page);
    ran.say(format!("judged {} dependencies: {eligible} feature-eligible", judged.len()));
    let spec_file = match ctx.mode.project() {
        Some(p) => {
            let f = p.layout.path::<layout::PlanFile>(&[("name", "klepto.toml")])?;
            write_whole(&f, spec_text.as_bytes())?;
            ran.say(format!("code_passenger spec: {}", p.layout.rel(&f)));
            ran.outputs.push(p.layout.rel(&f));
            Some(f)
        }
        None => None,
    };
    if apply {
        let f = spec_file.ok_or_else(|| Code::NeedsFolder.refuse("features --apply keeps its spec in .custodian/"))?;
        if spec.is_empty() {
            ran.say("nothing eligible: code_passenger has nothing to gate");
            return Ok(ran);
        }
        let root = ctx.root().to_path_buf();
        for pkg in spec.keys() {
            let line = custodian_kit::exec::passenger_gate(&root, &f, true).words(["--package", pkg.as_str()]);
            let ext = External::of("code-passenger.gate", &line)?.in_dir(&root);
            let run = facade::run_external(ctx, ext, ExternalNames { name: format!("gate {pkg}"), job: None, outputs: vec![ctx.rel(&f)] }, |_| {})?;
            ran.say(format!("{} → {:?} ({}) #{}", run.receipt.line, run.code, run.receipt.reason, run.receipt.seq));
            if !run.accepted {
                ran.code = 1;
            }
        }
    }
    Ok(ran)
}
