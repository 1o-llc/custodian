//! `custodian test [--affected | --all] [--observe failed|progress|all] [--ref REF] [--features F | --all-features |
//! --matrix]` and `custodian test report`.
//!
//! The lines: `--affected` (the default) reads custodian_kit's `affected_tests` over the change since the ref (the
//! running job's base commit, else HEAD; the files git does not track yet too) and takes the exact cargo test lines
//! it names; `--all` takes one line per workspace package. Every line runs on the workspace's one kept build
//! (custodian_kit's `proof_place`: the copy and build folder the transform's proof uses), so a crate whose source did
//! not move is not compiled again; "affected" is the proof's rule, the crates the change is in, their direct
//! dependents, and the readers of a public signature it alters.
//! Each line is built with xccute_commands' `CargoBuilder` and run through the executor with a receipt of its own;
//! its output goes through the observer set (what is shown is the `--observe` level's), and its counts, its
//! outcomes and its wall time go into the journal. `test report` reads them back: the slowest lines, the flakiest
//! tests, and maintenance's timed report (run as maintenance's own declared line) when the workspace has one.

use crate::codes::Result;
use crate::facade::{self, Ctx, External, ExternalNames, Ran};
use crate::journal::TestRun;
use crate::observe::{Observers, RunEvent};
use crate::readings::{self, Ask};
use crate::surface::{so, TestRun as Test};
use crate::words::ObserveLevel;
use custodian_kit::api::{ReadingSpec, Resolving, Scope};
use custodian_kit::testrun::Configuration;
use xccute_commands::CargoBuilder;


/// One cargo test line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub package: String,
    /// `all`, `lib+bins`, or custodian_kit's target word: `lib`, `bin:<name>`, `test:<name>`.
    pub target: String,
    pub filters: Vec<String>,
}

/// The cargo line, as xccute_commands builds it; a target word carrying a feature configuration (`lib [features
/// json]`, custodian_kit's `testrun::Configuration`) runs under it.
pub fn cargo_line(root: &std::path::Path, l: &Line) -> CargoBuilder {
    let mut c = CargoBuilder::new("test").manifest_path(root.join("Cargo.toml").to_string_lossy().into_owned()).package(l.package.clone());
    let (word, config) = Configuration::of_target(&l.target);
    for a in config.args() {
        c = c.arg(a);
    }
    // the target words custodian_kit's affected_tests writes: `lib`, `bin:<name>`, `test:<name>`
    match word.as_str() {
        "all" => {}
        "lib+bins" => c = c.lib().arg("--bins"),
        "lib" => c = c.lib(),
        t => match t.split_once(':') {
            Some((kind, name)) => c = c.args(&[&format!("--{kind}"), name]),
            None => c = c.args(&["--test", t]),
        },
    }
    if !l.filters.is_empty() {
        c = c.arg("--");
        for f in &l.filters {
            c = c.arg(f.clone());
        }
        c = c.arg("--exact");
    }
    c
}

/// The affected_tests spec over the change since `git_ref`.
pub fn affected_spec(git_ref: &str) -> Result<ReadingSpec> {
    let text = format!(
        "name = \"affected-tests\"\nquestion = \"which tests does the change since {git_ref} reach?\"\nkind = \"affected_tests\"\ntarget = \"*\"\n\n[change]\ngit_ref = {}\n",
        toml::Value::String(git_ref.into())
    );
    Ok(ReadingSpec::from_toml(&text)?)
}

