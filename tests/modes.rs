// Copyright © 2026 1o. All rights reserved.

//! The spec increment through the command line: the journal rebuilt from the receipts, the agent's brief and the
//! pre-edit guard, a bounded job written through a change spec (preflight, transform, close accepted) and one
//! refused for a hand edit, the decisions asked once, the plan of an intent, linkage records, the job lock, and the
//! help lines generated from the declarations.

mod common;

use common::*;
use std::path::Path;

const ADD_DIGEST: &str = "name = \"config-digest\"\nobjective = \"carry the config's digest\"\noperation = \"add_field\"\ntarget = \"core_lib::Config\"\n\n[field]\nname = \"digest\"\nty = \"String\"\nvis = \"pub\"\ndefault = \"String::new()\"\n";

fn change_spec(d: &Path, name: &str, text: &str) {
    let dir = d.join(".custodian/data/specs/changes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.toml")), text).unwrap();
}

#[test]
fn the_journal_is_rebuilt_from_the_receipts_a_checkout_carries() {
    let d = fixture("rebuild");
    ok(&d, &["init"], 0);
    spec(&d, "env.toml", "env_socket", "readers_of_env", "FIX_SOCKET");
    let first = ok(&d, &["run", "env.toml"], 0);
    let n = seq_of(&first);
    // the journal is state, not committed: a fresh checkout has the receipts and the readings, not the journal
    let j = d.join(".custodian/state/journal.sqlite");
    std::fs::rename(&j, d.join(".custodian/state/journal.sqlite.moved")).unwrap();
    let list = ok(&d, &["list"], 0);
    assert!(list.contains("rebuilt"), "the run says the journal was rebuilt: {list}");
    assert!(list.contains("| env_socket | readers_of_env |"), "{list}");
    // and the same search is found again in it
    let again = ok(&d, &["run", "env.toml"], 0);
    assert!(again.contains(&format!("yes: search #{n} from ")), "{again}");
}

#[test]
fn list_names_the_job_of_each_reading_and_counts_the_specs_in_jobs() {
    let d = fixture("list-jobs");
    ok(&d, &["agent", "init"], 0);
    ok(&d, &["job", "start", "look", "--intent", "who reads the socket key"], 0);
    let tagged = ok(&d, &["run", "FIX_SOCKET", "--kind", "readers_of_env"], 0);
    let id = receipt(&d, seq_of(&tagged))["job"].as_str().unwrap().to_string();
    let list = ok(&d, &["list"], 0);
    assert!(list.contains(&format!("look ({id})")), "the reading's row names its job: {list}");
}

#[test]
fn the_agent_brief_says_the_mode_first_and_the_guard_answers_the_hook() {
    let d = fixture("brief");
    ok(&d, &["agent", "init"], 0);
    let brief = ok(&d, &["agent", "brief"], 0);
    let first = brief.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')).unwrap_or_default().to_string();
    assert!(first.contains("mode: unbounded"), "the mode is the brief's first fact: {brief}");
    let head = ok(&d, &["agent", "brief", "--head"], 0);
    assert!(head.lines().count() < brief.lines().count(), "{head}");
    ok(&d, &["job", "start", "help", "--intent", "tidy the socket reads"], 0);
    let brief = ok(&d, &["agent", "brief"], 0);
    assert!(brief.contains("what does done look like for help?"), "an unknown outcome is the first decision: {brief}");
    // unbounded: a hand edit is allowed
    ok(&d, &["agent", "guard", "crates/core_lib/src/lib.rs"], 0);
}

#[test]
fn a_decision_is_asked_once_and_its_answer_declares_what_done_looks_like() {
    let d = fixture("decide");
    ok(&d, &["agent", "init"], 0);
    ok(&d, &["job", "start", "socket", "--intent", "tidy the socket reads"], 0);
    let open = run(&d, &["decide"]);
    assert_eq!(open.code, 1, "{}", open.all());
    let id = open.out.lines().find_map(|l| l.strip_prefix("- `").and_then(|r| r.split('`').next())).expect("a decision's id").to_string();
    let decided = ok(&d, &["decide", &id, "test:core_lib", "--why", "its tests are the proof"], 0);
    assert!(decided.contains("not asked again") && decided.contains("done is test:core_lib"), "{decided}");
    let none = ok(&d, &["decide"], 0);
    assert!(none.contains("none"), "{none}");
    let brief = ok(&d, &["agent", "brief"], 0);
    assert!(!brief.contains("what does done look like for socket?"), "answered, not asked again: {brief}");
    let closed = ok(&d, &["job", "close"], 0);
    assert!(closed.contains("outcome_known: false at open, known at #"), "the outcome became known when the decision was made: {closed}");
}

