//! `custodian self install|uninstall|status`: custodian installs itself through `installation`: a plan first
//! (every path decided, nothing written), the install from the plan (copied, its SHA-256 verified, renamed into
//! place, installation's receipt written), the receipt kept in the journal too when the run is journaled, and the
//! uninstall from the receipt. Never `cargo install --path`.

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::surface::{s, so, SelfLine};
use installation::{Installer, Scope};
use std::path::PathBuf;

pub const BIN: &str = "custodian";

fn installer(l: &SelfLine) -> Result<Installer> {
    match (so(&l.bin_dir), so(&l.state_dir)) {
        (Some(b), Some(st)) => Installer::user_in(BIN, PathBuf::from(b), PathBuf::from(st)),
        _ => Installer::new(BIN, Scope::User),
    }
    .map_err(|e| Code::Install.refuse(e))
}

pub fn self_line(ctx: &mut Ctx, l: &SelfLine) -> Result<Ran> {
    let inst = installer(l)?;
    let mut ran = Ran::named(format!("self {}", s(&l.action)));
    match s(&l.action).as_str() {
        "install" => {
            let from = so(&l.from).map(PathBuf::from).unwrap_or_else(|| ctx.exe.clone());
            let from = from.canonicalize().map_err(|e| Code::Install.refuse(format!("{}: {e}", from.display())))?;
            let plan = inst.plan(BIN, &from).map_err(|e| Code::Install.refuse(e))?;
            ran.print(plan.describe());
            if l.dry_run {
                ran.say(format!("planned: {} → {} (--dry-run: nothing written)", plan.source.display(), plan.destination.display()));
                return Ok(ran);
            }
            let receipt = inst.install(&plan).map_err(|e| Code::Install.refuse(e))?;
            let json = serde_json::to_string_pretty(&receipt).map_err(|e| Code::Install.refuse(e))?;
            ran.say(format!("installed {} → {} (sha256 {})", receipt.source.display(), receipt.destination.display(), crate::digest::short(&receipt.sha256, 16)));
            ran.say(format!("installation's receipt: {}", plan.receipt.display()));
            if !std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == inst.bin_dir())) {
                ran.say(format!("{} is not on PATH in this shell", inst.bin_dir().display()));
            }
            ran.outputs = vec![receipt.destination.display().to_string(), plan.receipt.display().to_string()];
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
