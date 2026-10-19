//! The observer set (ob-serve-a-bowl): a run's state (job states, step states, test progress) is a cold observable,
//! every change is published with a wake key whose field mask says what changed, and the `--observe` level is the
//! cold observer that decides what is shown: `failed` wakes for a failed test or step and a job's state, `progress`
//! also for each test line's counts and each step, `all` for every line. `custodian test --observe` and
//! `custodian watch` show what the observer is woken for, and nothing else.

use crate::codes::Result;
use crate::facade::{Ctx, Ran};
use crate::journal::Event;
use crate::surface::{so, Watch};
use crate::words::ObserveLevel;
use ob_serve_a_bowl::{cold_condition_matchers, ColdObservableRuntime, ColdObserverConfig, ObservableState, PayloadRef, StateTransition, WakeKey64, WakeProfileCode, WakeReasonCode};
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

/// What a run is doing, as the observers see it.
#[derive(Debug, Default, Clone)]
pub struct RunState {
    pub jobs: BTreeMap<String, String>,
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
    }
}

/// The fields an observe level is woken for.
pub fn interest(level: ObserveLevel) -> u16 {
    match level.word() {
        "failed" => FIELD_FAILED | FIELD_JOB,
        "progress" => FIELD_FAILED | FIELD_JOB | FIELD_STEP | FIELD_PROGRESS,
        _ => FIELD_FAILED | FIELD_JOB | FIELD_STEP | FIELD_PROGRESS | FIELD_LINE,
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
        std::thread::sleep(std::time::Duration::from_millis(500));
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
}