#[test]
fn an_intent_is_planned_and_a_step_becomes_a_change_spec() {
    let d = fixture("plan");
    ok(&d, &["init"], 0);
    let made = ok(&d, &["plan", "make", "add", "a", "digest", "to", "the", "config", "--name", "digest"], 0);
    assert!(made.contains("# plan") || made.contains("digest"), "{made}");
    let shown = ok(&d, &["plan", "show", "digest"], 0);
    assert!(shown.contains("## steps") || shown.contains("step"), "{shown}");
    let approved = ok(&d, &["plan", "approve", "digest", "1"], 0);
    assert!(approved.contains(".custodian/data/specs/changes/digest-1"), "{approved}");
    // the step's spec carries its TODO until it is filled in: the preflight says so and does not authorize it
    let name = approved.lines().find_map(|l| l.split(".custodian/data/specs/changes/").nth(1)).and_then(|r| r.split(' ').next()).unwrap().trim_end_matches(".toml").to_string();
    let pre = run(&d, &["preflight", &name]);
    assert_eq!(pre.code, 1, "{}", pre.all());
    assert!(pre.out.contains("TODO"), "{}", pre.out);
}

/// The lease-matrix job (receipts #108 to #123 of the kit): a plan made in a named job is the job's, the crates its
/// intent names are its targets, and a reading the job set out to make satisfies its `reading:` verification.
#[test]
fn a_plan_belongs_to_its_job_and_a_first_reading_verifies_it() {
    let d = fixture("plan-job");
    ok(&d, &["agent", "init"], 0);
    let started = ok(&d, &["job", "start", "scope", "--intent", "scope the merge of core_lib into app", "--verify", "reading:dependents-crate_core_lib"], 0);
    let id = started.split_whitespace().find(|w| w.starts_with("job-")).unwrap().to_string();
    let made = ok(&d, &["plan", "make", "merge", "core_lib", "into", "app,", "keeping", "every", "capability,", "the", "old", "crate", "kept", "on", "a", "branch", "--name", "merge", "--job", &id], 0);
    assert!(made.contains("named outright: crate:core_lib, crate:app"), "{made}");
    assert!(!made.contains("where does it go"), "{made}");
    assert_eq!(receipt(&d, seq_of(&made))["job"], id.as_str());
    let shown = ok(&d, &["plan", "show", "merge", "--job", &id], 0);
    assert_eq!(receipt(&d, seq_of(&shown))["job"], id.as_str());
    // a job that is not there is refused
    assert_eq!(run(&d, &["plan", "make", "merge", "core_lib", "--job", "job-000000000000"]).code, 2);
    // never read before the job: its first reading in the job satisfies the verification
    ok(&d, &["run", "crate:core_lib", "--kind", "dependents", "--job", &id], 0);
    let closed = ok(&d, &["job", "close", &id], 0);
    assert!(closed.contains("result: accepted"), "{closed}");
}

#[test]
fn a_linkage_record_is_added_and_checked() {
    let d = fixture("linkage");
    ok(&d, &["init"], 0);
    let refused = run(&d, &["linkage", "add", "socket"]);
    assert_eq!(refused.code, 2, "{}", refused.all());
    let added = ok(&d, &["linkage", "add", "socket-key", "--kind", "connected", "--why", "the socket is read from this key", "--bind", "core_lib::ENV_SOCKET", "--bind", "core_lib::socket"], 0);
    assert!(added.contains("record: .custodian/"), "{added}");
    let checked = run(&d, &["linkage", "check"]);
    assert!(checked.code == 0 || checked.code == 1, "{}", checked.all());
    assert!(checked.out.contains("socket-key"), "{}", checked.out);
}

