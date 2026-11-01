//! A hosted experiment's result wakes exactly the protocol task waiting for it: the result is written where
//! `ExperimentPaths` lays it out, the `experiment run` line keeps that path as its output and names it in the
//! journal's `experiment` event, the observers see the change, ob-serve-a-bowl's protocol bridge sends the targeted
//! wake to waker_protocol's scheduler, and the woken task reads the result back from what custodian keeps.

mod common;

use common::*;
use custodian::facade::Ctx;
use custodian::observe::Observers;
use custodian::runtime::{experiment_wait, ExperimentWait, EXPERIMENT_WAIT_KEY, EXPERIMENT_WAIT_VTABLE};
use custodian::words::ObserveLevel;
use std::path::{Path, PathBuf};
use waker_protocol::{MemoryCheckpointStore, TaskPolicy, TaskState, WakeReason, WakerProtocol};

const SPEC: &str = r#"{
  "schema": "experiments.spec.v3",
  "id": "ID",
  "scientific_method": {
    "question": { "text": "Does the candidate lower latency?" },
    "hypothesis": { "statement": "the candidate lowers latency", "rationale": null },
    "predictions": [],
    "independent_variables": [{ "name": "implementation", "description": null }],
    "dependent_variables": [{ "name": "latency", "unit": "ms", "description": null }],
    "controls": [],
    "criteria": [{
      "kind": "improvement",
      "baseline": { "condition": "baseline", "measurement": "latency" },
      "candidate": { "condition": "candidate", "measurement": "latency" },
      "direction": "lower_is_better",
      "minimum_fraction": 0.05
    }]
  },
  "target": { "root": ".", "label": "lab" },
  "measurements": {
    "measurements": [
      { "phase": "baseline", "condition": "baseline", "name": "latency", "unit": "ms",
        "command": { "label": "baseline latency", "program": "printf", "args": ["100"], "cwd": null }, "parser": "number" },
      { "phase": "candidate", "condition": "candidate", "name": "latency", "unit": "ms",
        "command": { "label": "candidate latency", "program": "printf", "args": ["90"], "cwd": null }, "parser": "number" }
    ]
  }
}"#;

fn project(tag: &str) -> PathBuf {
    let d = fresh(&format!("wake-{tag}"));
    std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"lab\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/lib.rs"), "pub fn one() -> u8 {\n    1\n}\n").unwrap();
    ok(&d, &["agent", "init"], 0);
    d
}

/// Experiment `id`'s spec, outside the project.
fn spec(id: &str) -> PathBuf {
    let f = fresh(&format!("wake-spec-{id}")).join(format!("{id}.json"));
    std::fs::write(&f, SPEC.replace("\"ID\"", &format!("\"{id}\""))).unwrap();
    f
}

fn result_line(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("result: ")).unwrap_or_else(|| panic!("no result line:\n{out}")).to_string()
}

fn state_of(sched: &WakerProtocol<MemoryCheckpointStore>, id: waker_protocol::TaskId) -> Option<TaskState> {
    sched.tasks.get(id).map(|e| e.state.clone())
}

fn wait_of(root: &Path, id: &str) -> ExperimentWait {
    experiment_wait(root, id).unwrap_or_else(|| panic!("the waiter for {id} was never polled"))
}

