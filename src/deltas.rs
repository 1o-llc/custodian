//! `custodian delta fmt|clippy [--base DIR] [--job JOB]`: the format or lint findings this tree adds to its base's
//! (custodian_kit's `delta`). The base is the one named, else the running experiment's specimen. The record is kept
//! under `data/deltas/<kind>/` and, in a job, its receipt is what a `delta:<kind>` verification reads: done (0) when
//! the tree adds no finding, findings (1) when it adds one or more; an inherited finding is never the tree's.

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::layout::write_whole;
use crate::surface::{s, so, Delta};
use std::path::PathBuf;

pub fn delta(ctx: &mut Ctx, l: &Delta) -> Result<Ran> {
    let kind: custodian_kit::delta::Kind = s(&l.kind).parse().map_err(|e: String| Code::WordRefused.refuse(e))?;
    let job = ctx.current_job(so(&l.job))?;
    let base = match so(&l.base) {
        Some(b) => std::env::current_dir().unwrap_or_default().join(b),
        None => job
            .as_deref()
            .and_then(|j| crate::readings::job_folder(ctx, j).ok().flatten())
            .and_then(|f| custodian_kit::provenance::read(&f))
            .and_then(|p| p.specimen)
            .filter(|s| s.form == "folder")
            .map(|s| PathBuf::from(s.path))
            .ok_or_else(|| Code::LineRefused.refuse("delta needs its base: --base DIR, or a running experiment whose specimen is a folder"))?,
    };
    let d = custodian_kit::delta::delta(kind, &base, ctx.root())?;
    let mut ran = Ran::named(kind.as_str());
    ran.job = job;
    ran.say(format!("delta {} against {}: {} findings in the base, {} here; {} inherited, {} introduced", kind.as_str(), base.display(), d.base_findings, d.candidate_findings, d.inherited.len(), d.introduced.len()));
    for f in &d.introduced {
        ran.say(format!("introduced: {}{} {}", f.file, f.line.map(|n| format!(":{n}")).unwrap_or_default(), f.key.lines().next().unwrap_or("")));
    }
    if let Some(p) = ctx.mode.project() {
        let file = p.layout.path::<crate::layout::DeltaFile>(&[("kind", kind.as_str()), ("stamp", &crate::journal::stamp())])?;
        write_whole(&file, (serde_json::to_string_pretty(&d).map_err(|e| Code::Write.refuse(e))? + "\n").as_bytes())?;
        ran.say(format!("delta record: {}", ctx.rel(&file)));
        ran.outputs = vec![ctx.rel(&file)];
    }
    ran.code = if d.clean() { 0 } else { 1 };
    Ok(ran)
}
