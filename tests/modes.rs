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
    assert!(closed.contains("outcome known: #"), "the outcome became known when the decision was made: {closed}");
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
