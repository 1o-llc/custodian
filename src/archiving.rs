//! `custodian archive make ROUTE... [--workspace-deps] [--attach ROUTE]... [--out ZIP] [--reveal] [--job JOB]`,
//! `archive check ...`, `archive verify ZIP`: the crates the routes name, whole, with the workspace crates they
//! depend on and declared attached files, as one ZIP whose bytes repeat and whose manifest records why each crate
//! is in it and every file's digest (custodian_kit's `archive`).
//!
//! The command line validates the words and runs the kit's declared line `custodian exec-archive`
//! (`custodian_kit::exec::ExecArchive`) through the facade as it runs every program: a child of the running
//! custodian ([`crate::facade::External::at`]), its receipt kept under the job with its status decided by the
//! words the line names (`archived`, `gaps`, `refused`), its summary (the last line it prints) said here. The
//! public line's own receipt is the facade's, as for every line. `custodian bundle` is untouched: a bundle is one
//! annotated text file for reading, held redacted; an archive is whole files for delivery.

use crate::codes::{Code, Refusal, Result};
use crate::facade::{self, Ctx, ExternalNames, ExternalRun, Ran};
use crate::layout;
use crate::surface::{s, so, ArchiveCheck, ArchiveMake, ArchiveVerify};
use custodian_kit::archive::{ArchiveSummary, CrateInclusionKind, VerifyReport};
use custodian_kit::exec::{Custodian as Kit, ExecArchiveRooted};
use std::path::{Path, PathBuf};

/// The operation's id in the plan, and the receipt's kind word through [`facade::KIT_COMMANDS`].
pub const EXEC_ARCHIVE: &str = "custodian.exec-archive";

/// `archive make`: the archive written (where `--out` says, else under `data/archives/`), read back, its receipt.
pub fn make(ctx: &mut Ctx, l: &ArchiveMake) -> Result<Ran> {
    let job = ctx.current_job(so(&l.job))?;
    let routes: Vec<String> = l.routes.iter().map(s).collect();
    let out = match so(&l.out) {
        Some(o) => absolute(&o),
        None => default_out(ctx, &routes)?,
    };
    let line = line_of(ctx, &routes, &l.attach.iter().map(s).collect::<Vec<_>>(), l.workspace_deps, l.reveal, Some(&out), job.as_deref());
    let (mut ran, run) = run(ctx, line, format!("archive {}", routes.join(" ")), job, vec![ctx.rel(&out)])?;
    say_summary(ctx, &mut ran, &summary_of(&run)?);
    ran.outputs = vec![ctx.rel(&out)];
    Ok(ran)
}

/// `archive check`: the plan said, nothing written.
pub fn check(ctx: &mut Ctx, l: &ArchiveCheck) -> Result<Ran> {
    let job = ctx.current_job(so(&l.job))?;
    let routes: Vec<String> = l.routes.iter().map(s).collect();
    let line = line_of(ctx, &routes, &l.attach.iter().map(s).collect::<Vec<_>>(), l.workspace_deps, l.reveal, None, job.as_deref());
    let (mut ran, run) = run(ctx, line, format!("archive check {}", routes.join(" ")), job, Vec::new())?;
    say_summary(ctx, &mut ran, &summary_of(&run)?);
    Ok(ran)
}

/// `archive verify`: the archive read back by the kit, through the same declared line.
pub fn verify(ctx: &mut Ctx, l: &ArchiveVerify) -> Result<Ran> {
    let job = ctx.current_job(so(&l.job))?;
    let file = absolute(&s(&l.file));
    let mut line = Kit::exec_archive(ctx.root().as_os_str()).verify(file.as_os_str());
    if let Some(j) = &job {
        line = line.job(j.as_str());
    }
    let (mut ran, run) = run(ctx, line, format!("archive verify {}", ctx.rel(&file)), job, Vec::new())?;
    let report: VerifyReport = last_line(&run)?;
    ran.say(format!("verified {}: {} files, every digest as the manifest says; crates {}", ctx.rel(&file), report.files, report.selected.join(", ")));
    Ok(ran)
}

/// The kit's declared line for a make or a check.
fn line_of(ctx: &Ctx, routes: &[String], attach: &[String], workspace_deps: bool, reveal: bool, out: Option<&Path>, job: Option<&str>) -> ExecArchiveRooted<Kit> {
    let mut line = Kit::exec_archive(ctx.root().as_os_str()).include(routes.to_vec()).attach(attach.to_vec());
    if workspace_deps {
        line = line.workspace_deps();
    }
    if reveal {
        line = line.reveal();
    }
    if let Some(o) = out {
        line = line.out(o.as_os_str());
    }
    if let Some(j) = job {
        line = line.job(j);
    }
    line
}

