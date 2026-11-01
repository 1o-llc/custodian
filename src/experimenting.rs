//! `custodian experiment run SPEC [STORAGE] [--job JOB]`: an `experiments` run hosted by custodian. The experiment's
//! runner decides what runs and in which order; custodian carries out each operation as its own: every protocol,
//! measurement and verification command is a declared argv line run through the facade with its receipt, and every
//! bounded change goes through custodian's `preflight` and `transform` lines (code_passenger writes it), each with
//! its receipt and the transform record a job's close counts. The nested lines run in this process under the writer
//! the `experiment run` line already holds; none of them asks for it again.
//!
//! The result is the experiment's own `ExperimentResult`, kept where `ExperimentPaths` lays it out under the run's
//! storage: STORAGE when named, else the job's `data/experiments`, else the project's `data/experiments`. Its path
//! is this line's output. The status is custodian's: done (0) when the experiment promotes, findings (1) when it
//! does not promote or is undecided (the scientific conclusion is in the result, never an execution failure), and
//! refused (2) when the experiment could not run to its conclusion.
//!
//! Once the result is written, the journal gets an `experiment` event naming it: its subject the experiment's id, its
//! state the promotion, its detail the result's path exactly as this line's output names it. A task waiting for the
//! experiment is woken from that event ([`crate::observe`]) and reads the result back through [`latest_result`].

use crate::codes::{Code, Refusal, Result};
use crate::facade::{self, Ctx, External, ExternalNames, Ran};
use crate::surface::{self, s, so, ExperimentRun, Line};
use experiments::{ChangeRequest, CommandRecord, CommandSpec, Evidence, ExperimentError, ExperimentHost, ExperimentResult, ExperimentRunner, MutationRecord, PromotionDecision};
use std::path::{Path, PathBuf};

pub fn run(ctx: &mut Ctx, l: &ExperimentRun) -> Result<Ran> {
    let job = ctx.current_job(so(&l.job))?;
    let here = std::env::current_dir().unwrap_or_default();
    let spec_file = here.join(s(&l.spec));
    let mut spec = experiments::read_spec(&spec_file).map_err(|e| Code::ExperimentRefused.refuse(format!("{}: {e}", spec_file.display())))?;
    // a relative target is the project's, as every path a hosted line names is (the resident runs elsewhere)
    if spec.target.root.is_relative() {
        spec.target.root = ctx.root().join(&spec.target.root);
    }
    let storage = match so(&l.storage) {
        Some(st) => here.join(st),
        None => storage_of(ctx, job.as_deref())?,
    };
    let mut ran = Ran::named(spec.id.clone());
    ran.job = job.clone();
    let mut host = Hosted { ctx, job: job.clone(), refusal: None, said: Vec::new() };
    let outcome = ExperimentRunner::new(&storage).run_with(&spec, &mut host);
    let (said, refusal) = (std::mem::take(&mut host.said), host.refusal.take());
    ran.said.extend(said);
    let (result, path) = match outcome {
        Ok(done) => done,
        // an operation custodian refused (a preflight, the writer, a program missing) is refused as itself
        Err(e) => return Err(refusal.unwrap_or_else(|| Code::ExperimentRefused.refuse(format!("{}: {e}", spec.id)))),
    };
    ran.say(format!("experiment {} run {}: {} commands, {} changes, {} evidence", result.experiment_id, result.run_id, result.commands.len(), result.mutations.len(), result.evidence.len()));
    ran.say(format!("scientific outcome: {:?} · promotion: {:?} · verification {}", result.evaluation.conclusion.outcome, result.promotion, if result.verification.passed { "passed" } else { "failed" }));
    let kept = ctx.rel(&path);
    ran.say(format!("result: {kept}"));
    // the result is written: the change that a waiting task is woken for, naming the output this line keeps
    if let Some(j) = ctx.journal.as_ref() {
        j.event(EVENT, job.as_deref(), &result.experiment_id, promotion_word(result.promotion), &kept, None)?;
    }
    ran.outputs = vec![kept];
    ran.code = status_of(result.promotion);
    Ok(ran)
}

/// The journal event a hosted run's result makes.
pub const EVENT: &str = "experiment";

/// The promotion as the result writes it (`experiments`' own words).
pub fn promotion_word(p: PromotionDecision) -> &'static str {
    match p {
        PromotionDecision::Promote => "promote",
        PromotionDecision::DoNotPromote => "do_not_promote",
        PromotionDecision::Undecided => "undecided",
    }
}

/// Experiment `id`'s latest hosted result, read from what custodian keeps: the path its newest `experiment` event
/// names, which must be the output of an `experiment run` receipt of that experiment, and the result read there.
/// `None` while it has none.
pub fn latest_result(ctx: &Ctx, id: &str) -> Result<Option<(String, ExperimentResult)>> {
    let journal = ctx.journal()?;
    let Some(e) = journal.events_since(0)?.into_iter().rev().find(|e| e.kind == EVENT && e.subject == id) else { return Ok(None) };
    let kept = journal.receipts()?.into_iter().any(|r| r.command == EVENT && r.name == id && r.outputs.contains(&e.detail));
    if !kept {
        return Err(Code::ExperimentRefused.refuse(format!("{id}: the event names {}, which no `experiment run` receipt kept", e.detail)));
    }
    let result = experiments::read_result(ctx.root().join(&e.detail)).map_err(|x| Code::ExperimentRefused.refuse(format!("{}: {x}", e.detail)))?;
    Ok(Some((e.detail, result)))
}

