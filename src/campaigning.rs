//! `custodian experiment campaign CAMPAIGN [STORAGE] [--job JOB] [--plan] [--again]`: an `experiments` campaign
//! governed by custodian. `experiments` owns what the campaign means (its plan, its designs, what a node's outcome
//! makes of its candidate, its stops, its result); custodian owns everything that touches a tree: the workspaces, the
//! state they are in, the writer, the receipts, and the order nodes run in. Nothing here writes source: every node is
//! the declared `experiment run` line run through the facade (its bounded changes through custodian's `preflight` and
//! `transform`, code_passenger the writer), and a rejected candidate is put back by the declared `transform --revert`
//! of the records its transforms kept, newest first.
//!
//! The plan is made and checked before anything runs (`--plan` shows it and stops there). A shared node runs in this
//! project, on this thread, one after another. An isolated node runs in a workspace of its own: a copy of its state
//! parent's accepted state (kept under the run's `states/` when that state was reached), made under the run's
//! `workspaces/`, initialized as a custodian folder of its own (its own journal and receipts, its own job lock), and
//! run on a thread of its own under this line's writer. Two nodes never write one workspace at once: each workspace
//! has an admission of one (uh-tom-mick's, as the process's own writer) and every node holds its workspace's job lock
//! (xccute's `Hold`) while it runs; at most `max_parallel` nodes run at once (an admission of that many).
//!
//! A tree's scientific state is every file custodian_kit's provenance reads (`tree_files`: build output, `.git`, the
//! passenger's store, `.custodian` and `oneo.project.json` left out), with its content and its mode, less the
//! campaign's own `exclude`; its digest is what a node must start from and what a resume checks.
//!
//! Progress is kept after every node (`progress.json` beside the runs): the same line again resumes the run it names,
//! refused when the campaign, an experiment, a change spec, a factor or a design moved, or when the project is not at
//! the accepted state the run recorded. The result (`runs/<run>/result.json`) is the line's output, and a `campaign`
//! event names it in the journal.

use crate::codes::{Code, LiaiseCode, Result};
use crate::facade::{self, Ctx, Ran};
use crate::layout::write_whole;
use crate::surface::{self, s, so, ExperimentCampaign, Line};
use experiments::campaign::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use uh_tom_mick::AtomicAdmission;

/// The journal event a campaign's result makes.
pub const EVENT: &str = "campaign";
/// The journal event of each state the campaign line leaves the project in, whatever its storage: `start`,
/// `accepted` (a shared node's end, its candidate accepted or put back) or `left` (put back it could not be), its
/// detail `<digest> <storage>`.
pub const STATE_EVENT: &str = "campaign-state";
/// Beside the runs: the progress of the run a resume continues.
pub const PROGRESS: &str = "progress.json";
const RUNS: &str = "runs";
const PLAN: &str = "plan.json";
const RESULT: &str = "result.json";
const NODES: &str = "nodes";
const STATES: &str = "states";
const WORKSPACES: &str = "workspaces";

pub fn run(ctx: &mut Ctx, l: &ExperimentCampaign) -> Result<Ran> {
    let project = ctx.mode.need_project()?.clone();
    let job = ctx.current_job(so(&l.job))?;
    let here = std::env::current_dir().unwrap_or_default();
    let file = here.join(s(&l.campaign));
    let root = ctx.root().to_path_buf();
    let loaded = LoadedCampaign::load(&file, &root).map_err(|e| Code::CampaignRefused.refuse(format!("{}: {}", file.display(), e.0.join(" · "))))?;
    let mut ran = Ran::named(loaded.plan.campaign_id.clone());
    ran.job = job.clone();
    // the writer custodian would run performs every operation the change specs use, or nothing runs (a plan too)
    if let Some(writer) = writer_performs(&loaded, crate::maint::passenger_path().as_deref())? {
        ran.say(writer);
    }
    if l.plan {
        show(&loaded.plan, &mut ran);
        ran.code = if loaded.plan.findings.is_empty() { 0 } else { 1 };
        return Ok(ran);
    }
    let storage = match so(&l.storage) {
        Some(st) => lexical(&here.join(st)),
        None => crate::experimenting::data_folder(ctx, job.as_deref())?.map(|d| d.join("campaigns")).ok_or_else(|| Code::NeedsFolder.refuse(root.display()))?,
    };
    collision(&root, &storage)?;
    // the job's battery floor set here, on this one thread, before any node runs (a shared node's lines then find it
    // set and leave the environment alone while isolated nodes run on other threads)
    crate::budget::before_step(ctx, job.as_deref(), "experiment campaign", custodian_kit::step_time::PREFLIGHT, 1)?;
    let line = Line::ExperimentCampaign(l.clone());
    let gov = Governor { loaded: &loaded, settings: project.settings.clone(), dir: storage.join(&loaded.plan.campaign_id), job, again: l.again };
    crate::runtime::under_job_lock(ctx, &line, |ctx| gov.govern(ctx, ran))
}

