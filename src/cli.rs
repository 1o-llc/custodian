//! custodian's own commands, beside the step-42 commands that main.rs keeps
//! as they were. Each says what it did and where, last.

use custodian_kit::jobs;
use custodian_kit::paths::{PathsTable, Resolving};
use custodian_kit::reading::refusal::ReadingRefusal;
use custodian_kit::reading::spec::{DeclaredRole, ReadingKind, ReadingSpec};
use custodian_kit::runs::{self, Ctx};
use custodian_kit::store::{Mode, Store};
use custodian_kit::testrun::{self, Observe};
use custodian_kit::{annotate, bundle, check, diff, features, reading, sweep, CustodianError};
use std::path::{Path, PathBuf};

/// The words custodian answers to, beside the step-42 commands.
pub const COMMANDS: &[(&str, &str)] = &[
    ("init", "init [--agent]: make .custodian/ here (plain: specs, readings, journal; --agent adds jobs)"),
    ("agent", "agent init: make .custodian/ with the job layout"),
    ("run", "run [SPEC] [--again]: one spec, or every queued spec in order (free mode: printed, nothing stored)"),
    ("read", "read --kind KIND --target TARGET [--name N]: an inline search, free mode"),
    ("exec-reading", "exec-reading --spec S --root R --out DIR|- --stamp T [--job ID]: the reading line the executor runs"),
    ("list", "list: the queued and done specs and every reading's latest"),
    ("kinds", "kinds: the spec vocabulary"),
    ("where", "where: every declared route and every named path"),
    ("diff", "diff NAME [SEQ_A SEQ_B]: what changed between two readings of one spec"),
    ("inventory", "inventory: render .custodian/data/inventory.md from the receipts"),
    ("job", "job start [NAME|ID] [--again] | job close ID | job run [NAME] | job brief ID [--bundle] | job import BUNDLE | job list"),
    ("triage", "triage CODE|@LOGFILE: the built-in triage job, started and closed"),
    ("test", "test [--affected [--ref R | --spec S]] [--all] [--observe failed|progress|all] | test report"),
    ("check", "check sensitivity|mechanism|boundary|supersedes|reads_env|golden|all: maintenance's own lines"),
    ("diagnostics", "diagnostics list [CRATE] | where CODE | from FILE|-: liaise codes, their declarations and sites"),
    ("features", "features [--apply]: primary constituents and feature-eligible dependencies, code_passenger's spec"),
    ("annotate", "annotate [--from CHECK_JSON] [--apply]: the maintenance attributes the receipts call for, dry run"),
    ("bundle", "bundle JOB [--reveal]: a job folder as one annotated file"),
    ("sweep", "sweep DIR: list an undeclared tree and offer routes to declare"),
    ("self", "self install [BIN --from DIR] [--scope user|system] [--dry-run] | self uninstall [BIN] | self status [BIN]"),
];

pub fn is_command(word: &str) -> bool {
    COMMANDS.iter().any(|(w, _)| *w == word)
}

/// The words after a command: flags, `--key value`, and the rest in order.
struct Args {
    rest: Vec<String>,
    flags: Vec<String>,
    kv: Vec<(String, String)>,
}

const KEYS: &[&str] = &["--kind", "--target", "--name", "--spec", "--root", "--out", "--stamp", "--job", "--ref", "--observe", "--from", "--scope"];

impl Args {
    fn of(args: &[String]) -> Args {
        let (mut rest, mut flags, mut kv) = (Vec::new(), Vec::new(), Vec::new());
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            if KEYS.contains(&a.as_str()) && i + 1 < args.len() {
                kv.push((a.clone(), args[i + 1].clone()));
                i += 2;
                continue;
            }
            if a.starts_with("--") {
                flags.push(a.clone());
            } else {
                rest.push(a.clone());
            }
            i += 1;
        }
        Args { rest, flags, kv }
    }
    fn flag(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x == f)
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.kv.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str())
    }
    fn pos(&self, i: usize) -> Option<&str> {
        self.rest.get(i).map(String::as_str)
    }
}

fn ctx() -> Ctx {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("custodian"));
    Ctx::find(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")), exe)
}

