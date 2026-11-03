//! `custodian experiment campaign`: an `experiments` campaign governed by custodian, through the real hosted stack
//! (custodian → experiments → code_passenger's binary, replays for the state transitions). Each node is a hosted
//! `experiment run`; a shared node evolves this project one after another, an isolated node runs in a governed copy
//! of its state parent's accepted state; a rejected candidate is put back by `transform --revert`; the plan is made
//! and refused before anything runs; a run resumes, and refuses to when what it was made from moved.

mod common;

use common::*;
use experiments::campaign::{CampaignResult, CampaignStatus, NodeResult, NodeStatus, StateAction};
use experiments::{CommandSpec, ExperimentSpec, MeasurementParser, MeasurementPhase, MeasurementSpec, MutationChange, MutationMode, MutationPlan, PromotionDecision};
use experiments::scientific_method::{self, Comparator, Criterion, Hypothesis, IndependentVariable, Operand, ScientificMethod};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const LIB: &str = "pub fn state() -> u32 {\n    0\n}\n";

/// A crate in agent mode, with its lock, its `STATE.txt` at `state 0`, and old notes.
fn project(tag: &str) -> PathBuf {
    let d = fresh(&format!("campaign-{tag}"));
    std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"lab\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/lib.rs"), LIB).unwrap();
    std::fs::write(d.join("STATE.txt"), state(0)).unwrap();
    std::fs::write(d.join("notes.txt"), "old notes\n").unwrap();
    let s = std::process::Command::new("cargo").args(["generate-lockfile", "--offline"]).current_dir(&d).status().unwrap();
    assert!(s.success());
    ok(&d, &["agent", "init"], 0);
    d
}

fn state(k: usize) -> String {
    format!("state {k}\n")
}

fn sha(bytes: &[u8]) -> String {
    custodian_kit::change::sha256_hex(bytes)
}

fn command(label: &str, program: &str, args: &[&str]) -> CommandSpec {
    CommandSpec { label: label.into(), program: program.into(), args: args.iter().map(|a| (*a).to_owned()).collect(), cwd: None, env: Vec::new() }
}

/// One file of a replay: its path, its bytes before (none: absent), after (none: deleted), and its mode after.
struct File<'a> {
    path: &'a str,
    before: Option<&'a str>,
    after: Option<&'a str>,
    mode: Option<&'a str>,
}

/// A replay change spec `name` in `dir`, its payloads beside it.
fn replay(dir: &Path, name: &str, files: &[File]) -> PathBuf {
    let pay = dir.join(name);
    std::fs::create_dir_all(&pay).unwrap();
    let mut text = format!("name = \"{name}\"\nobjective = \"the kept state, exactly\"\noperation = \"replay\"\ntarget = \"*\"\n");
    for (i, f) in files.iter().enumerate() {
        text.push_str(&format!("\n[[replay]]\npath = \"{}\"\n", f.path));
        match f.before {
            Some(b) => text.push_str(&format!("before_sha256 = \"{}\"\n", sha(b.as_bytes()))),
            None => text.push_str("absent_before = true\n"),
        }
        match f.after {
            Some(a) => {
                let p = pay.join(format!("{i}"));
                std::fs::write(&p, a).unwrap();
                text.push_str(&format!("after_sha256 = \"{}\"\npayload = \"{name}/{i}\"\n", sha(a.as_bytes())));
            }
            None => text.push_str("delete = true\n"),
        }
        if let Some(m) = f.mode {
            text.push_str(&format!("mode = \"{m}\"\n"));
        }
    }
    let p = dir.join(format!("{name}.toml"));
    std::fs::write(&p, text).unwrap();
    p
}

/// The replay `state k → k+1` of STATE.txt.
fn step(dir: &Path, k: usize) -> PathBuf {
    let (b, a) = (state(k), state(k + 1));
    replay(dir, &format!("state-{k}-to-{}", k + 1), &[File { path: "STATE.txt", before: Some(&b), after: Some(&a), mode: None }])
}

/// An experiment: its baseline checks STATE.txt is at `from` (a command that fails stops it), its change (if any),
/// its candidate measured by `exact` (STATE.txt at `to`), supported when it is.
fn experiment(dir: &Path, id: &str, from: Option<usize>, change: Option<&Path>, to: usize, shape: impl FnOnce(&mut ExperimentSpec)) -> PathBuf {
    let method = ScientificMethod::new(Hypothesis::new("the transition reproduces the next state")).independent(IndependentVariable::new("transition")).criterion(Criterion::compare(Operand::observation("candidate", "exact"), Comparator::Equal, Operand::literal(true)));
    let mut s = ExperimentSpec {
        schema: experiments::SCHEMA.into(),
        id: id.into(),
        description: None,
        scientific_method: method,
        target: experiments::ExperimentTarget { root: ".".into(), label: None },
        protocol: experiments::Protocol::default(),
        mutation: MutationPlan::default(),
        verification: experiments::Verification::default(),
        measurements: experiments::MeasurementPlan::default(),
        differences: Vec::new(),
        promotion: experiments::PromotionPolicy::default(),
    };
    if let Some(f) = from {
        s.protocol.baseline.push(command("at its baseline", "grep", &["-qx", &format!("state {f}"), "STATE.txt"]));
    }
    if let Some(c) = change {
        s.mutation = MutationPlan { mode: MutationMode::Bounded, changes: vec![MutationChange { label: id.into(), spec: c.to_path_buf() }], ..MutationPlan::default() };
    } else {
        s.mutation.mode = MutationMode::Bounded;
    }
    s.measurements.measurements.push(MeasurementSpec { phase: MeasurementPhase::Candidate, condition: "candidate".into(), name: "exact".into(), unit: None, command: command("exact", "grep", &["-qx", &format!("state {to}"), "STATE.txt"]), parser: MeasurementParser::ExitSuccess });
    shape(&mut s);
    let p = dir.join(format!("{id}.json"));
    experiments::write_spec(&p, &s).unwrap();
    p
}

fn writer() -> Option<PathBuf> {
    passenger_required()
}

/// `custodian experiment campaign FILE [more]` in `c`, with the writer on PATH.
fn campaign(c: &Path, bin: &Path, file: &Path, more: &[&str], code: i32) -> String {
    let mut words = vec!["experiment", "campaign", file.to_str().unwrap()];
    words.extend_from_slice(more);
    ok_with(c, &words, Some(bin), code)
}

fn result_of(c: &Path, out: &str) -> CampaignResult {
    let rel = out.lines().find_map(|l| l.strip_prefix("result: ")).unwrap_or_else(|| panic!("no result line:\n{out}"));
    experiments::read_campaign_result(c.join(rel)).unwrap()
}

fn node<'a>(r: &'a CampaignResult, id: &str) -> &'a NodeResult {
    r.nodes.iter().find(|n| n.plan.id == id).unwrap_or_else(|| panic!("no node {id}"))
}

/// The project's state as the campaign reads it.
fn digest(dir: &Path) -> String {
    custodian::campaigning::state_of(dir, &[]).unwrap().0.digest
}

/// Every receipt a governed folder keeps, oldest first.
fn receipts(dir: &Path) -> Vec<serde_json::Value> {
    let mut v: Vec<serde_json::Value> = std::fs::read_dir(dir.join(".custodian/data/receipts")).unwrap().map(|e| serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap()).collect();
    v.sort_by_key(|r| r["seq"].as_i64());
    v
}

/// The code-passenger lines that wrote (transform with --write, or a revert) in a governed folder.
fn writes(dir: &Path) -> Vec<String> {
    receipts(dir).iter().filter(|r| r["runner"] == "line").filter_map(|r| r["line"].as_str().map(str::to_owned)).filter(|l| l.starts_with("code-passenger ") && (l.contains(" --write") || l.contains(" --revert "))).collect()
}

/// A campaign file in `dir`.
fn doc(dir: &Path, name: &str, text: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p
}