/// The campaign's storage may not be part of the state it governs: inside the root it is refused unless it is in
/// custodian's own folder (left out of every state), and a root inside it is refused.
fn collision(root: &Path, storage: &Path) -> Result<()> {
    let canon = |p: &Path| lexical(&p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
    let (root, storage) = (canon(root), canon(storage));
    let own = root.join(crate::layout::FOLDER);
    if storage.starts_with(&root) && !storage.starts_with(&own) {
        return Err(Code::CampaignRefused.refuse(format!("storage {} is inside the governed root {}: it would be part of the state it records (keep it in {} or outside the root)", storage.display(), root.display(), own.display())));
    }
    if root.starts_with(&storage) {
        return Err(Code::CampaignRefused.refuse(format!("storage {} holds the governed root {}", storage.display(), root.display())));
    }
    Ok(())
}

/// `p` with `.` and `..` taken out, lexically.
fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The plan, as `--plan` shows it: one line per node, then the workspaces, the groups and what to expect.
fn show(plan: &CampaignPlan, ran: &mut Ran) {
    let (widest, ready, waves) = plan.expected_parallelism();
    ran.print(format!("campaign {} · plan {} · rigor {} · {} nodes in {} workspaces", plan.campaign_id, plan.digest, plan.rigor.word(), plan.nodes.len(), plan.workspaces.len()));
    let p = &plan.policy;
    ran.print(format!("policy: on_findings {}, on_refusal {}, on_error {}, on_undecided {}, max_parallel {} · expected: {waves} waves, at most {widest} at once ({ready} ready at most)", p.on_findings.word(), p.on_refusal.word(), p.on_error.word(), p.on_undecided.word(), p.max_parallel));
    for f in &plan.factors {
        ran.print(format!("factor {} (scope {}): {}", f.name, f.scope.word(), f.levels.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")));
    }
    for c in &plan.controls {
        ran.print(format!("control {} = {}", c.name, c.value));
    }
    for b in &plan.blocks {
        ran.print(format!("block {}: {}", b.name, b.levels.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")));
    }
    if !plan.exclude.is_empty() {
        ran.print(format!("excluded from the state: {}", plan.exclude.join(", ")));
    }
    for sp in &plan.specs {
        let mode = if sp.mutation == experiments::MutationMode::Bounded { "bounded" } else { "unbounded" };
        ran.print(format!("experiment {} ({}) {} · {mode}, {} changes", sp.path, sp.experiment_id, sp.digest, sp.changes.len()));
    }
    for g in &plan.groups {
        ran.print(format!("{} {}: {} · {} treatments × {} repeats{} = {} trials", g.kind, g.id, g.design.map(Design::word).unwrap_or("repeat"), g.treatments.len(), g.repeats, if g.blocks.is_empty() { String::new() } else { format!(" × blocks {}", g.blocks.join(", ")) }, g.nodes.len()));
    }
    for w in &plan.workspaces {
        ran.print(format!("workspace {} ({}) from {}: {}", w.id, w.strategy.word(), w.source.as_deref().unwrap_or("the campaign's start"), w.nodes.join(" → ")));
    }
    for n in &plan.nodes {
        let mut parts = vec![format!("{} {}", n.index + 1, n.id), format!("spec {} {}", n.spec, n.spec_digest)];
        parts.push(format!("after [{}]", n.after.join(", ")));
        parts.push(format!("from {}", n.from.as_deref().unwrap_or("the campaign's start")));
        parts.push(format!("{} {}", n.strategy.word(), n.workspace));
        if !n.assignment.is_empty() {
            parts.push(format!("set {}", n.assignment.iter().map(|a| format!("{}={}", a.factor, a.value)).collect::<Vec<_>>().join(",")));
        }
        if !n.blocks.is_empty() {
            parts.push(format!("block {}", n.blocks.iter().map(|b| format!("{}={}", b.name, b.level)).collect::<Vec<_>>().join(",")));
        }
        if let (Some(g), Some(t)) = (&n.group, &n.treatment) {
            parts.push(format!("{g} treatment {t}{}", n.design.map(|d| format!(" ({})", d.word())).unwrap_or_default()));
        }
        if let Some(r) = n.replicate {
            parts.push(format!("replicate {r}"));
        }
        ran.print(parts.join(" · "));
    }
    for f in &plan.findings {
        ran.say(format!("finding: {f}"));
    }
    ran.say(format!("plan only: nothing was run or written; {} isolated workspaces would be made, {} states kept for them", plan.nodes.iter().filter(|n| n.strategy == WorkspaceStrategy::Isolated).count(), plan.snapshots.len()));
}

/// A tree's scientific state: its identity, and each file (`<content digest> <mode>`) by its path.
pub fn state_of(root: &Path, exclude: &[String]) -> Result<(StateIdentity, BTreeMap<String, String>)> {
    let files = custodian_kit::provenance::tree_files(root)?;
    let mut map = BTreeMap::new();
    for (p, d) in files {
        if exclude.iter().any(|e| p == *e || p.starts_with(&format!("{e}/"))) {
            continue;
        }
        let meta = std::fs::symlink_metadata(root.join(&p)).map_err(|e| Code::Write.refuse(format!("{p}: {e}")))?;
        map.insert(p, format!("{d} {}", mode_of(&meta)));
    }
    let digest = format!("sha256:{}", crate::digest::of("campaign-state", &map));
    Ok((StateIdentity { digest, files: map.len() }, map))
}

#[cfg(unix)]
fn mode_of(meta: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    if meta.file_type().is_symlink() {
        "link".into()
    } else {
        format!("{:o}", meta.permissions().mode() & 0o7777)
    }
}

#[cfg(not(unix))]
fn mode_of(meta: &std::fs::Metadata) -> String {
    if meta.file_type().is_symlink() {
        "link".into()
    } else if meta.permissions().readonly() {
        "ro".into()
    } else {
        "rw".into()
    }
}

/// `from`'s state copied into `to` (which must not exist): every file of its state, content, mode and links as
/// they are, then read back. Refused unless the copy's state is `want`.
fn copy_state(from: &Path, to: &Path, exclude: &[String], want: &StateIdentity) -> Result<StateIdentity> {
    let (have, files) = state_of(from, exclude)?;
    if &have != want {
        return Err(Code::CampaignStateMoved.refuse(format!("{} is {} ({} files), the state to copy is {} ({} files)", from.display(), have.digest, have.files, want.digest, want.files)));
    }
    if to.exists() {
        return Err(Code::CampaignRefused.refuse(format!("{} is there already", to.display())));
    }
    // a link the copy keeps points inside the copy: an absolute one, or one that climbs out of the tree, would let a
    // node write through it into the state it was copied from (checked before anything is made)
    for rel in files.keys() {
        let src = from.join(rel);
        if std::fs::symlink_metadata(&src).is_ok_and(|m| m.file_type().is_symlink()) {
            let target = std::fs::read_link(&src).map_err(|e| Code::Write.refuse(format!("{}: {e}", src.display())))?;
            let at = lexical(&src.parent().unwrap_or(from).join(&target));
            if target.is_absolute() || !at.starts_with(lexical(from)) {
                return Err(Code::CampaignRefused.refuse(format!("{rel} is a link to {}, outside the tree it is copied from: an isolated workspace could write through it", target.display())));
            }
        }
    }
    let io = |p: &Path, e: std::io::Error| Code::Write.refuse(format!("{}: {e}", p.display()));
    std::fs::create_dir_all(to).map_err(|e| io(to, e))?;
    for rel in files.keys() {
        let (src, dst) = (from.join(rel), to.join(rel));
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).map_err(|e| io(d, e))?;
        }
        let meta = std::fs::symlink_metadata(&src).map_err(|e| io(&src, e))?;
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&src).map_err(|e| io(&src, e))?;
            link(&target, &dst).map_err(|e| io(&dst, e))?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| io(&dst, e))?;
            std::fs::set_permissions(&dst, meta.permissions()).map_err(|e| io(&dst, e))?;
        }
    }
    let (copied, _) = state_of(to, exclude)?;
    if &copied != want {
        return Err(Code::CampaignStateMoved.refuse(format!("the copy {} is {}, its source {}", to.display(), copied.digest, want.digest)));
    }
    Ok(copied)
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(not(unix))]
fn link(_target: &Path, at: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other(format!("{}: a link is copied only on unix", at.display())))
}

/// The first of `base`, `base.2`, `base.3`, ... that does not exist: a node run again gets a workspace of its own,
/// and the one an interrupted attempt left is kept as it was.
fn fresh(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    let name = base.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    (2..).map(|k| base.with_file_name(format!("{name}.{k}"))).find(|p| !p.exists()).unwrap_or_else(|| base.to_path_buf())
}

fn show_tools(t: &[(String, Option<String>, Option<String>)]) -> String {
    t.iter().map(|(n, v, d)| format!("{n} {} {}", v.as_deref().unwrap_or("-"), d.as_deref().unwrap_or("-"))).collect::<Vec<_>>().join("; ")
}

/// `--again` over a kept run: the project must be at a state that run recorded in it (its start, or a node's start or
/// accepted state there), else refused with what to put back.
fn nameable(kept: &CampaignProgress, root: &Path, now: &StateIdentity) -> Result<()> {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let mut named: Vec<&StateIdentity> = kept.start.iter().collect();
    named.extend(kept.workspaces.get("shared").and_then(|w| w.accepted.as_ref()));
    for r in kept.nodes.values().filter(|r| r.workspace_path.as_deref().map(canon) == Some(canon(root))) {
        named.extend(r.start.iter().chain(r.accepted.iter()));
    }
    if named.contains(&now) {
        return Ok(());
    }
    let last = kept.workspaces.get("shared").and_then(|w| w.accepted.clone()).or_else(|| kept.start.clone());
    let dirty: Vec<String> = kept.nodes.values().filter(|r| r.state == Some(StateAction::Discarded) && r.workspace_path.as_deref().map(canon) == Some(canon(root))).flat_map(|r| r.findings.iter().map(move |f| format!("{}: {f}", r.id))).collect();
    Err(Code::CampaignStateMoved.refuse(format!(
        "{} is {}, a state run {} never recorded (it left the project's accepted state at {}){}: put the project back to that state, then run --again",
        root.display(),
        now.digest,
        kept.run_id,
        last.map(|l| l.digest).unwrap_or_else(|| "none".into()),
        if dirty.is_empty() { String::new() } else { format!("; what it could not put back: {}", dirty.join(" · ")) }
    )))
}

/// The state the campaign line leaves the project in, as a journal event (see [`STATE_EVENT`]).
fn state_event(ctx: &Ctx, job: Option<&str>, campaign: &str, word: &str, digest: &str, storage: &Path) -> Result<()> {
    match ctx.journal.as_ref() {
        Some(j) => {
            // `<digest> <storage>`, a reason after ` — ` when there is one
            let (digest, why) = digest.split_once(" — ").map_or((digest, None), |(d, w)| (d, Some(w)));
            let detail = format!("{digest} {}{}", storage.display(), why.map(|w| format!(" — {w}")).unwrap_or_default());
            j.event(STATE_EVENT, job, campaign, word, &detail, None).map(|_| ())
        }
        None => Ok(()),
    }
}