/// The lines to run.
pub fn lines(ctx: &mut Ctx, all: bool, git_ref: &str, job: Option<String>, ran: &mut Ran) -> Result<Vec<Line>> {
    if all {
        let ws = ctx.workspace(&Scope::default(), Resolving::Free)?;
        let mut v: Vec<Line> = ws.klepto().packages.values().map(|p| Line { package: p.cargo_name.clone(), target: "all".into(), filters: Vec::new() }).collect();
        v.sort_by(|a, b| a.package.cmp(&b.package));
        ran.say(format!("--all: {} packages, one line each", v.len()));
        return Ok(v);
    }
    // the change: `git -C ROOT diff --name-only REF`, custodian_kit's declared git line, run with its receipt
    let root = ctx.root().to_path_buf();
    let git = External::of("git.diff", &custodian_kit::exec::git_diff_names(&root, git_ref))?;
    let diff = facade::run_external(ctx, git, ExternalNames { name: format!("diff {git_ref}"), job: job.clone(), outputs: Vec::new() }, |_| {})?;
    if !diff.accepted {
        return Err(facade::not_accepted(&diff));
    }
    // and the files git does not track yet (a new module's file, a new fixture): `git ls-files --others --exclude-standard`
    let new = External::of("git.diff", &custodian_kit::exec::Git::in_dir(root.as_os_str()).words(["ls-files", "--others", "--exclude-standard"]))?;
    let new = facade::run_external(ctx, new, ExternalNames { name: "untracked".into(), job: job.clone(), outputs: Vec::new() }, |_| {})?;
    // custodian's own folder is not the code; every other file is, Rust or not (a manifest, a test's expected output)
    let mut files: Vec<String> = diff.stdout.lines().chain(new.stdout.lines()).map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(&format!("{}/", crate::layout::FOLDER)) && !l.starts_with(".passenger/")).map(str::to_owned).collect();
    files.sort();
    files.dedup();
    ran.say(format!("the change since {git_ref}: {} file(s) ({} #{})", files.len(), diff.receipt.line, diff.receipt.seq));
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut spec = affected_spec(git_ref)?;
    spec.change.git_ref = None;
    // the public items whose signature the change alters: their readers' tests run too (custodian_kit's rule)
    spec.change.signatures = custodian_kit::testrun::signature_changes(&root, git_ref, &files);
    if !spec.change.signatures.is_empty() {
        ran.say(format!("public signatures the change alters: {}", spec.change.signatures.join(", ")));
    }
    spec.change.files = files;
    let (r, report) = readings::read_spec(ctx, &spec, &readings::From::Line, &Ask { again: false, job, word: String::new() })?;
    ran.said.extend(r.said);
    ran.outputs.extend(r.outputs);
    let Some(report) = report else { return Ok(Vec::new()) };
    Ok(custodian_kit::testrun::lines_of_reading(&report)
        .into_iter()
        .map(|t| Line {
            package: t.package,
            target: match t.target {
                None => "all".into(),
                Some(None) => "lib+bins".into(),
                Some(Some(t)) => t,
            },
            filters: t.filters,
        })
        .collect())
}

/// A cargo test output line: the test and its outcome.
pub fn outcome_of(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("test ")?;
    let (name, outcome) = rest.rsplit_once(" ... ")?;
    let outcome = outcome.split_whitespace().next()?;
    matches!(outcome, "ok" | "FAILED" | "ignored").then(|| (name.trim().to_string(), outcome.to_string()))
}

/// Each line under the configurations asked: `--features F` and `--all-features` one each, `--matrix` every
/// configuration of the line's crate (klepto's check matrix); none asked, cargo's default features.
pub fn configured(ctx: &mut Ctx, lines: Vec<Line>, l: &Test, ran: &mut Ran) -> Result<Vec<Line>> {
    let asked = |c: Configuration| move |line: Line| Line { target: c.target_with(&line.target), ..line };
    if let Some(f) = so(&l.features) {
        let c = Configuration { features: f.split(',').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect(), ..Default::default() };
        // a crate that does not declare them runs as it is (cargo refuses a feature a package does not have)
        let ws = ctx.workspace(&Scope::default(), Resolving::Free)?;
        let declares = |pkg: &str| ws.klepto().features.get(&pkg.replace('-', "_")).is_some_and(|m| c.features.iter().all(|x| m.features.contains_key(x)));
        let (with, without): (Vec<Line>, Vec<Line>) = lines.into_iter().partition(|line| declares(&line.package));
        ran.say(format!("{} lines with {}; {} lines as they are (their crate does not declare them)", with.len(), c.label(), without.len()));
        return Ok(with.into_iter().map(asked(c)).chain(without).collect());
    }
    if l.all_features {
        ran.say("each line with all-features".to_string());
        return Ok(lines.into_iter().map(asked(Configuration { all: true, ..Default::default() })).collect());
    }
    if !l.matrix {
        return Ok(lines);
    }
    let ws = ctx.workspace(&Scope::default(), Resolving::Free)?;
    let mut out = Vec::new();
    for line in lines {
        let model = ws.klepto().features.get(&line.package.replace('-', "_"));
        let configs = model.map(Configuration::matrix).filter(|m| !m.is_empty()).unwrap_or_else(|| vec![Configuration::default()]);
        for c in configs {
            out.push(Line { target: c.target_with(&line.target), ..line.clone() });
        }
    }
    ran.say(format!("--matrix: {} lines, each crate's line once per configuration of its check matrix", out.len()));
    Ok(out)
}

