//! Every subcommand on the fixture project, through the built program: the two modes, the cache, the queue, the
//! diff, the inventory, jobs as folders, triage, the tests, the checks, annotate, features, diagnostics, bundles,
//! the sweep, watch, the resident's socket, self install and the pre-edit hook.

mod common;

use common::*;
use std::path::Path;

#[test]
fn free_mode_prints_the_answer_and_stores_nothing() {
    let d = fixture("free");
    let out = ok(&d, &["run", "crate:core_lib", "--kind", "reach"], 0);
    assert!(out.contains("## binaries"), "{out}");
    assert!(out.contains("free mode (no .custodian here or above): nothing stored · custodian run crate:core_lib --kind reach → 0 (done)"), "{out}");
    assert!(!d.join(".custodian").exists());
}

#[test]
fn init_makes_the_folder_and_its_first_receipt() {
    let d = fixture("init");
    let out = ok(&d, &["init"], 0);
    for part in [".custodian/config/custodian.toml", ".custodian/config/v1.paths.1o.paths", ".custodian/config/features.toml", ".custodian/config/annotate.toml", ".custodian/state/journal.sqlite", ".custodian/data/specs/queued"] {
        assert!(d.join(part).exists(), "{part}\n{out}");
    }
    assert!(!d.join(".custodian/state/jobs").exists(), "plain mode has no jobs");
    assert_eq!(seq_of(&out), 1);
    let r = receipt(&d, 1);
    assert_eq!((r["command"].as_str(), r["exit_code"].as_i64(), r["reason"].as_str()), (Some("init"), Some(0), Some("done")));
    assert_eq!(r["line"], "custodian init");
    assert_eq!(r["plan_digest"].as_str().unwrap().len(), 64);
    // from a subdirectory, the folder above is found
    let out = ok(&d.join("crates/core_lib/src"), &["list"], 0);
    assert!(out.contains("journal: 1 receipts"), "{out}");
}