/// Custodian's status for an experiment that ran to its conclusion: promote is done; do-not-promote and undecided
/// are findings (the conclusion is the result's, not a failed run).
pub fn status_of(promotion: PromotionDecision) -> i32 {
    match promotion {
        PromotionDecision::Promote => 0,
        PromotionDecision::DoNotPromote | PromotionDecision::Undecided => 1,
    }
}

/// Where a hosted run keeps its storage when the line names none: the job's data folder, else the project's.
fn storage_of(ctx: &Ctx, job: Option<&str>) -> Result<PathBuf> {
    if let Some(folder) = job.map(|j| crate::readings::job_folder(ctx, j)).transpose()?.flatten() {
        return Ok(folder.join(custodian_kit::jobs::JOB_DATA).join("experiments"));
    }
    match ctx.mode.project() {
        // the project's data folder: where its receipts are kept
        Some(p) => Ok(p.layout.dir::<crate::layout::ReceiptFile>(&[("seq", "0")])?.parent().map(|d| d.join("experiments")).unwrap_or_default()),
        None => Ok(std::env::current_dir().unwrap_or_default().join(".experiment-storage")),
    }
}

/// The experiment's operations, carried out by custodian.
struct Hosted<'c> {
    ctx: &'c mut Ctx,
    job: Option<String>,
    /// The first refusal an operation met: the line is refused as it.
    refusal: Option<Refusal>,
    said: Vec<String>,
}

impl Hosted<'_> {
    fn refused(&mut self, r: Refusal) -> String {
        let text = r.to_string();
        self.refusal.get_or_insert(r);
        text
    }
}

impl ExperimentHost for Hosted<'_> {
    fn run_command(&mut self, phase: &str, command: &CommandSpec, cwd: &Path) -> std::result::Result<CommandRecord, ExperimentError> {
        let spec = xccute_runtime::CommandSpec::new(command.program.clone()).args(command.args.clone());
        let ext = External::argv(&format!("experiment.{phase}"), &spec).in_dir(cwd);
        let display = ext.preview.display().to_string();
        let names = ExternalNames { name: format!("{phase} {}", command.label), job: self.job.clone(), outputs: Vec::new() };
        let run = facade::run_external(self.ctx, ext, names, |_| {}).map_err(|r| ExperimentError::Contract { label: command.label.clone(), detail: self.refused(r) })?;
        self.said.push(format!("{phase} {}: {display} → {:?} #{}", command.label, run.code, run.receipt.seq));
        Ok(CommandRecord {
            phase: phase.to_owned(),
            label: command.label.clone(),
            program: command.program.clone(),
            args: command.args.clone(),
            cwd: cwd.to_path_buf(),
            display,
            status: run.code.unwrap_or(-1),
            stdout: run.stdout,
            stderr: run.stderr,
        })
    }

    fn apply_change(&mut self, change: &ChangeRequest) -> std::result::Result<(MutationRecord, Vec<Evidence>), String> {
        // custodian governs the project it runs in: a change to another tree is no change it can authorize
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        if canon(&change.root) != canon(self.ctx.root()) {
            return Err(self.refused(Code::ExperimentRefused.refuse(format!("change {}: the target {} is not the project custodian governs ({})", change.label, change.root.display(), self.ctx.root().display()))));
        }
        let spec: std::ffi::OsString = change.spec.clone().into_os_string();
        let job = self.job.clone().map(Into::into);
        // the change decided by custodian's own preflight, then made by its own transform line (code_passenger the
        // writer): both are declared lines run here, under the writer this run already holds
        let pre = facade::execute(self.ctx, &Line::Preflight(surface::Preflight { spec: spec.clone(), batch: false, job: job.clone() }), crate::cli::handle);
        self.said.push(format!("change {}: preflight receipt #{} → {} ({})", change.label, pre.receipt.seq, pre.code, pre.receipt.reason));
        if pre.code != 0 {
            let r = pre.refusal.clone().unwrap_or_else(|| Code::ExperimentRefused.refuse(format!("change {}: preflight #{} did not authorize {}: {}", change.label, pre.receipt.seq, change.spec.display(), pre.said.join(" · "))));
            return Err(self.refused(r));
        }
        let tr = facade::execute(self.ctx, &Line::Transform(surface::Transform { spec: Some(spec), write: true, job, ..surface::Transform::default() }), crate::cli::handle);
        self.said.push(format!("change {}: transform receipt #{} → {} ({})", change.label, tr.receipt.seq, tr.code, tr.receipt.reason));
        let record = tr.receipt.outputs.first().map(|o| self.ctx.root().join(o));
        if tr.code != 0 {
            let r = tr.refusal.clone().unwrap_or_else(|| Code::ExperimentRefused.refuse(format!("change {}: transform #{} → {}: {}", change.label, tr.receipt.seq, tr.code, tr.said.join(" · "))));
            return Err(self.refused(r));
        }
        let root = self.ctx.root().to_path_buf();
        let mut evidence: Vec<Evidence> = [("custodian_preflight_receipt", &pre.receipt_file), ("custodian_transform_receipt", &tr.receipt_file)]
            .into_iter()
            .filter_map(|(kind, file)| file.as_ref().map(|f| Evidence { kind: kind.into(), name: change.label.clone(), path: root.join(f) }))
            .collect();
        if let Some(r) = &record {
            evidence.insert(0, Evidence { kind: "code_passenger_transform".into(), name: change.label.clone(), path: r.clone() });
        }
        let m = MutationRecord {
            label: change.label.clone(),
            backend: change.backend,
            spec: change.spec.clone(),
            display: tr.receipt.line.clone(),
            status: tr.code,
            status_word: tr.receipt.reason.clone(),
            stdout: tr.printed.join("\n"),
            stderr: tr.said.join("\n"),
            transform_record: record,
        };
        Ok((m, evidence))
    }
}