/// A first run or `--again` of any campaign over a project some campaign line last left in a state it could not put
/// back (that campaign's latest `campaign-state` is `left`): refused unless the project is at a state a campaign line
/// recorded there (a start, an accepted state), whatever campaign and storage that was.
fn not_left_dirty(ctx: &Ctx, _id: &str, now: &StateIdentity) -> Result<()> {
    let Some(j) = ctx.journal.as_ref() else { return Ok(()) };
    let events: Vec<_> = j.events_since(0)?.into_iter().filter(|e| e.kind == STATE_EVENT).collect();
    let digest = |d: &str| d.split(' ').next().unwrap_or_default().to_owned();
    // each campaign's latest state; the unresolved ones are those whose latest is `left`
    let mut latest: BTreeMap<&str, &crate::journal::Event> = BTreeMap::new();
    for e in &events {
        latest.insert(e.subject.as_str(), e);
    }
    let Some(left) = latest.values().filter(|e| e.state == "left").max_by_key(|e| e.n) else { return Ok(()) };
    if events.iter().any(|e| e.state != "left" && digest(&e.detail) == now.digest) {
        return Ok(());
    }
    let rest = left.detail.split_once(' ').map(|(_, s)| s.to_owned()).unwrap_or_default();
    let (storage, why) = rest.split_once(" — ").map_or((rest.clone(), None), |(s, w)| (s.to_owned(), Some(w.to_owned())));
    let accepted = events.iter().rev().find(|e| e.subject == left.subject && (e.state == "accepted" || e.state == "start")).map(|e| digest(&e.detail)).unwrap_or_default();
    Err(Code::CampaignStateMoved.refuse(format!(
        "campaign {} last left {} at {} (journal event {}, its storage {storage}{}): it could not put the project back, and the project is {} now, no state a campaign line recorded; put it back to that campaign's last accepted state {accepted} (the run's result or progress in {storage} names what was left), then run the line again",
        left.subject,
        ctx.root().display(),
        digest(&left.detail),
        left.n,
        why.map(|w| format!("; {w}")).unwrap_or_default(),
        now.digest
    )))
}

/// Node `id` was running when the process ended: its workspace undone through the transform records its run kept
/// (newest first, each by the declared `transform --revert`), then read again; it must be the state the node started
/// from, else refused with exactly what was found. A node with no mark never ran anything (its mark is written before
/// its experiment line); a node that made its own isolated workspace runs again in a new one, so its old one is left.
fn recover(ctx: &mut Ctx, run_dir: &Path, plan: &CampaignPlan, id: &str, exclude: &[String], job: Option<&String>) -> Result<String> {
    let Some(node) = plan.node(id) else { return Ok(format!("{id}: interrupted, no longer in the plan")) };
    let file = run_dir.join(NODES).join(id).join(MARK);
    let Some(mark) = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Mark>(&b).ok()) else {
        return Ok(format!("{id}: interrupted before it ran anything; it runs again"));
    };
    if node.strategy == WorkspaceStrategy::Isolated {
        return Ok(format!("{id}: interrupted in its own workspace {}; it runs again in a new one", mark.workspace.display()));
    }
    let shared = mark.workspace == ctx.root();
    let mut own;
    let wctx: &mut Ctx = if shared {
        ctx
    } else {
        own = Ctx::at(&mark.workspace)?;
        &mut own
    };
    let (now, _) = state_of(&mark.workspace, exclude)?;
    if now == mark.start {
        return Ok(format!("{id}: interrupted; {} is at the state it started from; it runs again", mark.workspace.display()));
    }
    // only this node's own transforms: after its mark, and of a change spec its bound experiment names
    let own_specs = node_changes(run_dir, id, wctx.root());
    // a record is the node's when it ran one of the node's own change-spec files (and, when the record carries its
    // digest, that file as it is): never by a change's name alone, which two files may share
    let records: Vec<_> = records_since(wctx, mark.after_seq).into_iter().filter(|(_, r)| own_specs.iter().any(|(path, digest)| record_spec(wctx.root(), r) == *path && (r.spec_digest.is_empty() || digest.as_deref().is_none_or(|d| d == r.spec_digest)))).collect();
    let mut said = Vec::new();
    let (mut done, mut why, mut after) = (0, None, now.clone());
    // one at a time, newest first, reading the state after each: the walk stops at the node's start, never past it
    for rec in records.iter().rev() {
        let (n, w) = put_back(wctx, std::slice::from_ref(rec), if shared { job } else { None }, &mut said);
        done += n;
        after = state_of(&mark.workspace, exclude)?.0;
        if w.is_some() {
            why = w;
            break;
        }
        if after == mark.start {
            break;
        }
    }
    if why.is_some() || after != mark.start {
        let unnamed = unnamed_records(wctx, &file);
        // the project left as it is, journaled: no run, in any storage, starts from it until it is put back
        if shared {
            let storage = run_dir.parent().and_then(Path::parent).unwrap_or(run_dir);
            state_event(wctx, job.map(String::as_str), &plan.campaign_id, "left", &format!("{} — {id} was interrupted and could not be put back", after.digest), storage)?;
        }
        return Err(Code::CampaignStateMoved.refuse(format!(
            "{id} was interrupted in {}: {done} of {} of its transforms put back ({}){}; the workspace is {}, the state it started from {}{}: put the workspace back to the state it started from by hand, then run the line again",
            mark.workspace.display(),
            records.len(),
            said.join(" · "),
            why.map(|w| format!(", then {w}")).unwrap_or_default(),
            after.digest,
            mark.start.digest,
            if unnamed.is_empty() {
                String::new()
            } else {
                format!("; on disk and named by no receipt (the process ended while code-passenger wrote): {} — each lists the files it wrote with their sha256 before and after; put each of those files back to its digest before (the bytes are kept in the workspace's .passenger objects under that digest)", unnamed.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))
            }
        )));
    }
    Ok(format!("{id}: interrupted; {done} transforms put back newest first ({}); {} is at the state it started from; it runs again", said.join(" · "), mark.workspace.display()))
}

/// The change specs node `id`'s bound experiment names (`nodes/<id>/experiment.json`): each file, canonical, and its
/// digest as a transform record names it (`ChangeSpec::digest`), when it reads.
fn node_changes(run_dir: &Path, id: &str, root: &Path) -> Vec<(PathBuf, Option<String>)> {
    let Ok(spec) = experiments::read_spec(run_dir.join(NODES).join(id).join("experiment.json")) else { return Vec::new() };
    spec.mutation.changes.iter().map(|c| {
        let path = if c.spec.is_absolute() { c.spec.clone() } else { root.join(&c.spec) };
        let digest = custodian_kit::change::ChangeSpec::from_path(&path).ok().map(|s| s.digest());
        (path.canonicalize().unwrap_or(path), digest)
    }).collect()
}

/// A transform record's change spec, canonical.
fn record_spec(root: &Path, r: &custodian_kit::transform::TransformRecord) -> PathBuf {
    let p = Path::new(&r.spec);
    let p = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    p.canonicalize().unwrap_or(p)
}

/// Transform records on disk in this governed folder (its own transforms, its jobs'), written after `mark` was, that
/// no receipt names: what a process that ended while code-passenger wrote leaves.
fn unnamed_records(ctx: &Ctx, mark: &Path) -> Vec<PathBuf> {
    let since = std::fs::metadata(mark).and_then(|m| m.modified()).ok();
    let named: std::collections::BTreeSet<PathBuf> = ctx.journal().and_then(|j| j.receipts()).map(|rs| rs.iter().flat_map(|r| r.outputs.iter().map(|o| ctx.root().join(o))).collect()).unwrap_or_default();
    let folder = ctx.root().join(crate::layout::FOLDER);
    let mut dirs = vec![folder.join("data/transforms")];
    for state in ["running", "done"] {
        for job in std::fs::read_dir(folder.join("state/jobs").join(state)).into_iter().flatten().flatten() {
            dirs.push(job.path().join(custodian_kit::jobs::JOB_DATA).join("transforms"));
        }
    }
    let mut out = Vec::new();
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "json") && !p.to_string_lossy().ends_with(".revert.json") && !named.contains(&p) && since.is_some_and(|t| e.metadata().and_then(|m| m.modified()).is_ok_and(|m| m >= t)) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn file_ref(path: &Path) -> Result<FileRef> {
    let bytes = std::fs::read(path).map_err(|e| Code::Write.refuse(format!("{}: {e}", path.display())))?;
    Ok(FileRef { path: path.to_path_buf(), digest: experiments::campaign::digest::bytes(&bytes) })
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    write_whole(path, (serde_json::to_string_pretty(value).map_err(|e| Code::Write.refuse(e))? + "\n").as_bytes())
}