#[test]
fn the_same_search_is_reused_until_the_code_moves_or_again() {
    let d = fixture("cache");
    ok(&d, &["init"], 0);
    spec(&d, "env.toml", "env_socket", "readers_of_env", "FIX_SOCKET");
    let first = ok(&d, &["run", "env.toml"], 0);
    assert!(first.contains("decided (same_search): is there a reading of this spec over this code already? no: read"), "{first}");
    let n = seq_of(&first);
    let r = receipt(&d, n);
    assert_eq!(r["spec_hash"].as_str().unwrap().len(), 64);
    assert!(r["index_key"].as_str().unwrap().starts_with("sha256:") || r["index_key"].as_str().unwrap().len() >= 64, "{r}");
    let page = r["outputs"][0].as_str().unwrap().to_string();
    assert!(page.starts_with(".custodian/data/readings/env_socket/") && page.ends_with(".md"), "{page}");
    assert!(d.join(&page).is_file() && d.join(page.replace(".md", ".json")).is_file());

    let again = ok(&d, &["run", "env.toml"], 0);
    assert!(again.contains(&format!("yes: search #{n} from ")), "{again}");
    assert!(again.contains("same spec, same code: reused (--again reads again)"), "{again}");
    assert_eq!(receipt(&d, seq_of(&again))["reused_seq"], n);

    let forced = ok(&d, &["run", "env.toml", "--again"], 0);
    assert!(forced.contains("--again: read again"), "{forced}");
    assert!(receipt(&d, seq_of(&forced))["reused_seq"].is_null());

    // the code moves: a new index key, a new reading
    let lib = d.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn socket_again() -> String { std::env::var(\"FIX_SOCKET\").unwrap_or_default() }\n").unwrap();
    let moved = ok(&d, &["run", "env.toml"], 0);
    assert!(moved.contains("no: read"), "{moved}");

    // and the diff says what moved
    let diff = ok(&d, &["diff", "env_socket"], 0);
    assert!(diff.contains("compared env_socket: #") && diff.contains(": moved"), "{diff}");
    assert!(diff.contains("- reads: 1 → 2 (changed)") && diff.contains("## added (1)") && diff.contains("env-read **FIX_SOCKET**"), "{diff}");
}

#[test]
fn queued_specs_run_in_order_and_move_to_done() {
    let d = fixture("queue");
    ok(&d, &["init"], 0);
    spec(&d, ".custodian/data/specs/queued/a_env.toml", "a_env", "readers_of_env", "FIX_TOKEN");
    spec(&d, ".custodian/data/specs/queued/b_reach.toml", "b_reach", "reach", "crate:app");
    let out = ok(&d, &["run"], 0);
    let a = out.find(".custodian/data/specs/queued/a_env.toml: receipt #").unwrap();
    let b = out.find(".custodian/data/specs/queued/b_reach.toml: receipt #").unwrap();
    assert!(a < b, "{out}");
    assert!(out.contains("moved to done: .custodian/data/specs/queued/a_env.toml → .custodian/data/specs/done/a_env.toml"), "{out}");
    assert!(d.join(".custodian/data/specs/done/b_reach.toml").is_file());
    assert!(!d.join(".custodian/data/specs/queued/a_env.toml").exists());
    // a done spec runs again by its name
    let out = ok(&d, &["run", "a_env"], 0);
    assert!(out.contains("reused"), "{out}");
    let inv = ok(&d, &["list"], 0);
    assert!(inv.contains("- done: a_env, b_reach"), "{inv}");
    assert!(inv.contains("| a_env | readers_of_env | `FIX_TOKEN` |"), "{inv}");
    assert!(d.join(".custodian/data/inventory.md").is_file());
}

#[test]
fn a_refused_line_says_its_code_and_is_kept() {
    let d = fixture("refused");
    let r = run(&d, &["run", "x.toml", "--loud"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("[CUST1001] the words are not a declared custodian line: `--loud` is not a word of `custodian run`"), "{}", r.all());
    ok(&d, &["init"], 0);
    let r = run(&d, &["job", "list"]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.out.contains("[CUST1003] jobs are agent mode: custodian agent init"), "{}", r.all());
    assert!(r.out.contains("→ 2 (refused"), "the refusal has its receipt: {}", r.out);
    let r = run(&d, &["run", "nothing-here"]);
    assert!(r.out.contains("[CUST1201]"), "{}", r.all());
}

#[test]
fn a_folder_of_specs_is_a_job_from_start_to_close() {
    let d = fixture("job");
    ok(&d, &["agent", "init"], 0);
    for kind in ["triage", "test", "check"] {
        assert!(d.join(format!(".custodian/config/jobs/{kind}.toml")).is_file());
    }
    let q = d.join(".custodian/state/jobs/queued/batch");
    spec(&q, "a.toml", "batch_env", "readers_of_env", "FIX_SOCKET");
    spec(&q, "b.toml", "batch_reach", "reach", "crate:app");
    let out = ok(&d, &["job", "start", "batch"], 0);
    assert!(out.contains("decided (job_running): is a job already running? no: start"), "{out}");
    assert!(out.contains("steps: 2 in 2 stage(s): the folder's specs, in name order"), "{out}");
    let a = out.find("step batch_env done").unwrap();
    let b = out.find("step batch_reach done").unwrap();
    assert!(a < b, "{out}");
    assert!(d.join(".custodian/state/jobs/running/batch/v1.job.1o.job").is_file());
    assert!(d.join(".custodian/state/jobs/running/batch/readings/batch_env").is_dir(), "the readings are in the folder");

    // a second start is refused while one runs
    let second = run(&d, &["job", "start", "other"]);
    assert!(second.out.contains("[CUST1402]"), "{}", second.all());

    // a reading run while the job is open is tagged with it
    let tagged = ok(&d, &["run", "crate:core_lib", "--kind", "dependents"], 0);
    let id = receipt(&d, seq_of(&tagged))["job"].as_str().unwrap().to_string();
    assert!(id.starts_with("j-"), "{tagged}");

    let closed = ok(&d, &["job", "close"], 0);
    assert!(closed.contains(&format!("closed job {id}: 3 specs read")), "{closed}");
    let c: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join(".custodian/state/jobs/done/batch/closing.json")).unwrap()).unwrap();
    assert_eq!(c["schema"], "custodian.closing.v1");
    assert_eq!(c["specs_read"].as_array().unwrap().len(), 3);
    assert_eq!(c["steps"].as_array().unwrap().len(), 2);
    let brief = ok(&d, &["job", "brief", &id], 0);
    assert!(brief.contains("## specs read (3)") && brief.contains("## tests run (0)"), "{brief}");
    let list = ok(&d, &["job", "list"], 0);
    assert!(list.contains("## done (1)"), "{list}");

    // the job as one bundle, imported again under a new id with its origin
    let b = ok(&d, &["bundle", &id], 0);
    let file = b.lines().find_map(|l| l.strip_prefix("bundle: ")).unwrap().split(" (").next().unwrap().to_string();
    let imported = ok(&d, &["job", "import", &file], 0);
    assert!(imported.contains("imported ") && imported.contains("sha256:"), "{imported}");
    assert!(imported.contains("steps: 2 in 2 stage(s)"), "{imported}");
    let new_id = receipt(&d, seq_of(&imported))["job"].as_str().unwrap().to_string();
    assert_ne!(new_id, id);
    ok(&d, &["job", "close", &new_id], 0);
}