#[test]
fn the_resident_shows_the_plan_as_the_line_does() {
    let c = project("resident");
    let a = fresh("campaign-resident-artifacts");
    measuring(&a, "bench");
    let file = doc(&a, "plan.json", r#"{"id":"resident","factors":[{"name":"size","levels":[1,2]}],"controls":[{"name":"seed","value":7}],"blocks":[{"name":"machine","levels":["m1"]}],"steps":[{"sweep":{"id":"s","spec":"bench.json","design":"factorial","factors":["size"],"set":[],"blocks":["machine"]}}]}"#);
    let direct = ok(&c, &["experiment", "campaign", file.to_str().unwrap(), "--plan"], 0);
    let mut child = std::process::Command::new(BIN).arg("serve").current_dir(&c).stdout(std::process::Stdio::null()).spawn().unwrap();
    let mut answered = None;
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let r = run(&c, &["experiment", "campaign", file.to_str().unwrap(), "--plan"]);
        if r.out.contains("answered by the resident") {
            answered = Some(r);
            break;
        }
    }
    let r = answered.expect("the resident answered");
    ok(&c, &["stop"], 0);
    assert!(child.wait().unwrap().success());
    let plan = |out: &str| out.lines().find(|l| l.starts_with("campaign resident · plan ")).map(str::to_owned);
    assert_eq!(plan(&r.out), plan(&direct), "the same plan");
    assert!(r.out.contains("s.b01.t02.r01"), "{}", r.out);
}

#[test]
fn the_campaign_line_is_declared_once_and_reads_back() {
    use custodian::surface::{read, Custodian, Line};
    use xccute_contract::CompositeShellCommand;
    let argv = Custodian::experiment().campaign("v1.history.1o.campaign").storage("runs").job("j-1").plan().argv();
    let line = read(&argv).unwrap();
    let Line::ExperimentCampaign(l) = &line else { panic!("{line:?}") };
    assert_eq!((l.campaign.to_str(), l.storage.as_deref().and_then(|s| s.to_str()), l.job.as_deref().and_then(|s| s.to_str()), l.plan, l.again), (Some("v1.history.1o.campaign"), Some("runs"), Some("j-1"), true, false));
    assert_eq!(line.argv(), argv);
    assert_eq!((line.path(), line.status_word(0), line.status_word(1), line.status_word(2)), ("experiment campaign", "done", "findings", "refused"));
    assert!(read(&Custodian::experiment().campaign("c.json").again().argv()).is_ok());
    assert!(read(&["experiment".into(), "campaign".into()]).is_err(), "the campaign is required");
    assert!(read(&Custodian::experiment().campaign("c.json").plan().again().argv()).is_err(), "--plan changes nothing; --again runs fresh");
}

#[cfg(feature = "mcp")]
#[test]
fn the_mcp_tool_is_the_declared_line() {
    let t = custodian::mcp::TOOLS.iter().find(|t| t.name == "experiment_campaign").expect("registered");
    assert_eq!(t.path, "experiment campaign");
    let schema = serde_json::Value::Object(t.schema());
    let props: Vec<&str> = schema["properties"].as_object().unwrap().keys().map(String::as_str).collect();
    for f in ["campaign", "storage", "job", "plan", "again"] {
        assert!(props.contains(&f), "{props:?}");
    }
    assert_eq!(schema["required"], serde_json::json!(["campaign"]));
}

/// state 0 → A → state 1 → B → state 2 → C → state 3 in the project, one node after another: each starts from the
/// exact state the one before it left, each has its result, its preflight and its transform, code_passenger the one
/// writer.
#[test]
fn a_sequence_evolves_one_accepted_state_through_preflight_and_transform() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("sequence");
    let a = fresh("campaign-sequence-artifacts");
    for (k, id) in ["A", "B", "C"].iter().enumerate() {
        experiment(&a, id, Some(k), Some(&step(&a, k)), k + 1, |_| {});
    }
    let start = digest(&c);
    let file = doc(&a, "v1.history.1o.campaign", "campaign {\n  id \"history\"\n  sequence {\n    run { id \"A\" spec \"A.json\" }\n    run { id \"B\" spec \"B.json\" }\n    run { id \"C\" spec \"C.json\" }\n  }\n}\n");
    let out = campaign(&c, &bin, &file, &[], 0);
    let r = result_of(&c, &out);
    assert_eq!(r.status, CampaignStatus::Done, "{out}");
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(3));
    assert_eq!(r.start.as_ref().unwrap().digest, start);
    let mut prior = start.clone();
    for id in ["A", "B", "C"] {
        let n = node(&r, id);
        assert_eq!((n.run.status, n.run.promotion, n.run.state), (NodeStatus::Completed, Some(PromotionDecision::Promote), Some(StateAction::Accepted)), "{id}");
        assert_eq!(n.run.start.as_ref().unwrap().digest, prior, "{id} starts where the one before left");
        prior = n.run.accepted.as_ref().unwrap().digest.clone();
        // its own result, kept, its digest the file's
        let res = n.run.result.as_ref().unwrap();
        let bytes = std::fs::read(&res.path).unwrap();
        assert_eq!(res.digest, experiments::campaign::digest::bytes(&bytes));
        assert_eq!(experiments::read_result(&res.path).unwrap().promotion, PromotionDecision::Promote);
        assert!(n.run.evidence.iter().any(|e| e.kind == "code_passenger_transform"), "{id}");
    }
    assert_eq!(prior, digest(&c), "the project is at C's accepted state");
    assert_eq!(r.workspaces[0].accepted.as_ref().unwrap().digest, prior);
    // three preflights authorized, three writes, all code_passenger's transform with --write: one writer
    let all = receipts(&c);
    assert_eq!(all.iter().filter(|x| x["command"] == "preflight" && x["spec_hash"].is_string() && x["reason"] == "authorized").count(), 3);
    let w = writes(&c);
    assert_eq!(w.len(), 3, "{w:?}");
    assert!(w.iter().all(|l| l.contains(" transform --spec ") && l.contains("--write")), "{w:?}");
    // each node an `experiment run` line, the campaign line naming the result, and the journal's event
    assert_eq!(all.iter().filter(|x| x["line"].as_str().is_some_and(|l| l.starts_with("custodian experiment run "))).count(), 3);
    let line = receipt(&c, seq_of(&out));
    assert_eq!(line["outputs"].as_array().unwrap().len(), 1);
    assert!(line["line"].as_str().unwrap().starts_with("custodian experiment campaign "));
    // the order is the sequence's, one node at a time
    let started: Vec<&str> = r.order.iter().filter(|e| e.event == "started").map(|e| e.node.as_str()).collect();
    assert_eq!(started, ["A", "B", "C"]);
    assert!(r.order.iter().all(|e| e.running <= 1));
    // the plan is kept as evidence, its digest the result's
    let plan: serde_json::Value = serde_json::from_slice(&std::fs::read(&r.plan_file.as_ref().unwrap().path).unwrap()).unwrap();
    assert_eq!(plan["digest"].as_str(), Some(r.plan_digest.as_str()));
    // the same line again: the completed run answers, nothing runs
    let again = campaign(&c, &bin, &file, &[], 0);
    assert!(again.contains("completed already"), "{again}");
    assert_eq!(writes(&c).len(), 3);
}

/// B is not promoted: its candidate (STATE.txt moved, a script made executable, the notes deleted) is put back by
/// `transform --revert`, bytes and modes exactly, and C starts from state 1.
#[test]
fn a_rejected_candidate_is_put_back_exactly_and_the_next_starts_from_the_accepted_state() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("rollback");
    let a = fresh("campaign-rollback-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    let (s1, s2) = (state(1), state(2));
    let b = replay(&a, "b-candidate", &[
        File { path: "STATE.txt", before: Some(&s1), after: Some(&s2), mode: None },
        File { path: "scripts/measure.sh", before: None, after: Some("#!/bin/sh\nprintf 1\n"), mode: Some("755") },
        File { path: "notes.txt", before: Some("old notes\n"), after: None, mode: None },
    ]);
    // B's candidate is measured against a state it never reaches: not supported, not promoted
    experiment(&a, "B", Some(1), Some(&b), 9, |_| {});
    experiment(&a, "C", Some(1), Some(&step(&a, 1)), 2, |_| {});
    let file = doc(&a, "rollback.json", r#"{"id":"rollback","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}},{"run":{"id":"C","spec":"C.json"}}]}}]}"#);
    let out = campaign(&c, &bin, &file, &[], 1);
    let r = result_of(&c, &out);
    assert_eq!(r.status, CampaignStatus::Findings, "{out}");
    let (na, nb, nc) = (node(&r, "A"), node(&r, "B"), node(&r, "C"));
    assert_eq!((nb.run.status, nb.run.promotion, nb.run.state), (NodeStatus::Completed, Some(PromotionDecision::DoNotPromote), Some(StateAction::Restored)));
    assert_ne!(nb.run.candidate, nb.run.start, "B's candidate was written");
    assert_eq!(nb.run.accepted, na.run.accepted, "B's accepted state is A's");
    assert_eq!(nc.run.start, na.run.accepted, "C starts from state 1");
    assert_eq!(nc.run.state, Some(StateAction::Accepted));
    // the put-back is auditable: one revert, through custodian's transform line, code_passenger the writer
    assert_eq!(nb.run.restore.len(), 1, "{:?}", nb.run.restore);
    assert!(nb.run.restore[0].starts_with("transform --revert ") && nb.run.restore[0].contains(" → 0 "), "{:?}", nb.run.restore);
    let w = writes(&c);
    assert_eq!(w.iter().filter(|l| l.contains(" --revert ")).count(), 1, "{w:?}");
    // the files: C's state, and everything B made or deleted as it was before
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(2));
    assert!(!c.join("scripts/measure.sh").exists() && !c.join("scripts").exists());
    assert_eq!(std::fs::read_to_string(c.join("notes.txt")).unwrap(), "old notes\n");
    assert_eq!(std::fs::metadata(c.join("notes.txt")).unwrap().permissions().mode() & 0o777, 0o644);
}

