// Copyright © 2026 1o. All rights reserved.

//! custodian reads its own cost: the planned cost at `job start` and in the brief (each figure measured from this
//! workspace's receipts, or unknown), a step past the job's hours budget refused before it starts, and the closing
//! receipt's time per step kind with the kinds paid one member at a time named.

mod common;

use common::*;
use std::path::Path;

fn change_spec(d: &Path, name: &str, target: &str) {
    let dir = d.join(".custodian/data/specs/changes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.toml")), format!("name = \"{name}\"\nobjective = \"o\"\noperation = \"add_attribute\"\ntarget = \"{target}\"\nattribute = \"#[must_use]\"\n")).unwrap();
}

#[test]
fn the_planned_cost_is_measured_or_unknown_and_the_closing_names_steps_paid_one_at_a_time() {
    let d = fixture("costs");
    ok(&d, &["agent", "init"], 0);
    for (n, t) in [("token", "core_lib::token"), ("socket", "core_lib::socket"), ("env-or", "core_lib::env_or")] {
        change_spec(&d, n, t);
    }
    // nothing ran here yet: every figure is unknown, never a guess
    let started = ok(&d, &["job", "start", "attrs", "--intent", "three attributes"], 0);
    assert!(started.contains("planned cost: 3 preflights × unknown (none ran here yet), 1 compiles × unknown"), "{started}");
    // three preflights one by one: measured now, and the closing names them as paid one at a time
    for n in ["token", "socket", "env-or"] {
        ok(&d, &["preflight", n], 0);
    }
    let brief = ok(&d, &["agent", "brief"], 0);
    assert!(brief.contains("## planned cost") && brief.contains("3 preflights × ") && brief.contains("s (measured)"), "{brief}");
    ok(&d, &["job", "close"], 0);
    let done = std::fs::read_dir(d.join(".custodian/state/jobs/done")).unwrap().flatten().next().unwrap().path();
    let closing: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(done.join("closing.json")).unwrap()).unwrap();
    let kinds: Vec<&str> = closing["step_times"].as_array().unwrap().iter().filter_map(|t| t["kind"].as_str()).collect();
    assert!(kinds.contains(&"preflight"), "{closing}");
    let flags = closing["per_member"].as_array().unwrap();
    assert!(flags.iter().any(|f| f.as_str().unwrap().starts_with("3 preflights paid one by one") && f.as_str().unwrap().contains("preflight --batch")), "{closing}");
    assert!(std::fs::read_to_string(done.join("closing.md")).unwrap().contains("## time per step kind"));
}

#[test]
fn a_step_past_the_hours_budget_is_refused_before_it_starts_and_the_job_stays_open() {
    let d = fixture("budget");
    ok(&d, &["agent", "init"], 0);
    change_spec(&d, "token", "core_lib::token");
    change_spec(&d, "socket", "core_lib::socket");
    let started = ok(&d, &["job", "start", "tight", "--intent", "i", "--budget", "hours=0.000001,battery=20"], 0);
    assert!(started.contains("budget: hours=0.000001,battery=20"), "{started}");
    // the first preflight has no measured time here: nothing to refuse on
    ok(&d, &["preflight", "token"], 0);
    // now a preflight is measured, and one more would pass the budget
    let r = run(&d, &["preflight", "socket"]);
    assert_ne!(r.code, 0);
    assert!(r.all().contains("[CUST0043]") && r.all().contains("is measured at"), "{}", r.all());
    assert!(ok(&d, &["job", "list"], 0).contains("running"), "the job stays open");
    // a budget that is not one is refused at start
    ok(&d, &["job", "close"], 0);
    let r = run(&d, &["job", "start", "bad", "--intent", "i", "--budget", "days=2"]);
    assert_ne!(r.code, 0);
    assert!(r.all().contains("[CUST0043]"), "{}", r.all());
}