/// `custodian test`.
pub fn test(ctx: &mut Ctx, l: &Test) -> Result<Ran> {
    let level = ObserveLevel::of_word(&so(&l.observe).unwrap_or_else(|| "failed".into()))?;
    let job = ctx.current_job(so(&l.job))?;
    // the change is measured from the job's base (the commit it opened at), else from HEAD
    let base = match &job {
        Some(j) => crate::readings::job_folder(ctx, j)?.and_then(|d| custodian_kit::jobs::job_file(&d).base_commit),
        None => None,
    };
    let git_ref = so(&l.git_ref).or(base).unwrap_or_else(|| "HEAD".into());
    let mut ran = Ran::named(if l.all { "test --all" } else { "test --affected" });
    ran.job = job.clone();
    let lines = lines(ctx, l.all, &git_ref, job.clone(), &mut ran)?;
    if lines.is_empty() {
        ran.say(format!("no test lines: the change since {git_ref} reaches no test (custodian test --all runs every package)"));
        return Ok(ran);
    }
    let lines = configured(ctx, lines, l, &mut ran)?;
    crate::budget::before_step(ctx, job.as_deref(), &format!("{} test line(s)", lines.len()), custodian_kit::step_time::TEST, lines.len())?;
    // the lines run on the workspace's one kept build: custodian_kit's proof place (the copy and the build the
    // transform's proof uses), never a cold build in the workspace
    let place = custodian_kit::proof_place::ProofPlace::take(ctx.root())?;
    let root = place.root().to_path_buf();
    ran.say(format!("on the kept build: {} (the proof's place, synced)", place.target().display()));
    let mut obs = Observers::new(level);
    let (mut total_pass, mut total_fail) = (0, 0);
    for line in &lines {
        let cmd = cargo_line(&root, line);
        let mut ext = External::shell("cargo.test", &cmd).in_dir(&root);
        for (k, v) in place.env() {
            ext = ext.env(&k, v);
        }
        let mut results: Vec<(String, String)> = Vec::new();
        let names = ExternalNames { name: format!("{} {}", line.package, line.target), job: job.clone(), outputs: Vec::new() };
        // the live lines go where the run's own do; held aside while the line runs
        let mut live = std::mem::replace(&mut ctx.live, Box::new(|_| {}));
        let run = facade::run_external(ctx, ext, names, |out: &str| {
            if let Some((name, outcome)) = outcome_of(out) {
                results.push((name.clone(), outcome.clone()));
                for s in obs.publish(RunEvent::Test { name, outcome }) {
                    live(&s);
                }
            }
        });
        ctx.live = live;
        let run = run?;
        let passed = results.iter().filter(|(_, o)| o == "ok").count() as i64;
        let failed = results.iter().filter(|(_, o)| o == "FAILED").count() as i64;
        let ignored = results.iter().filter(|(_, o)| o == "ignored").count() as i64;
        total_pass += passed;
        total_fail += failed;
        let t = TestRun { seq: run.receipt.seq, job: job.clone(), package: line.package.clone(), target: line.target.clone(), line: run.receipt.line.clone(), exit_code: run.code, millis: run.millis, passed, failed, ignored };
        if let Some(j) = ctx.journal.as_ref() {
            // each outcome in cargo's words (`ok`, `FAILED`, `ignored`), as custodian_kit's tests table keeps them
            j.test_run(&t, &results)?;
            j.event("test", job.as_deref(), &format!("{} {}", line.package, line.target), "counts", &format!("{passed},{failed},{ignored}"), Some(run.receipt.seq))?;
            for (n, _) in results.iter().filter(|(_, o)| o == "FAILED") {
                j.event("test", job.as_deref(), n, "FAILED", &line.package, Some(run.receipt.seq))?;
            }
        }
        for s in obs.publish(RunEvent::Counts { line: format!("{} {}", line.package, line.target), passed: passed as u64, failed: failed as u64, ignored: ignored as u64 }) {
            (ctx.live)(&s);
        }
        ran.say(format!(
            "{} {}: {passed} passed, {failed} failed, {ignored} ignored, {} ms, exit {:?} ({}){}",
            line.package,
            line.target,
            run.millis,
            run.code,
            run.receipt.reason,
            run.receipt_file.as_ref().map(|f| format!(" · receipt #{} {}", run.receipt.seq, ctx.rel(f))).unwrap_or_default()
        ));
        if !run.accepted {
            ran.code = 1;
            if results.is_empty() {
                let tail: Vec<&str> = run.stderr.lines().rev().take(8).collect();
                for l in tail.into_iter().rev() {
                    ran.say(format!("  {l}"));
                }
            }
        }
    }
    ran.say(format!("ran {} cargo test line(s): {total_pass} passed, {total_fail} failed; results and times in the journal (custodian test report)", lines.len()));
    Ok(ran)
}

