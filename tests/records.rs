// Copyright © 2026 1o. All rights reserved.

//! The three lines that read the records, through the command line: `handoff` after a finished job (pinned whole,
//! its times, ids and receipt numbers read as words) and after a job that waits on a person; `why` of a route a
//! task history changed and a linkage record binds, with git's first commit from its own receipted line; `cost`,
//! a reading's cost the sum of its files and a reuse saving exactly that, and the brief's tokens line.
//!
//! `the_real_jobs_read_as_pinned` runs `handoff` over the private corpus named by `CUSTODIAN_PRIVATE_CORPUS`
//! (copies of real projects' `.custodian` records and their pinned texts, kept out of this public repository);
//! without it, it says so and passes.

mod common;

use common::*;
use std::path::Path;

/// The parts of a run that change from run to run, as words: a job's id, a time, a date, and how many runs the job
/// made (code-passenger's checkpoints at open and close are runs only where it is on PATH).
fn steady(text: &str) -> String {
    let counted = match text.split_once(" runs. ") {
        Some((head, tail)) => match head.rsplit_once(", ") {
            Some((h, n)) if n.chars().all(|c| c.is_ascii_digit()) => format!("{h}, N runs. {tail}"),
            _ => text.to_string(),
        },
        None => text.to_string(),
    };
    let text: &str = &counted;
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("job-") {
        out.push_str(&rest[..i]);
        let id = &rest[i + 4..];
        let n = id.chars().take_while(|c| c.is_ascii_hexdigit()).count();
        out.push_str(if n == 12 { "job-ID" } else { "job-" });
        rest = &id[if n == 12 { 12 } else { 0 }..];
    }
    out.push_str(rest);
    // `2026-10-19 21:19Z` and `21:19Z`
    let b: Vec<char> = out.chars().collect();
    let mut s = String::new();
    let mut i = 0;
    while i < b.len() {
        let date = i + 10 <= b.len() && b[i..i + 10].iter().enumerate().all(|(k, c)| if k == 4 || k == 7 { *c == '-' } else { c.is_ascii_digit() });
        let time = i + 6 <= b.len() && b[i..i + 6].iter().enumerate().all(|(k, c)| match k {
            2 => *c == ':',
            5 => *c == 'Z',
            _ => c.is_ascii_digit(),
        });
        if date {
            s.push_str("DATE");
            i += 10;
        } else if time {
            s.push_str("HH:MMZ");
            i += 6;
        } else {
            s.push(b[i]);
            i += 1;
        }
    }
    s
}

fn job_id(out: &str) -> String {
    out.split_whitespace().find(|w| w.starts_with("job-") && w.len() == 16).map(|w| w.trim_end_matches([':', ',', '.', ')'])).unwrap().to_string()
}

#[test]
fn a_handoff_after_a_finished_job_is_its_records_in_order() {
    let d = fixture("handoff");
    ok(&d, &["agent", "init"], 0);
    let q = d.join(".custodian/state/jobs/queued/batch");
    std::fs::create_dir_all(&q).unwrap();
    std::fs::write(q.join("v1.job.1o.job"), "{\n  job {\n    name \"batch\"\n    kind \"specs\"\n    count true\n  }\n}\n").unwrap();
    spec(&q, "a.toml", "batch_env", "readers_of_env", "FIX_SOCKET");
    let started = ok(&d, &["job", "start", "batch", "--intent", "say where the metrics are served", "--verify", "test:config_names"], 0);
    let id = job_id(&started);
    let lib = d.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn more() -> u8 {\n    2\n}\n").unwrap();
    ok(&d, &["test", "run", "--all", "--job", &id], 0);
    ok(&d, &["run", "crate:core_lib", "--kind", "dependents", "--job", &id], 0);
    let reused = ok(&d, &["run", "crate:core_lib", "--kind", "dependents", "--job", &id], 0);
    assert!(reused.contains("reused:"), "{reused}");
    let closed = ok(&d, &["job", "close", &id], 0);
    assert!(closed.contains("result: accepted"), "{closed}");
    let r = run(&d, &["handoff"]);
    assert_eq!(r.code, 0, "nothing waits on a person: {}", r.all());
    let text = steady(&r.out[..r.out.find("handoff of job").unwrap()]);
    let golden = include_str!("golden/handoff_fixture.txt");
    assert!(text == golden, "the handoff moved from tests/golden/handoff_fixture.txt; made now:\n{text}");
    // named by its name, or after a receipt
    assert_eq!(steady(&run(&d, &["handoff", "batch"]).out).lines().next(), text.lines().next());
    let since = ok(&d, &["handoff", "--since", &(seq_of(&closed) - 1).to_string()], 0);
    assert!(since.contains("Only what happened after receipt #") && since.contains("0 readings were made and 0 reused."), "{since}");
    assert!(ok(&d, &["handoff", "--for", "agent"], 0).contains("The routes and targets it read:\n- FIX_SOCKET\n- core_lib\n"));
}