#[test]
fn a_bounded_job_writes_its_code_through_a_change_spec_and_closes_accepted() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("no code-passenger with the transform line (CUSTODIAN_TEST_PASSENGER, or ../code_passenger built): skipped");
        return;
    };
    let b = Some(bin.as_path());
    let d = fixture("bounded");
    ok_with(&d, &["agent", "init", "--job-mode", "bounded"], b, 0);
    let start = ok_with(&d, &["job", "start", "digest", "--target", "core_lib::Config", "--intent", "carry the config's digest", "--verify", "test:core_lib"], b, 0);
    assert!(start.contains("bounded"), "{start}");
    // the hook's question: a hand edit of code is refused
    let guard = run_with(&d, &["agent", "guard", "crates/core_lib/src/lib.rs"], b);
    assert_eq!(guard.code, 2, "{}", guard.all());
    change_spec(&d, "config-digest", ADD_DIGEST);
    // a transform with no preflight is refused
    let early = run_with(&d, &["transform", "config-digest", "--write"], b);
    assert_eq!(early.code, 2, "{}", early.all());
    let pre = ok_with(&d, &["preflight", "config-digest"], b, 0);
    assert!(pre.contains("authorized"), "{pre}");
    let t = ok_with(&d, &["transform", "config-digest", "--write"], b, 0);
    assert!(t.contains("transform"), "{t}");
    let lib = std::fs::read_to_string(d.join("crates/core_lib/src/lib.rs")).unwrap();
    assert!(lib.contains("pub digest: String"), "{lib}");
    ok_with(&d, &["test", "run", "--all"], b, 0);
    let closed = ok_with(&d, &["job", "close"], b, 0);
    assert!(closed.contains("result: accepted"), "{closed}");
}

/// James's rule for a repeated shape: one member made alone and proved, the rest as one batch on their own
/// preflights: one compile, one transform receipt per member (its record its output), and the job's close covers each
/// member's lines as a single transform's.
#[test]
fn a_batch_of_one_shape_goes_under_one_proof_with_a_receipt_per_member_and_closes_accepted() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("no code-passenger with the transform line: skipped");
        return;
    };
    let b = Some(bin.as_path());
    let d = fixture("bounded-batch");
    ok_with(&d, &["agent", "init", "--job-mode", "bounded"], b, 0);
    ok_with(&d, &["job", "start", "must-use", "--target", "core_lib::env_or", "--intent", "every reader of the environment says its value is used", "--verify", "test:core_lib"], b, 0);
    let attr = |name: &str, target: &str| format!("name = \"{name}\"\nobjective = \"o\"\noperation = \"add_attribute\"\ntarget = \"{target}\"\nattribute = \"#[must_use]\"\n");
    // the one member of the shape alone, fully proved
    change_spec(&d, "env-or", &attr("env-or", "core_lib::env_or"));
    ok_with(&d, &["preflight", "env-or"], b, 0);
    let first = ok_with(&d, &["transform", "env-or", "--write"], b, 0);
    let proof = seq_of(&first);
    // the rest as one batch; a spec with no preflight is left out and named
    change_spec(&d, "token", &attr("token", "core_lib::token"));
    change_spec(&d, "socket", &attr("socket", "core_lib::socket"));
    change_spec(&d, "unread", &attr("unread", "core_lib::parse_mode"));
    ok_with(&d, &["preflight", "token"], b, 0);
    ok_with(&d, &["preflight", "socket"], b, 0);
    std::fs::write(d.join("batch.list"), "token\nsocket\nunread\n").unwrap();
    let p = proof.to_string();
    let batch = ok_with(&d, &["transform", "--batch", "batch.list", "--after-proof", &p, "--write"], b, 0);
    assert!(batch.contains(&format!("batch of 2 after proof #{p}")) && batch.contains("left out: unread"), "{batch}");
    let lib = std::fs::read_to_string(d.join("crates/core_lib/src/lib.rs")).unwrap();
    assert_eq!(lib.matches("#[must_use]").count(), 3, "{lib}");
    // a transform receipt per member, each its record
    let receipts: Vec<serde_json::Value> = std::fs::read_dir(d.join(".custodian/data/receipts")).unwrap().map(|e| serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap()).collect();
    for name in ["token", "socket"] {
        let r = receipts.iter().find(|r| r["command"] == "transform" && r["name"].as_str().is_some_and(|n| n.starts_with(&format!("{name} (batch of 2 after proof #{p}")))).unwrap_or_else(|| panic!("no receipt for {name}"));
        assert_eq!(r["accepted"], true);
        assert!(r["outputs"][0].as_str().unwrap().ends_with(&format!("{name}.json")));
    }
    ok_with(&d, &["test", "run", "--all"], b, 0);
    let closed = ok_with(&d, &["job", "close"], b, 0);
    assert!(closed.contains("result: accepted"), "{closed}");
}

