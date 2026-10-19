//! `custodian diagnostics list [CRATE] | where CODE | from LOGFILE` and `custodian triage CODE|@LOGFILE`.
//!
//! diagnostics is custodian_kit's `diagnostics` reading kind (every liaise code table read through klepto: each
//! code's meaning and declaration, and every site raising it), narrowed by a crate, a code, or the codes in a log's
//! text. triage is the job template custodian ships (custodian_kit's `TRIAGE`): diagnostics, then reach from each
//! raising item, the tests that reach them, and the marks there, each step reading the step before it, the job
//! closed into one receipt.

use crate::codes::{Code, Result};
use crate::facade::{Ctx, Ran};
use crate::readings::{self, Ask};
use std::path::Path;

/// A log named on the line as the kit reads it: `@<absolute path>`; `-` reads stdin into a file first.
fn log_target(ctx: &Ctx, log: &str) -> Result<String> {
    let file = if log == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).map_err(|e| Code::ReadingUnreadable.refuse(e))?;
        let dir = match ctx.mode.project() {
            Some(p) => p.layout.dir::<crate::layout::LogFile>(&[("name", "x")])?,
            None => std::env::temp_dir(),
        };
        let f = dir.join(format!("log-{}.txt", crate::journal::stamp()));
        crate::layout::write_whole(&f, text.as_bytes())?;
        f
    } else {
        let p = Path::new(log.trim_start_matches('@'));
        std::env::current_dir().unwrap_or_default().join(p)
    };
    if !file.is_file() {
        return Err(Code::ReadingUnreadable.refuse(format!("{}: no such log", file.display())));
    }
    Ok(format!("@{}", file.canonicalize().unwrap_or(file).display()))
}

/// `custodian diagnostics list|where|from`.
pub fn diagnostics(ctx: &mut Ctx, action: &str, arg: Option<&str>) -> Result<Ran> {
    let target = match (action, arg) {
        ("list", None) => "*".to_string(),
        ("list", Some(c)) => c.to_string(),
        ("where", Some(code)) => code.to_string(),
        ("from", Some(log)) => log_target(ctx, log)?,
        _ => return Err(Code::LineRefused.refuse(format!("diagnostics {action}: list [CRATE] | where CODE | from LOGFILE"))),
    };
    let mut spec = readings::inline_spec("diagnostics", &target)?;
    spec.name = format!("diagnostics-{action}-{}", readings::slug(arg.unwrap_or("all")));
    let job = ctx.current_job(None)?;
    let (ran, _) = readings::read_spec(ctx, &spec, &readings::From::Line, &Ask { job, ..Ask::default() })?;
    Ok(ran)
}

/// `custodian triage CODE|@LOGFILE`: custodian_kit's triage template, started and closed as one job; its brief printed.
pub fn triage(ctx: &mut Ctx, target: &str) -> Result<Ran> {
    ctx.mode.need_project()?.need_agent()?;
    let target = if target.starts_with('@') || Path::new(target).is_file() { log_target(ctx, target)? } else { target.to_string() };
    let (started, closing, file, said) = custodian_kit::jobs::run_template(&crate::jobs::kit(ctx), "triage", &target, false)?;
    let mut ran = Ran::named(&started.id);
    ran.job = Some(started.id.clone());
    ran.said.extend(started.said);
    ran.said.extend(said);
    ran.print(custodian_kit::jobs::render_closing(&closing));
    let file = if file.is_dir() { file.join("closing.md") } else { file };
    ran.say(format!("closing receipt: {}", ctx.rel(&file)));
    ran.outputs = vec![ctx.rel(&file)];
    Ok(ran)
}