#[test]
fn triage_runs_its_template_and_closes_into_one_receipt() {
    let d = fixture("triage");
    ok(&d, &["agent", "init"], 0);
    let out = ok(&d, &["triage", "FIX0002"], 0);
    assert!(out.contains("started job") && out.contains("(triage)"), "{out}");
    assert!(out.contains("steps: 4 in 2 stage(s)"), "{out}");
    assert!(out.contains("step codes done") && out.contains("step reach done") && out.contains("step tests done") && out.contains("step marks done"), "{out}");
    assert!(out.contains("## specs read"), "{out}");
    assert!(out.contains("closing receipt: .custodian/state/jobs/done/triage-FIX0002/closing.md"), "{out}");
    assert!(d.join(".custodian/state/jobs/done/triage-FIX0002/steps/v1.triage-codes.1o.reading").is_file(), "a step's spec is written in 1o");
    assert!(d.join(".custodian/state/jobs/done/triage-FIX0002/v1.job.1o.job").is_file(), "the job file is written in 1o");
    // a code nothing raises: the reach step has nothing to read and is skipped by the step gate
    let none = ok(&d, &["triage", "NOPE9999"], 0);
    assert!(none.contains("step reach skipped: no: step codes gave step reach nothing"), "{none}");
}

#[test]
fn tests_run_with_receipts_and_report() {
    let d = fixture("tests");
    ok(&d, &["init"], 0);
    git(&d);
    let none = ok(&d, &["test"], 0);
    assert!(none.contains("the change since HEAD: 0 file(s)"), "{none}");
    let lib = d.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap().replace("format!(\"serving metrics on {addr}\")", "format!(\"serving metrics on {addr} \")")).unwrap();
    let out = ok(&d, &["test", "--affected", "--observe", "all"], 0);
    assert!(out.contains("the change since HEAD: 1 file(s)"), "{out}");
    assert!(out.contains("test metrics_say_where ... ok"), "the observer shows every line at all: {out}");
    assert!(out.contains("core_lib test:metrics: 1 passed, 0 failed"), "{out}");
    let all = ok(&d, &["test", "--all"], 0);
    assert!(all.contains("--all: 2 packages, one line each"), "{all}");
    assert!(all.contains("core_lib all: 3 passed"), "{all}");
    let report = ok(&d, &["test", "report"], 0);
    assert!(report.contains("## the slowest lines") && report.contains("core_lib all"), "{report}");
    assert!(report.contains("## the flakiest tests") , "{report}");
}

#[test]
fn check_annotate_and_features_route_to_their_programs() {
    let d = fixture("maint");
    ok(&d, &["init"], 0);
    let out = ok(&d, &["check", "all"], 0);
    assert!(out.contains("maintenance check all --workspace") && out.contains("→ 0 (clean)"), "{out}");
    let json = out.lines().find_map(|l| l.strip_prefix("the check's JSON: ")).unwrap().to_string();
    assert!(d.join(&json).is_file());
    let r = run(&d, &["check", "everything"]);
    assert_eq!(r.code, 2, "{}", r.all());
    let ann = ok(&d, &["annotate"], 0);
    assert!(ann.contains(&format!("from {json}")), "the plan reads the latest check receipt: {ann}");
    assert!(d.join(".custodian/data/plans/annotate.toml").is_file());
    let feat = ok(&d, &["features"], 0);
    assert!(feat.contains("code_passenger spec: .custodian/data/plans/klepto.toml"), "{feat}");
}

#[test]
fn diagnostics_list_where_and_from_a_log() {
    let d = fixture("diag");
    ok(&d, &["init"], 0);
    let list = ok(&d, &["diagnostics", "list"], 0);
    assert!(list.contains("codes declared: 2"), "{list}");
    let w = ok(&d, &["diagnostics", "where", "FIX0002"], 0);
    assert!(w.contains("codes in this reading: 1"), "{w}");
    std::fs::write(d.join("box.log"), "08:00 appd: [FIX0001] the socket path is not set\n").unwrap();
    let from = ok(&d, &["diagnostics", "from", "box.log"], 0);
    assert!(from.contains("FIX0001") && from.contains("codes in this reading: 1"), "{from}");
}

