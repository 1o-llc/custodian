//! The queue's runner: a job interrupted between its start and its close resumes from its checkpoint in the
//! journal at its close, its steps not run again; queued jobs run on the queue by their kind's class.

mod common;

use common::*;
use custodian::facade::Ctx;
use custodian::runtime::{self, Limit};

fn queue(d: &std::path::Path, name: &str, kind: &str, target: &str) {
    let q = d.join(format!(".custodian/state/jobs/queued/{name}"));
    std::fs::create_dir_all(&q).unwrap();
    std::fs::write(q.join("v1.job.1o.job"), format!("{{\n  job {{\n    name \"{name}\"\n    kind \"{kind}\"\n    target \"{target}\"\n  }}\n}}\n")).unwrap();
}

#[test]
fn an_interrupted_job_resumes_at_its_close() {
    let d = fixture("resume");
    ok(&d, &["agent", "init"], 0);
    queue(&d, "one", "triage", "FIX0001");

    // the runner stops after the start: the job stays in jobs/running with its checkpoint
    let mut ctx = Ctx::at(&d).unwrap();
    ctx.exe = BIN.into();
    let r = runtime::run_folders(&ctx, &["one".into()], false, Limit { stages: Some(1) }).unwrap();
    assert_eq!(r.interrupted, ["one"], "{:#?}", r.said);
    assert!(d.join(".custodian/state/jobs/running/one").is_dir());
    let id = custodian_kit::jobs::job_file(&d.join(".custodian/state/jobs/running/one")).id.unwrap();
    let journal = ctx.journal().unwrap();
    let (_, stage, bytes) = journal.latest_checkpoint(&id).unwrap().expect("a checkpoint");
    assert_eq!(stage, 1, "saved after the start");
    let saved: runtime::JobState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!((saved.stage, saved.job.as_deref()), (1, Some(id.as_str())));
    let steps_before = journal.receipts_of_job(&id).unwrap().len();
    drop(ctx);

    // a new context: job start resumes it from the journal, closing it without running its steps again
    let out = ok(&d, &["job", "start"], 0);
    assert!(out.contains(&format!("resumed job {id} at its close")), "{out}");
    assert!(d.join(".custodian/state/jobs/done/one/closing.json").is_file(), "{out}");
    let ctx = Ctx::at(&d).unwrap();
    let after = ctx.journal().unwrap().receipts_of_job(&id).unwrap();
    assert!(after.iter().filter(|r| r.name == "codes").count() == 1, "the steps ran once");
    assert!(after.len() > steps_before, "the close added its own receipts");
    assert!(ctx.journal().unwrap().latest_checkpoint(&id).unwrap().is_none(), "a finished job keeps no checkpoint");
}

#[test]
fn queued_jobs_run_on_the_queue_by_class_and_close() {
    let d = fixture("queue-jobs");
    ok(&d, &["agent", "init"], 0);
    queue(&d, "a_features", "features", "core_lib");
    queue(&d, "b_triage", "triage", "FIX0001");
    let out = ok(&d, &["run"], 0);
    assert!(out.contains("job folder b_triage (triage, Interactive)") && out.contains("job folder a_features (features, Batch)"), "{out}");
    assert!(d.join(".custodian/state/jobs/done/a_features/closing.md").is_file() && d.join(".custodian/state/jobs/done/b_triage/closing.md").is_file(), "{out}");
    // triage's class is higher: it was started first
    let ctx = Ctx::at(&d).unwrap();
    let jobs = ctx.journal().unwrap().jobs().unwrap();
    let first = jobs.iter().min_by_key(|j| j.opened_utc.clone()).unwrap();
    let names: Vec<String> = jobs.iter().map(|j| j.name.clone()).collect();
    assert!(first.name == "b_triage" || jobs.iter().filter(|j| j.opened_utc == first.opened_utc).any(|j| j.name == "b_triage"), "{names:?}");
}
