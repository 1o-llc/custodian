//! `custodian experiment run`: an `experiments` run hosted by custodian. Its commands are custodian's declared lines
//! with receipts, its bounded changes go through custodian's preflight and transform (code_passenger the writer),
//! its result is the line's output, and the experiment's promotion decides the status (done, findings) while a run
//! that could not reach its conclusion is refused.

mod common;

use common::*;
use custodian::facade::{self, Ctx};
use experiments::{CommandSpec, ExperimentRunner, ExperimentSpec, MutationChange, MutationMode, MutationPlan, PromotionDecision};
use std::path::{Path, PathBuf};

/// experiments' `examples/minimal-latency.json`: latency 100 at the baseline, 90 for the candidate, a 5% improvement
/// the criterion.
const SPEC: &str = r#"{
  "schema": "experiments.spec.v3",
  "id": "minimal-latency",
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

const LIB: &str = "pub struct Registry<const WORKS: usize, const SUBSCRIBERS: usize> {\n    slots: [Option<u64>; WORKS],\n}\n\nimpl<const WORKS: usize, const SUBSCRIBERS: usize> Registry<WORKS, SUBSCRIBERS> {\n    pub fn new() -> Self {\n        Registry { slots: [None; WORKS] }\n    }\n\n    pub fn first_free(&self) -> Option<usize> {\n        self.slots.iter().position(|s| s.is_none())\n    }\n}\n";

const CHANGE: &str = "name = \"first-free\"\nobjective = \"one pass, the same answer\"\noperation = \"replace_body\"\ntarget = \"lab::Registry<WORKS,SUBSCRIBERS>::first_free\"\ncompatibility = \"preserve\"\n\n[function]\nname = \"first_free\"\n\n[[function.body]]\npiece = \"raw\"\ncode = '''\nlet mut found = None;\n\nfor (index, slot) in self.slots.iter().enumerate() {\n    if slot.is_none() && found.is_none() {\n        found = Some(index);\n    }\n}\nfound\n'''\n";

/// A crate in agent mode, with its lock when `locked`.
fn project(tag: &str, locked: bool) -> PathBuf {
    let d = fresh(&format!("experiment-{tag}"));
    std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"lab\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/lib.rs"), LIB).unwrap();
    if locked {
        let s = std::process::Command::new("cargo").args(["generate-lockfile", "--offline"]).current_dir(&d).status().unwrap();
        assert!(s.success());
    }
    ok(&d, &["agent", "init"], 0);
    d
}

fn command(label: &str, program: &str, args: &[&str]) -> CommandSpec {
    CommandSpec { label: label.into(), program: program.into(), args: args.iter().map(|a| (*a).to_owned()).collect(), cwd: None }
}

/// The experiment spec, shaped by `f`, written outside the project (with the change spec beside it): its path.
fn experiment(tag: &str, f: impl FnOnce(&mut ExperimentSpec, &Path)) -> PathBuf {
    let dir = fresh(&format!("experiment-artifacts-{tag}"));
    let first = dir.join("base.json");
    std::fs::write(&first, SPEC).unwrap();
    let mut spec = experiments::read_spec(&first).unwrap();
    let change = dir.join("first-free.toml");
    std::fs::write(&change, CHANGE).unwrap();
    f(&mut spec, &change);
    let file = dir.join(format!("{tag}.json"));
    experiments::write_spec(&file, &spec).unwrap();
    file
}

fn bounded(spec: &mut ExperimentSpec, change: &Path) {
    spec.mutation = MutationPlan { mode: MutationMode::Bounded, changes: vec![MutationChange { label: "one pass".into(), spec: change.to_path_buf() }], ..MutationPlan::default() };
}

/// Every receipt the project keeps.
fn receipts(dir: &Path) -> Vec<serde_json::Value> {
    let mut v: Vec<serde_json::Value> = std::fs::read_dir(dir.join(".custodian/data/receipts")).unwrap().map(|e| serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap()).collect();
    v.sort_by_key(|r| r["seq"].as_i64());
    v
}

fn result_of(dir: &Path, out: &str) -> (String, experiments::ExperimentResult) {
    let rel = out.lines().find_map(|l| l.strip_prefix("result: ")).unwrap_or_else(|| panic!("no result line:\n{out}")).to_string();
    let r = experiments::read_result(dir.join(&rel)).unwrap();
    (rel, r)
}

