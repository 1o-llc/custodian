//! `custodian requirements` and the `requirements` reading kind through the command line: a broken manifest read
//! without a scan, the same report reused by the journal until a manifest or the lock changes, nothing written into
//! the workspace, the line a preflight names, a plan that folds the findings into its reuse and steps, and the
//! agent brief that carries what is unresolved (the brief's acceptance cases 1, 6, 7, 9 and 10 at the line).

mod common;

use common::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every file of the workspace (the custodian folder left out) and its bytes.
fn workspace_files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut work = vec![dir.to_path_buf()];
    while let Some(d) = work.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".custodian" || n == "target") {
                continue;
            }
            if p.is_dir() {
                work.push(p);
            } else {
                out.insert(p.clone(), std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

fn record_of(d: &Path, out: &str) -> serde_json::Value {
    let seq = seq_of(out);
    let r = receipt(d, seq);
    let json = r["outputs"].as_array().unwrap().iter().filter_map(|o| o.as_str()).find(|o| o.ends_with(".json")).unwrap_or_else(|| panic!("no record: {r:#}")).to_string();
    serde_json::from_str(&std::fs::read_to_string(d.join(json)).unwrap()).unwrap()
}

#[test]
fn a_broken_manifest_is_read_without_a_scan_and_its_finding_names_the_root() {
    let d = fixture_of("req-broken", "req_inherit");
    ok(&d, &["init"], 0);
    // klepto cannot scan this workspace (cargo refuses it); the reading answers anyway (a reading: exit 0, its
    // findings in the report, so the journal can reuse it)
    let out = ok(&d, &["requirements"], 0);
    assert!(out.contains("`workspace_dependency_missing` · one_db_kernel: `hex` inherits from the workspace, and governing root Cargo.toml has no [workspace.dependencies].hex"), "{out}");
    assert!(out.contains("add `hex` to [workspace.dependencies] in Cargo.toml"), "{out}");
    assert!(out.contains("mutation performed: none"), "{out}");
    let rec = record_of(&d, &out);
    assert_eq!(rec["kind"], "requirements");
    assert_eq!(rec["requirements"]["state"]["agreement"].as_str().unwrap().split(':').next(), Some("agree"));
    // the same reading as a search on the run line: one key, so it is reused
    let again = ok(&d, &["run", ".", "--kind", "requirements"], 0);
    assert!(again.contains("reused:"), "{again}");
    // the agent brief carries what is unresolved
    let brief = ok(&d, &["agent", "brief"], 0);
    assert!(brief.contains("## unresolved requirements"), "{brief}");
    assert!(brief.contains("error · observed · workspace_dependency_missing · one_db_kernel"), "{brief}");
    assert!(brief.contains("the manifests and the lock it read are unchanged"), "{brief}");
}

#[test]
fn the_report_is_reused_until_a_manifest_or_the_lock_changes_and_the_workspace_is_never_written() {
    let d = fixture_of("req-reuse", "req_add");
    ok(&d, &["init"], 0);
    let before = workspace_files(&d);
    let first = ok(&d, &["requirements", "--intent", "add util to app"], 0);
    assert!(first.contains("`app` does not declare `util`"), "{first}");
    assert_eq!(workspace_files(&d), before, "no file of the workspace was written");
    // the same manifests, lock and intent: reused
    let second = ok(&d, &["requirements", "--intent", "add util to app"], 0);
    assert!(second.contains("reused:"), "{second}");
    // a source edit is not an input
    std::fs::write(d.join("app/src/lib.rs"), "//! changed\npub fn f() {}\n").unwrap();
    assert!(ok(&d, &["requirements", "--intent", "add util to app"], 0).contains("reused:"));
    // a manifest edit is: read again, and now the need is met
    let m = d.join("app/Cargo.toml");
    std::fs::write(&m, std::fs::read_to_string(&m).unwrap() + "util = { path = \"../util\", version = \"0.3\" }\n").unwrap();
    let third = ok(&d, &["requirements", "--intent", "add util to app"], 0);
    assert!(!third.contains("reused:"), "{third}");
    assert!(third.contains("`app` declares `util`"), "{third}");
    // the brief sees the manifests move under an older reading
    std::fs::write(&m, std::fs::read_to_string(&m).unwrap() + "# moved\n").unwrap();
    assert!(ok(&d, &["agent", "brief"], 0).contains("the manifests or the lock moved since: read again"));
}

#[test]
fn a_change_spec_gives_the_needs_and_its_preflight_names_the_line() {
    let d = fixture_of("req-spec", "req_add");
    ok(&d, &["init"], 0);
    let spec = d.join("add-util.toml");
    std::fs::write(&spec, "name = \"add-util\"\nobjective = \"app formats with util\"\noperation = \"add_dependency\"\ntarget = \"crate:app\"\n\n[dependency]\nname = \"util\"\nversion = \"0.3\"\nfeatures = [\"fmt\"]\n").unwrap();
    let out = ok(&d, &["requirements", "--spec", "add-util.toml"], 0);
    assert!(out.contains("need: `util` `0.3` features fmt in `app` (spec add-util: add_dependency)"), "{out}");
    assert!(out.contains("nothing in `app` turns on `util`'s `fmt`"), "{out}");
    let rec = record_of(&d, &out);
    assert_eq!(rec["target"], "crate:app", "the spec's crate is the route");
    assert_eq!(rec["requirements"]["needs"][0]["said"], "spec add-util: add_dependency");
    // the preflight names the line and decides nothing on it
    let pre = run(&d, &["preflight", "add-util.toml"]);
    assert!(pre.all().contains("requirements: custodian requirements crate:app --spec add-util.toml"), "{}", pre.all());
}

#[test]
fn a_plan_folds_the_requirements_into_its_reuse_and_steps() {
    let d = fixture_of("req-plan", "req_add");
    ok(&d, &["init"], 0);
    let out = run(&d, &["plan", "make", "add util to app"]);
    let plan: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join(".custodian/data/plans/intent/add-util-to-app.json")).unwrap_or_else(|e| panic!("{e}: {}", out.all()))).unwrap();
    let req = &plan["requirements"];
    assert!(req["unresolved"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("`app` does not declare `util`")), "{req:#}");
    assert_eq!(plan["steps"][0]["operation"], "add_dependency", "{plan:#}");
    assert_eq!(plan["steps"][0]["target"], "crate:app");
    assert!(plan["reuse"].as_array().unwrap().iter().any(|r| r["name"] == "requirements" && r["what"].as_str().unwrap().contains("the lock holds util 0.3.1")), "{plan:#}");
    let md = std::fs::read_to_string(d.join(".custodian/data/plans/intent/add-util-to-app.md")).unwrap();
    assert!(md.contains("## requirements (reading #"), "{md}");
}

#[test]
fn the_faces_word_is_checked_on_the_line() {
    let d = fixture_of("req-faces", "req_add");
    let r = run(&d, &["requirements", "--faces", "lock,no_cargo"]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("lock") && r.all().contains("no_cargo"), "{}", r.all());
    let r = run(&d, &["requirements", "--faces", "loud"]);
    assert_eq!(r.code, 2, "{}", r.all());
    // free mode, manifests only: printed, nothing kept, cargo not run
    let out = ok(&d, &["requirements", "--faces", "prerequisites,no_cargo"], 0);
    assert!(out.contains("cargo metadata: not run: no_cargo"), "{out}");
    assert!(!d.join(".custodian").exists());
}