/// Undecided (no criterion, promotion not requiring support): put back by default; accepted with
/// `on_undecided accept`.
#[test]
fn an_undecided_candidate_is_put_back_unless_the_policy_accepts_it() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    for (policy, want, file_state) in [("{}", StateAction::Restored, 0), (r#"{"on_undecided":"accept"}"#, StateAction::Accepted, 1)] {
        let c = project("undecided");
        let a = fresh("campaign-undecided-artifacts");
        experiment(&a, "U", Some(0), Some(&step(&a, 0)), 1, |s| {
            s.scientific_method.criteria.clear();
            s.promotion.require_supported = false;
        });
        let file = doc(&a, "undecided.json", &format!(r#"{{"id":"u","policy":{policy},"steps":[{{"run":{{"id":"U","spec":"U.json"}}}}]}}"#));
        let r = result_of(&c, &campaign(&c, &bin, &file, &[], 1));
        let n = node(&r, "U");
        assert_eq!((n.run.promotion, n.run.state), (Some(PromotionDecision::Undecided), Some(want)));
        assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(file_state));
    }
}

/// C's replay does not hold over state 2: custodian's preflight refuses it, nothing is written, the campaign stops
/// (on_refusal stop) and D is skipped; the line is refused and its result kept.
#[test]
fn a_refused_node_writes_nothing_and_stops_the_campaign() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("refusal");
    let a = fresh("campaign-refusal-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    experiment(&a, "B", Some(1), Some(&step(&a, 1)), 2, |_| {});
    // declared from state 5: not what the file is
    experiment(&a, "C", None, Some(&step(&a, 5)), 6, |_| {});
    experiment(&a, "D", Some(2), Some(&step(&a, 2)), 3, |_| {});
    let file = doc(&a, "refusal.json", r#"{"id":"refusal","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}},{"run":{"id":"C","spec":"C.json"}},{"run":{"id":"D","spec":"D.json"}}]}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST2102]"), "{}", r.all());
    let res = result_of(&c, &r.out);
    let (nb, nc) = (node(&res, "B"), node(&res, "C"));
    assert_eq!(nc.run.status, NodeStatus::Refused);
    assert!(nc.run.reason.as_deref().unwrap().contains("STATE.txt"), "{:?}", nc.run.reason);
    assert_eq!(nc.run.candidate, nc.run.start, "nothing written");
    assert_eq!(nc.run.accepted, nb.run.accepted);
    assert_eq!(node(&res, "D").run.status, NodeStatus::Skipped);
    assert!(res.stopped.as_deref().unwrap().contains("on_refusal stop"));
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(2));
    assert_eq!(writes(&c).len(), 2, "A's and B's writes, none of C's");
}

/// A, then two branches from A's accepted state, each isolated in its own governed workspace, then D after both,
/// from A's state again (no merge): the branches start from the same digest, see nothing of each other, and the
/// project never sees them.
#[test]
fn parallel_branches_start_from_one_state_in_isolated_workspaces_and_fan_in_without_merging() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("parallel");
    let a = fresh("campaign-parallel-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    let s1 = state(1);
    let left = replay(&a, "left", &[File { path: "LEFT.txt", before: None, after: Some("left\n"), mode: None }]);
    let right = replay(&a, "right", &[File { path: "RIGHT.txt", before: None, after: Some("right\n"), mode: None }]);
    experiment(&a, "B", Some(1), Some(&left), 1, |_| {});
    experiment(&a, "C", Some(1), Some(&right), 1, |_| {});
    experiment(&a, "D", Some(1), Some(&step(&a, 1)), 2, |_| {});
    let _ = s1;
    let file = doc(&a, "v1.fan.1o.campaign", "campaign {\n  id \"fan\"\n  policy { max_parallel 2 }\n  sequence {\n    run { id \"A\" spec \"A.json\" }\n    parallel {\n      run { id \"B\" spec \"B.json\" }\n      run { id \"C\" spec \"C.json\" }\n    }\n    run { id \"D\" spec \"D.json\" }\n  }\n}\n");
    let r = result_of(&c, &campaign(&c, &bin, &file, &[], 0));
    let (na, nb, nc, nd) = (node(&r, "A"), node(&r, "B"), node(&r, "C"), node(&r, "D"));
    assert_eq!(nb.run.start, na.run.accepted);
    assert_eq!(nc.run.start, na.run.accepted, "one source digest");
    let (wb, wc) = (nb.run.workspace_path.clone().unwrap(), nc.run.workspace_path.clone().unwrap());
    assert_ne!(wb, wc);
    assert!(wb.join("LEFT.txt").exists() && !wb.join("RIGHT.txt").exists(), "no leakage");
    assert!(wc.join("RIGHT.txt").exists() && !wc.join("LEFT.txt").exists(), "no leakage");
    assert!(!c.join("LEFT.txt").exists() && !c.join("RIGHT.txt").exists(), "the project never sees a branch");
    // each branch's workspace is governed: its own journal, its own preflight and write
    for w in [&wb, &wc] {
        assert!(w.join(".custodian").is_dir());
        assert_eq!(writes(w).len(), 1, "{}", w.display());
        assert!(w.starts_with(c.join(".custodian")), "kept with the campaign, outside the state it governs");
    }
    // D waited for both and started from A's state, in the project
    assert_eq!(nd.run.start, na.run.accepted);
    let pos = |id: &str, ev: &str| r.order.iter().position(|e| e.node == id && e.event == ev).unwrap();
    assert!(pos("D", "started") > pos("B", "finished") && pos("D", "started") > pos("C", "finished"));
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(2));
    assert_eq!(digest(&c), nd.run.accepted.as_ref().unwrap().digest);
    // the branches' accepted states are their own lineages, kept in the result
    assert!(r.workspaces.iter().any(|w| w.id == "isolated:B" && w.accepted == nb.run.accepted));
}

/// A repeat: N trials of one treatment from one source state, each in its own workspace with its own result, and one
/// summary of them.
#[test]
fn a_repeat_runs_independent_trials_and_summarizes_them() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("repeat");
    let a = fresh("campaign-repeat-artifacts");
    experiment(&a, "T", Some(0), Some(&step(&a, 0)), 1, |s| {
        s.measurements.measurements.push(MeasurementSpec { phase: MeasurementPhase::Candidate, condition: "candidate".into(), name: "replicate".into(), unit: None, command: command("replicate", "printenv", &["EXPERIMENT_REPLICATE"]), parser: MeasurementParser::Number });
    });
    let file = doc(&a, "repeat.json", r#"{"id":"rep","policy":{"max_parallel":3},"steps":[{"repeat":{"id":"again","times":3,"run":{"id":"treatment","spec":"T.json"}}}]}"#);
    let r = result_of(&c, &campaign(&c, &bin, &file, &[], 0));
    let trials: Vec<&NodeResult> = r.nodes.iter().filter(|n| n.plan.group.as_deref() == Some("again")).collect();
    assert_eq!(trials.len(), 3);
    let mut results = std::collections::BTreeSet::new();
    for (k, t) in trials.iter().enumerate() {
        assert_eq!(t.plan.replicate, Some(k as u32 + 1));
        assert_eq!(t.run.start.as_ref().unwrap().digest, r.start.as_ref().unwrap().digest, "one source");
        assert!(results.insert(t.run.result.clone().unwrap().path), "a result of its own");
        let res = experiments::read_result(&t.run.result.as_ref().unwrap().path).unwrap();
        let rep = res.observations.iter().find(|o| o.measurement.name == "replicate").unwrap();
        assert_eq!(rep.measurement.value, scientific_method::Scalar::Number(k as f64 + 1.0), "the binding reached the command");
    }
    let s = &r.summaries[0];
    assert_eq!((s.requested, s.completed, s.start_digest.as_deref()), (3, 3, Some(r.start.as_ref().unwrap().digest.as_str())));
    let m = s.measurements.iter().find(|m| m.name == "replicate").unwrap();
    assert_eq!((m.n, m.mean, m.min, m.max), (3, Some(2.0), Some(1.0), Some(3.0)));
    assert_eq!(digest(&c), r.start.as_ref().unwrap().digest, "the project is untouched by isolated trials");
}