#[test]
fn the_experiment_line_is_declared_once_and_reads_back() {
    use custodian::surface::{read, Custodian, Line};
    use xccute_contract::CompositeShellCommand;
    let argv = Custodian::experiment().run("round49.json").storage("runs").job("j-1").argv();
    let line = read(&argv).unwrap();
    let Line::ExperimentRun(l) = &line else { panic!("{line:?}") };
    assert_eq!((l.spec.to_str(), l.storage.as_deref().and_then(|s| s.to_str()), l.job.as_deref().and_then(|s| s.to_str())), (Some("round49.json"), Some("runs"), Some("j-1")));
    assert_eq!(line.argv(), argv);
    assert_eq!((line.path(), line.status_word(0), line.status_word(1), line.status_word(2)), ("experiment run", "done", "findings", "refused"));
    assert!(read(&Custodian::experiment().run("a.json").argv()).is_ok());
    assert!(read(&["experiment".into(), "run".into()]).is_err(), "the spec is required");
}

#[cfg(feature = "mcp")]
#[test]
fn the_mcp_tool_is_the_declared_line() {
    let t = custodian::mcp::TOOLS.iter().find(|t| t.name == "experiment_run").expect("registered");
    assert_eq!(t.path, "experiment run");
    let declared: Vec<&str> = t.fields().iter().map(|f| f.name).collect();
    let schema = serde_json::Value::Object(t.schema());
    let props: Vec<&str> = schema["properties"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(props.len(), declared.len(), "the schema is the declaration's fields: {props:?} {declared:?}");
    assert!(declared.iter().all(|f| props.contains(f)));
    assert_eq!(schema["required"], serde_json::json!(["spec"]));
}

#[test]
fn custodian_statuses_for_each_promotion() {
    use custodian::experimenting::status_of;
    assert_eq!(status_of(PromotionDecision::Promote), 0);
    assert_eq!(status_of(PromotionDecision::DoNotPromote), 1);
    assert_eq!(status_of(PromotionDecision::Undecided), 1);
}

#[test]
fn a_hosted_experiment_keeps_its_result_and_each_command_has_a_receipt_in_the_job() {
    let c = project("commands", true);
    ok(&c, &["job", "start", "exp", "--intent", "latency"], 0);
    let spec = experiment("commands", |s, _| {
        s.protocol.prepare = vec![command("prepare", "printf", &["prepared"])];
        s.verification.commands = vec![command("verify", "test", &["-f", "Cargo.lock"])];
    });
    let out = ok(&c, &["experiment", "run", spec.to_str().unwrap()], 0);
    let (rel, result) = result_of(&c, &out);
    assert!(rel.starts_with(".custodian/state/jobs/running/exp/data/experiments/") && rel.ends_with("result.json"), "in the job's data folder: {rel}");
    assert_eq!(result.promotion, PromotionDecision::Promote);
    assert_eq!(result.commands.len(), 4, "prepare, two measurements, verify");
    // the line's receipt names the result as its output, in the job
    let line = receipt(&c, seq_of(&out));
    let job = line["job"].as_str().expect("the running job").to_string();
    assert!(job.starts_with("job-"), "{line}");
    assert_eq!(line["outputs"], serde_json::json!([rel]));
    // each command its own receipt, a line custodian ran, in the same job
    let all = receipts(&c);
    for (word, label) in [("experiment.prepare", "prepare"), ("experiment.measurement:baseline", "baseline latency"), ("experiment.measurement:candidate", "candidate latency"), ("experiment.verification", "verify")] {
        let r = all.iter().find(|r| r["command"] == word).unwrap_or_else(|| panic!("no {word} receipt"));
        assert_eq!((r["job"].as_str(), r["runner"].as_str()), (Some(job.as_str()), Some("line")), "{r}");
        assert!(r["name"].as_str().unwrap().ends_with(label), "{r}");
        let rec = result.commands.iter().find(|x| x.label == label).unwrap();
        assert_eq!(r["line"].as_str(), Some(rec.display.as_str()), "the result's command is the receipt's line");
    }
}

#[test]
fn a_bounded_change_goes_through_custodians_preflight_and_transform() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("skipped: no code-passenger with the transform line (build ../code_passenger, or set CUSTODIAN_TEST_PASSENGER)");
        return;
    };
    let c = project("bounded", true);
    ok(&c, &["job", "start", "exp", "--intent", "one pass over the slots"], 0);
    let spec = experiment("bounded", |s, change| {
        bounded(s, change);
        // the candidate is measured after the change is written: the verification reads it
        s.verification.commands = vec![command("written", "grep", &["-q", "let mut found = None", "src/lib.rs"])];
    });
    let out = ok_with(&c, &["experiment", "run", spec.to_str().unwrap()], Some(&bin), 0);
    let (_, result) = result_of(&c, &out);
    assert!(result.verification.passed, "{out}");
    assert!(std::fs::read_to_string(c.join("src/lib.rs")).unwrap().contains("let mut found = None;"));
    // custodian's preflight authorized it and custodian's transform made it, through code_passenger, in the job
    let job = receipt(&c, seq_of(&out))["job"].as_str().unwrap().to_string();
    let all = receipts(&c);
    let pre = all.iter().find(|r| r["command"] == "preflight").expect("a preflight receipt");
    assert_eq!((pre["exit_code"].as_i64(), pre["job"].as_str()), (Some(0), Some(job.as_str())), "{pre}");
    let writer = all.iter().find(|r| r["command"] == "transform" && r["runner"] == "line").expect("code_passenger's transform line");
    let words = writer["line"].as_str().unwrap();
    assert!(words.starts_with("code-passenger ") && words.contains(" transform --spec ") && words.contains("--write"), "{writer}");
    assert_eq!(writer["job"].as_str(), Some(job.as_str()));
    // the transform record is the experiment's evidence
    let m = &result.mutations[0];
    let record = m.transform_record.clone().expect("a transform record");
    let t: custodian_kit::transform::TransformRecord = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    assert!(t.written && t.writer.as_deref().unwrap_or("").starts_with("code_passenger "), "{t:?}");
    assert_eq!(t.preflight_seq, pre["seq"].as_i64(), "written on the preflight custodian gave");
    assert!(record.starts_with(c.join(".custodian/state/jobs/running/exp/data/transforms")), "{}", record.display());
    let kinds: Vec<&str> = result.evidence.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["code_passenger_transform", "custodian_preflight_receipt", "custodian_transform_receipt"]);
    // the job's close counts the change as a transform's
    ok(&c, &["job", "close"], 0);
    let closing: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(c.join(".custodian/state/jobs/done/exp/closing.json")).unwrap()).unwrap();
    assert!(!closing["transforms"].as_array().unwrap().is_empty(), "{closing}");
}

