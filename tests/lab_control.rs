//! The laboratory-control workflow through the built programs (custodian, and code-passenger as its writer), on a
//! specimen made here: an immutable baseline with a formatting finding of its own and no Cargo.lock, as a historical
//! source archive is. Its candidate is a copy; the baseline is never written.
//!
//! immutable baseline → hypothesis (the job's intent) → bounded authorized change (the preflight, on a lock put on the
//! copy) → proved transformation (code-passenger) → candidate → measurements (the delta, a measurement file) →
//! accept/reject (the close) → preserved provenance (the job's folder, accepted or rejected).

mod common;

use common::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LIB: &str = "pub struct Registry<const WORKS: usize, const SUBSCRIBERS: usize> {\n    slots: [Option<u64>; WORKS],\n}\n\nimpl<const WORKS: usize, const SUBSCRIBERS: usize> Registry<WORKS, SUBSCRIBERS> {\n    pub fn new() -> Self {\n        Registry { slots: [None; WORKS] }\n    }\n\n    pub fn first_free(&self) -> Option<usize> {\n        self.slots.iter().position(|s| s.is_none())\n    }\n}\n\npub fn inherited( ) -> u8 { 1 }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn empty_has_room() {\n        assert_eq!(super::Registry::<4, 2>::new().first_free(), Some(0));\n    }\n}\n";

const SPEC: &str = "name = \"first-free\"\nobjective = \"one pass, the same answer\"\noperation = \"replace_body\"\ntarget = \"lab::Registry<WORKS,SUBSCRIBERS>::first_free\"\ncompatibility = \"preserve\"\n\n[function]\nname = \"first_free\"\n\n[[function.body]]\npiece = \"raw\"\ncode = '''\nlet mut found = None;\n\nfor (index, slot) in self.slots.iter().enumerate() {\n    if slot.is_none() && found.is_none() {\n        found = Some(index);\n    }\n}\nfound\n'''\n";

/// The specimen: a folder written once, never again.
fn specimen(tag: &str) -> PathBuf {
    let d = fresh(&format!("specimen-{tag}"));
    std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"lab\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/lib.rs"), LIB).unwrap();
    d
}

/// The candidate: a copy of the specimen.
fn candidate(of: &Path, tag: &str) -> PathBuf {
    let d = fresh(&format!("candidate-{tag}"));
    for f in ["Cargo.toml", "src/lib.rs"] {
        std::fs::create_dir_all(d.join(f).parent().unwrap()).unwrap();
        std::fs::copy(of.join(f), d.join(f)).unwrap();
    }
    d
}

/// Every file of a tree with its bytes: what "never written" is checked against.
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(p.strip_prefix(root).unwrap().display().to_string(), std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

fn lock(dir: &Path) {
    let s = std::process::Command::new("cargo").args(["generate-lockfile", "--offline"]).current_dir(dir).status().unwrap();
    assert!(s.success(), "cargo generate-lockfile --offline in {}", dir.display());
}

fn provenance(done: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(done.join("data/provenance.json")).unwrap()).unwrap()
}

fn done_folder(d: &Path, name: &str) -> PathBuf {
    d.join(".custodian/state/jobs/done").join(name)
}