#[test]
fn a_handoff_says_what_waits_on_a_person_first_and_the_agent_gets_the_lines() {
    let d = fixture("handoff-wait");
    ok(&d, &["agent", "init"], 0);
    ok(&d, &["job", "start", "socket", "--intent", "tidy the socket reads"], 0);
    ok(&d, &["job", "close"], 0);
    let r = run(&d, &["handoff", "socket", "--for", "agent"]);
    assert_eq!(r.code, 1, "a decision waits on a person: {}", r.all());
    let waiting = r.out.find("## Waiting on a person\n- Decision q-").expect("the decision, first");
    assert!(waiting < r.out.find("## What it was for").unwrap(), "{}", r.out);
    assert!(r.out.contains("what does done look like for socket?") && r.out.contains("tidy the socket reads."), "{}", r.out);
    assert!(r.out.contains("The lines, in order:\n    custodian decide q-") && r.out.contains("<answer> --why \"<the reason>\""), "{}", r.out);
    assert!(r.out.contains("1 waiting on a person"), "{}", r.out);
    // answered: decided, by whom the record says, never asked again
    let id = r.out.lines().find_map(|l| l.strip_prefix("- Decision ")).and_then(|l| l.split_whitespace().next()).unwrap().to_string();
    ok(&d, &["decide", &id, "test:core_lib", "--why", "its tests are the proof"], 0);
    let after = run(&d, &["handoff", "socket"]);
    assert!(after.out.contains(&format!("- {id}: what does done look like for socket? → test:core_lib. Why: its tests are the proof. By: the record names no one.")), "{}", after.out);
}

#[test]
fn why_says_who_introduced_a_route_and_for_what_from_the_records_and_git() {
    let d = fixture("why");
    ok(&d, &["agent", "init"], 0);
    git(&d);
    ok(&d, &["linkage", "add", "socket-key", "--kind", "connected", "--why", "the socket is read from this key", "--bind", "core_lib::ENV_SOCKET", "--bind", "crate:app"], 0);
    let q = d.join(".custodian/state/jobs/queued/grow");
    std::fs::create_dir_all(&q).unwrap();
    std::fs::write(q.join("v1.job.1o.job"), "{\n  job {\n    name \"grow\"\n    kind \"specs\"\n    count true\n  }\n}\n").unwrap();
    spec(&q, "a.toml", "grow_env", "readers_of_env", "FIX_SOCKET");
    ok(&d, &["job", "start", "grow", "--intent", "one more number from core_lib"], 0);
    let lib = d.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn more() -> u8 {\n    2\n}\n").unwrap();
    ok(&d, &["job", "close"], 0);
    let out = ok(&d, &["why", "crate:core_lib"], 0);
    assert!(out.starts_with("# why crate:core_lib\n\nThe crate core_lib, at crates/core_lib.\nIt came in with "), "{out}");
    assert!(out.contains(" by fixture, ") && out.contains(": fixture (git log, receipt #"), "git's first commit, with its receipt: {out}");
    assert!(out.contains("## task histories that changed it (1)\n\n- grow/base (job job-") && out.contains("one more number from core_lib — result pending — modify crates/core_lib/src/lib.rs"), "{out}");
    assert!(out.contains("- socket-key (connected): the socket is read from this key") && out.contains("binds core_lib::ENV_SOCKET to crate:app"), "{out}");
    // the git line kept its own receipt, in the kit's word
    let git_seq: i64 = out.split("(git log, receipt #").nth(1).unwrap().split(')').next().unwrap().parse().unwrap();
    assert_eq!(receipt(&d, git_seq)["command"], "git");
    // a route no record names: no record, never a guess
    let app = ok(&d, &["why", "crate:app"], 0);
    assert!(app.contains("No record: no task history has a change record in it.") && app.contains("binds crate:app to core_lib::ENV_SOCKET"), "{app}");
    assert_eq!(run(&d, &["why", "crates/core_lib"]).code, 2, "why reads crate: and decl: routes");
}