/// The library as the kept state 2 has it: one pass over the slots.
const LIB_AFTER: &str = "pub struct Registry<const WORKS: usize, const SUBSCRIBERS: usize> {\n    slots: [Option<u64>; WORKS],\n}\n\nimpl<const WORKS: usize, const SUBSCRIBERS: usize> Registry<WORKS, SUBSCRIBERS> {\n    pub fn new() -> Self {\n        Registry { slots: [None; WORKS] }\n    }\n\n    pub fn first_free(&self) -> Option<usize> {\n        let mut found = None;\n        for (index, slot) in self.slots.iter().enumerate() {\n            if slot.is_none() && found.is_none() {\n                found = Some(index);\n            }\n        }\n        found\n    }\n}\n";

/// The kept state's measurement script: what the candidate is measured by, run as a program (its mode is the replay's).
const MEASURE: &str = "#!/bin/sh\nprintf 90\n";

fn sha(bytes: &[u8]) -> String {
    custodian_kit::change::sha256_hex(bytes)
}

/// A replay to the kept state 2, its payloads beside it in `dir`: the library replaced, the measurement script made
/// executable, the old notes deleted. Its path.
fn replay_spec(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir.join("state-2")).unwrap();
    std::fs::write(dir.join("state-2/lib.rs"), LIB_AFTER).unwrap();
    std::fs::write(dir.join("state-2/measure.sh"), MEASURE).unwrap();
    let text = format!(
        "name = \"state-2\"\nobjective = \"the kept state 2, exactly\"\noperation = \"replay\"\ntarget = \"*\"\n\n[[replay]]\npath = \"src/lib.rs\"\nbefore_sha256 = \"{}\"\nafter_sha256 = \"{}\"\npayload = \"state-2/lib.rs\"\n\n[[replay]]\npath = \"scripts/measure.sh\"\nabsent_before = true\nafter_sha256 = \"{}\"\npayload = \"state-2/measure.sh\"\nmode = \"755\"\n\n[[replay]]\npath = \"notes.txt\"\nbefore_sha256 = \"{}\"\ndelete = true\n",
        sha(LIB.as_bytes()),
        sha(LIB_AFTER.as_bytes()),
        sha(MEASURE.as_bytes()),
        sha(b"old notes\n")
    );
    let p = dir.join("state-2.toml");
    std::fs::write(&p, text).unwrap();
    p
}