#[test]
fn a_finished_experiment_wakes_exactly_the_task_waiting_for_it() {
    let c = project("two");
    ok(&c, &["job", "start", "exp", "--intent", "wait for the results"], 0);
    let ctx = Ctx::at(&c).unwrap();
    let root = ctx.root().to_path_buf();

    // the scheduler owns two protocol tasks, one waiting for each experiment; each polled once, both waiting
    let mut sched = WakerProtocol::new(MemoryCheckpointStore::default());
    sched.register_vtable(EXPERIMENT_WAIT_KEY, &EXPERIMENT_WAIT_VTABLE);
    let a = sched.spawn_box(EXPERIMENT_WAIT_KEY, Box::new(ExperimentWait::new(&root, "exp-a")), TaskPolicy::default()).unwrap();
    let b = sched.spawn_box(EXPERIMENT_WAIT_KEY, Box::new(ExperimentWait::new(&root, "exp-b")), TaskPolicy::default()).unwrap();
    a.wake(WakeReason::Spawn).unwrap();
    b.wake(WakeReason::Spawn).unwrap();
    sched.run_once().unwrap();
    assert_eq!((wait_of(&root, "exp-a").polls, wait_of(&root, "exp-b").polls), (1, 1));
    assert_eq!((state_of(&sched, a.id()), state_of(&sched, b.id())), (Some(TaskState::Waiting), Some(TaskState::Waiting)));

    // each registers its condition; the journal's events so far (the job's start) wake no one
    let mut obs = Observers::new(ObserveLevel::of_word("failed").unwrap());
    obs.wait_for_experiment("exp-a", a.id());
    obs.wait_for_experiment("exp-b", b.id());
    let mut since = 0;
    let quiet = obs.feed(ctx.journal().unwrap(), &mut since, &sched.handle()).unwrap();
    assert_eq!(quiet.sent, 0, "{quiet:?}");
    sched.run_once().unwrap();
    assert_eq!((wait_of(&root, "exp-a").polls, wait_of(&root, "exp-b").polls), (1, 1), "nothing resumed");

    // experiment A runs, hosted by custodian: its result where ExperimentPaths put it, the line's output
    let out = ok(&c, &["experiment", "run", spec("exp-a").to_str().unwrap()], 0);
    let kept = result_line(&out);
    assert_eq!(receipt(&c, seq_of(&out))["outputs"], serde_json::json!([kept]), "the receipt's output is the result");

    // the journal's event reaches the observers; the bridge sends one wake, to A
    let sent = obs.feed(ctx.journal().unwrap(), &mut since, &sched.handle()).unwrap();
    assert_eq!((sent.sent, sent.errors.len()), (1, 0), "{sent:?}");
    assert_eq!(obs.state().experiments["exp-a"].result, kept, "the observers hold the line's own output, not a path of their own");
    sched.run_once().unwrap();

    // A resumed and read the result custodian kept; B was not polled again
    let wa = wait_of(&root, "exp-a");
    assert_eq!((wa.polls, wa.result.as_deref(), wa.failed.as_deref()), (2, Some(kept.as_str()), None));
    let written = experiments::read_result(root.join(&kept)).unwrap();
    assert_eq!(wa.run.as_deref(), Some(written.run_id.as_str()), "the run A read is the run that wrote the result");
    assert_eq!(state_of(&sched, a.id()), None, "A finished (its row is gone)");
    assert_eq!(wait_of(&root, "exp-b").polls, 1, "B was not resumed");
    assert_eq!(state_of(&sched, b.id()), Some(TaskState::Waiting));

    // A's experiment again: the same state, already true for A's observer; no wake for anyone
    ok(&c, &["experiment", "run", spec("exp-a").to_str().unwrap()], 0);
    let again = obs.feed(ctx.journal().unwrap(), &mut since, &sched.handle()).unwrap();
    assert_eq!(again.sent, 0, "{again:?}");
    sched.run_once().unwrap();
    assert_eq!(wait_of(&root, "exp-b").polls, 1);

    // B's experiment: B resumes and reads its own result
    let out = ok(&c, &["experiment", "run", spec("exp-b").to_str().unwrap()], 0);
    let kept_b = result_line(&out);
    let sent = obs.feed(ctx.journal().unwrap(), &mut since, &sched.handle()).unwrap();
    assert_eq!(sent.sent, 1, "{sent:?}");
    sched.run_once().unwrap();
    let wb = wait_of(&root, "exp-b");
    assert_eq!((wb.polls, wb.result.as_deref()), (2, Some(kept_b.as_str())));
    assert_eq!(state_of(&sched, b.id()), None);
}

#[test]
fn a_woken_task_trusts_only_what_custodian_kept() {
    // an event naming a path no `experiment run` receipt kept is refused, never read as the result
    let c = project("forged");
    let ctx = Ctx::at(&c).unwrap();
    ctx.journal().unwrap().event("experiment", None, "exp-c", "promote", ".custodian/data/elsewhere/result.json", None).unwrap();
    let e = custodian::experimenting::latest_result(&ctx, "exp-c").unwrap_err().to_string();
    assert!(e.contains("[CUST2101]") && e.contains("no `experiment run` receipt kept"), "{e}");
    assert!(custodian::experimenting::latest_result(&ctx, "exp-none").unwrap().is_none());
}

#[test]
fn watch_shows_an_experiments_result() {
    let c = project("watch");
    ok(&c, &["experiment", "run", spec("exp-w").to_str().unwrap()], 0);
    for level in ["failed", "progress", "all"] {
        let shown = ok(&c, &["watch", "--observe", level], 0);
        assert!(shown.contains("experiment exp-w promote: .custodian/data/experiments/"), "{level}: {shown}");
    }
}

#[test]
fn the_observers_and_the_runtime_make_no_result_path_of_their_own() {
    // where a result is kept is ExperimentPaths' (under custodian's layout): the observing side only carries the path
    for (name, source) in [("observe.rs", include_str!("../src/observe.rs")), ("runtime.rs", include_str!("../src/runtime.rs"))] {
        // the module's own code: its comments and its unit tests' sample paths are not paths it makes
        let module = source.split("#[cfg(test)]").next().unwrap_or(source);
        let code: Vec<&str> = module.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        for made in ["result.json", "ExperimentPaths", "\"experiments\"", "data/experiments", ".join(\"runs\")"] {
            assert!(!code.iter().any(|l| l.contains(made)), "{name}'s code names `{made}`");
        }
    }
}