#[test]
fn a_bundle_is_a_word_checked_and_reused() {
    let d = fixture("bundle");
    ok(&d, &["init"], 0);
    let one = ok(&d, &["bundle", "crate:core_lib", "--no-tests"], 0);
    assert!(one.contains("filters: filters=no_tests|redact_secret"), "{one}");
    let file = one.lines().find_map(|l| l.strip_prefix("bundle: ")).unwrap().to_string();
    assert!(d.join(&file).is_file(), "{file}");
    let two = ok(&d, &["bundle", "crate:core_lib", "--no-tests"], 0);
    assert!(two.contains("reused:"), "{two}");
    let other = ok(&d, &["bundle", "crate:core_lib"], 0);
    assert!(other.contains("no: read"), "another word is another search: {other}");
    let r = run(&d, &["bundle", "crate:core_lib", "--reveal", "--redact-level", "secret"]);
    assert_eq!(r.code, 2);
    assert!(r.out.contains("reveal conflicts with") || r.out.contains("conflicts"), "{}", r.all());
}

#[test]
fn the_sweep_lists_an_undeclared_tree() {
    let d = fixture("sweep");
    ok(&d, &["init"], 0);
    let out = ok(&d, &["sweep", "marketplace"], 0);
    assert!(out.contains("read sweep-marketplace (sweep marketplace)") && out.contains("routes offered: 1"), "{out}");
}

#[test]
fn watch_shows_what_the_observer_is_woken_for() {
    let d = fixture("watch");
    ok(&d, &["agent", "init"], 0);
    ok(&d, &["triage", "FIX0001"], 0);
    let failed = ok(&d, &["watch", "--observe", "failed"], 0);
    assert!(failed.contains(" running: ") && !failed.contains("step codes"), "{failed}");
    let progress = ok(&d, &["watch", "--observe", "progress"], 0);
    assert!(progress.contains("step codes done"), "{progress}");
    let since = ok(&d, &["watch", "--observe", "all", "--since", "1000"], 0);
    assert!(since.contains(": 0 shown"), "{since}");
}

#[test]
fn the_resident_answers_on_its_socket_and_stops() {
    let d = fixture("serve");
    ok(&d, &["agent", "init"], 0);
    spec(&d, "env.toml", "env_socket", "readers_of_env", "FIX_SOCKET");
    let mut child = std::process::Command::new(BIN).arg("serve").current_dir(&d).stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut answered = None;
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let r = run(&d, &["run", "env.toml"]);
        if r.out.contains("answered by the resident") {
            answered = Some(r.out);
            break;
        }
    }
    let first = answered.expect("the resident answered");
    assert!(first.contains("no: read"), "{first}");
    let second = ok(&d, &["run", "env.toml"], 0);
    assert!(second.contains("answered by the resident") && second.contains("reused"), "{second}");
    let busy = run(&d, &["serve"]);
    assert!(busy.err.contains("[CUST1503]"), "{}", busy.all());
    let stop = ok(&d, &["stop"], 0);
    assert!(stop.contains("stops") && stop.contains("answered by the resident"), "{stop}");
    let status = child.wait().unwrap();
    assert!(status.success());
    let after = ok(&d, &["run", "env.toml"], 0);
    assert!(!after.contains("answered by the resident") && after.contains("reused"), "the command line does it itself: {after}");
}

#[test]
fn self_install_plans_installs_keeps_the_receipt_and_uninstalls() {
    let d = fixture("install");
    ok(&d, &["init"], 0);
    let bin = d.join("inst/bin");
    let state = d.join("inst/state");
    let (b, s) = (bin.to_str().unwrap(), state.to_str().unwrap());
    let plan = ok(&d, &["self", "install", "--bin-dir", b, "--state-dir", s, "--dry-run"], 0);
    assert!(plan.contains("--dry-run: nothing written") && !bin.join("custodian").exists(), "{plan}");
    let done = ok(&d, &["self", "install", "--bin-dir", b, "--state-dir", s], 0);
    assert!(bin.join("custodian").is_file(), "{done}");
    let status = ok(&d, &["self", "status", "--bin-dir", b, "--state-dir", s], 0);
    assert!(status.contains("the file matches its receipt"), "{status}");
    let inv = ok(&d, &["list"], 0);
    assert!(inv.contains("## installs") && inv.contains("install custodian"), "{inv}");
    ok(&d, &["self", "uninstall", "--bin-dir", b, "--state-dir", s], 0);
    assert!(!bin.join("custodian").exists());
    ok(&d, &["self", "status", "--bin-dir", b, "--state-dir", s], 1);
}

