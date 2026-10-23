// Copyright © 2026 1o. All rights reserved.

//! custodian reads its own cost: the planned cost at `job start` and in the brief (each figure measured from this
//! workspace's receipts, or unknown), a job past its hours budget running and closing as any other (the hours are
//! information), and the closing
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
    assert!(started.contains("1 proof preflights × unknown (none ran here yet)") && started.contains("2 batch preflights (per function) × unknown") && started.contains("1 proof compiles × unknown") && started.contains("1 batch compiles × unknown"), "{started}");
    // three preflights one by one: measured now, and the closing names them as paid one at a time
    for n in ["token", "socket", "env-or"] {
        ok(&d, &["preflight", n], 0);
    }
    let brief = ok(&d, &["agent", "brief"], 0);
    assert!(brief.contains("## planned cost") && brief.contains("proof preflights × ") && brief.contains("s (measured)"), "{brief}");
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
fn a_job_past_its_hours_budget_runs_its_change_steps_and_tests_and_closes_accepted() {
    let bin = passenger_with_transform();
    let b = bin.as_deref();
    let d = fixture("budget");
    ok_with(&d, &["agent", "init"], b, 0);
    change_spec(&d, "token", "core_lib::token");
    change_spec(&d, "socket", "core_lib::socket");
    let started = ok_with(&d, &["job", "start", "tight", "--intent", "i", "--verify", "test:core_lib", "--budget", "hours=0.000001,battery=20"], b, 0);
    assert!(started.contains("budget: hours=0.000001,battery=20") && started.contains("only the battery stops a step"), "{started}");
    // far past its hours: every change step still runs (nothing is refused by hours)
    ok_with(&d, &["preflight", "token"], b, 0);
    ok_with(&d, &["preflight", "socket"], b, 0);
    if b.is_some() {
        ok_with(&d, &["transform", "token", "--write"], b, 0);
    }
    let brief = ok_with(&d, &["agent", "brief"], b, 0);
    assert!(brief.contains("over the budget by") && brief.contains("nothing is refused by hours"), "{brief}");
    ok_with(&d, &["test", "run", "--all"], b, 0);
    let closed = ok_with(&d, &["job", "close"], b, 0);
    assert!(closed.contains("result: accepted") && closed.contains("time: over the budget by"), "{closed}");
    // a budget that is not one is still refused at start
    let r = run(&d, &["job", "start", "bad", "--intent", "i", "--budget", "days=2"]);
    assert_ne!(r.code, 0);
    assert!(r.all().contains("[CUST0043]"), "{}", r.all());
}