/// What every node runs with.
struct Governor<'a> {
    loaded: &'a LoadedCampaign,
    settings: crate::mode::Settings,
    /// `<storage>/<campaign id>`.
    dir: PathBuf,
    job: Option<String>,
    again: bool,
}

/// What a node is given to run: where it runs is decided by its workspace.
struct Env<'a> {
    loaded: &'a LoadedCampaign,
    settings: &'a crate::mode::Settings,
    run_dir: PathBuf,
    exclude: Vec<String>,
    job: Option<String>,
    /// Each workspace's one writer.
    writers: BTreeMap<String, AtomicAdmission>,
}

impl Governor<'_> {
    fn govern(&self, ctx: &mut Ctx, mut ran: Ran) -> Result<Ran> {
        let plan = &self.loaded.plan;
        let root = ctx.root().to_path_buf();
        let exclude = plan.exclude.clone();
        let progress_file = self.dir.join(PROGRESS);
        let (mut now, files) = state_of(&root, &exclude)?;
        let tools = tools();
        if plan.rigor == Rigor::Strict {
            // strict: the writer's identity is established before anything runs, or nothing runs
            if plan.specs.iter().any(|s| !s.changes.is_empty()) && !tools.iter().any(|t| t.name == "code-passenger" && t.digest.is_some()) {
                return Err(Code::CampaignRefused.refuse("strict: the writer's digest cannot be established (no code-passenger on PATH)"));
            }
            // and its starting state is exact: nothing the code digest reads is left out
            for e in &exclude {
                let all = custodian_kit::provenance::tree_files(&root)?;
                if let Some(p) = all.keys().find(|p| *p == e || p.starts_with(&format!("{e}/"))) {
                    return Err(Code::CampaignRefused.refuse(format!("strict: exclude `{e}` leaves out {p}, a file of the starting tree; strict's starting state is exact")));
                }
            }
        }
        let _ = files;
        let kept = progress_file.exists().then(|| experiments::read_campaign_progress(&progress_file).map_err(|e| Code::CampaignRefused.refuse(format!("{}: {e}", progress_file.display())))).transpose()?;
        let mut runner = match (kept, self.again) {
            (Some(kept), false) => {
                let done = self.dir.join(RUNS).join(&kept.run_id).join(RESULT);
                if done.is_file() {
                    return self.finished(ctx, &done, &kept, &now, ran);
                }
                // each node the process ended in: undone through its own records, back to the state it started from
                let run_dir = self.dir.join(RUNS).join(&kept.run_id);
                for id in &kept.running {
                    let line = recover(ctx, &run_dir, plan, id, &exclude, self.job.as_ref())?;
                    ran.say(line);
                }
                now = state_of(&root, &exclude)?.0;
                let r = CampaignRunner::resume(plan.clone(), kept).map_err(|e| Code::CampaignRefused.refuse(e.0.join(" · ")))?;
                if plan.rigor == Rigor::Strict {
                    let then: Vec<(String, Option<String>, Option<String>)> = r.progress().tools.iter().map(|t| (t.name.clone(), t.version.clone(), t.digest.clone())).collect();
                    let here: Vec<(String, Option<String>, Option<String>)> = tools.iter().map(|t| (t.name.clone(), t.version.clone(), t.digest.clone())).collect();
                    if !then.is_empty() && then != here {
                        return Err(Code::CampaignRefused.refuse(format!("strict: run {} began with other tools ({}); these are {}: run it fresh with --again", r.progress().run_id, show_tools(&then), show_tools(&here))));
                    }
                }
                let want = r.workspace("shared").and_then(|w| w.accepted.clone()).or_else(|| r.start_state().cloned());
                if want.as_ref() != Some(&now) {
                    return Err(Code::CampaignStateMoved.refuse(format!("{} is {} ({} files); run {} recorded its accepted state {}: put it back, or run the campaign fresh with --again", root.display(), now.digest, now.files, r.progress().run_id, want.map(|w| w.digest).unwrap_or_else(|| "none".into()))));
                }
                let kept_nodes = r.progress().nodes.len();
                ran.say(format!("resumed run {}: {kept_nodes} of {} nodes kept as they completed", r.progress().run_id, plan.nodes.len()));
                r
            }
            (kept, _) => {
                // --again over a run kept: only from a state that run can name, never a tree it left dirty
                if let Some(k) = &kept {
                    nameable(k, &root, &now)?;
                }
                // whatever storage it was kept in: never from a tree this campaign last left dirty
                not_left_dirty(ctx, &plan.campaign_id, &now)?;
                // a run of its own: named by the time, never a run kept before it
                let id = fresh(&self.dir.join(RUNS).join(format!("{}-{}", plan.campaign_id, crate::journal::stamp())));
                CampaignRunner::new(plan.clone(), &id.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
        };
        runner.set_tools(tools.clone());
        state_event(ctx, self.job.as_deref(), &plan.campaign_id, "start", now.digest.as_str(), &self.dir)?;
        runner.set_start(now.clone());
        let run_dir = self.dir.join(RUNS).join(&runner.progress().run_id);
        let plan_file = run_dir.join(PLAN);
        if !plan_file.exists() {
            write_json(&plan_file, plan)?;
        }
        write_json(&progress_file, runner.progress())?;
        ran.say(format!("campaign {} run {} · plan {} · start {} ({} files)", plan.campaign_id, runner.progress().run_id, plan.digest, now.digest, now.files));
        let env = Env {
            loaded: self.loaded,
            settings: &self.settings,
            run_dir: run_dir.clone(),
            exclude: exclude.clone(),
            job: self.job.clone(),
            writers: plan.workspaces.iter().map(|w| (w.id.clone(), AtomicAdmission::new(1).expect("an admission of one"))).collect(),
        };
        // the campaign's start, kept when an isolated workspace is made from it
        let start = run_dir.join(STATES).join(START);
        if plan.snapshots.iter().any(|s| s == START) && !start.exists() {
            let first = runner.start_state().cloned().unwrap_or_else(|| now.clone());
            copy_state(&root, &start, &exclude, &first)?;
        }
        engine(ctx, &env, &mut runner, &progress_file)?;
        runner.close();
        write_json(&progress_file, runner.progress())?;
        let facts = HostFacts { tools, plan_file: Some(file_ref(&plan_file)?), enforced: enforced(plan.rigor), excluded: excluded(&exclude) };
        let result = runner.result(facts);
        let result_file = run_dir.join(RESULT);
        experiments::write_campaign_result(&result_file, &result).map_err(|e| Code::Write.refuse(format!("{}: {e}", result_file.display())))?;
        self.conclude(ctx, &result, &result_file, ran)
    }

    /// The run kept has a result: it answers again while the project is at the state it left; else refused.
    fn finished(&self, ctx: &mut Ctx, file: &Path, kept: &CampaignProgress, now: &StateIdentity, mut ran: Ran) -> Result<Ran> {
        let result = experiments::read_campaign_result(file).map_err(|e| Code::CampaignRefused.refuse(format!("{}: {e}", file.display())))?;
        if result.plan_digest != self.loaded.plan.digest {
            return Err(Code::CampaignRefused.refuse(format!("run {} completed with plan {}; the plan is {} now: run it fresh with --again", result.run_id, result.plan_digest, self.loaded.plan.digest)));
        }
        let left = kept.workspaces.get("shared").and_then(|w| w.accepted.clone()).or_else(|| kept.start.clone());
        if left.as_ref() != Some(now) {
            return Err(Code::CampaignStateMoved.refuse(format!("run {} completed and left {}; {} is {} now: run it fresh with --again", result.run_id, left.map(|w| w.digest).unwrap_or_default(), ctx.root().display(), now.digest)));
        }
        ran.say(format!("run {} completed already, over this state: its result answers (--again runs it fresh)", result.run_id));
        let rel = ctx.rel(file);
        ran.say(format!("result: {rel}"));
        ran.outputs = vec![rel];
        status(&result, &mut ran);
        Ok(ran)
    }

    fn conclude(&self, ctx: &mut Ctx, result: &CampaignResult, file: &Path, mut ran: Ran) -> Result<Ran> {
        let c = &result.counts;
        ran.say(format!("{} nodes: {} completed ({} promote, {} do not promote, {} undecided), {} refused, {} failed, {} skipped; {} accepted, {} restored, {} discarded", c.nodes, c.completed, c.promote, c.do_not_promote, c.undecided, c.refused, c.failed, c.skipped, c.accepted, c.restored, c.discarded));
        for w in &result.workspaces {
            ran.say(format!("workspace {}: {}{}", w.id, w.accepted.as_ref().map(|a| a.digest.clone()).unwrap_or_else(|| "no accepted state".into()), if w.discarded { " (discarded)" } else { "" }));
        }
        for n in result.nodes.iter().filter(|n| n.run.status != NodeStatus::Completed) {
            ran.say(format!("{} {}: {}", n.plan.id, n.run.status.word(), n.run.reason.clone().unwrap_or_default()));
        }
        if let Some(s) = &result.stopped {
            ran.say(format!("stopped: {s}"));
        }
        if let Some(stop) = &result.stop {
            ran.say(format!("running when it stopped, each run to its end: {}", if stop.in_flight.is_empty() { "none".into() } else { stop.in_flight.join(", ") }));
        }
        let rel = ctx.rel(file);
        ran.say(format!("result: {rel}"));
        if let Some(j) = ctx.journal.as_ref() {
            j.event(EVENT, self.job.as_deref(), &result.campaign_id, result.status.word(), &rel, None)?;
        }
        ran.outputs = vec![rel];
        status(result, &mut ran);
        Ok(ran)
    }
}

fn status(result: &CampaignResult, ran: &mut Ran) {
    match result.status {
        CampaignStatus::Done => ran.code = 0,
        CampaignStatus::Findings => ran.code = 1,
        CampaignStatus::Refused => {
            let why = result.stopped.clone().or_else(|| result.nodes.iter().find(|n| matches!(n.run.status, NodeStatus::Refused | NodeStatus::Failed)).map(|n| format!("{}: {}", n.plan.id, n.run.reason.clone().unwrap_or_default()))).unwrap_or_else(|| "a node did not run".into());
            ran.refusal = Some(Code::CampaignRefused.refuse(format!("campaign {} run {}: {why}", result.campaign_id, result.run_id)));
        }
    }
}

/// What the code-passenger at `program` says of itself (`--version`, as an argv line): its name and version on the
/// first line (`code_passenger 0.7.0 (custodian_kit 0.5.1)`), and from 0.7.0 the operations it performs on the next
/// (`operations: make_struct ... replay`).
fn passenger_says(program: &Path) -> Option<String> {
    use xccute_runtime::ShellCommand;
    xccute_runtime::CommandSpec::new(program.display().to_string()).args(vec!["--version".to_string()]).to_command().output().ok().map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Whether the code-passenger at `program` (the one this process would run: the first on PATH) performs every
/// operation the campaign's change specs use, read from what its `version` says. Refused, naming the operation, the
/// program and its version, when it says no operations at all (before 0.7.0 none is said) or not that one. The line it
/// is, when the campaign makes a change and a writer is on PATH; none otherwise.
pub fn writer_performs(loaded: &LoadedCampaign, program: Option<&Path>) -> Result<Option<String>> {
    // each operation used, with the first experiment and change that use it; each change file where the
    // experiments' own rule resolves it (a node's binding: the declared target)
    let mut used: BTreeMap<String, String> = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for node in loaded.plan.nodes.iter().filter(|n| seen.insert(n.spec.clone())) {
        let bound = loaded.bound(&node.id, &loaded.root).map_err(|e| Code::CampaignRefused.refuse(e.0.join(" · ")))?;
        for c in &bound.mutation.changes {
            let spec = custodian_kit::change::ChangeSpec::from_path(&c.spec).map_err(|e| Code::CampaignRefused.refuse(format!("{} change {}: {e}", node.spec, c.label)))?;
            used.entry(spec.op().as_str().to_owned()).or_insert_with(|| format!("{}'s change {} ({})", node.spec, c.label, c.spec.display()));
        }
    }
    let Some((op, first)) = used.first_key_value() else { return Ok(None) };
    // no writer on PATH at all is the rule that was there before this one: strict refuses it before anything runs,
    // elsewhere the first change is refused at its node
    let Some(program) = program else { return Ok(None) };
    let says = passenger_says(program).unwrap_or_default();
    let version = says.lines().next().unwrap_or("no version said").to_owned();
    let performs: Vec<&str> = says.lines().find_map(|l| l.strip_prefix("operations:")).map(|o| o.split_whitespace().collect()).unwrap_or_default();
    if performs.is_empty() {
        return Err(Code::CampaignRefused.refuse(format!("code-passenger {} ({version}) does not say the operations it performs (its version lists none), so it is not shown to perform `{op}`, which {first} uses: code-passenger 0.7.0 or later says them", program.display())));
    }
    if let Some((op, first)) = used.iter().find(|(o, _)| !performs.contains(&o.as_str())) {
        return Err(Code::CampaignRefused.refuse(format!("code-passenger {} ({version}) does not perform `{op}`, which {first} uses", program.display())));
    }
    Ok(Some(format!("writer: code-passenger {} ({version}) performs {}", program.display(), used.keys().cloned().collect::<Vec<_>>().join(", "))))
}

/// The tools the campaign ran with.
fn tools() -> Vec<ToolVersion> {
    let digest = |p: &Path| std::fs::read(p).ok().map(|b| experiments::campaign::digest::bytes(&b));
    let me = std::env::current_exe().ok();
    let mine = me.as_deref().and_then(digest);
    let passenger = crate::maint::passenger_path();
    // the writer's own word for its version (`code_passenger 0.6.0 (custodian_kit 0.5.0)`): what it says first
    let version = passenger.as_deref().and_then(passenger_says).and_then(|o| o.lines().next().map(str::to_owned));
    vec![
        ToolVersion { name: "custodian".into(), version: Some(env!("CARGO_PKG_VERSION").into()), digest: mine.clone(), path: me.clone() },
        // linked into custodian: its digest is the binary's that carries it
        ToolVersion { name: "experiments".into(), version: Some(format!("{} (linked into custodian)", experiments::VERSION)), path: me, digest: mine },
        ToolVersion { name: "code-passenger".into(), version, digest: passenger.as_deref().and_then(digest), path: passenger },
    ]
}

/// What the scientific state leaves out: custodian_kit's provenance, then the campaign's own.
fn excluded(exclude: &[String]) -> Vec<String> {
    let mut v: Vec<String> = ["target/", ".git/", ".passenger/", ".custodian/", "oneo.project.json (at the root)"].iter().map(|s| s.to_string()).collect();
    v.extend(exclude.iter().cloned());
    v
}

/// What custodian established for every node, by rigor; nothing more is claimed.
fn enforced(rigor: Rigor) -> Vec<String> {
    vec![
        "the plan made and checked before any node ran; nodes run in its order, as their dependencies completed".into(),
        "each node's start state (every file's path, content and mode, less what is excluded) read and compared with its state parent's accepted state before it ran; a node that did not start there was refused".into(),
        "each experiment and change spec read again and its digest compared with the plan's before its node ran".into(),
        "each command given exactly its node's binding (campaign, trial, replicate, treatment, factor levels, controls, blocks) as environment variables, kept in its record".into(),
        "each source change made by code_passenger through custodian's preflight and transform lines, with receipts".into(),
        match rigor {
            Rigor::Exploratory => "each candidate compared with its start: a change no transform record declares is recorded as a finding".into(),
            Rigor::Controlled => "each candidate compared with its start: a change no transform record declares is a finding, and the candidate is not accepted".into(),
            Rigor::Strict => "each candidate compared with its start: a change no transform record declares refuses the node".into(),
        },
        "each rejected candidate put back by `transform --revert` of its transform records, newest first, and the state read again: equal to the start, or the workspace is discarded".into(),
        "each isolated workspace made from a kept copy of its state parent's accepted state, its digest read back, with its own journal and one writer".into(),
        "not controlled by the campaign: the machine, its load and its clock, and the environment beyond the binding".into(),
    ]
}

/// The nodes run: what is ready launched in plan order while a slot is free; a shared node on this thread (this
/// project's context), an isolated one on a thread of its own; every node's end recorded, its state kept when an
/// isolated workspace is made from it, the progress written.
fn engine(ctx: &mut Ctx, env: &Env, runner: &mut CampaignRunner, progress_file: &Path) -> Result<()> {
    let slots = AtomicAdmission::new(runner.plan().policy.max_parallel.max(1)).expect("an admission of at least one");
    let mut kept: Result<()> = Ok(());
    let mut keep = |r: Result<()>| {
        if let Err(e) = r {
            kept = Err(e);
        }
    };
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel::<NodeRecord>();
        // each running node's slot, held here until its end is recorded: what runs and what the runner counts as
        // running are the same nodes
        let mut held: BTreeMap<String, uh_tom_mick::AdmissionGuard<'_>> = BTreeMap::new();
        loop {
            while let Ok(rec) = rx.try_recv() {
                held.remove(&rec.id);
                keep(end(env, runner, rec, progress_file));
            }
            let mut shared: Option<(PlanNode, uh_tom_mick::AdmissionGuard<'_>)> = None;
            for node in runner.ready() {
                let Ok(slot) = slots.try_enter() else { break };
                runner.start(&node.id);
                // what is running is kept too: a resume runs it again
                keep(write_json(progress_file, runner.progress()));
                if node.workspace == "shared" {
                    shared = Some((node, slot));
                    break;
                }
                let expected = runner.expected_start(&node);
                let existing = runner.workspace(&node.workspace).and_then(|w| w.path.clone());
                let source = node.from.clone().unwrap_or_else(|| START.to_owned());
                let tx = tx.clone();
                held.insert(node.id.clone(), slot);
                scope.spawn(move || {
                    let rec = isolated(env, &node, expected, existing, &source);
                    let _ = tx.send(rec);
                });
            }
            if let Some((node, slot)) = shared {
                let expected = runner.expected_start(&node);
                let root = ctx.root().to_path_buf();
                let rec = hosted(ctx, env, &node, expected, &root, env.job.clone());
                let (word, digest) = match (&rec.state, &rec.accepted, &rec.left) {
                    (Some(StateAction::Discarded), _, left) => ("left", left.as_ref().map(|l| l.digest.clone()).unwrap_or_else(|| "unreadable".into())),
                    (_, Some(a), _) => ("accepted", a.digest.clone()),
                    _ => ("start", rec.start.as_ref().map(|s| s.digest.clone()).unwrap_or_default()),
                };
                keep(state_event(ctx, env.job.as_deref(), &env.loaded.plan.campaign_id, word, &digest, env.run_dir.parent().and_then(Path::parent).unwrap_or(&env.run_dir)));
                keep(end(env, runner, rec, progress_file));
                drop(slot);
                continue;
            }
            if held.is_empty() {
                break;
            }
            match rx.recv() {
                Ok(rec) => {
                    held.remove(&rec.id);
                    keep(end(env, runner, rec, progress_file));
                }
                Err(_) => break,
            }
        }
    });
    kept
}

/// A node's end: its accepted state kept when a workspace is made from it, its record given to the runner, the
/// progress written.
fn end(env: &Env, runner: &mut CampaignRunner, rec: NodeRecord, progress_file: &Path) -> Result<()> {
    let rec = keep_state(env, runner, rec);
    runner.finish(rec);
    write_json(progress_file, runner.progress())
}

/// The node's accepted state kept under the run's `states/` when an isolated workspace is made from it.
fn keep_state(env: &Env, runner: &CampaignRunner, mut rec: NodeRecord) -> NodeRecord {
    if rec.status != NodeStatus::Completed || rec.state == Some(StateAction::Discarded) || !runner.plan().snapshots.contains(&rec.id) {
        return rec;
    }
    let (Some(ws), Some(accepted)) = (rec.workspace_path.clone(), rec.accepted.clone()) else { return rec };
    let to = env.run_dir.join(STATES).join(&rec.id);
    let to = fresh(&to);
    if let Err(e) = copy_state(&ws, &to, &env.exclude, &accepted) {
        rec.findings.push(format!("its accepted state was not kept for the workspaces made from it: {e}"));
    }
    rec
}

/// The kept copy of `source`'s accepted state (the last one made, when a node ran again).
fn kept_state(env: &Env, source: &str) -> Option<PathBuf> {
    let base = env.run_dir.join(STATES).join(source);
    let mut found = base.exists().then(|| base.clone());
    for k in 2.. {
        let p = base.with_file_name(format!("{source}.{k}"));
        if !p.exists() {
            break;
        }
        found = Some(p);
    }
    found
}

/// An isolated node: its workspace made (or, `continue`, the one its lineage evolves), then run there.
fn isolated(env: &Env, node: &PlanNode, expected: Option<StateIdentity>, existing: Option<PathBuf>, source: &str) -> NodeRecord {
    let made = match node.strategy {
        WorkspaceStrategy::Continue => existing.ok_or_else(|| Code::CampaignRefused.refuse(format!("{}: the workspace {} it continues was never made", node.id, node.workspace))),
        _ => make_workspace(env, node, expected.as_ref(), source),
    };
    let ws = match made {
        Ok(ws) => ws,
        Err(e) => return refused(node, e.to_string()),
    };
    let mut wctx = match Ctx::at(&ws) {
        Ok(c) => c,
        Err(e) => return refused(node, e.to_string()),
    };
    // this workspace's own job lock, held while its node runs: no other process opens it meanwhile
    let line = Line::ExperimentRun(surface::ExperimentRun { spec: node.spec.clone().into(), ..surface::ExperimentRun::default() });
    match crate::runtime::under_job_lock(&mut wctx, &line, |c| Ok(hosted(c, env, node, expected.clone(), &ws, None))) {
        Ok(rec) => rec,
        Err(e) => refused(node, e.to_string()),
    }
}

/// A governed copy of `source`'s accepted state, made for `node`: the copy, its digest read back, then a custodian
/// folder of its own (the project's settings, then the declared `init` line, its first receipt).
fn make_workspace(env: &Env, node: &PlanNode, expected: Option<&StateIdentity>, source: &str) -> Result<PathBuf> {
    let want = expected.ok_or_else(|| Code::CampaignRefused.refuse(format!("{}: its state parent {source} has no accepted state", node.id)))?;
    let from = kept_state(env, source).ok_or_else(|| Code::CampaignStateMoved.refuse(format!("{}: {source}'s accepted state was not kept (its workspace was discarded, or it never completed)", node.id)))?;
    let ws = fresh(&env.run_dir.join(WORKSPACES).join(&node.id));
    copy_state(&from, &ws, &env.exclude, want)?;
    let layout = crate::layout::Layout::at(&ws)?;
    write_whole(&layout.path::<crate::layout::Settings>(&[])?, toml::to_string(env.settings).map_err(|e| Code::Write.refuse(e))?.as_bytes())?;
    let mut wctx = Ctx::at(&ws)?;
    let init = facade::execute(&mut wctx, &Line::Init(surface::Init::default()), crate::cli::handle);
    if init.code != 0 {
        return Err(init.refusal.unwrap_or_else(|| Code::CampaignRefused.refuse(format!("{}: init {}", ws.display(), init.said.join(" · ")))));
    }
    Ok(ws)
}

fn refused(node: &PlanNode, why: String) -> NodeRecord {
    let mut r = NodeRecord::new(&node.id, NodeStatus::Refused);
    r.reason = Some(why);
    r
}

/// A running node's mark, kept in its folder under the run before it runs anything (`running.json`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Mark {
    workspace: PathBuf,
    start: StateIdentity,
    after_seq: i64,
}

