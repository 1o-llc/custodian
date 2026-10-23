//! `custodian self install|uninstall|status`: custodian installs itself through `installation`: a plan first
//! (every path decided, nothing written), the install from the plan (copied, its SHA-256 verified, renamed into
//! place, installation's receipt written), the receipt kept in the journal too when the run is journaled, and the
//! uninstall from the receipt. Never `cargo install --path`. The install writes the agent skill beside the receipt
//! (`<state dir>/custodian/SKILL.md`, or `--skill-dir`): the rules an agent works by here and every line, in the
//! shape an agent harness loads a skill (a folder, a SKILL.md with `name` and `description` frontmatter).

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};

use installation::{Installer, Scope};
use std::path::PathBuf;

pub const BIN: &str = "custodian";

/// What a `self` line asks: its action and its options.
#[derive(Debug, Clone, Default)]
pub struct SelfAsk {
    pub action: &'static str,
    pub from: Option<String>,
    pub bin_dir: Option<String>,
    pub state_dir: Option<String>,
    pub skill_dir: Option<String>,
    pub dry_run: bool,
    pub force: bool,
}

/// The rules an agent works by in a repository custodian serves, in order: the skill's body.
pub const SKILL_RULES: [&str; 6] = [
    "First line in any repo: `custodian agent brief`. Read what is pending before anything else.",
    "Before reading code by hand: run the reading for the question (dependents, reach, impact, readers of env, duplicates, requirements). Reuse is free.",
    "Before changing code: `job start` with an intent and a verify line. Strict mode for a known change, help mode for an investigation.",
    "In strict mode: specs only. If the vocabulary lacks the operation, extend code_passenger, never hand-edit.",
    "A decision you cannot make: `decide` asks it at the start. Never mid-task, never twice.",
    "At the end: `job close`, then `handoff` is the message. Nothing else is written by hand.",
];

/// The skill's file: its frontmatter, the six rules, and every line as `custodian` with no words lists them.
pub fn skill_text() -> String {
    let mut s = String::from("---\nname: custodian\ndescription: How an agent works in a repository custodian serves: the first line, the readings before reading code, jobs, strict-mode specs, decisions asked once, and the handoff. Use it in any repository with a .custodian folder, or before reading or changing Rust code there.\n---\n\n# custodian\n\n");
    for (i, r) in SKILL_RULES.iter().enumerate() {
        s.push_str(&format!("{}. {r}\n", i + 1));
    }
    s.push_str("\n## Every line\n\n```text\n");
    s.push_str(&crate::cli::usage());
    s.push_str("```\n");
    s
}

/// The skill's folder: `--skill-dir`, else `custodian` beside the install receipt.
fn skill_dir(inst: &Installer, l: &SelfAsk) -> PathBuf {
    l.skill_dir.clone().map(PathBuf::from).unwrap_or_else(|| inst.state_dir().join(BIN))
}

fn installer(l: &SelfAsk) -> Result<Installer> {
    match (l.bin_dir.clone(), l.state_dir.clone()) {
        (Some(b), Some(st)) => Installer::user_in(BIN, PathBuf::from(b), PathBuf::from(st)),
        _ => Installer::new(BIN, Scope::User),
    }
    .map_err(|e| Code::Install.refuse(e))
}

pub fn self_line(ctx: &mut Ctx, l: &SelfAsk) -> Result<Ran> {
    let inst = installer(l)?;
    let mut ran = Ran::named(format!("self {}", l.action));
    match l.action {
        "install" => {
            let from = l.from.clone().map(PathBuf::from).unwrap_or_else(|| ctx.exe.clone());
            let from = from.canonicalize().map_err(|e| Code::Install.refuse(format!("{}: {e}", from.display())))?;
            let plan = inst.plan(BIN, &from).map_err(|e| Code::Install.refuse(e))?;
            ran.print(plan.describe());
            let skill = skill_dir(&inst, l).join("SKILL.md");
            if l.dry_run {
                ran.say(format!("planned: {} → {} (--dry-run: nothing written)", plan.source.display(), plan.destination.display()));
                ran.say(format!("planned: the agent skill → {}", skill.display()));
                return Ok(ran);
            }
            let receipt = inst.install(&plan).map_err(|e| Code::Install.refuse(e))?;
            let json = serde_json::to_string_pretty(&receipt).map_err(|e| Code::Install.refuse(e))?;
            ran.say(format!("installed {} → {} (sha256 {})", receipt.source.display(), receipt.destination.display(), crate::digest::short(&receipt.sha256, 16)));
            ran.say(format!("installation's receipt: {}", plan.receipt.display()));
            crate::layout::write_whole(&skill, skill_text().as_bytes())?;
            ran.say(format!("the agent skill: {}", skill.display()));
            if !std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == inst.bin_dir())) {
                ran.say(format!("{} is not on PATH in this shell", inst.bin_dir().display()));
            }
            ran.outputs = vec![receipt.destination.display().to_string(), plan.receipt.display().to_string(), skill.display().to_string()];
            ran.install = Some(("install".into(), BIN.into(), receipt.destination.display().to_string(), receipt.sha256.clone(), json));
        }
        "uninstall" => match inst.uninstall(BIN, l.force).map_err(|e| Code::Install.refuse(e))? {
            Some(r) => {
                ran.say(format!("uninstalled {} (the receipt from {})", r.destination.display(), r.installed_at_unix));
                ran.install = Some(("uninstall".into(), BIN.into(), r.destination.display().to_string(), r.sha256.clone(), serde_json::to_string_pretty(&r).unwrap_or_default()));
            }
            None => ran.say(format!("nothing to uninstall: no receipt for {BIN} in {}", inst.state_dir().display())),
        },
        _ => match inst.receipt(BIN).map_err(|e| Code::Install.refuse(e))? {
            Some(r) => {
                let now = installation::sha256_file(&r.destination).ok();
                let same = now.as_deref() == Some(r.sha256.as_str());
                ran.say(format!("installed: {} sha256 {} ({})", r.destination.display(), crate::digest::short(&r.sha256, 16), if same { "the file matches its receipt" } else { "the file no longer matches its receipt" }));
                let skill = skill_dir(&inst, l).join("SKILL.md");
                ran.say(match std::fs::read_to_string(&skill) {
                    Ok(t) if t == skill_text() => format!("the agent skill: {}", skill.display()),
                    Ok(_) => format!("the agent skill: {} (written by another version; `self install` writes this one's)", skill.display()),
                    Err(_) => format!("the agent skill: none at {}", skill.display()),
                });
                if !same {
                    ran.code = 1;
                }
            }
            None => {
                ran.say(format!("not installed: no receipt for {BIN} in {}", inst.state_dir().display()));
                ran.code = 1;
            }
        },
    }
    Ok(ran)
}