/// An experiment with no change, measuring what its binding gives it: a factor `size` (as a number) and a factor
/// `matcher`, a control `seed` and a block `machine`.
fn measuring(a: &Path, id: &str) -> PathBuf {
    experiment(a, id, None, None, 0, |s| {
        s.scientific_method.independent_variables = vec![IndependentVariable::new("matcher"), IndependentVariable::new("size")];
        s.scientific_method.controls = vec![scientific_method::Control::new("seed")];
        for (name, var) in [("size", "EXPERIMENT_FACTOR_SIZE"), ("seed", "EXPERIMENT_CONTROL_SEED")] {
            s.measurements.measurements.push(MeasurementSpec { phase: MeasurementPhase::Candidate, condition: "candidate".into(), name: name.into(), unit: None, command: command(name, "printenv", &[var]), parser: MeasurementParser::Number });
        }
        for (name, var) in [("matcher", "EXPERIMENT_FACTOR_MATCHER"), ("machine", "EXPERIMENT_BLOCK_MACHINE")] {
            s.measurements.measurements.push(MeasurementSpec { phase: MeasurementPhase::Candidate, condition: "candidate".into(), name: name.into(), unit: None, command: command(name, "printenv", &[var]), parser: MeasurementParser::Text });
        }
    })
}

/// A factorial sweep, 2 × 3 × 2 repeats × 2 blocks: the exact Cartesian product, every trial's assignment reaching
/// its commands, recorded, and summarized by treatment and block and by each level.
#[test]
fn a_factorial_sweep_runs_its_exact_expansion_with_each_assignment_bound() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("factorial");
    let a = fresh("campaign-factorial-artifacts");
    measuring(&a, "bench");
    let file = doc(&a, "v1.grid.1o.campaign", "campaign {\n  id \"grid\"\n  policy { max_parallel 4 }\n  factor { name \"matcher\" level \"scalar\" level \"simd\" }\n  factor { name \"size\" level 1 level 2 level 3 }\n  control { name \"seed\" value 7 }\n  block { name \"machine\" level \"m1\" level \"m2\" }\n  sweep {\n    id \"grid\"\n    spec \"bench.json\"\n    design \"factorial\"\n    factor \"matcher\"\n    factor \"size\"\n    repeats 2\n    block \"machine\"\n  }\n}\n");
    let r = result_of(&c, &campaign(&c, &bin, &file, &[], 0));
    assert_eq!(r.nodes.len(), 2 * 3 * 2 * 2);
    let mut combos = std::collections::BTreeSet::new();
    for n in &r.nodes {
        let res = experiments::read_result(&n.run.result.as_ref().unwrap().path).unwrap();
        let got = |name: &str| res.observations.iter().find(|o| o.measurement.name == name).unwrap().measurement.value.clone();
        let set = |f: &str| n.plan.assignment.iter().find(|a| a.factor == f).unwrap().value.to_string();
        assert_eq!(got("size"), scientific_method::Scalar::Number(set("size").parse().unwrap()));
        assert_eq!(got("matcher"), scientific_method::Scalar::Text(set("matcher")));
        assert_eq!(got("seed"), scientific_method::Scalar::Number(7.0), "the control");
        assert_eq!(got("machine"), scientific_method::Scalar::Text(n.plan.blocks[0].level.to_string()), "the block, apart from the factors");
        // the record keeps what each command was given
        let cmd = res.commands.iter().find(|x| x.label == "size").unwrap();
        assert_eq!(cmd.env, n.plan.bindings);
        combos.insert((set("matcher"), set("size"), n.plan.blocks[0].level.to_string(), n.plan.replicate.unwrap()));
        // the binding wrote nothing: each trial's candidate is its start
        assert_eq!(n.run.candidate, n.run.start, "{}", n.plan.id);
    }
    assert_eq!(combos.len(), 24, "every combination once");
    assert_eq!(r.summaries.len(), 6 * 2);
    for s in &r.summaries {
        let size: f64 = s.assignment.iter().find(|a| a.factor == "size").unwrap().value.to_string().parse().unwrap();
        let m = s.measurements.iter().find(|m| m.name == "size").unwrap();
        assert_eq!((s.requested, s.completed, m.n, m.mean, m.stddev), (2, 2, 2, Some(size), Some(0.0)));
    }
    // by each level within each block (never pooled across blocks): 2 matchers × 2 repeats each
    let by_size: Vec<(String, String, usize)> = r.marginals.iter().filter(|m| m.factor == "size").map(|m| (m.blocks[0].level.to_string(), m.level.to_string(), m.completed)).collect();
    assert_eq!(by_size, [("m1", "1"), ("m1", "2"), ("m1", "3"), ("m2", "1"), ("m2", "2"), ("m2", "3")].map(|(b, l)| (b.to_string(), l.to_string(), 4)));
    assert!(r.order.iter().all(|e| e.running <= 4), "max_parallel");
}

/// One factor at a time: the reference, then each factor moved alone; the design is kept in the results.
#[test]
fn one_factor_at_a_time_runs_one_moved_factor_per_treatment() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("ofat");
    let a = fresh("campaign-ofat-artifacts");
    measuring(&a, "bench");
    let file = doc(&a, "ofat.json", r#"{"id":"ofat","policy":{"max_parallel":3},"controls":[{"name":"seed","value":7}],"blocks":[{"name":"machine","levels":["m1"]}],
        "factors":[{"name":"matcher","levels":["scalar","simd"]},{"name":"size","levels":[1,2,3]}],
        "steps":[{"sweep":{"id":"ofat","spec":"bench.json","design":"one_factor_at_a_time","factors":["matcher","size"],"reference":[{"factor":"matcher","value":"scalar"},{"factor":"size","value":1}],"blocks":["machine"]}}]}"#);
    let r = result_of(&c, &campaign(&c, &bin, &file, &[], 0));
    let names: Vec<&str> = r.summaries.iter().map(|s| s.treatment.as_str()).collect();
    assert_eq!(names, ["reference", "matcher=simd", "size=2", "size=3"]);
    assert_eq!(r.summaries.iter().map(|s| s.varied.as_deref()).collect::<Vec<_>>(), [None, Some("matcher"), Some("size"), Some("size")]);
    for n in &r.nodes {
        assert_eq!(n.plan.design, Some(experiments::campaign::Design::OneFactorAtATime));
        let res = experiments::read_result(&n.run.result.as_ref().unwrap().path).unwrap();
        let size = res.observations.iter().find(|o| o.measurement.name == "size").unwrap();
        let set = n.plan.assignment.iter().find(|a| a.factor == "size").unwrap().value.to_string();
        assert_eq!(size.measurement.value, scientific_method::Scalar::Number(set.parse().unwrap()));
    }
}

