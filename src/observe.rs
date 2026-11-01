//! The observer set (ob-serve-a-bowl): a run's state (job states, step states, test progress) is a cold observable,
//! every change is published with a wake key whose field mask says what changed, and the `--observe` level is the
//! cold observer that decides what is shown: `failed` wakes for a failed test or step and a job's state, `progress`
//! also for each test line's counts and each step, `all` for every line. `custodian test --observe` and
//! `custodian watch` show what the observer is woken for, and nothing else.
//!
//! An experiment's result is a change too: a hosted run's journal event names the result its line kept (the path
//! `ExperimentPaths` chose, the receipt's output), and a protocol task waiting for that experiment is woken through
//! ob-serve-a-bowl's protocol bridge on waker_protocol's scheduler ([`Observers::wait_for_experiment`],
//! [`Observers::feed`]). The wake says only that the experiment has a result; the task reads it from the journal and
//! the receipt.

use crate::codes::Result;
use crate::facade::{Ctx, Ran};
use crate::journal::Event;
use crate::surface::{so, Watch};
use crate::words::ObserveLevel;
use ob_serve_a_bowl::{cold_condition_matchers, ColdObservableRuntime, ColdObserverConfig, ObservableState, ObserverId, PayloadRef, ProtocolBridgeReport, StateTransition, TargetTaskId, WakeKey64, WakeProfileCode, WakeReasonCode};
use std::collections::BTreeMap;

/// A job's state changed (queued, running, resumed, steps-done, done).
pub const FIELD_JOB: u16 = 1 << 0;
/// A step started, finished or was skipped.
pub const FIELD_STEP: u16 = 1 << 1;
/// A test line's counts moved.
pub const FIELD_PROGRESS: u16 = 1 << 2;
/// A test or a step failed.
pub const FIELD_FAILED: u16 = 1 << 3;
/// Any other line a run shows.
pub const FIELD_LINE: u16 = 1 << 4;
/// A hosted experiment has a result.
pub const FIELD_EXPERIMENT: u16 = 1 << 5;

/// An experiment as the observers see it: its latest run's disposition and the result that run's line kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentState {
    pub job: Option<String>,
    /// The promotion (`promote`, `do_not_promote`, `undecided`): every one is terminal.
    pub disposition: String,
    /// The result's path as the line's receipt names it (relative to the root): never made here.
    pub result: String,
}

/// What a run is doing, as the observers see it.
#[derive(Debug, Default, Clone)]
pub struct RunState {
    pub jobs: BTreeMap<String, String>,
    /// Each experiment with a result, by its id.
    pub experiments: BTreeMap<String, ExperimentState>,
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
    /// The text of the last change, which a woken observer shows.
    pub last: String,
}

/// A change.
#[derive(Debug, Clone)]
pub enum RunEvent {
    Job { job: String, state: String, detail: String },
    Step { job: String, step: String, state: String, detail: String },
    Test { name: String, outcome: String },
    Counts { line: String, passed: u64, failed: u64, ignored: u64 },
    /// A hosted run of experiment `experiment` kept its result at `result` (the journal event's detail).
    Experiment { job: Option<String>, experiment: String, disposition: String, result: String },
    Line(String),
}

impl ObservableState for RunState {
    type Event = RunEvent;