fn say(lines: &[String]) {
    for l in lines {
        println!("{l}");
    }
}

fn fail(e: CustodianError) -> ! {
    eprintln!("{e}");
    std::process::exit(1);
}

/// Runs one of custodian's commands; exits non-zero on a refusal.
pub fn dispatch(word: &str, args: &[String]) {
    if let Err(e) = run(word, args) {
        fail(e);
    }
}

fn admitted(c: &Ctx) -> custodian_kit::Result<Option<custodian_kit::store::Writer>> {
    match &c.store {
        Some(s) => {
            let w = s.admit();
            let decided = custodian_kit::decisions::writer(w.as_ref().map(|_| ()).map_err(|e| e.to_string()));
            println!("{}", decided.said());
            w.map(Some)
        }
        None => Ok(None),
    }
}

fn run(word: &str, args: &[String]) -> custodian_kit::Result<()> {
    let a = Args::of(args);
    match word {
        "init" | "agent" => {
            let mode = if word == "agent" || a.flag("--agent") { Mode::Agent } else { Mode::Plain };
            let here = std::env::current_dir().map_err(|e| ReadingRefusal::Write.refuse(e))?;
            let (store, made) = Store::init(&here, mode)?;
            let paths = custodian_kit::paths::paths_file(&store)?;
            if !paths.exists() {
                custodian_kit::store::write_whole(&paths, b"# routes this project declares: [decl] name = [\"dir/*.toml\"], [path] raw paths a job may name\n[decl]\n\npath = []\n")?;
            }
            println!("initialized {} ({:?}) at {}", custodian_kit::store::FOLDER, mode, store.root.display());
            for m in made {
                println!("  {}", m.strip_prefix(&store.root).unwrap_or(&m).display());
            }
            println!("  {}", paths.strip_prefix(&store.root).unwrap_or(&paths).display());
        }
        "exec-reading" => {
            let (Some(spec), Some(root), Some(out), Some(stamp)) = (a.get("--spec"), a.get("--root"), a.get("--out"), a.get("--stamp")) else {
                return Err(ReadingRefusal::SpecInvalid.refuse("exec-reading --spec S --root R --out DIR|- --stamp T"));
            };
            let summary = reading::exec_reading(Path::new(spec), Path::new(root), out, stamp, a.get("--job").is_some())?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        "run" => {
            let c = ctx();
            let _w = admitted(&c)?;
            match a.pos(0) {
                Some(spec) => {
                    let ran = runs::run_reading(&c, Path::new(spec), a.flag("--again"), None)?;
                    if let Some(p) = &ran.printed {
                        println!("{p}");
                    }
                    if let Some(s) = &ran.summary {
                        for x in &s.counts {
                            println!("  {}: {}", x.what, x.n);
                        }
                    }
                    say(&ran.said);
                }
                None => {
                    let ran = runs::run_queue(&c, a.flag("--again"))?;
                    if ran.is_empty() {
                        println!("nothing queued in .custodian/data/specs/queued");
                    }
                    for r in ran {
                        say(&r.said);
                    }
                }
            }
        }
        "read" => {
            let (Some(kind), Some(target)) = (a.get("--kind"), a.get("--target")) else {
                return Err(ReadingRefusal::SpecInvalid.refuse("read --kind KIND --target TARGET [--name N]"));
            };
            let kind: ReadingKind = kind.parse().map_err(|e: String| ReadingRefusal::SpecInvalid.refuse(e))?;
            let name = a.get("--name").unwrap_or("inline");
            let spec = format!("name = {}\nquestion = \"inline search\"\nkind = \"{}\"\ntarget = {}\n", toml::Value::String(name.into()), kind.as_str(), toml::Value::String(target.into()));
            ReadingSpec::from_toml(&spec)?;
            let tmp = std::env::temp_dir().join(format!("custodian-inline-{}.toml", std::process::id()));
            std::fs::write(&tmp, spec).map_err(|e| ReadingRefusal::Write.refuse(e))?;
            let mut c = ctx();
            c.store = None;
            let ran = runs::run_reading(&c, &tmp, true, None)?;
            if let Some(p) = &ran.printed {
                println!("{p}");
            }
            say(&ran.said);
        }
        "list" => {
            let c = ctx();
            let store = c.store()?;
            for (state, dir) in [("queued", store.dir_of(&custodian_kit::store::named::QUEUED_SPEC, &[("name", "x")])?), ("done", store.dir_of(&custodian_kit::store::named::DONE_SPEC, &[("name", "x")])?)] {
                let mut v: Vec<String> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
                v.sort();
                println!("specs {state}: {}", if v.is_empty() { "-".to_string() } else { v.join(", ") });
            }
            let journal = store.journal()?;
            let mut last: std::collections::BTreeMap<String, custodian_kit::store::Receipt> = Default::default();
            for r in journal.runs()?.into_iter().filter(|r| r.command == "reading") {
                last.insert(r.name.clone(), r);
            }
            for (name, r) in last {
                println!("{name}: #{} {} {}{}", r.seq, r.utc, r.outputs.first().cloned().unwrap_or_default(), r.reused_seq.map(|n| format!(" (reuse of #{n})")).unwrap_or_default());
            }
        }
        "kinds" => {
            print!("{}", ReadingKind::doc_table());
            println!();
            print!("{}", DeclaredRole::doc_table());
        }
        "where" => {
            let c = ctx();
            let table = PathsTable::load(&c.root, c.store.as_ref())?;
            for (name, at) in table.routes() {
                println!("{name}\t{at}");
            }
        }
        "diff" => {
            let c = ctx();
            let store = c.store()?;
            let name = a.pos(0).ok_or_else(|| ReadingRefusal::SpecInvalid.refuse("diff NAME [SEQ_A SEQ_B]"))?;
            let readings = store.journal()?.readings_of(name)?;
            let pick = |seq: Option<&str>, fallback: Option<&custodian_kit::store::Receipt>| -> Option<custodian_kit::store::Receipt> {
                match seq.and_then(|s| s.trim_start_matches('#').parse::<i64>().ok()) {
                    Some(n) => readings.iter().find(|r| r.seq == n).cloned(),
                    None => fallback.cloned(),
                }
            };
            let n = readings.len();
            let (before, after) = (pick(a.pos(1), n.checked_sub(2).and_then(|i| readings.get(i))), pick(a.pos(2), readings.last()));
            let (Some(b), Some(af)) = (before, after) else {
                return Err(ReadingRefusal::NoDiff.refuse(format!("{name}: {n} readings")));
            };
            let load = |r: &custodian_kit::store::Receipt| -> custodian_kit::Result<reading::ReadingReport> {
                let j = r.outputs.iter().find(|o| o.ends_with(".json")).ok_or_else(|| ReadingRefusal::ReadingUnreadable.refuse(format!("#{}", r.seq)))?;
                reading::load(&c.root.join(j))
            };
            let d = diff::compare(&load(&b)?, &load(&af)?);
            print!("{}", diff::render(&d));
            println!("compared run #{} with run #{}", b.seq, af.seq);
        }
        "inventory" => {
            let c = ctx();
            let p = runs::inventory(c.store()?)?;
            println!("inventory: {}", c.rel(&p));
        }
        "job" => {
            let c = ctx();
            let _w = admitted(&c)?;
            match a.pos(0) {
                Some("start") => {
                    let s = jobs::start(&c, a.pos(1), a.flag("--again"))?;
                    say(&s.said);
                    println!("job {} is running in {} (close it: custodian job close {})", s.id, c.rel(&s.folder), s.id);
                }
                Some("close") => {
                    let id = a.pos(1).ok_or_else(|| ReadingRefusal::NoJob.refuse("job close ID"))?;
                    let (_c, _done, said) = jobs::close(&c, id)?;
                    say(&said);
                }
                Some("run") => {
                    let s = jobs::start(&c, a.pos(1), a.flag("--again"))?;
                    say(&s.said);
                    let (_c, _d, said) = jobs::close(&c, &s.id)?;
                    say(&said);
                }
                Some("brief") => {
                    let id = a.pos(1).ok_or_else(|| ReadingRefusal::NoJob.refuse("job brief ID"))?;
                    print!("{}", jobs::brief(&c, id)?);
                    if a.flag("--bundle") {
                        let (out, digest, held) = bundle_job(&c, id, a.flag("--reveal"))?;
                        println!("bundle: {} ({digest}, {held} windows held redacted)", c.rel(&out));
                    }
                }
                Some("import") => {
                    let b = a.pos(1).ok_or_else(|| ReadingRefusal::MissingInput.refuse("job import BUNDLE"))?;
                    let said = import(&c, Path::new(b), a.flag("--again"))?;
                    say(&said);
                }
                Some("list") | None => {
                    let store = c.store()?;
                    for st in custodian_kit::store::JobState::ALL {
                        let dir = store.jobs_dir(*st)?;
                        let mut v: Vec<String> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
                        v.sort();
                        println!("{}: {}", st.as_str(), if v.is_empty() { "-".into() } else { v.join(", ") });
                    }
                }
                Some(other) => return Err(ReadingRefusal::NoJob.refuse(format!("job {other}: start, close, run, brief, import, list"))),
            }
        }
        "triage" => {
            let c = ctx();
            let _w = admitted(&c)?;
            let target = a.pos(0).ok_or_else(|| ReadingRefusal::TargetMissing.refuse("triage CODE|@LOGFILE"))?;
            let (started, _closing, done, said) = jobs::run_template(&c, "triage", target, a.flag("--again"))?;
            say(&started.said);
            say(&said);
            print!("{}", jobs::brief(&c, &started.id)?);
            let _ = done;
        }
        "test" => {
            let c = ctx();
            if a.pos(0) == Some("report") {
                print!("{}", testrun::report(&c)?);
                return Ok(());
            }
            let _w = admitted(&c)?;
            let observe: Observe = a.get("--observe").unwrap_or("failed").parse().map_err(|e: String| ReadingRefusal::SpecInvalid.refuse(e))?;
            let lines = if a.flag("--all") {
                testrun::lines_all(&c)?
            } else {
                let spec_text = match (a.get("--spec"), a.get("--ref")) {
                    (Some(s), _) => std::fs::read_to_string(s).map_err(|e| ReadingRefusal::SpecUnreadable.refuse(e))?,
                    (None, r) => format!("name = \"affected-tests\"\nquestion = \"which tests reach what changed?\"\nkind = \"affected_tests\"\ntarget = \"*\"\n[change]\ngit_ref = {}\n", toml::Value::String(r.unwrap_or("HEAD").into())),
                };
                let tmp = match &c.store {
                    Some(s) => s.dir_of(&custodian_kit::store::named::PLAN, &[("name", "x")])?.join("affected-tests.toml"),
                    None => std::env::temp_dir().join(format!("custodian-affected-{}.toml", std::process::id())),
                };
                custodian_kit::store::write_whole(&tmp, spec_text.as_bytes())?;
                let ran = runs::run_reading(&c, &tmp, a.flag("--again"), None)?;
                say(&ran.said);
                let json = ran.summary.as_ref().and_then(|s| s.json.clone()).or_else(|| ran.receipt.as_ref().and_then(|r| r.outputs.iter().find(|o| o.ends_with(".json")).map(|o| c.root.join(o))));
                match json {
                    Some(j) => testrun::lines_of_reading(&reading::load(&j)?),
                    None => return Err(ReadingRefusal::NotInitialized.refuse("test --affected keeps its reading in .custodian/; run custodian init")),
                }
            };
            if lines.is_empty() {
                println!("no tests reach the change");
            }
            let run = testrun::run(&c, &lines, observe, None)?;
            say(&run.said);
        }
        "check" => {
            let c = ctx();
            let _w = admitted(&c)?;
            let which = a.pos(0).unwrap_or("all");
            if which == "all" && c.store.as_ref().is_some_and(|s| s.settings().is_ok_and(|x| x.mode == Mode::Agent)) {
                let (_, said) = check::check_all_job(&c)?;
                say(&said);
            } else {
                let checked = check::run_check(&c, check::check_named(which)?, None)?;
                say(&checked.said);
            }
        }
        "diagnostics" => {
            let c = ctx();
            let (target, name) = match (a.pos(0), a.pos(1)) {
                (Some("list"), Some(krate)) => (krate.to_string(), "diagnostics-list".to_string()),
                (Some("list"), None) | (None, _) => ("*".to_string(), "diagnostics-list".to_string()),
                (Some("where"), Some(code)) => (code.to_string(), format!("diagnostics-{code}")),
                (Some("from"), Some(file)) => {
                    let path = if file == "-" {
                        let mut s = String::new();
                        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).map_err(|e| ReadingRefusal::MissingInput.refuse(e))?;
                        let p = std::env::temp_dir().join(format!("custodian-log-{}.txt", std::process::id()));
                        std::fs::write(&p, s).map_err(|e| ReadingRefusal::Write.refuse(e))?;
                        p
                    } else {
                        PathBuf::from(file).canonicalize().map_err(|e| ReadingRefusal::MissingInput.refuse(e))?
                    };
                    (format!("@{}", path.display()), "diagnostics-from".to_string())
                }
                _ => return Err(ReadingRefusal::SpecInvalid.refuse("diagnostics list [CRATE] | where CODE | from FILE|-")),
            };
            let spec = format!("name = {}\nquestion = \"which liaise codes, declared where, raised where?\"\nkind = \"diagnostics\"\ntarget = {}\n", toml::Value::String(name.clone()), toml::Value::String(target));
            let tmp = std::env::temp_dir().join(format!("custodian-{name}-{}.toml", std::process::id()));
            std::fs::write(&tmp, spec).map_err(|e| ReadingRefusal::Write.refuse(e))?;
            let _w = admitted(&c)?;
            let ran = runs::run_reading(&c, &tmp, a.flag("--again"), None)?;
            if let Some(p) = &ran.printed {
                println!("{p}");
            }
            say(&ran.said);
        }
        "features" => {
            let c = ctx();
            let _w = admitted(&c)?;
            let f = features::run(&c, a.flag("--apply"))?;
            say(&f.said);
        }
        "annotate" => {
            let c = ctx();
            let _w = admitted(&c)?;
            let r = annotate::run(&c, a.get("--from").map(Path::new), a.flag("--apply"))?;
            say(&r.said);
        }
        "bundle" => {
            let c = ctx();
            let id = a.pos(0).ok_or_else(|| ReadingRefusal::NoJob.refuse("bundle JOB"))?;
            let (out, digest, held) = bundle_job(&c, id, a.flag("--reveal"))?;
            println!("bundle: {} ({digest}, {held} windows held redacted)", c.rel(&out));
        }
        "sweep" => {
            let c = ctx();
            let dir = a.pos(0).unwrap_or(".");
            let table = PathsTable::load(&c.root, c.store.as_ref()).ok();
            let s = sweep::sweep(&c.root, Path::new(dir), table.as_ref())?;
            let page = sweep::page(&s);
            match &c.store {
                Some(store) => {
                    let d = store.reading_dir("sweep")?;
                    let p = d.join(format!("{}.md", runs::stamp()));
                    custodian_kit::store::write_whole(&p, page.as_bytes())?;
                    print!("{page}");
                    println!("reading: {} (a sweep executes nothing)", c.rel(&p));
                }
                None => print!("{page}"),
            }
        }
        "self" => self_cmd(&a)?,
        other => return Err(ReadingRefusal::SpecInvalid.refuse(format!("unknown command {other}"))),
    }
    Ok(())
}