/// Rigor and controls: a candidate command writes a file no change declares. Exploratory records it and accepts;
/// controlled makes it a finding and does not accept; strict refuses the node. The control's value is in every
/// command's record; strict refuses to start without the writer's digest.
#[test]
fn the_rigor_decides_what_an_undeclared_change_makes_of_a_candidate() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    for (rigor, status, state, line) in [("exploratory", NodeStatus::Completed, StateAction::Accepted, 1), ("controlled", NodeStatus::Completed, StateAction::Discarded, 1), ("strict", NodeStatus::Refused, StateAction::Discarded, 2)] {
        let c = project(&format!("rigor-{rigor}"));
        let a = fresh("campaign-rigor-artifacts");
        experiment(&a, "X", Some(0), Some(&step(&a, 0)), 1, |s| {
            s.scientific_method.controls = vec![scientific_method::Control::new("seed")];
            s.protocol.candidate.push(command("a stray write", "touch", &["stray.txt"]));
        });
        let file = doc(&a, "rigor.json", &format!(r#"{{"id":"rigor","rigor":"{rigor}","controls":[{{"name":"seed","value":7}}],"steps":[{{"run":{{"id":"X","spec":"X.json","workspace":"isolated"}}}}]}}"#));
        let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
        assert_eq!(r.code, line, "{rigor}: {}", r.all());
        let res = result_of(&c, &r.out);
        let n = node(&res, "X");
        assert_eq!((n.run.status, n.run.state), (status, Some(state)), "{rigor}: {:?} {:?}", n.run.reason, n.run.findings);
        let said = format!("{:?} {:?}", n.run.reason, n.run.findings);
        assert!(said.contains("undeclared change") && said.contains("+stray.txt"), "{rigor}: {said}");
        assert!(res.enforced.iter().any(|e| e.contains("not controlled by the campaign: the machine")), "what is not controlled is said");
        if rigor == "exploratory" {
            let x = experiments::read_result(&n.run.result.as_ref().unwrap().path).unwrap();
            assert!(x.commands.iter().all(|c| c.env.iter().any(|v| v.name == "EXPERIMENT_CONTROL_SEED" && v.value == "7")), "the control in every command's record");
        }
        assert!(!c.join("stray.txt").exists(), "isolated: the project never sees it");
    }
    // strict without the writer on PATH: refused before anything runs, nothing made
    let c = project("rigor-no-writer");
    let a = fresh("campaign-rigor-artifacts");
    experiment(&a, "X", Some(0), Some(&step(&a, 0)), 1, |_| {});
    let file = doc(&a, "strict.json", r#"{"id":"strict","rigor":"strict","steps":[{"run":{"id":"X","spec":"X.json"}}]}"#);
    let before = digest(&c);
    // a PATH with no code-passenger on it
    let r = run_env(&c, &["experiment", "campaign", file.to_str().unwrap()], &[("PATH", "/usr/bin:/bin")]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("strict: the writer's digest cannot be established"), "{}", r.all());
    assert_eq!(digest(&c), before);
    assert!(!c.join(".custodian/data/campaigns/strict").exists());
}

/// At most max_parallel isolated nodes at once, while shared nodes never overlap one another.
#[test]
fn isolated_nodes_run_at_most_max_parallel_at_once_and_shared_nodes_one_at_a_time() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("concurrency");
    let a = fresh("campaign-concurrency-artifacts");
    experiment(&a, "slow", None, None, 0, |s| s.protocol.prepare.push(command("a second", "sleep", &["1"])));
    for (k, id) in ["A", "B", "C"].iter().enumerate() {
        experiment(&a, id, Some(k), Some(&step(&a, k)), k + 1, |_| {});
    }
    let file = doc(&a, "conc.json", r#"{"id":"conc","policy":{"max_parallel":2},"steps":[
        {"repeat":{"id":"slow","times":4,"run":{"id":"s","spec":"slow.json"}}},
        {"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}},{"run":{"id":"C","spec":"C.json"}}]}}]}"#);
    let r = result_of(&c, &campaign(&c, &bin, &file, &[], 0));
    let most = r.order.iter().map(|e| e.running).max().unwrap();
    assert_eq!(most, 2, "{:?}", r.order);
    // the shared nodes: each starts after the one before finished
    let pos = |id: &str, ev: &str| r.order.iter().position(|e| e.node == id && e.event == ev).unwrap();
    assert!(pos("B", "started") > pos("A", "finished") && pos("C", "started") > pos("B", "finished"));
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(3));
}

/// The process stops while B runs; the same line again resumes: A is not run again, B and C run, one result.
#[test]
fn an_interrupted_campaign_resumes_without_running_a_completed_node_again() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("resume");
    let a = fresh("campaign-resume-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    experiment(&a, "B", Some(1), Some(&step(&a, 1)), 2, |s| s.protocol.prepare.push(command("long enough to stop", "sleep", &["4"])));
    experiment(&a, "C", Some(2), Some(&step(&a, 2)), 3, |_| {});
    let file = doc(&a, "resume.json", r#"{"id":"resume","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}},{"run":{"id":"C","spec":"C.json"}}]}}]}"#);
    let progress = c.join(".custodian/data/campaigns/resume/progress.json");
    let mut child = std::process::Command::new(BIN).args(["experiment", "campaign", file.to_str().unwrap()]).current_dir(&c).env_remove("XDG_RUNTIME_DIR").env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
    let mut a_done = None;
    for _ in 0..600 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Ok(p) = experiments::read_campaign_progress(&progress) {
            if p.nodes.contains_key("A") && p.running.iter().any(|r| r == "B") {
                a_done = Some(p);
                break;
            }
        }
    }
    let kept = a_done.expect("A completed and B started");
    child.kill().unwrap();
    let _ = child.wait();
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(1), "stopped before B wrote");
    let a_result = kept.nodes["A"].result.clone().unwrap();
    // a stale resume is refused: the experiment C moved
    let original = std::fs::read(a.join("C.json")).unwrap();
    let mut moved = experiments::read_spec(a.join("C.json")).unwrap();
    moved.description = Some("edited after the run began".into());
    experiments::write_spec(a.join("C.json"), &moved).unwrap();
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST2102]") && r.all().contains("experiment C.json is sha256:") && r.all().contains("--again"), "{}", r.all());
    std::fs::write(a.join("C.json"), original).unwrap();
    // a factor added since: refused too
    let factor = doc(&a, "resume.json", r#"{"id":"resume","factors":[{"name":"transition","levels":["exact"]}],"steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}},{"run":{"id":"C","spec":"C.json"}}]}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", factor.to_str().unwrap()], Some(&bin));
    assert!(r.code == 2 && r.all().contains("cannot resume"), "{}", r.all());
    let file = doc(&a, "resume.json", r#"{"id":"resume","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}},{"run":{"id":"C","spec":"C.json"}}]}}]}"#);
    // the project moved: refused
    std::fs::write(c.join("notes.txt"), "moved\n").unwrap();
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    assert!(r.code == 2 && r.all().contains("[CUST2103]"), "{}", r.all());
    std::fs::write(c.join("notes.txt"), "old notes\n").unwrap();
    // the same line again: resumed
    let out = campaign(&c, &bin, &file, &[], 0);
    assert!(out.contains("resumed run ") && out.contains("1 of 3 nodes kept"), "{out}");
    let r = result_of(&c, &out);
    assert_eq!(r.run_id, kept.run_id);
    assert_eq!(node(&r, "A").run.result.as_ref(), Some(&a_result), "A's result is the one it had");
    assert_eq!(r.order.iter().filter(|e| e.node == "A" && e.event == "started").count(), 1, "A ran once");
    assert_eq!(r.counts.completed, 3);
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(3));
    assert_eq!(writes(&c).len(), 3, "one write per transition, none repeated");
    // --again: a fresh run over the project as it is now (state 3, so its baselines fail: refused, a new run)
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap(), "--again"], Some(&bin));
    let again = result_of(&c, &r.out);
    assert_ne!(again.run_id, kept.run_id);
    assert_eq!(node(&again, "A").run.status, NodeStatus::Failed, "A's baseline is state 0, the project is at 3");
}