/// A hosted experiment whose bounded change is a replay: custodian's preflight authorizes the whole transition,
/// custodian's transform line has code_passenger write it (the only writer), the files and their modes are exactly the
/// kept state's, the candidate is measured by the script the replay made, the result is kept, and the transform record
/// is the experiment's evidence and the job's.
#[test]
fn a_bounded_replay_goes_through_custodians_preflight_and_code_passengers_transform() {
    let Some(bin) = passenger_required() else {
        eprintln!("skipped: no code-passenger with the transform line (build ../code_passenger, or set CUSTODIAN_TEST_PASSENGER)");
        return;
    };
    let c = project("replay", true);
    std::fs::write(c.join("notes.txt"), "old notes\n").unwrap();
    ok(&c, &["job", "start", "exp", "--intent", "the kept state 2"], 0);
    let spec = experiment("replay", |s, change| {
        let replay = replay_spec(change.parent().unwrap());
        s.mutation = MutationPlan { mode: MutationMode::Bounded, changes: vec![MutationChange { label: "state 2".into(), spec: replay }], ..MutationPlan::default() };
        // the candidate is measured by the script the replay made, run as a program
        s.measurements.measurements[1].command = command("candidate latency", "./scripts/measure.sh", &[]);
        s.verification.commands = vec![command("state 2", "test", &["!", "-e", "notes.txt"])];
    });
    let out = ok_with(&c, &["experiment", "run", spec.to_str().unwrap()], Some(&bin), 0);
    let (rel, result) = result_of(&c, &out);
    assert!(rel.starts_with(".custodian/state/jobs/running/exp/data/experiments/"), "{rel}");
    assert_eq!(result.promotion, PromotionDecision::Promote, "{out}");
    assert!(result.verification.passed, "{out}");
    // the files and modes are exactly the kept state's
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::read_to_string(c.join("src/lib.rs")).unwrap(), LIB_AFTER);
    assert_eq!(std::fs::read_to_string(c.join("scripts/measure.sh")).unwrap(), MEASURE);
    assert_eq!(std::fs::metadata(c.join("scripts/measure.sh")).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(!c.join("notes.txt").exists());
    // custodian's preflight authorized it, in the job; code_passenger's transform line wrote it on that receipt
    let job = receipt(&c, seq_of(&out))["job"].as_str().unwrap().to_string();
    let all = receipts(&c);
    // the preflight's own receipt (the one bound to the spec's digest; the line's receipt beside it is the facade's)
    let pre: Vec<&serde_json::Value> = all.iter().filter(|r| r["command"] == "preflight" && r["spec_hash"].is_string()).collect();
    assert_eq!(pre.len(), 1, "{pre:?}");
    assert_eq!((pre[0]["exit_code"].as_i64(), pre[0]["job"].as_str(), pre[0]["reason"].as_str()), (Some(0), Some(job.as_str()), Some("authorized")), "{}", pre[0]);
    // one writer: the one code-passenger transform line, with --write, on the preflight's receipt (the other
    // code-passenger lines of the run, the passenger's own store and checkpoints, write no file of the tree)
    let passenger: Vec<&str> = all.iter().filter(|r| r["runner"] == "line").filter_map(|r| r["line"].as_str()).filter(|l| l.starts_with("code-passenger ")).collect();
    assert!(passenger.iter().all(|l| l.contains(" transform --spec ") || l.contains(" passenger ")), "{passenger:?}");
    let writers: Vec<&serde_json::Value> = all.iter().filter(|r| r["runner"] == "line" && r["line"].as_str().is_some_and(|l| l.starts_with("code-passenger ") && l.contains(" transform "))).collect();
    assert_eq!(writers.len(), 1, "{writers:?}");
    let words = writers[0]["line"].as_str().unwrap();
    assert!(words.contains(" transform --spec ") && words.contains("state-2.toml") && words.contains("--write") && words.contains(&format!("--preflight {}", pre[0]["seq"])), "{words}");
    assert_eq!((writers[0]["command"].as_str(), writers[0]["job"].as_str()), (Some("transform"), Some(job.as_str())));
    // the transform record: the replay, the files it names exactly, written on that preflight by code_passenger
    let m = &result.mutations[0];
    let record = m.transform_record.clone().expect("a transform record");
    assert!(record.starts_with(c.join(".custodian/state/jobs/running/exp/data/transforms")), "{}", record.display());
    let t: custodian_kit::transform::TransformRecord = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    assert!(t.written && t.writer.as_deref().unwrap_or("").starts_with("code_passenger "), "{t:?}");
    assert_eq!(t.operation, "replay");
    assert_eq!(t.preflight_seq, pre[0]["seq"].as_i64());
    let files: Vec<(&str, &str, &str)> = t.files.iter().map(|f| (f.path.as_str(), f.before.as_str(), f.after.as_str())).collect();
    let (old, lib, new, measure) = (sha(b"old notes\n"), sha(LIB.as_bytes()), sha(LIB_AFTER.as_bytes()), sha(MEASURE.as_bytes()));
    assert_eq!(files, [("notes.txt", old.as_str(), "none"), ("scripts/measure.sh", "none", measure.as_str()), ("src/lib.rs", lib.as_str(), new.as_str())]);
    assert!(t.postconditions.iter().all(|(_, h)| *h) && t.postconditions.iter().any(|(p, _)| p == &format!("replay scripts/measure.sh: sha256 {measure}, mode 755")), "{:?}", t.postconditions);
    let kinds: Vec<&str> = result.evidence.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["code_passenger_transform", "custodian_preflight_receipt", "custodian_transform_receipt"]);
    // the job's close counts the replay as a transform's, and every line it wrote is covered by it
    ok(&c, &["job", "close"], 0);
    let closing: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(c.join(".custodian/state/jobs/done/exp/closing.json")).unwrap()).unwrap();
    assert!(!closing["transforms"].as_array().unwrap().is_empty(), "{closing}");
    assert!(closing["uncovered"].as_array().is_none_or(|u| u.is_empty()), "{closing}");
}