const MARK: &str = "running.json";

/// `records` put back, newest first, each through the declared `transform --revert`; each line said in `said`. How
/// many completed, and why the next did not.
fn put_back(ctx: &mut Ctx, records: &[(PathBuf, custodian_kit::transform::TransformRecord)], job: Option<&String>, said: &mut Vec<String>) -> (usize, Option<String>) {
    let mut done = 0;
    for (file, _) in records.iter().rev() {
        let back = facade::execute(ctx, &Line::Transform(surface::Transform { revert: Some(file.clone().into_os_string()), job: job.cloned().map(Into::into), ..surface::Transform::default() }), crate::cli::handle);
        said.push(format!("transform --revert {} → {} ({}) receipt #{}", ctx.rel(file), back.code, back.receipt.reason, back.receipt.seq));
        if back.code != 0 {
            return (done, Some(format!("the revert of {} did not complete: {}", ctx.rel(file), back.said.join(" · "))));
        }
        done += 1;
    }
    (done, None)
}

/// The newest receipt number in this context's journal.
fn last_seq(ctx: &Ctx) -> Result<i64> {
    Ok(ctx.journal()?.receipts()?.last().map(|x| x.seq).unwrap_or(0))
}

/// The transform records written in this context after receipt `after`, in the order they were written: each named
/// by the output of a `transform` receipt (custodian's line and code_passenger's both name it; once each).
fn records_since(ctx: &Ctx, after: i64) -> Vec<(PathBuf, custodian_kit::transform::TransformRecord)> {
    let Ok(receipts) = ctx.journal().and_then(|j| j.receipts()) else { return Vec::new() };
    let mut out: Vec<(PathBuf, custodian_kit::transform::TransformRecord)> = Vec::new();
    let mine = receipts.iter().filter(|r| r.seq > after && r.command == "transform" && r.accepted);
    // a record put back already (its revert's own record, `<record>.revert.json`, named by a receipt) is not one to put
    // back again
    let reverted: std::collections::BTreeSet<PathBuf> = mine.clone().flat_map(|r| r.outputs.iter()).filter_map(|o| o.strip_suffix(".revert.json")).map(|o| ctx.root().join(format!("{o}.json"))).collect();
    for r in mine {
        for o in r.outputs.iter().filter(|o| o.ends_with(".json") && !o.ends_with(".revert.json")) {
            let p = ctx.root().join(o);
            if out.iter().any(|(q, _)| q == &p) || reverted.contains(&p) {
                continue;
            }
            if let Some(rec) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<custodian_kit::transform::TransformRecord>(&t).ok()).filter(|t| t.written) {
                out.push((p, rec));
            }
        }
    }
    out
}

