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
        let (now, _) = state_of(&root, &exclude)?;
        let tools = tools();
        // strict: the writer's identity is established before anything runs, or nothing runs
        if plan.rigor == Rigor::Strict && plan.specs.iter().any(|s| !s.changes.is_empty()) && !tools.iter().any(|t| t.name == "code-passenger" && t.digest.is_some()) {
            return Err(Code::CampaignRefused.refuse("strict: the writer's digest cannot be established (no code-passenger on PATH)"));
        }
        let mut runner = match (progress_file.exists(), self.again) {
            (true, false) => {
                let kept = experiments::read_campaign_progress(&progress_file).map_err(|e| Code::CampaignRefused.refuse(format!("{}: {e}", progress_file.display())))?;
                let done = self.dir.join(RUNS).join(&kept.run_id).join(RESULT);
                if done.is_file() {
                    return self.finished(ctx, &done, &now, ran);
                }
                let r = CampaignRunner::resume(plan.clone(), kept).map_err(|e| Code::CampaignRefused.refuse(e.0.join(" · ")))?;
                let want = r.workspace("shared").and_then(|w| w.accepted.clone()).or_else(|| r.start_state().cloned());
                if want.as_ref() != Some(&now) {
                    return Err(Code::CampaignStateMoved.refuse(format!("{} is {} ({} files); run {} recorded its accepted state {}: put it back, or run the campaign fresh with --again", root.display(), now.digest, now.files, r.progress().run_id, want.map(|w| w.digest).unwrap_or_else(|| "none".into()))));
                }
                let kept_nodes = r.progress().nodes.len();
                ran.say(format!("resumed run {}: {kept_nodes} of {} nodes kept as they completed", r.progress().run_id, plan.nodes.len()));
                r
            }
            _ => {
                // a run of its own: named by the time, never a run kept before it
                let id = fresh(&self.dir.join(RUNS).join(format!("{}-{}", plan.campaign_id, crate::journal::stamp())));
                CampaignRunner::new(plan.clone(), &id.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
        };
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
    fn finished(&self, ctx: &mut Ctx, file: &Path, now: &StateIdentity, mut ran: Ran) -> Result<Ran> {
        let result = experiments::read_campaign_result(file).map_err(|e| Code::CampaignRefused.refuse(format!("{}: {e}", file.display())))?;
        if result.plan_digest != self.loaded.plan.digest {
            return Err(Code::CampaignRefused.refuse(format!("run {} completed with plan {}; the plan is {} now: run it fresh with --again", result.run_id, result.plan_digest, self.loaded.plan.digest)));
        }
        let left = result.workspaces.iter().find(|w| w.id == "shared").and_then(|w| w.accepted.clone()).or_else(|| result.start.clone());
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

/// The tools the campaign ran with.
fn tools() -> Vec<ToolVersion> {
    let digest = |p: &Path| std::fs::read(p).ok().map(|b| experiments::campaign::digest::bytes(&b));
    let me = std::env::current_exe().ok();
    let passenger = crate::maint::passenger_path();
    vec![
        ToolVersion { name: "custodian".into(), version: Some(env!("CARGO_PKG_VERSION").into()), digest: me.as_deref().and_then(digest), path: me },
        ToolVersion { name: "experiments".into(), version: Some(experiments::VERSION.into()), path: None, digest: None },
        ToolVersion { name: "code-passenger".into(), version: None, digest: passenger.as_deref().and_then(digest), path: passenger },
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

/// The newest receipt number in this context's journal.
fn last_seq(ctx: &Ctx) -> i64 {
    ctx.journal().and_then(|j| j.receipts()).ok().and_then(|r| r.last().map(|x| x.seq)).unwrap_or(0)
}

/// The transform records written in this context after receipt `after`, in the order they were written: each named
/// by the output of a `transform` receipt (custodian's line and code_passenger's both name it; once each).
fn records_since(ctx: &Ctx, after: i64) -> Vec<(PathBuf, custodian_kit::transform::TransformRecord)> {
    let Ok(receipts) = ctx.journal().and_then(|j| j.receipts()) else { return Vec::new() };
    let mut out: Vec<(PathBuf, custodian_kit::transform::TransformRecord)> = Vec::new();
    for r in receipts.iter().filter(|r| r.seq > after && r.command == "transform" && r.accepted) {
        for o in r.outputs.iter().filter(|o| o.ends_with(".json") && !o.ends_with(".revert.json")) {
            let p = ctx.root().join(o);
            if out.iter().any(|(q, _)| q == &p) {
                continue;
            }
            if let Some(rec) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<custodian_kit::transform::TransformRecord>(&t).ok()).filter(|t| t.written) {
                out.push((p, rec));
            }
        }
    }
    out
}

/// The changes between `start` and `candidate` no transform record declares: a path no record names, or one whose
/// content is not what its last record left.
fn undeclared(start: &BTreeMap<String, String>, candidate: &BTreeMap<String, String>, records: &[(PathBuf, custodian_kit::transform::TransformRecord)]) -> Vec<String> {
    let mut declared: BTreeMap<&str, &str> = BTreeMap::new();
    for (_, r) in records {
        for f in &r.files {
            declared.insert(f.path.as_str(), f.after.as_str());
        }
    }
    let content = |v: Option<&String>| v.map(|v| v.split(' ').next().unwrap_or_default().trim_start_matches("sha256:").to_owned()).unwrap_or_else(|| "none".into());
    custodian_kit::provenance::differences(start, candidate)
        .into_iter()
        .filter(|d| match declared.get(&d[1..]) {
            None => true,
            Some(after) => content(candidate.get(&d[1..])) != *after,
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
    let before = last_seq(ctx);
    let line = Line::ExperimentRun(surface::ExperimentRun { spec: spec_file.clone().into_os_string(), storage: Some(node_dir.join("experiment").into_os_string()), job: job.clone().map(Into::into) });
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
    let (candidate, candidate_files) = match state_of(ws, &env.exclude) {
        Ok(s) => s,
        Err(e) => {
            rec.reason = Some(e.to_string());
            return rec;
        }
    };
    rec.candidate = Some(candidate.clone());
    let records = records_since(ctx, before);
    let changed = undeclared(&start_files, &candidate_files, &records);
    let d = CampaignRunner::decide(node, &execution, &changed);
    rec.status = d.status;
    rec.reason = d.reason;
    rec.findings = d.findings;
    if let Execution::Concluded { promotion, outcome, result, run_id, observations } = execution {
        rec.promotion = Some(promotion);
        rec.outcome = Some(outcome);
        rec.result = Some(result);
        rec.run_id = Some(run_id);
        rec.observations = Some(observations);
    }
    if d.accept {
        rec.state = Some(StateAction::Accepted);
        rec.accepted = Some(candidate);
    } else {
        // put back, newest first, each through the declared `transform --revert`; then read again
        let mut why = None;
        for (file, _) in records.iter().rev() {
            let back = facade::execute(ctx, &Line::Transform(surface::Transform { revert: Some(file.clone().into_os_string()), job: job.clone().map(Into::into), ..surface::Transform::default() }), crate::cli::handle);
            rec.restore.push(format!("transform --revert {} → {} ({}) receipt #{}", ctx.rel(file), back.code, back.receipt.reason, back.receipt.seq));
            if back.code != 0 {
                why = Some(format!("the revert of {} did not complete: {}", ctx.rel(file), back.said.join(" · ")));
                break;
            }
        }
        let now = state_of(ws, &env.exclude).map(|(s, _)| s).ok();
        if why.is_none() && now.as_ref() == Some(&start) {
            rec.state = Some(StateAction::Restored);
        } else {
            rec.state = Some(StateAction::Discarded);
            let why = why.unwrap_or_else(|| format!("after {} reverts the workspace is {}, not its start {}", records.len(), now.map(|n| n.digest).unwrap_or_default(), start.digest));
            rec.findings.push(format!("the previous accepted state could not be put back: {why}"));
        }
        rec.accepted = Some(start);
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
        let u = undeclared(&start, &cand, &[(PathBuf::from("r.json"), rec)]);
        assert_eq!(u, ["~b", "+c"], "a's change is declared; b's mode and c are not");
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