fn bundle_job(c: &Ctx, which: &str, reveal: bool) -> custodian_kit::Result<(PathBuf, String, usize)> {
    let store = c.store()?;
    let journal = store.journal()?;
    let row = journal.job(which)?.ok_or_else(|| ReadingRefusal::NoJob.refuse(which))?;
    let folder = c.root.join(&row.folder);
    let receipts: Vec<(i64, String)> = journal.runs_of_job(&row.id)?.into_iter().map(|r| (r.seq, serde_json::to_string_pretty(&r).unwrap_or_default())).collect();
    bundle::bundle_job(&c.root, &folder, &row.id, &receipts, reveal)
}

fn import(c: &Ctx, b: &Path, again: bool) -> custodian_kit::Result<Vec<String>> {
    let store = c.store()?;
    let name = format!("imported-{}", runs::stamp());
    let queued = store.job_dir(custodian_kit::store::JobState::Queued, &name)?;
    let (folder, digest) = bundle::unbundle_job(b, &queued)?;
    let jf_path = folder.join("job.toml");
    let mut jf: jobs::JobFile = std::fs::read_to_string(&jf_path).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default();
    jf.id = None;
    jf.name = Some(name.clone());
    jf.origin = Some(digest.clone());
    custodian_kit::store::write_whole(&jf_path, toml::to_string(&jf).map_err(|e| ReadingRefusal::Write.refuse(e))?.as_bytes())?;
    let mut said = vec![format!("imported {} ({digest}) into {}", b.display(), c.rel(&folder))];
    let s = jobs::start(c, Some(&name), again)?;
    said.extend(s.said);
    let (_c, _d, closed) = jobs::close(c, &s.id)?;
    said.extend(closed);
    Ok(said)
}