/// The changes between `start` and `candidate` no transform record declares: a path no record names, one whose
/// content is not what its last record left, or one whose mode moved when no record declares that mode (a replay's
/// postcondition `replay <path>: sha256 <hex>, mode <bits>`).
fn undeclared(start: &BTreeMap<String, String>, candidate: &BTreeMap<String, String>, records: &[(PathBuf, custodian_kit::transform::TransformRecord)]) -> Vec<String> {
    let mut declared: BTreeMap<&str, (&str, Option<String>)> = BTreeMap::new();
    for (_, r) in records {
        for f in &r.files {
            let mode = r.postconditions.iter().find_map(|(p, _)| p.strip_prefix(&format!("replay {}: ", f.path)).and_then(|rest| rest.rsplit_once(", mode ")).map(|(_, m)| m.trim().to_owned()));
            declared.insert(f.path.as_str(), (f.after.as_str(), mode));
        }
    }
    let part = |v: Option<&String>, i: usize| v.and_then(|v| v.split(' ').nth(i)).map(|x| x.trim_start_matches("sha256:").to_owned());
    custodian_kit::provenance::differences(start, candidate)
        .into_iter()
        .filter(|d| {
            let p = &d[1..];
            match declared.get(p) {
                None => true,
                Some((after, mode)) => {
                    let content = part(candidate.get(p), 0).unwrap_or_else(|| "none".into());
                    let (was, now) = (part(start.get(p), 1), part(candidate.get(p), 1));
                    // a mode a replay declares is the one the file must have; with none declared, a file the node
                    // made must carry no execute bit (what a write makes by default) and a file it changed its old one
                    let mode_moved = match (mode, &was, &now) {
                        (Some(m), _, Some(n)) => m != n,
                        (None, None, Some(n)) => u32::from_str_radix(n, 8).is_ok_and(|bits| bits & 0o111 != 0),
                        (None, Some(w), Some(n)) => w != n,
                        _ => false,
                    };
                    content != *after || mode_moved
                }
            }
        })
        .collect()
}