/// The whole workflow, accepted: the preflight refuses a candidate with no lock and writes none; with a lock put on the
/// copy it authorizes; code-passenger proves and writes on that preflight's receipt; the format delta holds the
/// candidate to what it adds (the specimen's own finding is inherited); a measurement is kept; the close accepts on
/// the evidence, and the provenance names the specimen, the lock, the change and its authorization, the writer, the
/// measurement and the disposition. The specimen is the same bytes at the end.
#[test]
fn the_scientific_workflow_runs_end_to_end_and_keeps_its_provenance() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("skipped: no code-passenger with the transform line (build ../code_passenger, or set CUSTODIAN_TEST_PASSENGER)");
        return;
    };
    let s = specimen("accept");
    let before = files(&s);
    let c = candidate(&s, "accept");
    ok(&c, &["agent", "init"], 0);
    // the spec lives with the experiment's artifacts, outside the candidate, as the runner keeps it
    let spec = fresh("artifacts").join("first-free.toml");
    std::fs::write(&spec, SPEC).unwrap();
    let spec_arg = spec.to_str().unwrap();
    // missing lockfile: the preflight refuses, says exactly what to do, and writes no lock anywhere
    let r = run(&c, &["preflight", spec_arg]);
    assert_eq!(r.code, 1, "{}", r.all());
    assert!(r.all().contains("[CUST0046] no Cargo.lock in the workspace at ") && r.all().contains("never on the baseline it was taken from"), "{}", r.all());
    assert!(!c.join("Cargo.lock").exists() && !s.join("Cargo.lock").exists());
    // the lock, put on the copy by the experimenter: the dependency state, fixed and recorded
    lock(&c);
    let out = ok(&c, &["job", "start", "exp", "--intent", "one pass over the slots", "--specimen", s.to_str().unwrap(), "--verify", "delta:fmt", "--verify", "test:lab"], 0);
    assert!(out.contains("experiment: differs from the specimen at open: +Cargo.lock"), "{out}");
    let pre = ok(&c, &["preflight", spec_arg], 0);
    assert!(pre.contains("authorized"), "{pre}");
    let wrote = ok_with(&c, &["transform", spec_arg, "--write"], Some(&bin), 0);
    assert!(wrote.contains("written"), "{wrote}");
    let lib = std::fs::read_to_string(c.join("src/lib.rs")).unwrap();
    assert!(lib.contains("        let mut found = None;\n\n        for (index, slot)"), "{lib}");
    // the inherited formatting finding is not the candidate's
    let delta = ok(&c, &["delta", "fmt"], 0);
    assert!(delta.contains(" inherited, 0 introduced") && !delta.contains("introduced: "), "{delta}");
    let bench = fresh("bench").join("bench.json");
    std::fs::write(&bench, "{\"ns_per_subscribe\": 41}\n").unwrap();
    ok(&c, &["job", "measure", bench.to_str().unwrap(), "--label", "throughput"], 0);
    let closed = ok(&c, &["job", "close"], 0);
    assert!(closed.contains("result: accepted"), "{closed}");
    let p = provenance(&done_folder(&c, "exp"));
    assert_eq!(p["specimen"]["form"], "folder");
    assert_eq!(p["specimen"]["state"]["lock"], serde_json::Value::Null);
    assert_eq!(p["open_vs_specimen"], serde_json::json!(["+Cargo.lock"]));
    let change = &p["changes"][0];
    assert_eq!((change["spec_name"].as_str(), change["preflight_decision"].as_str(), change["written"].as_bool()), (Some("first-free"), Some("authorized"), Some(true)), "{change}");
    assert_eq!(change["code_digest"], p["open"]["code_digest"], "the code the change was authorized over is the code the experiment opened on");
    assert_eq!(change["code_digest_after"], p["close"]["code_digest"], "the code it wrote is the code the experiment closed on");
    assert!(change["writer"].as_str().unwrap().starts_with("code_passenger "), "{change}");
    // the change, and the timing file custodian's test line writes (the record lists every file the job left)
    let changed: Vec<&str> = p["close_vs_open"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(changed.contains(&"~src/lib.rs") && changed.iter().all(|c| *c == "~src/lib.rs" || c.starts_with("+.maintenance/timings/")), "{changed:?}");
    assert_eq!(p["measurements"][0]["label"], "throughput");
    assert_eq!((p["disposition"]["result"].as_str(), p["disposition"]["by"].as_str()), (Some("accepted"), Some("evidence")));
    assert_eq!(files(&s), before, "the specimen is never written");
}

/// A rejected experiment is kept exactly as an accepted one is: its folder in done, its provenance whole, the
/// rejection and its reason recorded as the experimenter's.
#[test]
fn a_rejected_experiment_is_kept_as_an_accepted_one_is() {
    let s = specimen("reject");
    let c = candidate(&s, "reject");
    lock(&c);
    ok(&c, &["agent", "init"], 0);
    ok(&c, &["job", "start", "slow", "--intent", "try it", "--specimen", s.to_str().unwrap(), "--verify", "test:lab"], 0);
    let bench = fresh("bench-slow").join("bench.json");
    std::fs::write(&bench, "{\"ns_per_subscribe\": 97}\n").unwrap();
    ok(&c, &["job", "measure", bench.to_str().unwrap(), "--label", "throughput"], 0);
    let closed = run(&c, &["job", "close", "--reject", "slower than the base: 97 ns against 41"]);
    assert!(closed.all().contains("result: rejected (rejected at close: slower than the base: 97 ns against 41)"), "{}", closed.all());
    let done = done_folder(&c, "slow");
    let p = provenance(&done);
    assert_eq!((p["disposition"]["result"].as_str(), p["disposition"]["by"].as_str()), (Some("rejected"), Some("experimenter")));
    let kept = done.join(p["measurements"][0]["file"].as_str().unwrap());
    assert!(kept.starts_with(&done) && kept.is_file(), "the measurement is kept in the done folder: {}", kept.display());
    assert!(done.join("closing.json").is_file());
}