/// Nothing is written before a plan holds: a refused plan, `--plan`, and a storage that would be part of the state
/// leave the project and its storage as they were.
#[test]
fn a_plan_is_shown_and_refused_without_touching_anything() {
    let c = project("plan");
    let a = fresh("campaign-plan-artifacts");
    measuring(&a, "bench");
    let before = digest(&c);
    let matrix = doc(&a, "v1.matrix.1o.campaign", "campaign {\n  id \"matrix\"\n  policy { max_parallel 4 }\n  factor { name \"matcher\" level \"scalar\" level \"simd\" }\n  factor { name \"size\" level 1 level 10 level 100 level 1000 }\n  factor { name \"geo\" scope \"campaign\" level \"low\" level \"medium\" level \"high\" }\n  control { name \"seed\" value 7 }\n  block { name \"machine\" level \"lab-1\" }\n  sweep {\n    id \"grid\"\n    spec \"bench.json\"\n    design \"factorial\"\n    factor \"matcher\"\n    factor \"size\"\n    factor \"geo\"\n    repeats 10\n    block \"machine\"\n  }\n}\n");
    let out = ok(&c, &["experiment", "campaign", matrix.to_str().unwrap(), "--plan"], 0);
    assert!(out.contains("campaign matrix · plan sha256:") && out.contains("240 nodes"), "{out}");
    assert!(out.contains("sweep grid: factorial · 24 treatments × 10 repeats × blocks machine = 240 trials"), "{out}");
    assert!(out.contains("240 grid.b01.t24.r10") && out.contains("set matcher=simd,size=1000,geo=high") && out.contains("block machine=lab-1") && out.contains("replicate 10"), "{out}");
    assert!(out.contains("at most 4 at once"), "{out}");
    assert!(out.contains("plan only: nothing was run or written"), "{out}");
    let refused = doc(&a, "cycle.json", r#"{"id":"cycle","steps":[{"run":{"id":"a","spec":"bench.json","after":["b"],"workspace":"isolated"}},{"run":{"id":"b","spec":"bench.json","after":["a"],"workspace":"isolated"}}]}"#);
    let r = run(&c, &["experiment", "campaign", refused.to_str().unwrap()]);
    assert!(r.code == 2 && r.all().contains("[CUST2102]") && r.all().contains("a cycle"), "{}", r.all());
    let inside = run(&c, &["experiment", "campaign", matrix.to_str().unwrap(), "kept-here"]);
    assert!(inside.code == 2 && inside.all().contains("inside the governed root"), "{}", inside.all());
    assert_eq!(digest(&c), before, "nothing in the project moved");
    assert!(!c.join(".custodian/data/campaigns").exists() && !c.join("kept-here").exists(), "no storage made");
}

/// The history shape: one campaign, fifty ordered experiments, each a replay State01 → State02 → ... → State51 over
/// one evolving accepted state in the project, fifty results, one campaign result, no shell anywhere.
#[test]
fn fifty_ordered_replays_take_one_governed_state_from_state01_to_state51() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("fifty");
    std::fs::write(c.join("STATE.txt"), state(1)).unwrap();
    let a = fresh("campaign-fifty-artifacts");
    let mut text = String::from("campaign {\n  schema \"experiments.campaign.v1\"\n  id \"history\"\n  rigor \"strict\"\n  sequence {\n");
    for k in 1..=50 {
        let id = format!("state{k:02}-to-{:02}", k + 1);
        let mut files = vec![File { path: "STATE.txt", before: None, after: None, mode: None }];
        let (b, af) = (state(k), state(k + 1));
        files[0] = File { path: "STATE.txt", before: Some(&b), after: Some(&af), mode: None };
        let kept = format!("history/{k:02}.txt");
        let body = format!("kept at {k}\n");
        files.push(File { path: &kept, before: None, after: Some(&body), mode: None });
        let change = replay(&a, &format!("transition{k:02}"), &files);
        experiment(&a, &id, Some(k), Some(&change), k + 1, |_| {});
        text.push_str(&format!("    run {{ id \"{id}\" spec \"{id}.json\" }}\n"));
    }
    text.push_str("  }\n}\n");
    let file = doc(&a, "v1.history.1o.campaign", &text);
    let out = campaign(&c, &bin, &file, &[], 0);
    let r = result_of(&c, &out);
    assert_eq!((r.status, r.counts.nodes, r.counts.completed, r.counts.accepted), (CampaignStatus::Done, 50, 50, 50), "{out}");
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(51));
    assert_eq!(std::fs::read_dir(c.join("history")).unwrap().count(), 50);
    let mut prior = r.start.clone().unwrap();
    let mut results = std::collections::BTreeSet::new();
    for n in &r.nodes {
        assert_eq!(n.run.start.as_ref(), Some(&prior), "{}", n.plan.id);
        prior = n.run.accepted.clone().unwrap();
        assert!(results.insert(n.run.result.clone().unwrap().path));
    }
    assert_eq!(results.len(), 50);
    assert_eq!(digest(&c), prior.digest);
    let all = receipts(&c);
    assert_eq!(writes(&c).len(), 50);
    assert!(!all.iter().filter_map(|x| x["line"].as_str()).any(|l| l.starts_with("sh ") || l.starts_with("bash ") || l.contains(".command")), "no shell");
    assert_eq!(all.iter().filter(|x| x["line"].as_str().is_some_and(|l| l.starts_with("custodian experiment campaign "))).count(), 1, "one campaign line");
}


/// A command leaves a file no one can read: the candidate cannot be read. The result is kept, the transform is still
/// put back, the workspace is discarded (it is not its start: the file stays) and the campaign stops.
#[test]
fn a_candidate_that_cannot_be_read_is_still_put_back_and_its_result_kept() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("unreadable");
    let a = fresh("campaign-unreadable-artifacts");
    experiment(&a, "X", Some(0), Some(&step(&a, 0)), 1, |s| {
        s.protocol.candidate.push(command("make", "touch", &["locked.txt"]));
        s.protocol.candidate.push(command("lock", "chmod", &["000", "locked.txt"]));
    });
    experiment(&a, "Y", Some(0), Some(&step(&a, 0)), 1, |_| {});
    let file = doc(&a, "unreadable.json", r#"{"id":"unreadable","steps":[{"sequence":{"steps":[{"run":{"id":"X","spec":"X.json"}},{"run":{"id":"Y","spec":"Y.json"}}]}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    let _ = std::fs::set_permissions(c.join("locked.txt"), std::fs::Permissions::from_mode(0o644));
    assert_eq!(r.code, 2, "{}", r.all());
    let res = result_of(&c, &r.out);
    let x = node(&res, "X");
    assert_eq!((x.run.status, x.run.promotion, x.run.state), (NodeStatus::Refused, Some(PromotionDecision::Promote), Some(StateAction::Discarded)));
    assert!(x.run.reason.as_deref().unwrap().contains("the candidate state could not be read"), "{:?}", x.run.reason);
    assert!(x.run.result.as_ref().is_some_and(|f| f.path.is_file()), "the result is kept");
    assert_eq!(x.run.restore.len(), 1, "the transform was put back: {:?}", x.run.restore);
    assert_eq!((x.run.accepted.as_ref(), x.run.left.as_ref()), (None, None), "no accepted state on disk; the state left could not be read");
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(0), "STATE.txt put back");
    assert_eq!(node(&res, "Y").run.status, NodeStatus::Skipped);
    assert!(res.workspaces[0].discarded && res.stopped.is_some());
}

/// Exploratory never lets a write Code Passenger did not make into a state another node starts from: the candidate is
/// not accepted, the shared workspace (it keeps the stray file) is discarded, the campaign stops; `--again` refuses the
/// dirty tree, naming what to put back, and runs once it is put back.
#[test]
fn exploratory_never_accepts_an_undeclared_write_into_the_shared_lineage_and_again_refuses_a_dirty_tree() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("dirty");
    let a = fresh("campaign-dirty-artifacts");
    experiment(&a, "X", Some(0), Some(&step(&a, 0)), 1, |s| s.protocol.candidate.push(command("stray", "touch", &["stray.txt"])));
    experiment(&a, "Y", Some(1), Some(&step(&a, 1)), 2, |_| {});
    let file = doc(&a, "dirty.json", r#"{"id":"dirty","rigor":"exploratory","steps":[{"sequence":{"steps":[{"run":{"id":"X","spec":"X.json"}},{"run":{"id":"Y","spec":"Y.json"}}]}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    assert_eq!(r.code, 2, "{}", r.all());
    let res = result_of(&c, &r.out);
    let x = node(&res, "X");
    assert_eq!(x.run.state, Some(StateAction::Discarded));
    assert!(x.run.findings[0].contains("+stray.txt") && x.run.findings[0].contains("never takes a change Code Passenger did not make"), "{:?}", x.run.findings);
    assert_eq!(node(&res, "Y").run.status, NodeStatus::Skipped, "nothing starts from it");
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(0), "X's transform put back");
    // the same line refuses the moved project; --again refuses a state the run never recorded, naming the stray file
    let again = run_with(&c, &["experiment", "campaign", file.to_str().unwrap(), "--again"], Some(&bin));
    assert_eq!(again.code, 2, "{}", again.all());
    assert!(again.all().contains("[CUST2103]") && again.all().contains("never recorded") && again.all().contains("+stray.txt") && again.all().contains("put the project back"), "{}", again.all());
    // nor from another storage: the journal says this campaign last left the project dirty
    let elsewhere = a.join("other-storage");
    let other = run_with(&c, &["experiment", "campaign", file.to_str().unwrap(), elsewhere.to_str().unwrap(), "--again"], Some(&bin));
    assert!(other.code == 2 && other.all().contains("[CUST2103]") && other.all().contains("campaign dirty last left"), "{}", other.all());
    let first_elsewhere = run_with(&c, &["experiment", "campaign", file.to_str().unwrap(), elsewhere.to_str().unwrap()], Some(&bin));
    assert!(first_elsewhere.code == 2 && first_elsewhere.all().contains("campaign dirty last left"), "a first run in another storage too: {}", first_elsewhere.all());
    // nor another campaign: the tree one campaign left dirty is no start for any
    let another = doc(&a, "another.json", r#"{"id":"another","steps":[{"run":{"id":"Y","spec":"Y.json"}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", another.to_str().unwrap()], Some(&bin));
    assert!(r.code == 2 && r.all().contains("[CUST2103]") && r.all().contains("campaign dirty last left"), "another campaign over it: {}", r.all());
    std::fs::remove_file(c.join("stray.txt")).unwrap();
    let fresh_run = run_with(&c, &["experiment", "campaign", file.to_str().unwrap(), "--again"], Some(&bin));
    assert_ne!(result_of(&c, &fresh_run.out).run_id, res.run_id, "the project put back to a state the run recorded: a fresh run");
}