    fn apply(&mut self, e: &RunEvent) -> StateTransition {
        match e {
            RunEvent::Job { job, state, detail } => {
                self.jobs.insert(job.clone(), state.clone());
                self.last = format!("job {job} {state}: {detail}");
                StateTransition::changed(FIELD_JOB)
            }
            RunEvent::Step { job, step, state, detail } => {
                self.last = format!("job {job} step {step} {state}: {detail}");
                StateTransition::changed(if state == "failed" { FIELD_STEP | FIELD_FAILED } else { FIELD_STEP })
            }
            RunEvent::Test { name, outcome } => {
                match outcome.as_str() {
                    "ok" => self.passed += 1,
                    "ignored" => self.ignored += 1,
                    _ => self.failed += 1,
                }
                self.last = format!("test {name} ... {outcome}");
                StateTransition::changed(if outcome == "ok" || outcome == "ignored" { FIELD_LINE } else { FIELD_FAILED | FIELD_LINE })
            }
            RunEvent::Counts { line, passed, failed, ignored } => {
                self.last = format!("{line}: {passed} passed, {failed} failed, {ignored} ignored");
                StateTransition::changed(FIELD_PROGRESS)
            }
            RunEvent::Experiment { job, experiment, disposition, result } => {
                self.experiments.insert(experiment.clone(), ExperimentState { job: job.clone(), disposition: disposition.clone(), result: result.clone() });
                self.last = format!("experiment {experiment} {disposition}: {result}");
                StateTransition::changed(FIELD_EXPERIMENT)
            }
            RunEvent::Line(l) => {
                self.last = l.clone();
                StateTransition::changed(FIELD_LINE)
            }
        }
    }
}

fn mask(e: &RunEvent) -> u16 {
    match e {
        RunEvent::Job { .. } => FIELD_JOB,
        RunEvent::Step { state, .. } if state == "failed" => FIELD_STEP | FIELD_FAILED,
        RunEvent::Step { .. } => FIELD_STEP,
        RunEvent::Test { outcome, .. } if outcome != "ok" && outcome != "ignored" => FIELD_FAILED | FIELD_LINE,
        RunEvent::Test { .. } | RunEvent::Line(_) => FIELD_LINE,
        RunEvent::Counts { .. } => FIELD_PROGRESS,
        RunEvent::Experiment { .. } => FIELD_EXPERIMENT,
    }
}

/// The fields an observe level is woken for.
pub fn interest(level: ObserveLevel) -> u16 {
    match level.word() {
        "failed" => FIELD_FAILED | FIELD_JOB | FIELD_EXPERIMENT,
        "progress" => FIELD_FAILED | FIELD_JOB | FIELD_EXPERIMENT | FIELD_STEP | FIELD_PROGRESS,
        _ => FIELD_FAILED | FIELD_JOB | FIELD_EXPERIMENT | FIELD_STEP | FIELD_PROGRESS | FIELD_LINE,
    }
}

/// The observer set of one run.
pub struct Observers {
    rt: ColdObservableRuntime<RunState>,
    pub level: ObserveLevel,
}

impl Observers {
    /// The set with the one cold observer `level` names.
    pub fn new(level: ObserveLevel) -> Observers {
        let mut rt = ColdObservableRuntime::new(RunState::default());
        let want = interest(level);
        let name: &'static str = match level.word() {
            "failed" => "observe-failed",
            "progress" => "observe-progress",
            _ => "observe-all",
        };
        rt.register_cold(
            ColdObserverConfig::new(name, WakeProfileCode::InlineQueue, move |_s: &RunState, t: &StateTransition| t.touches(want))
                .with_interest_mask(want)
                .with_matchers(cold_condition_matchers(1, u128::from(want)))
                .wake_every_match(),
        );
        Observers { rt, level }
    }

    /// Publishes a change; the lines the observer was woken for.
    pub fn publish(&mut self, e: RunEvent) -> Vec<String> {
        let key = match WakeKey64::typed(WakeReasonCode::Condition, WakeProfileCode::InlineQueue, 3, mask(&e), 1, WakeKey64::FLAG_COLD as u16) {
            Ok(k) => k,
            Err(_) => return Vec::new(),
        };
        self.rt.publish_stateful(key, PayloadRef::default(), e);
        let woke = self.rt.drain_cold_inline();
        woke.iter().map(|_| self.rt.state().last.clone()).collect()
    }

    pub fn state(&self) -> &RunState {
        self.rt.state()
    }

    /// A cold observer for one protocol task: woken once, on the experiment class of change, the first time
    /// experiment `experiment` has a result (a rising edge: an unchanged state never wakes it again). The wake goes to
    /// `task` through [`Observers::send_wakes`]; it carries nothing of the result.
    pub fn wait_for_experiment(&mut self, experiment: &str, task: waker_protocol::TaskId) -> ObserverId {
        let id = experiment.to_owned();
        self.rt.register_cold(
            ColdObserverConfig::new("experiment-result", WakeProfileCode::ConditionCache, move |s: &RunState, t: &StateTransition| t.touches(FIELD_EXPERIMENT) && s.experiments.contains_key(&id))
                .with_interest_mask(FIELD_EXPERIMENT)
                .with_matchers(cold_condition_matchers(1, u128::from(FIELD_EXPERIMENT)))
                .with_target(TargetTaskId::from(task))
                .wake_on_rising_edge(),
        )
    }

    /// The targeted wakes the cold observers made, sent to the scheduler through ob-serve-a-bowl's protocol bridge.
    pub fn send_wakes(&mut self, handle: &waker_protocol::SchedulerHandle) -> ProtocolBridgeReport {
        self.rt.send_cold_condition_protocol_wakes(handle)
    }

    /// The journal's events after `since`, published; the targeted wakes sent to the scheduler. `since` moves to the
    /// last event read.
    pub fn feed(&mut self, journal: &crate::journal::Journal, since: &mut i64, handle: &waker_protocol::SchedulerHandle) -> Result<ProtocolBridgeReport> {
        for e in journal.events_since(*since)? {
            *since = e.n;
            self.publish(event_of(&e));
        }
        Ok(self.send_wakes(handle))
    }
}