/// A formatting finding the candidate adds is its own: the delta finds it, the `delta:fmt` verification fails, and
/// the experiment is not accepted.
#[test]
fn a_finding_the_candidate_introduces_fails_the_experiment() {
    let s = specimen("introduce");
    let c = candidate(&s, "introduce");
    lock(&c);
    ok(&c, &["agent", "init"], 0);
    ok(&c, &["job", "start", "messy", "--intent", "a hand edit", "--specimen", s.to_str().unwrap(), "--verify", "delta:fmt"], 0);
    std::fs::write(c.join("src/lib.rs"), format!("{LIB}\npub fn added( ) -> u8 {{ 2 }}\n")).unwrap();
    let r = run(&c, &["delta", "fmt"]);
    assert_eq!(r.code, 1, "{}", r.all());
    assert!(r.out.contains(" inherited, 1 introduced") && r.out.contains("introduced: src/lib.rs:") && r.out.contains("pub fn added( )"), "{}", r.out);
    let closed = run(&c, &["job", "close"]);
    assert!(closed.all().contains("result: failed_test"), "{}", closed.all());
    let p = provenance(&done_folder(&c, "messy"));
    assert_eq!(p["disposition"]["result"], "failed_test");
}

/// The protocol's own form of the write: code-passenger on its own line, on the preflight number custodian gave
/// (`code-passenger --root . transform --spec ... --preflight N --write`). The closed experiment's provenance names
/// that change, its spec, its preflight and its writer, as it does for custodian's `transform` line.
#[test]
fn a_write_code_passenger_makes_on_its_own_line_is_the_experiments_change() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("skipped: no code-passenger with the transform line (build ../code_passenger, or set CUSTODIAN_TEST_PASSENGER)");
        return;
    };
    let s = specimen("own-line");
    let c = candidate(&s, "own-line");
    lock(&c);
    ok(&c, &["agent", "init"], 0);
    let artifacts = fresh("artifacts-own-line");
    let spec = artifacts.join("first-free.toml");
    std::fs::write(&spec, SPEC).unwrap();
    ok(&c, &["job", "start", "own", "--intent", "one pass over the slots", "--specimen", s.to_str().unwrap(), "--verify", "test:lab"], 0);
    let pre = ok(&c, &["preflight", spec.to_str().unwrap()], 0);
    let n: i64 = pre.split("· receipt #").nth(1).and_then(|r| r.split_whitespace().next()).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("no receipt number: {pre}"));
    let out = artifacts.join("written.json");
    let w = std::process::Command::new(bin.join("code-passenger"))
        .args(["--root", ".", "transform", "--spec", spec.to_str().unwrap(), "--preflight", &n.to_string(), "--write", "--out", out.to_str().unwrap()])
        .current_dir(&c)
        .output()
        .unwrap();
    assert!(w.status.success(), "{}{}", String::from_utf8_lossy(&w.stdout), String::from_utf8_lossy(&w.stderr));
    let closed = ok(&c, &["job", "close"], 0);
    assert!(closed.contains("result: accepted"), "{closed}");
    let p = provenance(&done_folder(&c, "own"));
    let change = &p["changes"][0];
    assert_eq!((change["spec_name"].as_str(), change["preflight_seq"].as_i64(), change["preflight_decision"].as_str(), change["written"].as_bool()), (Some("first-free"), Some(n), Some("authorized"), Some(true)), "{p}");
    assert_eq!(change["code_digest_after"], p["close"]["code_digest"], "the code it wrote is the code the experiment closed on");
    assert!(change["writer"].as_str().unwrap().starts_with("code_passenger "), "{change}");
}