#[test]
fn the_pre_edit_hook_prints_what_a_file_reaches() {
    let d = fixture("hook");
    let file = d.join("crates/core_lib/src/lib.rs");
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("hooks/pre_edit_reach.sh");
    let mut child = std::process::Command::new(hook)
        .env("CUSTODIAN", BIN)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(format!("{{\"tool_name\":\"Edit\",\"tool_input\":{{\"file_path\":\"{}\"}}}}", file.display()).as_bytes()).unwrap();
    let o = child.wait_with_output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success());
    assert!(out.contains("binaries this file reaches:") && out.contains("- app: appd"), "{out}");
    assert!(out.contains("read reach-crates_core_lib_src_lib_rs (reach crates/core_lib/src/lib.rs)"), "{out}");
    assert!(out.contains("free mode"), "{out}");
}

fn passenger_here() -> bool {
    let on = custodian::maint::passenger_installed();
    if !on {
        eprintln!("code-passenger is not on PATH: this test's writing half is skipped");
    }
    on
}

#[test]
fn a_count_is_a_reading_with_history_reports_and_growth() {
    let d = fixture("count");
    ok(&d, &["init"], 0);
    let one = ok(&d, &["count", "--tests"], 0);
    assert!(one.contains("## by crate") && one.contains("| core_lib |") && one.contains("test lines"), "{one}");
    assert!(one.contains("counted *: ") && one.contains("digest sha256:"), "{one}");
    let a = seq_of(&one);
    let by_lang = ok(&d, &["count", "crate:core_lib", "--by", "language"], 0);
    assert!(by_lang.contains("## by language") && by_lang.contains("| Rust |"), "{by_lang}");
    let reused = ok(&d, &["count", "--tests"], 0);
    assert!(reused.contains("reused"), "the same count over the same code is a reuse: {reused}");
    let lib = d.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn grown() -> u8 {\n    1\n}\n").unwrap();
    let two = ok(&d, &["count"], 0);
    let b = seq_of(&two);
    let diff = ok(&d, &["diff", "count-workspace"], 0);
    assert!(diff.contains("grew: +4 −0 across 1 files in 1 crates"), "{diff}");
    let hist = ok(&d, &["count", "history"], 0);
    let (pa, pb) = (hist.find(&format!("| #{a} |")).unwrap(), hist.find(&format!("| #{b} |")).unwrap());
    assert!(pb < pa, "newest first: {hist}");
    let only = ok(&d, &["count", "history", "core_lib"], 0);
    assert!(only.contains("of crate:core_lib") && !only.contains(&format!("| #{a} |")), "{only}");
    let rep = ok(&d, &["count", "report", &a.to_string(), &b.to_string()], 0);
    assert!(rep.contains(&format!("# count report: #{a} → #{b} (*)")) && rep.contains("| digest | sha256:") && rep.contains("| commit |"), "{rep}");
    assert!(rep.contains("- lines: +4 −0"), "{rep}");
    let file = rep.lines().find_map(|l| l.strip_prefix("report: ")).unwrap().split(" (+ ").next().unwrap().to_string();
    assert!(d.join(&file).is_file(), "the report is a reading: {file}");
    let today = receipt(&d, a)["utc"].as_str().unwrap()[..10].to_string();
    let amb = run(&d, &["count", "report", &today, &b.to_string()]);
    assert_eq!(amb.code, 2);
    assert!(amb.out.contains("[CUST1205]"), "a date that fits several counts is refused: {}", amb.all());
    let none = run(&d, &["count", "report", "nothing", &b.to_string()]);
    assert!(none.out.contains("[CUST1204]"), "{}", none.all());
}

