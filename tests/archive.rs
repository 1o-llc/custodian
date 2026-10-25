//! `custodian archive make|check|verify` on the fixture project (`app` depends on `core_lib` by path): the public
//! line goes through the facade like every line and leaves its receipt; the work is the kit's declared line
//! `custodian exec-archive`, run as a child of the program through the same facade, with a receipt of its own whose
//! status is the word the line names. The archive's own properties (the closure, provenance, attachments, the
//! manifest's digests, repeatable bytes, unsafe paths refused, read back, nothing half-written) are shown on the
//! files the lines leave.

mod common;

use common::*;
use custodian::facade::{self, Ctx};
use custodian::surface::{self, Line};
use std::ffi::OsString;
use std::path::Path;

fn entries(zip: &Path) -> Vec<String> {
    let o = std::process::Command::new("unzip").args(["-Z1", zip.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "unzip -Z1 {}: {}", zip.display(), String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect()
}

fn manifest(zip: &Path) -> String {
    let o = std::process::Command::new("unzip").args(["-p", zip.to_str().unwrap(), "META/archive-manifest.tsv"]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// The two receipts an `archive make` leaves, both the facade's: the public line's (answered in this process) and
/// the kit's `exec-archive` line's (a child through the same facade), each with xccute's plan, operation and receipt
/// digests, the declared status word, the timing, the outputs and the job.
#[test]
fn archive_make_runs_the_kits_declared_line_through_the_facade_and_leaves_both_receipts() {
    let d = fixture("archive-make");
    ok(&d, &["init"], 0);
    let out = ok(&d, &["archive", "make", "crate:app", "--workspace-deps", "--attach", "path:Cargo.toml", "--attach", "path:Cargo.lock"], 0);
    assert!(out.contains("crates: app (explicit), core_lib (dependency of app)"), "{out}");
    assert!(out.contains("attached: Cargo.lock → workspace/Cargo.lock") && out.contains("attached: Cargo.toml → workspace/Cargo.toml"), "{out}");
    assert!(out.contains("cargo: cargo metadata --manifest-path ") && out.contains("--format-version 1 --no-deps → 0 ("), "cargo metadata is the declared line: {out}");
    let n = seq_of(&out);
    // the public line's receipt: the facade's, answered in this process
    let public = receipt(&d, n);
    assert_eq!((public["command"].as_str(), public["exit_code"].as_i64(), public["reason"].as_str(), public["runner"].as_str()), (Some("archive"), Some(0), Some("done"), Some("in_process")), "{public}");
    assert_eq!(public["line"], "custodian archive make crate:app --workspace-deps --attach path:Cargo.toml --attach path:Cargo.lock");
    assert_eq!(public["plan_id"], "custodian");
    for digest in ["plan_digest", "operation_digest", "receipt_digest"] {
        assert_eq!(public[digest].as_str().map(str::len), Some(64), "{digest}: {public}");
    }
    let zip = public["outputs"][0].as_str().unwrap().to_string();
    assert!(zip.starts_with(".custodian/data/archives/crate_app/") && zip.ends_with(".zip"), "pathgen's place: {zip}");
    assert!(d.join(&zip).is_file() && !d.join(format!("{zip}.partial")).exists() && !d.join(zip.replace(".zip", ".zip.partial")).exists(), "written whole");
    // the kit's line's receipt: the one before, a child run through the same facade, its status the line's word
    let kit = receipt(&d, n - 1);
    assert_eq!((kit["command"].as_str(), kit["exit_code"].as_i64(), kit["reason"].as_str(), kit["runner"].as_str(), kit["accepted"].as_bool()), (Some("archive"), Some(0), Some("archived"), Some("line"), Some(true)), "{kit}");
    let line = kit["line"].as_str().unwrap();
    assert!(line.starts_with("custodian exec-archive --root ") && line.contains(" --include crate:app --attach path:Cargo.toml --attach path:Cargo.lock --workspace-deps --out "), "{line}");
    assert_eq!(kit["plan_id"], "custodian");
    assert_eq!(kit["disposition"], "Continue", "an accepted status goes on, by the line's policy");
    assert!(kit["millis"].as_i64().is_some_and(|m| m >= 0));
    assert_eq!(kit["outputs"][0], zip);
    for digest in ["plan_digest", "operation_digest", "receipt_digest"] {
        assert_eq!(kit[digest].as_str().map(str::len), Some(64), "{digest}: {kit}");
    }
    assert_ne!(kit["operation_digest"], public["operation_digest"], "two operations");
    assert!(out.contains(&format!("receipt #{n}:")) && out.contains(&format!("→ 0 (archived) #{}", n - 1)), "{out}");

    // what the archive holds: both crates with their manifests, the attachments under workspace/, the manifest
    let names = entries(&d.join(&zip));
    for want in ["META/archive-manifest.tsv", "crates/app/Cargo.toml", "crates/app/src/main.rs", "crates/core_lib/Cargo.toml", "crates/core_lib/src/lib.rs", "workspace/Cargo.toml", "workspace/Cargo.lock"] {
        assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
    }
    let m = manifest(&d.join(&zip));
    assert!(m.starts_with("schema\tcustodian.archive.v1\n"), "{m}");
    assert!(m.contains("crate\texplicit\tapp\n") && m.contains("crate\tdependency\tcore_lib\tapp\n"), "{m}");
    assert!(m.contains("attached\tCargo.toml\tworkspace/Cargo.toml\n") && !m.contains("missing\t"), "{m}");
    assert_eq!(m.lines().filter(|l| l.starts_with("file\tsha256:")).count(), names.len() - 1, "every payload file's digest: {m}");

    // the same job again elsewhere: the same bytes; the same place again: refused, nothing overwritten
    let twice = d.join("twice.zip");
    ok(&d, &["archive", "make", "crate:app", "--workspace-deps", "--attach", "path:Cargo.toml", "--attach", "path:Cargo.lock", "--out", twice.to_str().unwrap()], 0);
    assert_eq!(std::fs::read(d.join(&zip)).unwrap(), std::fs::read(&twice).unwrap(), "byte for byte");
    let before = std::fs::read(&twice).unwrap();
    let r = run(&d, &["archive", "make", "crate:app", "--workspace-deps", "--out", twice.to_str().unwrap()]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST0130]") && r.all().contains("refusing to overwrite"), "{}", r.all());
    assert_eq!(std::fs::read(&twice).unwrap(), before);
    let refused = receipt(&d, seq_of(&r.out) - 1);
    assert_eq!((refused["reason"].as_str(), refused["exit_code"].as_i64(), refused["accepted"].as_bool()), (Some("refused"), Some(2), Some(false)), "the kit's line's refusal is its receipt: {refused}");

    // read back through the same line
    let v = ok(&d, &["archive", "verify", &zip], 0);
    assert!(v.contains(&format!("verified {zip}: {} files, every digest as the manifest says; crates app, core_lib", names.len() - 1)), "{v}");
    let vr = receipt(&d, seq_of(&v) - 1);
    assert!(vr["line"].as_str().unwrap().contains(" --verify "), "{vr}");
    assert_eq!(vr["reason"], "archived");
    // a byte flipped: the verifier refuses it
    let mut bytes = std::fs::read(&twice).unwrap();
    let at = bytes.windows(7).position(|w| w == b"version").unwrap();
    bytes[at] = b'V';
    std::fs::write(&twice, bytes).unwrap();
    let r = run(&d, &["archive", "verify", twice.to_str().unwrap()]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST0132]") && r.all().contains("CRC mismatch"), "{}", r.all());
}

/// `archive check` says the plan and writes nothing; a gap is a finding (status 1 on both lines); free mode keeps
/// nothing; an unsafe attached path is refused before anything is written.
#[test]
fn archive_check_finds_gaps_and_refuses_unsafe_paths_in_either_mode() {
    let d = fixture("archive-check");
    let free = run(&d, &["archive", "check", "crate:app"]);
    assert_eq!(free.code, 1, "{}", free.all());
    assert!(free.out.contains("gap: app depends on core_lib, which is not in the archive (--workspace-deps takes the closure)"), "{}", free.out);
    assert!(free.out.contains("→ 1 (gaps) #0") && free.out.contains("free mode (no .custodian here or above): nothing stored · custodian archive check crate:app → 1 (findings)"), "{}", free.out);
    assert!(!d.join(".custodian").exists());
    // free mode make without --out: the folder, or a name
    let r = run(&d, &["archive", "make", "crate:app"]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST1002]") && r.all().contains("--out FILE"), "{}", r.all());

    ok(&d, &["init"], 0);
    let out = run(&d, &["archive", "check", "crate:app", "crate:core_lib"]);
    assert_eq!(out.code, 0, "{}", out.all());
    assert!(out.out.contains("crates: app (explicit), core_lib (explicit)") && out.out.contains("nothing written (archive check)"), "{}", out.out);
    let n = seq_of(&out.out);
    assert_eq!(receipt(&d, n)["line"], "custodian archive check crate:app crate:core_lib");
    assert!(receipt(&d, n - 1)["line"].as_str().unwrap().contains("--include crate:app --include crate:core_lib"));
    assert!(!d.join(".custodian/data/archives").exists(), "a check writes no archive");

    let r = run(&d, &["archive", "check", "crate:app", "--attach", "path:../outside.txt"]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST0130]") && r.all().contains("unsafe archive path rejected"), "{}", r.all());
    let r = run(&d, &["archive", "make", "crate:app", "--attach", "path:../outside.txt", "--out", d.join("never.zip").to_str().unwrap()]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(!d.join("never.zip").exists() && !d.join("never.zip.partial").exists(), "nothing written before the refusal");
    // a route that is not a whole crate is refused by the line itself, before anything runs
    let r = run(&d, &["archive", "check", "crate:app/src/main.rs"]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("is not a crate:<name> route"), "{}", r.all());
}

/// Inside a job the archive is the job's: both receipts carry its id, the job's closing receipt counts them, and an
/// attached path the table does not declare is refused (a job reads declared routes only).
#[test]
fn an_archive_in_a_job_is_the_jobs_and_reads_declared_routes_only() {
    let d = fixture("archive-job");
    ok(&d, &["agent", "init"], 0);
    ok(&d, &["job", "start", "delivery", "--intent", "deliver app with its workspace crates"], 0);
    let r = run(&d, &["archive", "make", "crate:app", "--workspace-deps", "--attach", "path:Cargo.toml"]);
    assert_eq!(r.code, 2, "{}", r.all());
    assert!(r.all().contains("[CUST0023]"), "a job reads declared routes only: {}", r.all());
    let out = ok(&d, &["archive", "make", "crate:app", "--workspace-deps", "--attach", "crate:core_lib/Cargo.toml"], 0);
    let n = seq_of(&out);
    let (public, kit) = (receipt(&d, n), receipt(&d, n - 1));
    let id = public["job"].as_str().expect("the job's id").to_string();
    assert!(id.starts_with("job-"), "{public}");
    assert_eq!(kit["job"].as_str(), Some(id.as_str()), "{kit}");
    assert!(kit["line"].as_str().unwrap().ends_with(&format!(" --job {id}")), "{kit}");
    let closed = ok(&d, &["job", "close"], 0);
    assert!(closed.contains("verifications, result"), "{closed}");
    let ctx = Ctx::at(&d).unwrap();
    let of_job = ctx.journal().unwrap().receipts_of_job(&id).unwrap();
    assert!(of_job.iter().filter(|r| r.command == "archive").count() >= 2, "the job's receipts hold both archive lines: {:?}", of_job.iter().map(|r| (&r.command, &r.line)).collect::<Vec<_>>());
}

/// The same facade, in process: `archive check` executed as the command line executes every line
/// (`facade::execute` with the handler) is one `RuntimeOperation` in the `custodian` plan, decided by the words the
/// line names, kept as a receipt with the kit's line's receipt before it.
#[test]
fn the_archive_line_executes_through_the_facade_in_process_like_every_line() {
    let d = fixture("archive-facade");
    ok(&d, &["init"], 0);
    let mut ctx = Ctx::at(&d).unwrap();
    ctx.exe = BIN.into();
    let line = surface::read(&["archive", "check", "crate:app", "--workspace-deps"].iter().map(OsString::from).collect::<Vec<_>>()).unwrap();
    assert!(matches!(line, Line::ArchiveCheck(_)));
    assert_eq!((line.path(), line.segment(), line.status_word(0), line.status_word(1), line.status_word(2)), ("archive check", "archive", "done", "findings", "refused"));
    let outcome = facade::execute(&mut ctx, &line, custodian::cli::handle);
    assert_eq!((outcome.code, outcome.receipt.command.as_str(), outcome.receipt.reason.as_str(), outcome.receipt.runner.as_str()), (0, "archive", "done", "in_process"), "{outcome:?}");
    assert_eq!(outcome.receipt.line, "custodian archive check crate:app --workspace-deps");
    assert_eq!(outcome.receipt.plan_id, "custodian");
    assert!(outcome.said.iter().any(|s| s == "crates: app (explicit), core_lib (dependency of app)"), "{:?}", outcome.said);
    let file = outcome.receipt_file.expect("journaled");
    assert!(d.join(&file).is_file());
    let kit = ctx.journal().unwrap().receipt(outcome.receipt.seq - 1).unwrap().expect("the kit's line's receipt, the one before");
    assert_eq!((kit.command.as_str(), kit.reason.as_str(), kit.runner.as_str(), kit.exit_code), ("archive", "archived", "line", Some(0)));
    assert!(kit.line.starts_with("custodian exec-archive --root "), "{}", kit.line);
    // the decision is xccute's, by the line's named statuses
    let d1 = line.decide(1);
    assert_eq!((d1.disposition, d1.reason.as_str()), (xccute_runtime::ExitDisposition::Stop, "findings"));
}