fn self_cmd(a: &Args) -> custodian_kit::Result<()> {
    use installation::{build_binary, Installer, Scope};
    let ie = |e: installation::InstallError| ReadingRefusal::Write.refuse(e);
    let bin = a.pos(1).filter(|b| !b.starts_with("--")).unwrap_or("custodian");
    let scope = match a.get("--scope") {
        Some("system") => Scope::System { root: None },
        _ => Scope::User,
    };
    let installer = Installer::new(bin, scope).map_err(ie)?;
    match a.pos(0) {
        Some("install") => {
            let dir = a.get("--from").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
            let built = build_binary(&dir, bin, true).map_err(ie)?;
            let plan = installer.plan(bin, &built).map_err(ie)?;
            print!("{}", plan.describe());
            if a.flag("--dry-run") {
                println!("dry run: nothing installed");
                return Ok(());
            }
            let receipt = installer.install(&plan).map_err(ie)?;
            println!("installed {} (sha256 {}), receipt {}", receipt.destination.display(), receipt.sha256, plan.receipt.display());
            let c = ctx();
            if let Some(store) = &c.store {
                let r = custodian_kit::store::Receipt {
                    schema: custodian_kit::store::RECEIPT_SCHEMA.into(),
                    seq: 0,
                    utc: runs::now_utc(),
                    command: "self-install".into(),
                    name: bin.into(),
                    spec: None,
                    spec_hash: None,
                    code_digest: None,
                    index_key: None,
                    plan_id: format!("install:{bin}"),
                    plan_digest: custodian_kit::digest::of_value(&plan),
                    operation_digest: custodian_kit::digest::of_value(&plan),
                    receipt_digest: custodian_kit::digest::of_value(&receipt),
                    line: format!("installation: {} → {}", plan.source.display(), plan.destination.display()),
                    exit_code: Some(0),
                    accepted: true,
                    disposition: "Stop".into(),
                    reason: "installed".into(),
                    outputs: vec![plan.receipt.display().to_string(), receipt.destination.display().to_string()],
                    job: None,
                    reused_seq: None,
                    millis: 0,
                };
                let (r, f) = runs::keep(store, r)?;
                println!("journal: run #{} self-install, receipt {}", r.seq, c.rel(&f));
            }
        }
        Some("uninstall") => match installer.uninstall(bin, a.flag("--force")).map_err(ie)? {
            Some(r) => println!("uninstalled {} (receipt said sha256 {})", r.destination.display(), r.sha256),
            None => println!("{bin}: no install receipt"),
        },
        Some("status") | None => match installer.receipt(bin).map_err(ie)? {
            Some(r) => println!("{}: {} sha256 {} installed at unix {} from {}", r.bin_name, r.destination.display(), r.sha256, r.installed_at_unix, r.source.display()),
            None => println!("{bin}: not installed through installation"),
        },
        Some(other) => return Err(ReadingRefusal::SpecInvalid.refuse(format!("self {other}: install, uninstall, status"))),
    }
    let _ = Resolving::Free;
    Ok(())
}