/// A replay whose last file is not as declared before is refused by custodian's preflight: nothing is written, no
/// transform runs, and the run is refused.
#[test]
fn a_replay_that_does_not_hold_is_refused_by_the_preflight_and_writes_nothing() {
    let c = project("replay-refused", true);
    std::fs::write(c.join("notes.txt"), "other notes\n").unwrap();
    let spec = experiment("replay-refused", |s, change| {
        let replay = replay_spec(change.parent().unwrap());
        s.mutation = MutationPlan { mode: MutationMode::Bounded, changes: vec![MutationChange { label: "state 2".into(), spec: replay }], ..MutationPlan::default() };
    });
    let r = run(&c, &["experiment", "run", spec.to_str().unwrap()]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("notes.txt: as declared before: sha256 "), "{}", r.all());
    assert_eq!(std::fs::read_to_string(c.join("src/lib.rs")).unwrap(), LIB, "nothing written");
    assert!(!c.join("scripts").exists() && c.join("notes.txt").exists());
    assert!(!receipts(&c).iter().any(|x| x["command"] == "transform"), "no transform ran");
}

#[test]
fn a_refused_preflight_writes_nothing_and_refuses_the_run() {
    // no Cargo.lock: custodian's preflight refuses (CUST0046) before any transform
    let c = project("refused", false);
    let spec = experiment("refused", bounded);
    let r = run(&c, &["experiment", "run", spec.to_str().unwrap()]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST0046]"), "{}", r.all());
    assert_eq!(std::fs::read_to_string(c.join("src/lib.rs")).unwrap(), LIB, "nothing written");
    assert!(!receipts(&c).iter().any(|x| x["command"] == "transform"), "no transform ran");
    assert!(!c.join(".custodian/data/experiments").exists() || walk(&c.join(".custodian/data/experiments")).iter().all(|p| !p.ends_with("result.json")), "no result");
}

