//! Exact tree measurements, hosted: custodian reads both trees by the one rule a campaign reads a state by (path,
//! content and mode, the kit's left-out paths and the campaign's `exclude` aside), and the result keeps both digests,
//! both file counts and every path where they differ. And a campaign's writer: `--plan` and a run refuse a
//! code-passenger that does not say it performs an operation the change specs use.

mod common;

use common::*;
use experiments::scientific_method::{Comparator, Criterion, Hypothesis, IndependentVariable, Operand, ScientificMethod};
use experiments::{ExactTree, ExperimentSpec, MeasurementPhase, MeasurementSource, MeasurementSpec, MutationChange, MutationMode, MutationPlan, PromotionDecision};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn set_mode(root: &Path, rel: &str, bits: u32) {
    std::fs::set_permissions(root.join(rel), std::fs::Permissions::from_mode(bits)).unwrap();
}

/// The tree a specimen holds: a text file, a script (755), a nested file.
fn specimen(root: &Path) {
    put(root, "a.txt", b"alpha\n");
    put(root, "bin/run.sh", b"#!/bin/sh\nexit 0\n");
    set_mode(root, "bin/run.sh", 0o755);
    put(root, "src/deep/c.rs", b"pub fn c() {}\n");
}

/// A project in agent mode whose `specimen/` is that tree.
fn project(tag: &str) -> PathBuf {
    let d = fresh(&format!("exact-{tag}"));
    put(&d, "Cargo.toml", b"[package]\nname = \"lab\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    put(&d, "src/lib.rs", b"pub fn lab() {}\n");
    specimen(&d.join("specimen"));
    ok(&d, &["agent", "init"], 0);
    d
}

/// The expected tree outside the project: the specimen's tree, then `f`.
fn expected(tag: &str, f: impl FnOnce(&Path)) -> PathBuf {
    let d = fresh(&format!("exact-expected-{tag}"));
    specimen(&d);
    f(&d);
    d
}

/// An experiment, outside the project, whose candidate is the exact tree `specimen` = `equals`, supported when equal.
fn experiment(tag: &str, equals: &Path, shape: impl FnOnce(&mut ExperimentSpec)) -> PathBuf {
    let method = ScientificMethod::new(Hypothesis::new("the specimen is the expected tree")).independent(IndependentVariable::new("source_state")).criterion(Criterion::compare(Operand::observation("candidate", "candidate_state_exact"), Comparator::Equal, Operand::literal(true)));
    let mut s = ExperimentSpec {
        schema: experiments::SCHEMA.into(),
        id: format!("exact-{tag}"),
        description: None,
        scientific_method: method,
        target: experiments::ExperimentTarget { root: ".".into(), label: None },
        protocol: experiments::Protocol::default(),
        mutation: MutationPlan { mode: MutationMode::Bounded, ..MutationPlan::default() },
        verification: experiments::Verification::default(),
        measurements: experiments::MeasurementPlan::default(),
        differences: Vec::new(),
        promotion: experiments::PromotionPolicy::default(),
    };
    s.measurements.measurements.push(MeasurementSpec {
        phase: MeasurementPhase::Candidate,
        condition: "candidate".into(),
        name: "candidate_state_exact".into(),
        unit: None,
        source: MeasurementSource::ExactTree(ExactTree { path: "specimen".into(), equals_tree: equals.to_path_buf() }),
    });
    shape(&mut s);
    let dir = fresh(&format!("exact-artifacts-{tag}"));
    let p = dir.join(format!("{tag}.json"));
    experiments::write_spec(&p, &s).unwrap();
    p
}

fn result_of(dir: &Path, out: &str) -> experiments::ExperimentResult {
    let rel = out.lines().find_map(|l| l.strip_prefix("result: ")).unwrap_or_else(|| panic!("no result line:\n{out}"));
    experiments::read_result(dir.join(rel)).unwrap()
}

fn digest(tree: &Path) -> String {
    custodian::campaigning::state_of(tree, &[]).unwrap().0.digest
}

#[test]
fn equal_trees_are_true_and_the_result_keeps_both_digests() {
    let c = project("equal");
    let want = expected("equal", |_| {});
    let file = experiment("equal", &want, |_| {});
    let out = ok(&c, &["experiment", "run", file.to_str().unwrap()], 0);
    let r = result_of(&c, &out);
    assert_eq!(r.promotion, PromotionDecision::Promote, "{out}");
    let t = &r.exact_trees[0];
    assert!(t.equal && t.differing_paths.is_empty(), "{t:?}");
    assert_eq!((t.observed_digest.as_str(), t.expected_digest.as_str()), (digest(&c.join("specimen")).as_str(), digest(&want).as_str()));
    assert_eq!(t.observed_digest, t.expected_digest);
    assert_eq!((t.observed_files, t.expected_files), (3, 3));
    assert_eq!((t.path.as_path(), t.equals_tree.as_path(), t.phase.as_str()), (c.join("specimen").as_path(), want.as_path(), "measurement:candidate"));
    assert!(r.commands.is_empty(), "no command ran: the comparison is custodian's own");
    assert!(out.contains("candidate_state_exact") && out.contains(": equal"), "{out}");
}

/// One way the expected tree differs: its name, the change made to it, the path that differs.
type Case = (&'static str, fn(&Path), &'static str);

#[test]
fn one_byte_one_mode_one_extra_and_one_missing_file_are_each_not_equal_naming_the_path() {
    let c = project("unequal");
    let observed = digest(&c.join("specimen"));
    let cases: [Case; 4] = [
        ("byte", |d| put(d, "a.txt", b"alphA\n"), "~a.txt"),
        ("mode", |d| set_mode(d, "bin/run.sh", 0o644), "~bin/run.sh"),
        ("extra", |d| put(d, "src/deep/extra.rs", b"\n"), "-src/deep/extra.rs"),
        ("missing", |d| std::fs::remove_file(d.join("src/deep/c.rs")).unwrap(), "+src/deep/c.rs"),
    ];
    for (tag, change, path) in cases {
        let want = expected(tag, change);
        let file = experiment(tag, &want, |_| {});
        let out = ok(&c, &["experiment", "run", file.to_str().unwrap()], 1);
        let r = result_of(&c, &out);
        assert_eq!(r.promotion, PromotionDecision::DoNotPromote, "{tag}: {out}");
        let t = &r.exact_trees[0];
        assert!(!t.equal, "{tag}");
        assert_eq!(t.differing_paths, [path], "{tag}");
        assert_eq!((t.observed_digest.as_str(), t.expected_digest.as_str()), (observed.as_str(), digest(&want).as_str()), "{tag}");
        assert_ne!(t.observed_digest, t.expected_digest, "{tag}");
        assert!(out.contains(&format!("1 paths differ ({path})")), "{tag}: {out}");
    }
    assert_eq!(digest(&c.join("specimen")), observed, "measuring changes nothing");
}

#[test]
fn a_campaign_node_leaves_out_the_campaigns_exclude_from_both_trees() {
    let c = project("exclude");
    put(&c, "specimen/run/out.txt", b"scratch\n");
    let want = expected("exclude", |_| {});
    let file = experiment("exclude", &want, |_| {});
    // alone, nothing is excluded: the scratch file is a difference
    let out = ok(&c, &["experiment", "run", file.to_str().unwrap()], 1);
    assert_eq!(result_of(&c, &out).exact_trees[0].differing_paths, ["+run/out.txt"]);
    // a node of a campaign whose exclude names it: equal, and the record says what was left out
    let doc = file.parent().unwrap().join("c.json");
    std::fs::write(&doc, format!(r#"{{"id":"exclude","exclude":["specimen/run"],"steps":[{{"run":{{"id":"one","spec":"{}"}}}}]}}"#, file.file_name().unwrap().to_str().unwrap())).unwrap();
    let out = ok(&c, &["experiment", "campaign", doc.to_str().unwrap()], 0);
    let rel = out.lines().find_map(|l| l.strip_prefix("result: ")).unwrap();
    let campaign = experiments::read_campaign_result(c.join(rel)).unwrap();
    let node = campaign.nodes.iter().find(|n| n.plan.id == "one").unwrap();
    // the node's record names the experiment's result, which carries the exact tree record
    let result = experiments::read_result(&node.run.result.as_ref().unwrap().path).unwrap();
    let t = &result.exact_trees[0];
    assert!(t.equal && t.excluded == ["run"], "{t:?}");
    assert_eq!(node.run.promotion, Some(PromotionDecision::Promote));
}

/// A program named `code-passenger` in a fresh folder that says `said` for its version.
fn fake_passenger(tag: &str, said: &str) -> PathBuf {
    let d = fresh(&format!("exact-fake-passenger-{tag}"));
    put(&d, "code-passenger", format!("#!/bin/sh\nprintf '%s\\n' {said}\n").as_bytes());
    set_mode(&d, "code-passenger", 0o755);
    d
}

/// A campaign over one experiment whose change is a replay of `a.txt`.
fn replay_campaign(c: &Path) -> PathBuf {
    let want = expected("writer", |d| put(d, "a.txt", b"beta\n"));
    let made = custodian_kit::change::replay_from_trees(&c.join("specimen"), &want, Some("specimen")).unwrap();
    let dir = fresh("exact-writer-artifacts");
    let spec = made.write(&dir.join("change"), "a-to-beta", "a.txt alpha to beta").unwrap();
    let file = experiment("writer", &want, |s| s.mutation.changes.push(MutationChange { label: "a-to-beta".into(), spec }));
    let doc = file.parent().unwrap().join("c.json");
    std::fs::write(&doc, format!(r#"{{"id":"writer","steps":[{{"run":{{"id":"one","spec":"{}"}}}}]}}"#, file.file_name().unwrap().to_str().unwrap())).unwrap();
    doc
}

#[test]
fn a_campaign_refuses_a_writer_that_does_not_say_it_performs_an_operation_its_specs_use() {
    let c = project("writer");
    let doc = replay_campaign(&c);
    let plan = ["experiment", "campaign", doc.to_str().unwrap(), "--plan"];
    // 0.6.0 says its version and no operations: refused, naming the operation, the program and its version
    let old = fake_passenger("old", "'code_passenger 0.6.0 (custodian_kit 0.5.0)'");
    let r = run_with(&c, &plan, Some(&old));
    assert_eq!(r.code, 2, "{}", r.all());
    for says in ["`replay`", &old.join("code-passenger").display().to_string(), "code_passenger 0.6.0 (custodian_kit 0.5.0)", "its version lists none"] {
        assert!(r.all().contains(says), "{says}: {}", r.all());
    }
    // a real run is refused the same way before anything runs
    let r = run_with(&c, &["experiment", "campaign", doc.to_str().unwrap()], Some(&old));
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("its version lists none"), "{}", r.all());
    assert!(!c.join(".custodian/data/campaigns").exists(), "nothing ran");
    // one that lists operations, but not this one
    let other = fake_passenger("other", "'code_passenger 0.7.0 (custodian_kit 0.5.1)' 'operations: make_fn rename'");
    let r = run_with(&c, &plan, Some(&other));
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("does not perform `replay`") && r.all().contains("code_passenger 0.7.0 (custodian_kit 0.5.1)"), "{}", r.all());
    // the writer built from this tree says it performs replay: the plan goes on
    let Some(bin) = passenger_required() else { return eprintln!("skipped: no code-passenger with the transform line") };
    let r = run_with(&c, &plan, Some(&bin));
    assert_eq!(r.code, 0, "{}", r.all());
    assert!(r.all().contains(&format!("writer: code-passenger {} (code_passenger ", bin.join("code-passenger").display())) && r.all().contains(") performs replay"), "{}", r.all());
}