/// `custodian test report`: the slowest lines, the flakiest tests, and maintenance's timed report when present.
pub fn report(ctx: &mut Ctx) -> Result<Ran> {
    let journal = ctx.journal()?;
    let runs = journal.test_runs()?;
    let mut s = String::from("# test report\n\n## the slowest lines (the last run of each)\n\n");
    let mut last: std::collections::BTreeMap<(String, String), &TestRun> = Default::default();
    for r in &runs {
        last.insert((r.package.clone(), r.target.clone()), r);
    }
    let mut slow: Vec<&&TestRun> = last.values().collect();
    slow.sort_by_key(|r| std::cmp::Reverse(r.millis));
    if slow.is_empty() {
        s.push_str("no test lines in the journal yet (custodian test)\n");
    }
    for r in slow.iter().take(20) {
        s.push_str(&format!("- {} ms  {} {} ({} passed, {} failed) #{}\n", r.millis, r.package, r.target, r.passed, r.failed, r.seq));
    }
    s.push_str("\n## the flakiest tests (failed in some runs, passed in others)\n\n");
    let outcomes = journal.test_outcomes()?;
    let flaky: Vec<_> = outcomes.iter().filter(|(_, _, runs, fails)| *fails > 0 && fails < runs).collect();
    if flaky.is_empty() {
        s.push_str("none\n");
    }
    for (p, t, runs, fails) in &flaky {
        s.push_str(&format!("- {p} {t}: failed {fails} of {runs} runs\n"));
    }
    let failing: Vec<_> = outcomes.iter().filter(|(_, _, runs, fails)| *fails > 0 && fails == runs).collect();
    if !failing.is_empty() {
        s.push_str("\n## failing in every run\n\n");
        for (p, t, runs, _) in failing {
            s.push_str(&format!("- {p} {t}: {runs} of {runs}\n"));
        }
    }
    let mut ran = Ran::named("test report");
    let timings = ctx.root().join(maintenance::registry::DIR).join(maintenance::timed::TIMINGS);
    if timings.is_dir() {
        let line = maintenance::runner::Maintenance::timed_report(ctx.root());
        let ext = External::of("maintenance.timed", &line)?;
        let out = maintenance::runner::execute(&line);
        let run = facade::finish_external(ctx, &ext, ExternalNames { name: "timed report".into(), ..Default::default() }, Some(out.code), out.stdout.clone(), String::new(), 0)?;
        match serde_json::from_str::<maintenance::timed::TimingReport>(&out.stdout) {
            Ok(t) => {
                s.push_str("\n## maintenance's timed report (#[timed_test])\n\n");
                s.push_str(&t.render(20));
            }
            Err(_) => s.push_str(&format!("\n## maintenance's timed report\n\n{}\n", out.stdout)),
        }
        ran.say(format!("joined maintenance's timed report: {} → {} ({}) #{}", run.receipt.line, out.code, out.word, run.receipt.seq));
    } else {
        s.push_str(&format!("\n(no maintenance timings at {}: tests marked #[timed_test] write them)\n", ctx.rel(&timings)));
    }
    ran.print(s);
    ran.say(format!("reported {} test line runs and {} tests from the journal", runs.len(), outcomes.len()));
    Ok(ran)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xccute_runtime::ShellCommand;

    #[test]
    fn cargo_lines_are_xccute_commands_lines() {
        let root = std::path::Path::new("/w");
        let l = Line { package: "core_lib".into(), target: "lib+bins".into(), filters: vec!["tests::a".into(), "tests::b".into()] };
        assert_eq!(cargo_line(root, &l).build(), "cargo test --lib --package core_lib --manifest-path /w/Cargo.toml --bins -- tests::a tests::b --exact");
        let t = Line { package: "core_lib".into(), target: "test:metrics".into(), filters: vec!["metrics_say_where".into()] };
        assert_eq!(cargo_line(root, &t).build(), "cargo test --package core_lib --manifest-path /w/Cargo.toml --test metrics -- metrics_say_where --exact");
        let b = Line { package: "app".into(), target: "lib".into(), filters: vec![] };
        assert_eq!(cargo_line(root, &b).build(), "cargo test --lib --package app --manifest-path /w/Cargo.toml");
        assert_eq!(outcome_of("test tests::a ... ok"), Some(("tests::a".into(), "ok".into())));
        assert_eq!(outcome_of("test result: ok. 2 passed"), None);
    }
}
