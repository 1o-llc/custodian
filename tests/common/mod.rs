//! The fixture project and the program, for the tests: every test gets its own copy of `tests/fixtures/ws` in a
//! fresh directory under the system temp directory, and runs the built `custodian` in it.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const BIN: &str = env!("CARGO_BIN_EXE_custodian");

/// A fresh copy of the fixture workspace.
pub fn fixture(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join("custodian-it").join(format!("{name}-{}-{nanos}", std::process::id()));
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ws"), &dir);
    dir.canonicalize().unwrap()
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.file_name().is_some_and(|n| n == "target") {
            continue;
        }
        if p.is_dir() {
            copy(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// What a run of `custodian` left.
pub struct Ran {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Ran {
    pub fn all(&self) -> String {
        format!("{}{}", self.out, self.err)
    }
}

/// `custodian <words>` in `dir`.
pub fn run(dir: &Path, words: &[&str]) -> Ran {
    let o: Output = Command::new(BIN).args(words).current_dir(dir).env_remove("XDG_RUNTIME_DIR").output().unwrap();
    Ran { code: o.status.code().unwrap_or(-1), out: String::from_utf8_lossy(&o.stdout).into_owned(), err: String::from_utf8_lossy(&o.stderr).into_owned() }
}

/// `custodian <words>` that must exit with `code`.
pub fn ok(dir: &Path, words: &[&str], code: i32) -> String {
    let r = run(dir, words);
    assert_eq!(r.code, code, "custodian {}:\n{}", words.join(" "), r.all());
    r.out
}

/// A spec file in `dir`.
pub fn spec(dir: &Path, file: &str, name: &str, kind: &str, target: &str) -> PathBuf {
    let p = dir.join(file);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(&p, format!("name = \"{name}\"\nquestion = \"the fixture's question\"\nkind = \"{kind}\"\ntarget = \"{target}\"\n")).unwrap();
    p
}

/// The fixture as a git repository with one commit.
pub fn git(dir: &Path) {
    for args in [vec!["init", "-q"], vec!["add", "-A"], vec!["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture"]] {
        let s = Command::new("git").args(&args).current_dir(dir).status().unwrap();
        assert!(s.success(), "git {args:?}");
    }
}

/// The receipt number a run ended with.
pub fn seq_of(out: &str) -> i64 {
    let line = out.lines().rev().find(|l| l.starts_with("receipt #")).unwrap_or_else(|| panic!("no receipt line:\n{out}"));
    line["receipt #".len()..].split(':').next().unwrap().parse().unwrap()
}

/// A receipt file, read.
pub fn receipt(dir: &Path, seq: i64) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join(format!(".custodian/data/receipts/{seq}.json"))).unwrap()).unwrap()
}