/// A journal event as a change.
pub fn event_of(e: &Event) -> RunEvent {
    let job = e.job.clone().unwrap_or_default();
    match e.kind.as_str() {
        "job" => RunEvent::Job { job, state: e.state.clone(), detail: e.detail.clone() },
        "step" => RunEvent::Step { job, step: e.subject.clone(), state: e.state.clone(), detail: e.detail.clone() },
        "test" if e.state == "counts" => {
            let n: Vec<u64> = e.detail.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            RunEvent::Counts { line: e.subject.clone(), passed: *n.first().unwrap_or(&0), failed: *n.get(1).unwrap_or(&0), ignored: *n.get(2).unwrap_or(&0) }
        }
        "test" => RunEvent::Test { name: e.subject.clone(), outcome: e.state.clone() },
        "experiment" => RunEvent::Experiment { job: e.job.clone(), experiment: e.subject.clone(), disposition: e.state.clone(), result: e.detail.clone() },
        _ => RunEvent::Line(format!("{} {} {}", e.subject, e.state, e.detail)),
    }
}

/// `custodian watch [--observe LEVEL] [--since N] [--follow]`: the journal's events after N, through the observer.
pub fn watch(ctx: &mut Ctx, l: &Watch) -> Result<Ran> {
    let level = ObserveLevel::of_word(&so(&l.observe).unwrap_or_else(|| "progress".into()))?;
    let mut obs = Observers::new(level);
    let mut since: i64 = so(&l.since).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut ran = Ran::named("watch");
    let mut shown = 0usize;
    loop {
        let events = ctx.journal()?.events_since(since)?;
        for e in &events {
            since = e.n;
            for line in obs.publish(event_of(e)) {
                shown += 1;
                if l.follow {
                    (ctx.live)(&format!("[{}] {line}", e.utc));
                } else {
                    ran.print(format!("[{}] {line}", e.utc));
                }
            }
        }
        if !l.follow {
            break;
        }
        std::thread::sleep(crate::timing::WATCH_POLL);
    }
    ran.say(format!("watched the journal's events through the `{}` observer: {shown} shown, up to event {since}", level.word()));
    Ok(ran)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_is_woken_for_its_own_changes() {
        let events = || {
            vec![
                RunEvent::Job { job: "j".into(), state: "running".into(), detail: "".into() },
                RunEvent::Test { name: "a".into(), outcome: "ok".into() },
                RunEvent::Test { name: "b".into(), outcome: "FAILED".into() },
                RunEvent::Counts { line: "core_lib".into(), passed: 1, failed: 1, ignored: 0 },
                RunEvent::Step { job: "j".into(), step: "codes".into(), state: "done".into(), detail: "".into() },
            ]
        };
        let shown = |w: &str| {
            let mut o = Observers::new(ObserveLevel::of_word(w).unwrap());
            events().into_iter().flat_map(|e| o.publish(e)).collect::<Vec<_>>()
        };
        assert_eq!(shown("failed"), ["job j running: ", "test b ... FAILED"]);
        assert_eq!(shown("progress").len(), 4);
        assert_eq!(shown("all").len(), 5);
        let mut o = Observers::new(ObserveLevel::of_word("all").unwrap());
        for e in events() {
            o.publish(e);
        }
        assert_eq!((o.state().passed, o.state().failed), (1, 1));
    }

    fn done(experiment: &str, result: &str) -> RunEvent {
        RunEvent::Experiment { job: Some("job-1".into()), experiment: experiment.into(), disposition: "promote".into(), result: result.into() }
    }

    /// The targeted wakes waiting in the condition queue, as the scheduler's wake messages the bridge makes of them:
    /// (task, reason) each.
    fn wakes(o: &mut Observers) -> Vec<(waker_protocol::TaskId, waker_protocol::WakeReason)> {
        o.rt.drain_cold_condition_protocol_wake_messages()
            .take_messages()
            .into_iter()
            .map(|m| match m {
                waker_protocol::SchedulerMsg::Wake { id, reason } => (id, reason),
                _ => panic!("the bridge makes wake messages"),
            })
            .collect()
    }

    #[test]
    fn an_experiment_event_carries_the_path_the_line_kept() {
        let e = Event { n: 7, utc: "u".into(), kind: "experiment".into(), job: Some("job-1".into()), subject: "exp-a".into(), state: "do_not_promote".into(), detail: ".custodian/state/jobs/running/x/data/experiments/a b/result.json".into(), seq: None };
        let RunEvent::Experiment { job, experiment, disposition, result } = event_of(&e) else { panic!("not an experiment event") };
        assert_eq!((job.as_deref(), experiment.as_str(), disposition.as_str(), result.as_str()), (Some("job-1"), "exp-a", "do_not_promote", e.detail.as_str()), "verbatim");
        assert_eq!(mask(&done("exp-a", "r")), FIELD_EXPERIMENT);
    }

    #[test]
    fn a_waiter_is_woken_once_for_its_own_experiment_and_never_for_another_change() {
        let task = waker_protocol::TaskId { slot: 7, generation: 3 };
        let mut o = Observers::new(ObserveLevel::of_word("all").unwrap());
        o.wait_for_experiment("exp-a", task);
        // other classes of change, and another experiment's result: nothing for A's waiter
        o.publish(RunEvent::Job { job: "j".into(), state: "running".into(), detail: "".into() });
        o.publish(RunEvent::Test { name: "t".into(), outcome: "FAILED".into() });
        o.publish(RunEvent::Step { job: "j".into(), step: "s".into(), state: "failed".into(), detail: "".into() });
        o.publish(done("exp-b", "b/result.json"));
        assert!(wakes(&mut o).is_empty());
        // its own: one wake, to its task, a condition
        o.publish(done("exp-a", "a/result.json"));
        assert_eq!(wakes(&mut o), [(task, waker_protocol::WakeReason::Condition)]);
        // the same state again, or another experiment after it: a rising edge, never again
        o.publish(done("exp-a", "a/result.json"));
        o.publish(done("exp-b", "b2/result.json"));
        assert!(wakes(&mut o).is_empty());
        assert_eq!(o.state().experiments["exp-a"].result, "a/result.json");
    }

    #[test]
    fn each_level_shows_an_experiments_result() {
        for w in ["failed", "progress", "all"] {
            let mut o = Observers::new(ObserveLevel::of_word(w).unwrap());
            assert_eq!(o.publish(done("exp-a", "a/result.json")), ["experiment exp-a promote: a/result.json"], "{w}");
        }
    }
}
