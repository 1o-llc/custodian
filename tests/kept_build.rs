// Copyright © 2026 1o. All rights reserved.

//! `custodian test run`: its lines run on the workspace's one kept build (the proof's place), never a cold build in
//! the workspace; after a body swap in one crate, the crates compiled are that crate and its direct dependents, not
//! a dependent's dependent; a second run over the same code compiles nothing. And James's battery rule: on a dying
//! battery the cargo line is refused with its own code, and the job stays open.

mod common;

use common::*;
use std::path::{Path, PathBuf};

/// A three-crate chain, `base` ← `mid` ← `far`, each with a test that calls down the chain (so a test in `far`
/// reaches `base`), committed.
fn chain(name: &str) -> PathBuf {
    let d = fresh(name);
    let w = |p: &str, t: &str| {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, t).unwrap();
    };
    w("Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\n");
    w("crates/base/Cargo.toml", "[package]\nname = \"base\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    w("crates/base/src/lib.rs", "pub fn level() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn level_is_one() {\n        assert_eq!(super::level(), 1);\n    }\n}\n");
    w("crates/mid/Cargo.toml", "[package]\nname = \"mid\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbase = { path = \"../base\" }\n");
    w("crates/mid/src/lib.rs", "pub fn twice() -> u32 {\n    base::level() * 2\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn twice_is_two() {\n        assert_eq!(super::twice(), 2);\n    }\n}\n");
    w("crates/far/Cargo.toml", "[package]\nname = \"far\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmid = { path = \"../mid\" }\n");
    w("crates/far/src/lib.rs", "pub fn four() -> u32 {\n    mid::twice() * 2\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn four_is_four() {\n        assert_eq!(super::four(), 4);\n    }\n}\n");
    git(&d);
    d
}

/// The crates the kept build has compiled (cargo's own fingerprint folders, `<crate>-<hash>`).
fn compiled(target: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(target.join("debug/.fingerprint")).map(|r| r.flatten().filter_map(|e| e.file_name().to_string_lossy().rsplit_once('-').map(|(n, _)| n.to_string())).collect()).unwrap_or_default();
    v.sort();
    v.dedup();
    v
}

/// The newest modification among the kept build's compiled files.
fn newest(target: &Path) -> std::time::SystemTime {
    std::fs::read_dir(target.join("debug/deps")).unwrap().flatten().filter_map(|e| e.metadata().ok()?.modified().ok()).max().unwrap()
}

#[test]
fn a_test_run_after_a_swap_compiles_the_crate_and_its_direct_dependents_on_the_kept_build() {
    let d = chain("kept-build");
    let place = fresh("kept-build-place");
    let env = [("CODE_PASSENGER_PROOF_DIR", place.to_str().unwrap()), ("CUSTODIAN_BATTERY", "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t80%; charging; 1:00 remaining present: true\n")];
    let r = run_env(&d, &["init"], &env);
    assert_eq!(r.code, 0, "{}", r.all());
    // the swap: base::level's body, its signature kept
    std::fs::write(d.join("crates/base/src/lib.rs"), std::fs::read_to_string(d.join("crates/base/src/lib.rs")).unwrap().replace("    1\n", "    { 1 }\n")).unwrap();
    let r = run_env(&d, &["test", "run", "--affected"], &env);
    assert_eq!(r.code, 0, "{}", r.all());
    let out = r.all();
    assert!(out.contains("base lib") && out.contains("mid lib") && !out.contains("far lib"), "base's and mid's tests, not far's: {out}");
    let target = place.join("target");
    assert_eq!(compiled(&target), ["base", "mid"], "only the crate the change is in and its direct dependent compiled");
    assert!(!d.join("target").exists(), "nothing was built in the workspace");
    // the same code again: the kept build answers, nothing compiles
    let before = newest(&target);
    let r = run_env(&d, &["test", "run", "--affected"], &env);
    assert_eq!(r.code, 0, "{}", r.all());
    assert_eq!(newest(&target), before, "a second run compiled nothing: the build folder is the one kept");
}

#[test]
fn a_dying_battery_refuses_the_cargo_line_and_the_job_stays_open() {
    let d = chain("battery");
    let place = fresh("battery-place");
    let low = [("CODE_PASSENGER_PROOF_DIR", place.to_str().unwrap()), ("CUSTODIAN_BATTERY", "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t14%; discharging; 0:20 remaining present: true\n")];
    assert_eq!(run_env(&d, &["agent", "init"], &low).code, 0);
    assert_eq!(run_env(&d, &["job", "start", "swap", "--intent", "a body"], &low).code, 0);
    std::fs::write(d.join("crates/base/src/lib.rs"), std::fs::read_to_string(d.join("crates/base/src/lib.rs")).unwrap().replace("    1\n", "    { 1 }\n")).unwrap();
    for words in [&["test", "run", "--affected"][..], &["test", "run", "--all"][..]] {
        let r = run_env(&d, words, &low);
        assert_ne!(r.code, 0);
        assert!(r.all().contains("[CUST0042]") && r.all().contains("14%, not charging"), "{}", r.all());
    }
    assert!(!place.join("target").exists(), "no cargo line ran");
    let jobs = run_env(&d, &["job", "list"], &low);
    assert!(jobs.out.contains("running"), "the job stays open: {}", jobs.all());
}
