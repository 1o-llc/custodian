//! The questions custodian asks before it acts, declared once as xccute decisions. Each gate is a
//! `DecisionGuideTemplate`: a goal, a path whose one required observation names the tool that answers it, and the
//! question that observation answers. The answer comes back as facts and a go or no-go with its reason; the run
//! says it and the receipt keeps the reason.
//!
//! - same search: is there a reading of this spec over this code already? (reuse it: "search #N from <time>")
//! - job running: is a job already running? (a second `job start` is refused)
//! - step input: did the earlier step give this step its input? (triage: a step with nothing to read is skipped)

use crate::journal::{Journal, ReadingRow};
use custodian_kit::api::Receipt;
use xccute_decisions::{DecisionGuideTemplate, DecisionObservationTool, DecisionPathStep, DecisionPathTemplate, DecisionQuestionSpec};
use xccute_runtime::RuntimeObservationKind;

/// The journal as an observation tool: it answers from receipts and the readings index.
pub fn journal_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "custodian.journal",
        program: "custodian",
        kind: RuntimeObservationKind::Custom("journal_query".into()),
        responsibility: "answers from the project's journal: the runs made here, their receipts and the readings index",
        default_question: "has this been answered here before?",
        compact_fact_shape: "#seq utc command name",
        success_meaning: "an earlier run answers",
        empty_meaning: "no earlier run answers",
    }
}

/// The jobs folder as an observation tool.
pub fn jobs_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "custodian.jobs",
        program: "custodian",
        kind: RuntimeObservationKind::Custom("job_state".into()),
        responsibility: "answers which job folders sit in jobs/running",
        default_question: "is a job already running?",
        compact_fact_shape: "job ids in jobs/running",
        success_meaning: "a job is running",
        empty_meaning: "no job is running",
    }
}

/// A job's earlier step receipts as an observation tool.
pub fn steps_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "custodian.job_step",
        program: "custodian",
        kind: RuntimeObservationKind::Custom("job_step_input".into()),
        responsibility: "answers from an earlier step's receipt and reading what a step takes as its input",
        default_question: "did the earlier step give this step its input?",
        compact_fact_shape: "N values from step S, rows R, field F",
        success_meaning: "the step has its input",
        empty_meaning: "the step has nothing to read",
    }
}

/// One gate: its guide, declared once.
fn guide(id: &str, goal: &str, tool: &DecisionObservationTool, question: &str, operation: &str) -> DecisionGuideTemplate {
    let path = DecisionPathTemplate::new(format!("{id}.path"), goal).step(DecisionPathStep::required_observation(
        format!("{id}.observe"),
        tool.tool_id,
        id,
        operation,
        tool.responsibility,
    ));
    DecisionGuideTemplate::new(id, goal, path).ask(DecisionQuestionSpec::required(id, operation, tool, question))
}

/// The same-search gate.
pub fn same_search_guide() -> DecisionGuideTemplate {
    guide("same_search", "spend a search only when the spec or the code changed", &journal_tool(), "is there a reading of this spec over this code already?", "custodian.run")
}

/// The job-running gate.
pub fn job_running_guide() -> DecisionGuideTemplate {
    guide("job_running", "one job open at a time, so its readings are its own", &jobs_tool(), "is a job already running?", "custodian.job")
}

/// The step-input gate.
pub fn step_input_guide() -> DecisionGuideTemplate {
    guide("step_input", "a step that reads an earlier step's receipt runs only on what that step gave", &steps_tool(), "did the earlier step give this step its input?", "custodian.step")
}

/// Every gate, for the README and the test that each guide covers its path.
pub fn guides() -> Vec<DecisionGuideTemplate> {
    vec![same_search_guide(), job_running_guide(), step_input_guide()]
}

/// An answered question.
#[derive(Debug, Clone)]
pub struct Decided {
    pub guide: String,
    pub question: String,
    pub facts: Vec<String>,
    /// Go on as asked.
    pub go: bool,
    pub reason: String,
}

impl Decided {
    fn of(g: &DecisionGuideTemplate, facts: Vec<String>, go: bool, reason: String) -> Decided {
        let question = g.questions.first().map(|q| q.question.clone()).unwrap_or_default();
        Decided { guide: g.guide_id.clone(), question, facts, go, reason }
    }

    /// The line a run says: `decided: <question> <reason>`.
    pub fn said(&self) -> String {
        format!("decided ({}): {} {}", self.guide, self.question, self.reason)
    }
}

/// Same search: the newest reading of this spec over this code (the kit's one key: the spec's hash and the code's
/// digest), unless `again`.
pub fn same_search(journal: &Journal, spec_hash: &str, code_digest: &str, again: bool) -> crate::codes::Result<(Decided, Option<(ReadingRow, Receipt)>)> {
    let g = same_search_guide();
    let prev = journal.same_search(spec_hash, code_digest)?;
    let decided = match (&prev, again) {
        (Some((row, r)), false) => Decided::of(&g, vec![format!("#{} {} {}", row.seq, r.utc, row.name)], false, format!("yes: search #{} from {}, same spec, same code: reused (--again reads again)", row.seq, r.utc)),
        (Some((row, r)), true) => Decided::of(&g, vec![format!("#{} {}", row.seq, r.utc)], true, format!("yes, search #{} from {}; --again: read again", row.seq, r.utc)),
        (None, _) => Decided::of(&g, Vec::new(), true, "no: read".into()),
    };
    Ok((decided, prev))
}

/// Job running: the folders in `jobs/running`.
pub fn job_running(running: &[String]) -> Decided {
    let g = job_running_guide();
    if running.is_empty() {
        Decided::of(&g, Vec::new(), true, "no: start".into())
    } else {
        Decided::of(&g, running.to_vec(), false, format!("yes, {}: close it first (custodian job close)", running.join(", ")))
    }
}

/// Step input: the values an earlier step gave.
pub fn step_input(step: &str, from: &str, values: &[String]) -> Decided {
    let g = step_input_guide();
    if values.is_empty() {
        Decided::of(&g, Vec::new(), false, format!("no: step {from} gave step {step} nothing; {step} is skipped"))
    } else {
        Decided::of(&g, vec![format!("{} values from {from}", values.len())], true, format!("yes: {} values from step {from}", values.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gate_covers_its_path() {
        for g in guides() {
            g.validate_path_coverage().unwrap_or_else(|e| panic!("{}: {e:?}", g.guide_id));
            assert_eq!(g.required_questions().count(), 1);
        }
        let d = job_running(&["j-1".into()]);
        assert!(!d.go);
        assert_eq!(d.said(), "decided (job_running): is a job already running? yes, j-1: close it first (custodian job close)");
        assert!(step_input("reach", "codes", &[]).reason.contains("skipped"));
    }
}