/// A path as given, from where the run started.
fn absolute(p: &str) -> PathBuf {
    let p = Path::new(p);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

/// Journaled: `data/archives/<routes as a name>/<stamp>.zip`; free mode names its own place.
fn default_out(ctx: &Ctx, routes: &[String]) -> Result<PathBuf> {
    let Some(p) = ctx.mode.project() else {
        return Err(Code::NeedsFolder.refuse("archive make with no --out: name where the archive goes (--out FILE), or init the folder and it goes under data/archives"));
    };
    let name = crate::readings::slug(&routes.join("+"));
    p.layout.path::<layout::ArchiveFile>(&[("name", &name), ("stamp", &crate::journal::stamp())])
}

/// The kit's line run through the facade as a child of this custodian, its receipt kept; `archived` and `gaps` are
/// answers (0 and 1), `refused` is the kit's refusal passed through.
fn run(ctx: &mut Ctx, line: ExecArchiveRooted<Kit>, name: String, job: Option<String>, outputs: Vec<String>) -> Result<(Ran, ExternalRun)> {
    let ext = facade::External::at(EXEC_ARCHIVE, &ctx.exe, &line)?;
    let run = facade::run_external(ctx, ext, ExternalNames { name: name.clone(), job: job.clone(), outputs }, |_| {})?;
    let mut ran = Ran::named(name);
    ran.job = job;
    ran.say(format!("{} → {} ({}) #{}", run.receipt.line, run.code.map_or("-".to_string(), |c| c.to_string()), run.receipt.reason, run.receipt.seq));
    match run.code {
        Some(0) => {}
        Some(1) => ran.code = 1,
        _ => return Err(refusal_of(&run)),
    }
    Ok((ran, run))
}

/// The last line the child printed, read as `T`.
fn last_line<T: serde::de::DeserializeOwned>(run: &ExternalRun) -> Result<T> {
    let line = run.stdout.lines().rev().find(|l| !l.trim().is_empty()).ok_or_else(|| Code::StepFailed.refuse(format!("{}: printed no summary", run.receipt.line)))?;
    serde_json::from_str(line).map_err(|e| Code::StepFailed.refuse(format!("{}: its summary did not read: {e}", run.receipt.line)))
}

fn summary_of(run: &ExternalRun) -> Result<ArchiveSummary> {
    last_line(run)
}

/// The kit's refusal as the child printed it last on stderr (`[CUST0130] ...`), else the facade's.
fn refusal_of(run: &ExternalRun) -> Refusal {
    let kit = run.stderr.lines().rev().find(|l| l.starts_with("[CUST")).and_then(|l| {
        let (code, message) = l.split_once("] ")?;
        Some(Refusal { code: code.trim_start_matches('[').to_string(), message: message.to_string() })
    });
    kit.unwrap_or_else(|| facade::not_accepted(run))
}

fn say_summary(ctx: &Ctx, ran: &mut Ran, s: &ArchiveSummary) {
    let crates: Vec<String> = s
        .crates
        .iter()
        .map(|c| match c.kind {
            CrateInclusionKind::Explicit => format!("{} (explicit)", c.name),
            CrateInclusionKind::Dependency => format!("{} (dependency of {})", c.name, c.required_by.join(", ")),
        })
        .collect();
    ran.say(format!("crates: {}", crates.join(", ")));
    for a in &s.attachments {
        ran.say(format!("attached: {} → {}", a.source, a.archive_path));
    }
    for g in &s.gaps {
        ran.say(format!("gap: {} depends on {}, which is not in the archive (--workspace-deps takes the closure)", g.from, g.dependency));
    }
    ran.say(format!("files: {} ({} bytes), each with its sha256 in {}", s.files, s.bytes, custodian_kit::archive::MANIFEST_PATH));
    if !s.revealed.is_empty() {
        ran.say(format!("revealed (--reveal): {} file(s) holding a secret item copied as they are: {}", s.revealed.len(), s.revealed.join(", ")));
    }
    for c in &s.cargo {
        ran.say(format!("cargo: {} → {} ({} ms)", c.line, c.code.map_or("-".to_string(), |x| x.to_string()), c.millis));
    }
    match (&s.archive, &s.digest, &s.verified) {
        (Some(a), Some(d), Some(v)) => ran.say(format!("archive: {} ({d}); read back: {} files, {} crates", ctx.rel(Path::new(a)), v.files, v.selected.len())),
        _ => ran.say("nothing written (archive check)"),
    }
}