/// `--preflight`: the batch preflights its members itself, in one run over one open workspace, each with its own
/// preflight receipt; a member the preflight refuses is left out and named.
#[test]
fn a_batch_preflights_its_members_in_one_run() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("no code-passenger with the transform line: skipped");
        return;
    };
    let b = Some(bin.as_path());
    let d = fixture("bounded-batch-preflight");
    ok_with(&d, &["agent", "init", "--job-mode", "bounded"], b, 0);
    ok_with(&d, &["job", "start", "must-use", "--target", "core_lib::env_or", "--intent", "i", "--verify", "test:core_lib"], b, 0);
    let attr = |name: &str, target: &str| format!("name = \"{name}\"\nobjective = \"o\"\noperation = \"add_attribute\"\ntarget = \"{target}\"\nattribute = \"#[must_use]\"\n");
    change_spec(&d, "env-or", &attr("env-or", "core_lib::env_or"));
    ok_with(&d, &["preflight", "env-or"], b, 0);
    let proof = seq_of(&ok_with(&d, &["transform", "env-or", "--write"], b, 0)).to_string();
    change_spec(&d, "token", &attr("token", "core_lib::token"));
    change_spec(&d, "socket", &attr("socket", "core_lib::socket"));
    change_spec(&d, "nowhere", &attr("nowhere", "core_lib::no_such_function"));
    std::fs::write(d.join("batch.list"), "token\nsocket\nnowhere\n").unwrap();
    let batch = ok_with(&d, &["transform", "--batch", "batch.list", "--preflight", "--after-proof", &proof, "--write"], b, 0);
    assert!(batch.contains("preflights: 2 of 3 authorized, over one open workspace"), "{batch}");
    assert!(batch.contains(&format!("batch of 2 after proof #{proof}")) && batch.contains("left out: nowhere"), "{batch}");
    let lib = std::fs::read_to_string(d.join("crates/core_lib/src/lib.rs")).unwrap();
    assert_eq!(lib.matches("#[must_use]").count(), 3, "{lib}");
    ok_with(&d, &["test", "run", "--all"], b, 0);
    assert!(ok_with(&d, &["job", "close"], b, 0).contains("result: accepted"));
}

/// `preflight --batch LIST`: one reading of the code for every member, each member's verdict its own preflight's, one
/// receipt listing them; the batch transform takes that receipt as each member's gate.
#[test]
fn a_batch_preflight_is_one_receipt_and_the_batch_transform_goes_on_it() {
    let d = fixture("preflight-batch");
    ok(&d, &["agent", "init", "--job-mode", "bounded"], 0);
    let attr = |name: &str, target: &str| format!("name = \"{name}\"\nobjective = \"o\"\noperation = \"add_attribute\"\ntarget = \"{target}\"\nattribute = \"#[must_use]\"\n");
    change_spec(&d, "token", &attr("token", "core_lib::token"));
    change_spec(&d, "socket", &attr("socket", "core_lib::socket"));
    change_spec(&d, "nowhere", &attr("nowhere", "core_lib::no_such_function"));
    std::fs::write(d.join("batch.list"), "token\nsocket\nnowhere\n").unwrap();
    // each member alone first: the verdicts the batch must give
    let alone: Vec<i32> = ["token", "socket", "nowhere"].iter().map(|n| run(&d, &["preflight", n]).code).collect();
    assert_eq!(alone, [0, 0, 1]);
    let out = ok(&d, &["preflight", "--batch", "batch.list"], 1);
    assert!(out.contains("authorized: **2 of 3**") && out.contains("| token | authorized |") && out.contains("| nowhere | not authorized |"), "{out}");
    let line = out.lines().find(|l| l.contains("batch preflight record: ")).unwrap_or_else(|| panic!("{out}"));
    let r = receipt(&d, line.rsplit_once("receipt #").unwrap().1.split(' ').next().unwrap().parse().unwrap());
    assert_eq!((r["command"].as_str(), r["reason"].as_str()), (Some("preflight-batch"), Some("2 of 3 authorized")));
    let Some(bin) = passenger_with_transform() else {
        eprintln!("no code-passenger with the transform line: the transform half skipped");
        return;
    };
    let b = Some(bin.as_path());
    ok_with(&d, &["job", "start", "must-use", "--target", "core_lib::env_or", "--intent", "i", "--verify", "test:core_lib"], b, 0);
    change_spec(&d, "env-or", &attr("env-or", "core_lib::env_or"));
    ok_with(&d, &["preflight", "env-or"], b, 0);
    let proof = seq_of(&ok_with(&d, &["transform", "env-or", "--write"], b, 0)).to_string();
    // the code moved: the batch is preflighted again, then the transform goes on that one receipt
    ok_with(&d, &["preflight", "--batch", "batch.list"], b, 1);
    let batch = ok_with(&d, &["transform", "--batch", "batch.list", "--after-proof", &proof, "--write"], b, 0);
    assert!(batch.contains(&format!("batch of 2 after proof #{proof}")) && batch.contains("left out: nowhere"), "{batch}");
    let lib = std::fs::read_to_string(d.join("crates/core_lib/src/lib.rs")).unwrap();
    assert_eq!(lib.matches("#[must_use]").count(), 3, "{lib}");
}

