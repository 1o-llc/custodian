//! The runner: a job interrupted between stages resumes from its checkpoint in the journal at the stage after the
//! last one it completed, and the stages before it are not run again.

mod common;

use common::*;
use custodian::facade::Ctx;
use custodian::jobs::{self, StartAsk};
use custodian::runtime::Limit;

#[test]
fn an_interrupted_job_resumes_at_its_last_completed_stage() {
    let d = fixture("resume");
    ok(&d, &["agent", "init"], 0);
    let q = d.join(".custodian/state/jobs/queued/three");
    spec(&q, "a.toml", "three_a", "readers_of_env", "FIX_SOCKET");
    spec(&q, "b.toml", "three_b", "reach", "crate:app");
    spec(&q, "c.toml", "three_c", "dependents", "crate:core_lib");

    // the runner stops after one stage: the job stays in jobs/running with its checkpoint
    let mut ctx = Ctx::at(&d).unwrap();
    ctx.exe = BIN.into();
    let ran = jobs::start(&mut ctx, &StartAsk { which: Some("three".into()), limit: Limit { stages: Some(1) }, ..Default::default() }).unwrap();
    let id = ran.job.clone().unwrap();
    assert!(ran.said.iter().any(|s| s.contains("stopped between stages; its checkpoint is in the journal")), "{:#?}", ran.said);
    assert!(d.join(".custodian/state/jobs/running/three").is_dir());
    let journal = ctx.journal().unwrap();
    let (_, step, bytes) = journal.latest_checkpoint(&id).unwrap().expect("a checkpoint");
    assert_eq!(step, 1, "saved after the first stage");
    let saved: custodian::runtime::JobState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!((saved.next, saved.done.len(), saved.stages.len()), (1, 1, 3));
    let first_runs = journal.receipts_of_job(&id).unwrap().iter().filter(|r| r.name == "three_a").count();
    drop(ctx);

    // a new process: job start resumes it from the journal
    let mut ctx = Ctx::at(&d).unwrap();
    ctx.exe = BIN.into();
    let resumed = jobs::start(&mut ctx, &StartAsk::default()).unwrap();
    assert!(resumed.said.iter().any(|s| s.starts_with(&format!("resumed job {id} at stage 2 of 3"))), "{:#?}", resumed.said);
    let journal = ctx.journal().unwrap();
    let receipts = journal.receipts_of_job(&id).unwrap();
    assert_eq!(receipts.iter().filter(|r| r.name == "three_a").count(), first_runs, "the first stage did not run again");
    assert!(receipts.iter().any(|r| r.name == "three_b") && receipts.iter().any(|r| r.name == "three_c"));
    assert!(journal.latest_checkpoint(&id).unwrap().is_none(), "a finished job keeps no checkpoint");
    let events = journal.events_since(0).unwrap();
    let order: Vec<&str> = events.iter().filter(|e| e.job.as_deref() == Some(&id) && e.kind == "step" && e.state == "done").map(|e| e.subject.as_str()).collect();
    assert_eq!(order, ["three_a", "three_b", "three_c"]);
    let closed = jobs::close(&mut ctx, Some(&id)).unwrap();
    assert!(closed.said.iter().any(|s| s.starts_with(&format!("closed job {id}: 3 specs read"))), "{:#?}", closed.said);
}

#[test]
fn queued_jobs_run_together_on_the_queue_and_close() {
    let d = fixture("queue-jobs");
    ok(&d, &["agent", "init"], 0);
    spec(&d.join(".custodian/state/jobs/queued/one"), "a.toml", "one_a", "readers_of_env", "FIX_TOKEN");
    spec(&d.join(".custodian/state/jobs/queued/two"), "a.toml", "two_a", "reach", "crate:core_lib");
    let out = ok(&d, &["run"], 0);
    assert!(out.contains("queued job") && out.contains("(specs, Standard)"), "{out}");
    assert!(d.join(".custodian/state/jobs/done/one/closing.md").is_file() && d.join(".custodian/state/jobs/done/two/closing.md").is_file(), "{out}");
}