#[test]
fn a_decision_not_to_promote_is_findings_and_a_run_that_cannot_finish_is_refused() {
    let c = project("statuses", true);
    let worse = experiment("worse", |s, _| s.measurements.measurements[1].command.args = vec!["100".into()]);
    let out = ok(&c, &["experiment", "run", worse.to_str().unwrap()], 1);
    let (_, result) = result_of(&c, &out);
    assert_eq!(result.promotion, PromotionDecision::DoNotPromote);
    assert_eq!(receipt(&c, seq_of(&out))["reason"], "findings");
    let missing = experiment("missing", |s, _| s.protocol.prepare = vec![command("prepare", "custodian-no-such-program", &[])]);
    let r = run(&c, &["experiment", "run", missing.to_str().unwrap()]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST1601]"), "the program missing, as custodian says it: {}", r.all());
}

#[test]
fn standalone_experiments_in_custodians_build_cannot_write_code_itself() {
    // custodian builds experiments without its code-passenger feature: a change is made only through the hosted path
    let c = fresh("experiment-standalone");
    let spec = experiment("standalone", |s, change| {
        bounded(s, change);
        s.target.root = c.clone();
    });
    let spec = experiments::read_spec(&spec).unwrap();
    let e = ExperimentRunner::new(c.join("storage")).run(&spec).unwrap_err().to_string();
    assert!(e.contains("without the `code-passenger` feature"), "{e}");
}

#[test]
fn nested_lines_run_under_the_writer_the_experiment_line_holds() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("skipped: no code-passenger with the transform line");
        return;
    };
    // the writer is this process's: put code_passenger on this process's PATH for the nested transform
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    let c = project("writer", true);
    let spec = experiment("writer", bounded);
    let argv = custodian::surface::Custodian::experiment().run(spec.as_os_str().to_os_string());
    let line = custodian::surface::read(&xccute_contract::CompositeShellCommand::argv(&argv)).unwrap();
    let mut ctx = Ctx::at(&c).unwrap();
    {
        // held elsewhere in the process: the line is refused before any operation
        let _held = facade::writer().try_enter().unwrap();
        let o = custodian::cli::run_line(&mut ctx, &line);
        assert_eq!((o.code, o.refusal.as_ref().map(|r| r.code.as_str())), (2, Some("CUST1303")), "{}", o.text());
        assert_eq!(std::fs::read_to_string(c.join("src/lib.rs")).unwrap(), LIB);
    }
    // the line holds it for the whole run; its preflight and transform run under it and never ask for it again
    let before = receipts(&c).len();
    let o = custodian::cli::run_line(&mut ctx, &line);
    assert_eq!(o.code, 0, "{}", o.text());
    let all = receipts(&c);
    let nested = &all[before..];
    for word in ["preflight", "transform"] {
        assert!(nested.iter().any(|r| r["command"] == word && r["accepted"] == true), "{word}: {nested:?}");
    }
    assert!(nested.iter().all(|r| !r["reason"].as_str().unwrap_or("").contains("CUST1303")), "no nested line was refused the writer: {nested:?}");
    assert!(facade::writer().try_enter().is_ok(), "released when the line ends");
}

#[test]
fn the_resident_runs_the_experiment_line_to_the_same_outcome() {
    let c = project("resident", true);
    let spec = experiment("resident", |_, _| {});
    let direct = ok(&c, &["experiment", "run", spec.to_str().unwrap()], 0);
    let (_, direct_result) = result_of(&c, &direct);
    let mut child = std::process::Command::new(BIN).arg("serve").current_dir(&c).stdout(std::process::Stdio::null()).spawn().unwrap();
    let mut answered = None;
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let r = run(&c, &["experiment", "run", spec.to_str().unwrap()]);
        if r.out.contains("answered by the resident") {
            answered = Some(r);
            break;
        }
    }
    let r = answered.expect("the resident answered");
    ok(&c, &["stop"], 0);
    assert!(child.wait().unwrap().success());
    assert_eq!(r.code, 0, "{}", r.all());
    let (_, resident_result) = result_of(&c, &r.out);
    assert_eq!((resident_result.promotion, resident_result.evaluation.conclusion.outcome), (direct_result.promotion, direct_result.evaluation.conclusion.outcome));
    assert_ne!(resident_result.run_id, direct_result.run_id, "a run of its own");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}
