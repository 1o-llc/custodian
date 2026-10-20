//! `custodian test run` under feature configurations: `--features F`, `--all-features`, and `--matrix` (each line
//! once per configuration of its crate's check matrix), each line's target word carrying its configuration.

mod common;

use common::*;

/// The fixture with a feature `json` in core_lib and a test that exists only with it on.
fn with_a_feature(tag: &str) -> std::path::PathBuf {
    let d = fixture(tag);
    let m = d.join("crates/core_lib/Cargo.toml");
    std::fs::write(&m, std::fs::read_to_string(&m).unwrap() + "\n[features]\njson = []\n").unwrap();
    std::fs::create_dir_all(d.join("crates/core_lib/tests")).unwrap();
    std::fs::write(d.join("crates/core_lib/tests/configured.rs"), "#[cfg(feature = \"json\")]\n#[test]\nfn only_with_json() {}\n\n#[test]\nfn always() {}\n").unwrap();
    d
}

#[test]
fn a_test_run_takes_features_all_features_and_the_matrix() {
    let d = with_a_feature("features");
    let plain = ok(&d, &["test", "run", "--all"], 0);
    assert!(plain.contains("core_lib all: ") && !plain.contains("[features"), "{plain}");
    let json = ok(&d, &["test", "run", "--all", "--features", "json"], 0);
    assert!(json.contains("1 lines with features json; 1 lines as they are") && json.contains("core_lib all [features json]: ") && json.contains("app all: "), "{json}");
    let all = ok(&d, &["test", "run", "--all", "--all-features"], 0);
    assert!(all.contains("app all [all-features]: "), "{all}");
    let m = ok(&d, &["test", "run", "--all", "--matrix"], 0);
    assert!(m.contains("--matrix: ") && m.contains("core_lib all [no-default-features]: ") && m.contains("core_lib all [no-default-features, features json]: "), "{m}");
    // the configured test ran where its feature was on
    let on = m.lines().find(|l| l.starts_with("core_lib all [no-default-features, features json]: ")).unwrap();
    let off = m.lines().find(|l| l.starts_with("core_lib all [no-default-features]: ")).unwrap();
    let passed = |l: &str| l.split(": ").nth(1).and_then(|r| r.split(' ').next()).and_then(|n| n.parse::<u32>().ok()).unwrap();
    assert_eq!(passed(on), passed(off) + 1, "{on}\n{off}");
    let refused = run(&d, &["test", "run", "--all", "--features", "json", "--matrix"]);
    assert_ne!(refused.code, 0, "{}", refused.all());
}