/// Killed after B's transform, while its candidate is measured: the same line puts B's transform back through its
/// record, finds the project at B's start, and runs B again; the interrupted attempt is kept.
#[test]
fn a_run_killed_after_a_transform_resumes_by_putting_it_back() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("killed-after-transform");
    let a = fresh("campaign-killed-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    experiment(&a, "B", Some(1), Some(&step(&a, 1)), 2, |s| s.protocol.candidate.push(command("slow candidate", "sleep", &["6"])));
    let file = doc(&a, "killed.json", r#"{"id":"killed","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}}]}}]}"#);
    // B's transform written and its receipts kept (custodian's transform line has ended), its candidate still running
    kill_when(&c, &bin, &file, ".custodian/data/campaigns/killed/progress.json", "B", |c| std::fs::read_to_string(c.join("STATE.txt")).unwrap_or_default() == state(2) && transformed(c) == 2);
    let out = campaign(&c, &bin, &file, &[], 0);
    assert!(out.contains("B: interrupted; 1 transforms put back newest first"), "{out}");
    let r = result_of(&c, &out);
    assert_eq!((node(&r, "B").run.state, r.counts.completed), (Some(StateAction::Accepted), 2));
    assert!(r.attempts.iter().any(|x| x.id == "B" && x.reason.as_deref().is_some_and(|w| w.starts_with("interrupted"))), "{:?}", r.attempts);
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(2));
    assert_eq!(writes(&c).iter().filter(|l| l.contains(" --revert ")).count(), 1);
}

/// The same in an isolated lineage evolving in place (`continue`).
#[test]
fn a_continue_workspace_killed_after_a_transform_resumes_by_putting_it_back() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("killed-continue");
    let a = fresh("campaign-killed-continue-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    experiment(&a, "B", Some(1), Some(&step(&a, 1)), 2, |s| s.protocol.candidate.push(command("slow candidate", "sleep", &["6"])));
    let file = doc(&a, "cont.json", r#"{"id":"cont","steps":[{"run":{"id":"A","spec":"A.json","workspace":"isolated"}},{"run":{"id":"B","spec":"B.json","from":"A","workspace":"continue"}}]}"#);
    let ws = |c: &Path| c.join(".custodian/data/campaigns/cont/runs");
    let at = |c: &Path| walk_files(&ws(c)).into_iter().find(|p| p.ends_with("workspaces/A/STATE.txt")).and_then(|p| p.parent().map(Path::to_path_buf));
    kill_when(&c, &bin, &file, ".custodian/data/campaigns/cont/progress.json", "B", |c| at(c).is_some_and(|w| std::fs::read_to_string(w.join("STATE.txt")).unwrap_or_default() == state(2) && transformed(&w) == 2));
    let out = campaign(&c, &bin, &file, &[], 0);
    assert!(out.contains("B: interrupted; 1 transforms put back newest first"), "{out}");
    let r = result_of(&c, &out);
    let b = node(&r, "B");
    assert_eq!(b.run.state, Some(StateAction::Accepted));
    assert_eq!(std::fs::read_to_string(b.run.workspace_path.as_ref().unwrap().join("STATE.txt")).unwrap(), state(2));
}

/// The process ended after B was decided and accepted but before its end was written: the progress still says B
/// runs. The rule: a node whose end is not recorded is undone (its transforms put back to its recorded start) and
/// runs again, whatever it was decided.
#[test]
fn a_node_accepted_but_not_recorded_is_undone_and_runs_again() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("killed-after-accept");
    let a = fresh("campaign-killed-accept-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    experiment(&a, "B", Some(1), Some(&step(&a, 1)), 2, |_| {});
    experiment(&a, "C", Some(2), Some(&step(&a, 2)), 3, |s| s.protocol.prepare.push(command("long enough to stop", "sleep", &["6"])));
    let file = doc(&a, "accepted.json", r#"{"id":"accepted","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}},{"run":{"id":"C","spec":"C.json"}}]}}]}"#);
    let pf = c.join(".custodian/data/campaigns/accepted/progress.json");
    kill_when(&c, &bin, &file, ".custodian/data/campaigns/accepted/progress.json", "C", |_| true);
    // the progress as it stood before B's end was written: B running, the shared workspace at A's accepted state
    let mut p = experiments::read_campaign_progress(&pf).unwrap();
    let b = p.nodes.remove("B").unwrap();
    let a_accepted = p.nodes["A"].accepted.clone();
    p.running = vec!["B".into()];
    let shared = p.workspaces.get_mut("shared").unwrap();
    shared.accepted = a_accepted.clone();
    shared.last = Some("A".into());
    experiments::write_campaign_progress(&pf, &p).unwrap();
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(2), "the tree is at B's accepted state");
    let out = campaign(&c, &bin, &file, &[], 0);
    assert!(out.contains("B: interrupted; 1 transforms put back newest first"), "{out}");
    let r = result_of(&c, &out);
    assert_eq!(node(&r, "B").run.start, a_accepted, "B ran again from A's state");
    assert_ne!(node(&r, "B").run.run_id, b.run_id);
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(3));
}