#[test]
fn a_job_that_counts_says_the_lines_it_touched() {
    let d = fixture("count-job");
    ok(&d, &["agent", "init"], 0);
    let q = d.join(".custodian/state/jobs/queued/grow");
    std::fs::create_dir_all(&q).unwrap();
    std::fs::write(q.join("v1.job.1o.job"), "{\n  job {\n    name \"grow\"\n    kind \"specs\"\n    count true\n  }\n}\n").unwrap();
    spec(&q, "a.toml", "grow_env", "readers_of_env", "FIX_TOKEN");
    let start = ok(&d, &["job", "start", "grow"], 0);
    assert!(start.contains("open count #"), "{start}");
    let main = d.join("crates/app/src/main.rs");
    std::fs::write(&main, std::fs::read_to_string(&main).unwrap() + "\npub fn more() -> u8 {\n    2\n}\n").unwrap();
    let close = ok(&d, &["job", "close"], 0);
    assert!(close.contains("lines touched: job: +4 −0 across 1 files in 1 crates (*)"), "{close}");
    let brief = ok(&d, &["job", "brief", "grow"], 0);
    assert!(brief.contains("## lines touched (1)") && brief.contains("counts/job-workspace.diff.json"), "{brief}");
    let c: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join(".custodian/state/jobs/done/grow/closing.json")).unwrap()).unwrap();
    assert_eq!(c["lines_touched"][0]["moved"]["added"]["lines"], 4);
    let rep = ok(&d, &["count", "report", "grow:open", "grow:close"], 0);
    assert!(rep.contains("- lines: +4 −0"), "{rep}");
    let amb = run(&d, &["count", "report", "grow", "grow:close"]);
    assert!(amb.out.contains("[CUST1205]") && amb.out.contains("<job>:open"), "{}", amb.all());
    if passenger_here() {
        assert!(c["passenger_open"].as_str().is_some() && c["passenger_close"].as_str().is_some(), "{c}");
        assert!(brief.contains("## the tree, as code_passenger keeps it"), "{brief}");
    }
}

#[test]
fn init_macros_annotate_apply_and_features_apply_write_through_code_passenger() {
    if !passenger_here() {
        return;
    }
    let d = fixture("writers");
    // a dependency only one public item of core_lib reaches: feature-eligible
    std::fs::create_dir_all(d.join("crates/helper/src")).unwrap();
    std::fs::write(d.join("crates/helper/Cargo.toml"), "[package]\nname = \"helper\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::write(d.join("crates/helper/src/lib.rs"), "pub fn shout(s: &str) -> String {\n    s.to_uppercase()\n}\n").unwrap();
    let toml = d.join("crates/core_lib/Cargo.toml");
    std::fs::write(&toml, std::fs::read_to_string(&toml).unwrap() + "\n[dependencies]\nhelper = { path = \"../helper\" }\n").unwrap();
    let lib = d.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn loud(s: &str) -> String {\n    helper::shout(s)\n}\n").unwrap();
    std::fs::remove_file(d.join("Cargo.lock")).ok();

    let init = ok(&d, &["init", "--macros"], 0);
    assert!(init.contains("dep add app maintenance --path") && init.contains("dep add core_lib maintenance --path"), "{init}");
    for c in ["app", "core_lib", "helper"] {
        assert!(std::fs::read_to_string(d.join(format!("crates/{c}/Cargo.toml"))).unwrap().contains("maintenance = { path = "), "{c}");
    }

    let line = std::fs::read_to_string(&lib).unwrap().lines().position(|l| l.contains("fn config_names")).unwrap() + 1;
    std::fs::create_dir_all(d.join(".maintenance/timings")).unwrap();
    std::fs::write(
        d.join(".maintenance/timings/latest.jsonl"),
        format!("{{\"test\":\"core_lib::tests::config_names\",\"file\":\"crates/core_lib/src/lib.rs\",\"line\":{line},\"ms\":40,\"max_ms\":2000,\"class\":null,\"threshold_from\":\"default\",\"outcome\":\"passed\",\"at_unix_ms\":1}}\n"),
    )
    .unwrap();
    let ann = ok(&d, &["annotate", "--apply"], 0);
    assert!(ann.contains("attr place --file crates/core_lib/src/lib.rs --item core_lib::tests::config_names"), "{ann}");
    let text = std::fs::read_to_string(&lib).unwrap();
    assert!(text.contains("    #[timed_test(max = \"250ms\")]\n    #[test]\n    fn config_names()"), "{text}");

    let feat = ok(&d, &["features", "--apply"], 0);
    assert!(feat.contains("**helper** in core_lib: feature-eligible") && feat.contains("gate --spec") && feat.contains("--write --package core_lib"), "{feat}");
    let toml = std::fs::read_to_string(&toml).unwrap();
    assert!(toml.contains("optional = true") && toml.contains("[features]"), "{toml}");
}