/// One node run in `ctx` (its workspace `ws`): its start checked, its experiment bound and run as the declared
/// `experiment run` line, its candidate read and compared, its outcome decided by the campaign, and a rejected
/// candidate put back.
fn hosted(ctx: &mut Ctx, env: &Env, node: &PlanNode, expected: Option<StateIdentity>, ws: &Path, job: Option<String>) -> NodeRecord {
    let mut rec = NodeRecord::new(&node.id, NodeStatus::Refused);
    rec.workspace_path = Some(ws.to_path_buf());
    let Some(writer) = env.writers.get(&node.workspace) else { return refused(node, format!("{} is no workspace of the plan", node.workspace)) };
    let _writer = match writer.try_enter() {
        Ok(g) => g,
        Err(e) => return refused(node, format!("{} has its writer already ({e:?}): one writer per workspace", node.workspace)),
    };
    let (start, start_files) = match state_of(ws, &env.exclude) {
        Ok(s) => s,
        Err(e) => return refused(node, e.to_string()),
    };
    rec.start = Some(start.clone());
    if expected.as_ref() != Some(&start) {
        rec.reason = Some(format!("{} is {} ({} files); its state parent {} accepted {}: a node starts from its state parent's accepted state, never from what is on disk", ws.display(), start.digest, start.files, node.from.as_deref().unwrap_or("(the campaign's start)"), expected.map(|e| e.digest).unwrap_or_else(|| "nothing".into())));
        return rec;
    }
    let bound = match env.loaded.bound(&node.id, ws) {
        Ok(b) => b,
        Err(e) => {
            rec.reason = Some(e.to_string());
            return rec;
        }
    };
    let node_dir = env.run_dir.join(NODES).join(&node.id);
    let spec_file = node_dir.join("experiment.json");
    if let Err(e) = std::fs::create_dir_all(&node_dir).map_err(|e| e.to_string()).and_then(|_| experiments::write_spec(&spec_file, &bound).map_err(|e| e.to_string())) {
        rec.reason = Some(format!("{}: {e}", spec_file.display()));
        return rec;
    }
    rec.bound_spec = file_ref(&spec_file).ok();
    // the journal unread is no number to undo from: the node is refused before it runs anything
    let before = match last_seq(ctx) {
        Ok(n) => n,
        Err(e) => {
            rec.reason = Some(format!("the journal cannot be read, so this node's transforms could not be told from others': {e}"));
            return rec;
        }
    };
    // what a resume needs to undo this node if the process ends before its end is recorded: where it runs, the
    // state it started from, the last receipt before it (its transforms are every one after it)
    let mark = Mark { workspace: ws.to_path_buf(), start: start.clone(), after_seq: before };
    if let Err(e) = write_json(&node_dir.join(MARK), &mark) {
        rec.reason = Some(e.to_string());
        return rec;
    }
    // the campaign's exclude, what an exact tree measurement leaves out of both trees as the campaign's state does
    let line = Line::ExperimentRun(surface::ExperimentRun { spec: spec_file.clone().into_os_string(), storage: Some(node_dir.join("experiment").into_os_string()), job: job.clone().map(Into::into), exclude: env.exclude.iter().map(Into::into).collect() });
    let out = facade::execute(ctx, &line, crate::cli::handle);
    let execution = match (&out.refusal, out.receipt.outputs.first()) {
        (None, Some(o)) => {
            let path = ctx.root().join(o);
            match experiments::read_result(&path) {
                Ok(r) => {
                    rec.evidence = r.evidence.clone();
                    Execution::Concluded { promotion: r.promotion, outcome: r.evaluation.conclusion.outcome, result: file_ref(&path).unwrap_or(FileRef { path: path.clone(), digest: String::new() }), run_id: r.run_id.clone(), observations: r.observations.clone() }
                }
                Err(e) => Execution::Failed(format!("{}: {e}", path.display())),
            }
        }
        (Some(r), _) if r.code == Code::ExperimentRefused.code() => Execution::Failed(r.to_string()),
        (Some(r), _) => Execution::Refused(r.to_string()),
        (None, None) => Execution::Failed(format!("experiment run #{} named no result", out.receipt.seq)),
    };
    // what the experiment concluded is the node's whatever happens to the workspace after
    if let Execution::Concluded { promotion, outcome, result, run_id, observations } = &execution {
        rec.promotion = Some(*promotion);
        rec.outcome = Some(*outcome);
        rec.result = Some(result.clone());
        rec.run_id = Some(run_id.clone());
        rec.observations = Some(observations.clone());
    }
    let records = records_since(ctx, before);
    let d = match state_of(ws, &env.exclude) {
        Ok((candidate, candidate_files)) => {
            rec.candidate = Some(candidate);
            let changed = undeclared(&start_files, &candidate_files, &records);
            CampaignRunner::decide(node, &execution, &changed)
        }
        // the candidate cannot be read: never accepted; put back as far as the records go, then read again
        Err(e) => Decision { status: NodeStatus::Refused, accept: false, reason: Some(format!("the candidate state could not be read: {e}")), findings: Vec::new() },
    };
    rec.status = d.status;
    rec.reason = d.reason;
    rec.findings = d.findings;
    if d.accept {
        rec.state = Some(StateAction::Accepted);
        rec.accepted = rec.candidate.clone();
    } else {
        let (back, why) = put_back(ctx, &records, job.as_ref(), &mut rec.restore);
        let now = state_of(ws, &env.exclude);
        match (&why, &now) {
            (None, Ok((n, _))) if n == &start => {
                rec.state = Some(StateAction::Restored);
                rec.accepted = Some(start);
            }
            _ => {
                rec.state = Some(StateAction::Discarded);
                rec.left = now.as_ref().ok().map(|(n, _)| n.clone());
                let read = match &now {
                    Ok((n, _)) => n.digest.clone(),
                    Err(e) => format!("unreadable ({e})"),
                };
                let why = why.unwrap_or_else(|| format!("after {back} reverts the workspace is {read}, not its start {}", start.digest));
                rec.findings.push(format!("the previous accepted state could not be put back: {why}"));
            }
        }
    }
    if let Ok(all) = ctx.journal().and_then(|j| j.receipts()) {
        let dir = ctx.layout().ok().and_then(|l| l.dir::<crate::layout::ReceiptFile>(&[("seq", "1")]).ok());
        rec.receipts = all.iter().filter(|r| r.seq > before).map(|r| match &dir {
            Some(d) => format!("{} #{} {} → {}", d.join(format!("{}.json", r.seq)).display(), r.seq, r.line, r.reason),
            None => format!("#{} {} → {}", r.seq, r.line, r.reason),
        }).collect();
    }
    rec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_no_record_declares_is_undeclared() {
        let m = |v: &[(&str, &str)]| v.iter().map(|(p, d)| (p.to_string(), d.to_string())).collect::<BTreeMap<_, _>>();
        let start = m(&[("a", "sha256:aa 644"), ("b", "sha256:bb 644")]);
        let cand = m(&[("a", "sha256:a2 644"), ("b", "sha256:bb 755"), ("c", "sha256:cc 644")]);
        let rec: custodian_kit::transform::TransformRecord = serde_json::from_value(serde_json::json!({
            "schema": "x", "spec_name": "s", "spec": "s", "spec_digest": "d", "operation": "replay", "target": "*",
            "files": [{"path": "a", "before": "aa", "after": "a2"}], "checks": [], "postconditions": [], "written": true
        })).unwrap();
        let u = undeclared(&start, &cand, &[(PathBuf::from("r.json"), rec.clone())]);
        assert_eq!(u, ["~b", "+c"], "a's change is declared; b's mode and c are not");
        // a declared file whose mode moved too: undeclared, unless the replay declares that mode
        let cand = m(&[("a", "sha256:a2 755"), ("b", "sha256:bb 644")]);
        assert_eq!(undeclared(&start, &cand, &[(PathBuf::from("r.json"), rec.clone())]), ["~a"]);
        let mut with_mode = rec;
        with_mode.postconditions = vec![("replay a: sha256 a2, mode 755".into(), true)];
        assert!(undeclared(&start, &cand, &[(PathBuf::from("r.json"), with_mode)]).is_empty());
        // a file the node made: executable with no mode declared is undeclared; plain, or with its mode declared, not
        let rec_new: custodian_kit::transform::TransformRecord = serde_json::from_value(serde_json::json!({
            "schema": "x", "spec_name": "s", "spec": "s", "spec_digest": "d", "operation": "replay", "target": "*",
            "files": [{"path": "n", "before": "none", "after": "nn"}], "checks": [], "postconditions": [], "written": true
        })).unwrap();
        let made = |mode: &str| m(&[("a", "sha256:aa 644"), ("b", "sha256:bb 644"), ("n", &format!("sha256:nn {mode}"))]);
        assert_eq!(undeclared(&start, &made("755"), &[(PathBuf::from("r.json"), rec_new.clone())]), ["+n"]);
        assert!(undeclared(&start, &made("644"), &[(PathBuf::from("r.json"), rec_new.clone())]).is_empty());
        let mut declared_755 = rec_new;
        declared_755.postconditions = vec![("replay n: sha256 nn, mode 755".into(), true)];
        assert!(undeclared(&start, &made("755"), &[(PathBuf::from("r.json"), declared_755)]).is_empty());
    }

    #[test]
    fn the_scientific_state_leaves_out_the_runtime_and_reads_every_files_mode() {
        let d = std::env::temp_dir().join(format!("custodian-state-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        let (bare, files) = state_of(&d, &["run".into()]).unwrap();
        assert_eq!(bare.files, 1);
        assert!(files["src/lib.rs"].starts_with("sha256:"), "{files:?}");
        // custodian's folder, the build, git, the passenger's store, init's root file and the campaign's own exclude
        for (dir, file) in [(".custodian/data", "r.json"), ("target/debug", "x"), (".git", "HEAD"), (".passenger", "o"), ("run/out", "result.json")] {
            std::fs::create_dir_all(d.join(dir)).unwrap();
            std::fs::write(d.join(dir).join(file), "runtime").unwrap();
        }
        std::fs::write(d.join("oneo.project.json"), "{}").unwrap();
        assert_eq!(state_of(&d, &["run".into()]).unwrap().0, bare, "runtime paths are not the scientific state");
        assert_ne!(state_of(&d, &[]).unwrap().0, bare, "an output kept in the tree is, unless excluded");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.join("src/lib.rs"), std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_ne!(state_of(&d, &["run".into()]).unwrap().0, bare, "a mode is part of the state");
        }
    }

    #[test]
    fn storage_inside_the_root_is_custodians_own_folder_or_refused() {
        let root = std::env::temp_dir().join("custodian-collision");
        assert!(collision(&root, &root.join(".custodian/data/campaigns")).is_ok());
        assert!(collision(&root, &root.join("runs")).is_err());
        assert!(collision(&root, &root).is_err());
        assert!(collision(&root.join("inner"), &root).is_err());
        assert!(collision(&root, &std::env::temp_dir().join("elsewhere")).is_ok());
    }
}
