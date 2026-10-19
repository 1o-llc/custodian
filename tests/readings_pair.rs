//! The readings of a spec have one source, the receipts: `custodian diff` and `differences readings` pick the same
//! two (differences_kit's `reading_pair`), and the readings index is a cache rebuilt from the receipts.

mod common;
use common::*;
use std::path::Path;

/// The pair `custodian diff` compared, from its line `compared NAME: #A (…) → #B (…): …`.
fn pair_of(out: &str, name: &str) -> (i64, i64) {
    let line = out.lines().find(|l| l.starts_with(&format!("compared {name}: #"))).unwrap_or_else(|| panic!("no compared line:\n{out}"));
    let mut seqs = line.split('#').skip(1).map(|s| s.split(|c: char| !c.is_ascii_digit()).next().unwrap().parse::<i64>().unwrap());
    (seqs.next().unwrap(), seqs.next().unwrap())
}

/// The pair differences_kit picks (what `differences readings` compares).
fn kit_pair(root: &Path, name: &str, a: Option<i64>, b: Option<i64>) -> (i64, i64) {
    let p = differences_kit::Project::find(root).unwrap();
    let [x, y] = differences_kit::reading_pair(&p, name, a, b).unwrap();
    (x.0.seq, y.0.seq)
}

#[test]
fn custodian_diff_and_differences_readings_pick_the_same_pair_from_the_receipts() {
    let d = fixture("pair");
    ok(&d, &["init"], 0);
    spec(&d, "env.toml", "env_socket", "readers_of_env", "FIX_SOCKET");
    let first = seq_of(&ok(&d, &["run", "env.toml"], 0));
    let reused = seq_of(&ok(&d, &["run", "env.toml"], 0));
    assert_eq!(receipt(&d, reused)["reused_seq"], first);
    let lib = d.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn socket_again() -> String { std::env::var(\"FIX_SOCKET\").unwrap_or_default() }\n").unwrap();
    let moved = seq_of(&ok(&d, &["run", "env.toml"], 0));

    // the last two readings, a reuse left out
    let diff = ok(&d, &["diff", "env_socket"], 0);
    assert_eq!(pair_of(&diff, "env_socket"), (first, moved));
    assert_eq!(kit_pair(&d, "env_socket", None, None), (first, moved));
    // a reuse's number names the reading it reused, on both lines
    let from_reuse = ok(&d, &["diff", "env_socket", "--from", &reused.to_string()], 0);
    assert_eq!(pair_of(&from_reuse, "env_socket"), (first, moved));
    assert_eq!(kit_pair(&d, "env_socket", Some(reused), None), (first, moved));
    // a number that is no reading is refused on both lines
    assert_eq!(run(&d, &["diff", "env_socket", "--from", "1"]).code, 2);
    assert!(differences_kit::reading_pair(&differences_kit::Project::find(&d).unwrap(), "env_socket", Some(1), None).is_err());

    // the index is a cache: the journal moved away, it is rebuilt from the receipts with the readings they made
    let j = d.join(".custodian/state/journal.sqlite");
    std::fs::rename(&j, d.join(".custodian/state/journal.sqlite.before")).unwrap();
    let list = ok(&d, &["list"], 0);
    assert!(list.contains("2 readings, 0 jobs put back"), "{list}");
    let index: Vec<i64> = custodian::journal::Journal::open(&j).unwrap().readings_of("env_socket").unwrap().iter().map(|r| r.seq).collect();
    let receipts: Vec<i64> = differences_kit::readings_of(&differences_kit::Project::find(&d).unwrap(), "env_socket").unwrap().iter().map(|(r, _)| r.seq).collect();
    assert_eq!(index, [first, moved]);
    assert_eq!(index, receipts);
    assert_eq!(pair_of(&ok(&d, &["diff", "env_socket"], 0), "env_socket"), (first, moved));
}

/// A project's folder as it was committed (`CUSTODIAN_PAIR_DATA`: a copy of the project root holding `.custodian/`,
/// never the project itself, since `custodian diff` keeps a receipt): every spec with two readings is compared by both
/// lines, and both pick the same pair.
#[test]
#[ignore = "needs CUSTODIAN_PAIR_DATA, a copy of a project root holding .custodian/"]
fn on_a_projects_committed_folder_both_lines_pick_the_same_pair() {
    let root = std::path::PathBuf::from(std::env::var_os("CUSTODIAN_PAIR_DATA").expect("CUSTODIAN_PAIR_DATA"));
    let p = differences_kit::Project::find(&root).unwrap();
    let mut names: Vec<String> = p.journal().unwrap().runs().unwrap().into_iter().map(|r| r.name).collect();
    names.sort();
    names.dedup();
    let mut compared = 0;
    for name in names {
        if differences_kit::readings_of(&p, &name).unwrap().len() < 2 {
            continue;
        }
        let out = ok(&root, &["diff", &name], 0);
        assert_eq!(pair_of(&out, &name), kit_pair(&root, &name, None, None), "{name}");
        println!("{name}: {:?}", pair_of(&out, &name));
        compared += 1;
    }
    assert!(compared > 0, "no spec there has two readings");
    println!("{compared} spec(s) compared, the same pair on both lines");
}