#[test]
fn a_readings_cost_is_its_files_a_reuse_saves_it_and_the_brief_says_both() {
    let d = fixture("cost");
    ok(&d, &["agent", "init"], 0);
    let first = ok(&d, &["run", "FIX_SOCKET", "--kind", "readers_of_env"], 0);
    let again = ok(&d, &["run", "FIX_SOCKET", "--kind", "readers_of_env"], 0);
    assert!(again.contains("reused:") && again.contains("saved by reuse: ~"), "{again}");
    let read = |seq: i64| -> serde_json::Value { serde_json::from_str(&std::fs::read_to_string(d.join(format!(".custodian/data/costs/{seq}.json"))).unwrap()).unwrap() };
    let (c, s) = (read(seq_of(&first)), read(seq_of(&again)));
    // the cost is the sum of the files the reading names, as they are on disk
    let files = c["files"].as_array().unwrap();
    assert!(!files.is_empty(), "{c}");
    let mut sum = 0;
    for f in files {
        let bytes = std::fs::metadata(d.join(f["path"].as_str().unwrap())).unwrap().len();
        assert_eq!(f["bytes"].as_u64(), Some(bytes));
        sum += bytes;
    }
    assert_eq!(c["cost"]["bytes"].as_u64(), Some(sum));
    assert_eq!(c["cost"]["tokens"].as_u64(), Some(sum.div_ceil(4)));
    // the reuse saves exactly that
    assert_eq!(s["saved"], c["cost"]);
    assert_eq!(s["reused_seq"].as_i64(), Some(seq_of(&first)));
    let tokens = custodian_kit::cost::grouped(sum.div_ceil(4));
    let cost = ok(&d, &["cost"], 0);
    assert!(cost.contains(&format!("- tokens: ~{tokens} read, ~{tokens} saved by reuse (estimate: bytes/4 of the files each reading touched)")), "{cost}");
    assert!(cost.contains("| readers_of_env | 1 |") && cost.contains("| - | 1 |"), "{cost}");
    let brief = ok(&d, &["agent", "brief"], 0);
    assert!(brief.contains(&format!("- tokens: ~{tokens} read, ~{tokens} saved by reuse (estimate: bytes/4 of the files each reading touched)")), "{brief}");
    assert!(!brief.contains("not measured here"), "{brief}");
}

/// The real jobs, read from a private corpus: `CUSTODIAN_PRIVATE_CORPUS/cases.txt`, one case a line:
/// `<project dir> <job> <james|agent> <pinned text file>`, each path relative to the corpus. Each project is
/// copied fresh, `custodian handoff JOB --for READER` run in the copy, and its text compared with the pinned one.
#[test]
fn the_real_jobs_read_as_pinned() {
    let Some(corpus) = std::env::var_os("CUSTODIAN_PRIVATE_CORPUS").map(std::path::PathBuf::from) else {
        eprintln!("CUSTODIAN_PRIVATE_CORPUS is not set: the real jobs are not read here");
        return;
    };
    let cases = std::fs::read_to_string(corpus.join("cases.txt")).unwrap();
    for line in cases.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let w: Vec<&str> = line.split_whitespace().collect();
        let (project, job, reader, pinned) = (w[0], w[1], w[2], w[3]);
        let dir = std::env::temp_dir().join("custodian-it").join(format!("corpus-{}-{}", job, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        copy_tree(&corpus.join(project), &dir);
        let r = run(&dir, &["handoff", job, "--for", reader]);
        let text = &r.out[..r.out.find(&format!("handoff of job {job}")).unwrap_or(r.out.len())];
        let want = std::fs::read_to_string(corpus.join(pinned)).unwrap();
        assert!(text == want, "{line}: the handoff moved from {pinned}; made now:\n{text}{}", r.err);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.file_name().is_some_and(|n| n == "journal.sqlite") {
            continue;
        }
        if p.is_dir() {
            copy_tree(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}