/// A link that leaves the tree is never copied into an isolated workspace; strict refuses an exclude that leaves out
/// source; both before anything is made.
#[test]
fn links_out_of_the_tree_and_strict_excludes_of_source_are_refused() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("links");
    let a = fresh("campaign-links-artifacts");
    experiment(&a, "X", Some(0), Some(&step(&a, 0)), 1, |_| {});
    std::os::unix::fs::symlink(c.join("notes.txt"), c.join("out-link")).unwrap();
    let file = doc(&a, "links.json", r#"{"id":"links","steps":[{"run":{"id":"X","spec":"X.json","workspace":"isolated"}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    assert!(r.code == 2 && r.all().contains("out-link is a link to") && r.all().contains("outside the tree"), "{}", r.all());
    assert!(!c.join(".custodian/data/campaigns/links/runs").exists() || walk_files(&c.join(".custodian/data/campaigns/links/runs")).iter().all(|p| !p.to_string_lossy().contains("/states/")), "no copy made");
    // a link inside the tree is kept as it is
    std::fs::remove_file(c.join("out-link")).unwrap();
    std::os::unix::fs::symlink("notes.txt", c.join("in-link")).unwrap();
    let file = doc(&a, "links2.json", r#"{"id":"links2","steps":[{"run":{"id":"X","spec":"X.json","workspace":"isolated"}}]}"#);
    let r = result_of(&c, &campaign(&c, &bin, &file, &[], 0));
    let ws = node(&r, "X").run.workspace_path.clone().unwrap();
    assert_eq!(std::fs::read_link(ws.join("in-link")).unwrap(), PathBuf::from("notes.txt"));
    let strict = doc(&a, "strict.json", r#"{"id":"strict","rigor":"strict","exclude":["src"],"steps":[{"run":{"id":"X","spec":"X.json"}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", strict.to_str().unwrap()], Some(&bin));
    assert!(r.code == 2 && r.all().contains("strict: exclude `src` leaves out src/lib.rs, a file of the starting tree"), "{}", r.all());
    // any file of the starting tree, source or not
    std::fs::create_dir_all(c.join("run")).unwrap();
    std::fs::write(c.join("run/out.txt"), "kept output\n").unwrap();
    let strict = doc(&a, "strict-run.json", r#"{"id":"strict-run","rigor":"strict","exclude":["run"],"steps":[{"run":{"id":"X","spec":"X.json"}}]}"#);
    let r = run_with(&c, &["experiment", "campaign", strict.to_str().unwrap()], Some(&bin));
    assert!(r.code == 2 && r.all().contains("strict: exclude `run` leaves out run/out.txt"), "{}", r.all());
}

/// `custodian experiment campaign FILE` started, killed once node `id` is running and `when` holds.
fn kill_when(c: &Path, bin: &Path, file: &Path, progress: &str, id: &str, when: impl Fn(&Path) -> bool) {
    let progress = c.join(progress);
    let mut child = std::process::Command::new(BIN).args(["experiment", "campaign", file.to_str().unwrap()]).current_dir(c).env_remove("XDG_RUNTIME_DIR").env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
    let mut seen = false;
    for _ in 0..800 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if experiments::read_campaign_progress(&progress).is_ok_and(|p| p.running.iter().any(|r| r == id)) && when(c) {
            seen = true;
            break;
        }
    }
    child.kill().unwrap();
    let _ = child.wait();
    assert!(seen, "{id} never ran as expected before the kill");
}

/// How many of custodian's own `transform` lines a governed folder has receipts of.
fn transformed(dir: &Path) -> usize {
    std::fs::read_dir(dir.join(".custodian/data/receipts")).into_iter().flatten().flatten().filter_map(|e| std::fs::read_to_string(e.path()).ok()).filter_map(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).filter(|r| r["command"] == "transform" && r["runner"] == "in_process").count()
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && !p.is_symlink() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}


/// A mark naming an older receipt than its node's (as a mark from a journal read wrong would): the resume puts back
/// only this node's own transforms, one at a time, and stops at its start; A's accepted transform is never touched.
#[test]
fn a_stale_mark_never_puts_back_another_nodes_transform() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("stale-mark");
    let a = fresh("campaign-stale-mark-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    experiment(&a, "B", Some(1), Some(&step(&a, 1)), 2, |s| s.protocol.candidate.push(command("slow candidate", "sleep", &["6"])));
    let file = doc(&a, "stale.json", r#"{"id":"stale","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}}]}}]}"#);
    kill_when(&c, &bin, &file, ".custodian/data/campaigns/stale/progress.json", "B", |c| std::fs::read_to_string(c.join("STATE.txt")).unwrap_or_default() == state(2) && transformed(c) == 2);
    let runs = c.join(".custodian/data/campaigns/stale/runs");
    let mark = walk_files(&runs).into_iter().find(|p| p.ends_with("nodes/B/running.json")).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&mark).unwrap()).unwrap();
    m["after_seq"] = serde_json::json!(0);
    std::fs::write(&mark, m.to_string()).unwrap();
    let out = campaign(&c, &bin, &file, &[], 0);
    assert!(out.contains("B: interrupted; 1 transforms put back newest first"), "{out}");
    let reverts: Vec<String> = writes(&c).into_iter().filter(|l| l.contains(" --revert ")).collect();
    assert_eq!(reverts.len(), 1, "only B's: {reverts:?}");
    assert!(reverts[0].contains("state-1-to-2"), "{reverts:?}");
    let r = result_of(&c, &out);
    assert_eq!(node(&r, "B").run.start, node(&r, "A").run.accepted, "B ran again from state 1");
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(2));
}

/// The process ended while code-passenger wrote: the tree moved and no receipt names the record on disk. The resume
/// is refused, naming that record and what to do with it.
#[test]
fn a_write_no_receipt_names_is_refused_with_the_record_to_put_back() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("unreceipted");
    let a = fresh("campaign-unreceipted-artifacts");
    experiment(&a, "A", Some(0), Some(&step(&a, 0)), 1, |_| {});
    experiment(&a, "B", Some(1), Some(&step(&a, 1)), 2, |s| s.protocol.prepare.push(command("long enough to stop", "sleep", &["6"])));
    let file = doc(&a, "unreceipted.json", r#"{"id":"unreceipted","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}}]}}]}"#);
    let runs = c.join(".custodian/data/campaigns/unreceipted/runs");
    kill_when(&c, &bin, &file, ".custodian/data/campaigns/unreceipted/progress.json", "B", |_| walk_files(&runs).iter().any(|p| p.ends_with("nodes/B/running.json")));
    // what code-passenger leaves when the process ends mid-write: the file written, its record on disk, no receipt
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(c.join("STATE.txt"), state(2)).unwrap();
    let record = c.join(".custodian/data/transforms/state-1-to-2/unreceipted.json");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "{}\n").unwrap();
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    assert_eq!(r.code, 2, "{}", r.all());
    let all = r.all();
    assert!(all.contains("[CUST2103]") && all.contains("B was interrupted") && all.contains("named by no receipt") && all.contains(&record.display().to_string()) && all.contains("put each of those files back to its digest before"), "{all}");
    assert_eq!(std::fs::read_to_string(c.join("STATE.txt")).unwrap(), state(2), "nothing was put back by guess");
}


/// Two nodes whose change specs share a name in different files; B's mark names an older receipt and the process
/// ended while code-passenger wrote B's change (no receipt names it). The resume never takes A's record for B's (a
/// record is a node's by its change-spec file, never by name), so nothing is put back by guess; the refusal says to
/// put the tree back and run the line again, and journals the project as left: `--again`, in another storage, refuses
/// it too.
#[test]
fn a_record_is_matched_by_its_change_file_never_by_name_and_a_refused_recovery_is_journaled_as_left() {
    let Some(bin) = writer() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let c = project("same-name");
    let a = fresh("campaign-same-name-artifacts");
    let (da, db) = (a.join("ca"), a.join("cb"));
    std::fs::create_dir_all(&da).unwrap();
    std::fs::create_dir_all(&db).unwrap();
    let ra = replay(&da, "same", &[File { path: "a.txt", before: None, after: Some("a\n"), mode: None }]);
    let (s0, s1) = (state(0), state(1));
    let rb = replay(&db, "same", &[File { path: "STATE.txt", before: Some(&s0), after: Some(&s1), mode: None }]);
    experiment(&a, "A", Some(0), Some(&ra), 0, |_| {});
    experiment(&a, "B", Some(0), Some(&rb), 1, |s| s.protocol.prepare.push(command("long enough to stop", "sleep", &["6"])));
    let file = doc(&a, "same.json", r#"{"id":"same","steps":[{"sequence":{"steps":[{"run":{"id":"A","spec":"A.json"}},{"run":{"id":"B","spec":"B.json"}}]}}]}"#);
    let runs = c.join(".custodian/data/campaigns/same/runs");
    kill_when(&c, &bin, &file, ".custodian/data/campaigns/same/progress.json", "B", |_| walk_files(&runs).iter().any(|p| p.ends_with("nodes/B/running.json")));
    // B's change written with no receipt (the process ended mid-write); B's mark naming an older receipt
    std::fs::write(c.join("STATE.txt"), state(1)).unwrap();
    let mark = walk_files(&runs).into_iter().find(|p| p.ends_with("nodes/B/running.json")).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&mark).unwrap()).unwrap();
    m["after_seq"] = serde_json::json!(0);
    std::fs::write(&mark, m.to_string()).unwrap();
    let r = run_with(&c, &["experiment", "campaign", file.to_str().unwrap()], Some(&bin));
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("0 of 0 of its transforms put back"), "A's record is not B's: {}", r.all());
    assert!(r.all().contains("put the workspace back to the state it started from by hand, then run the line again") && !r.all().contains("fresh with --again"), "{}", r.all());
    assert!(c.join("a.txt").exists(), "A's accepted change stands");
    assert!(writes(&c).iter().all(|l| !l.contains(" --revert ")), "nothing put back by guess");
    // journaled as left: --again in another storage refuses it
    let other = a.join("other-storage");
    let g = run_with(&c, &["experiment", "campaign", file.to_str().unwrap(), other.to_str().unwrap(), "--again"], Some(&bin));
    assert!(g.code == 2 && g.all().contains("campaign same last left") && g.all().contains("B was interrupted and could not be put back"), "{}", g.all());
}