#[test]
fn a_bounded_job_with_a_hand_edit_is_refused_at_close_and_rejected() {
    let Some(bin) = passenger_with_transform() else {
        eprintln!("no code-passenger with the transform line: skipped");
        return;
    };
    let b = Some(bin.as_path());
    let d = fixture("bounded-hand");
    ok_with(&d, &["agent", "init"], b, 0);
    ok_with(&d, &["job", "start", "hand", "--mode", "bounded", "--target", "core_lib::Config", "--intent", "i", "--verify", "test:core_lib"], b, 0);
    let lib = d.join("crates/core_lib/src/lib.rs");
    // the pre-edit hook blocks the edit (exit 2) and says why on stderr
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("hooks/pre_edit_reach.sh");
    let mut child = std::process::Command::new(hook)
        .env("CUSTODIAN", BIN)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(format!("{{\"tool_name\":\"Edit\",\"tool_input\":{{\"file_path\":\"{}\"}}}}", lib.display()).as_bytes()).unwrap();
    let o = child.wait_with_output().unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("bounded"), "{}", String::from_utf8_lossy(&o.stderr));
    // an edit made anyway is found at close
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn by_hand() {}\n").unwrap();
    let closed = run_with(&d, &["job", "close"], b);
    assert_eq!(closed.code, 2, "{}", closed.all());
    assert!(closed.all().contains("CUST0031") && closed.out.contains("rejected"), "{}", closed.all());
}

#[test]
fn a_bounded_job_needs_its_writer() {
    let d = fixture("no-writer");
    ok(&d, &["agent", "init"], 0);
    // a PATH with no code-passenger on it
    let o = std::process::Command::new(BIN).args(["job", "start", "b", "--mode", "bounded"]).current_dir(&d).env("PATH", "/usr/bin:/bin").output().unwrap();
    let all = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert_eq!(o.status.code(), Some(2), "{all}");
    assert!(all.contains("CUST0037"), "{all}");
}

#[test]
fn two_job_starts_at_once_open_one_job() {
    let d = fixture("lock");
    ok(&d, &["agent", "init"], 0);
    assert!(d.join(".custodian/jobs.lock").exists());
    let spawn = |name: &str| std::process::Command::new(BIN).args(["job", "start", name]).current_dir(&d).output().unwrap();
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| spawn("one"));
        let b = s.spawn(|| spawn("two"));
        (a.join().unwrap(), b.join().unwrap())
    });
    let codes = [a.status.code(), b.status.code()];
    assert_eq!(codes.iter().filter(|c| **c == Some(0)).count(), 1, "{codes:?}\n{}\n{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&b.stdout));
    let list = ok(&d, &["job", "list"], 0);
    assert!(list.contains("## running (1)"), "{list}");
}

#[test]
fn the_help_lines_are_generated_from_the_declarations() {
    let d = std::env::temp_dir();
    let r = run(&d, &[]);
    let golden = include_str!("golden/help.txt");
    assert!(r.out == golden, "the help moved from tests/golden/help.txt; the help made now:\n{}", r.out);
}
