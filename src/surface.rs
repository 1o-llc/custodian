//! Every line custodian answers to, declared once with simple_impl_derive over xccute_contract: the `custodian`
//! program (a `CompositeShell` root) and one `SimpleSubCommand` leaf per subcommand, each naming its exit statuses
//! (0 `done`, 1 `findings`, 2 `refused`).
//!
//! A subcommand with actions (`job start`, `self install`, `diagnostics where`) is one leaf whose first word is the
//! action, checked by its `each` rule, because a nested surface does not yet carry a leaf's optional setters or its
//! exit statuses (simple_impl_derive 0.2.6; maintenance's `check <which>` is the same shape).
//!
//! [`read`] reads the words after `custodian` into the declared line through the line's own setters and refuses a
//! word no setter takes; the line is then the authority: its `argv()` is what runs, what is previewed and what the
//! receipt keeps. `tests::every_line_reads_back_from_its_own_words` holds the reader and the declaration together.

use crate::codes::{Code, Result};
use simple_impl_derive::{CompositeShell, SimpleSubCommand};
use std::ffi::{OsStr, OsString};
use xccute_contract::{CommandPreview, CommandValidationError, CommandValidationResult, CompositeShellCommand, ValidatedCommand};
use xccute_runtime::{ExitStatusDecision, ExitStatusPolicy, RuntimeExitStatus};

/// The `custodian` program.
#[derive(CompositeShell, Debug, Clone, Copy, Default)]
#[shell(program = "custodian")]
#[composite(command = "init", ty = Init)]
#[composite(command = "agent", ty = Agent, init = "action")]
#[composite(command = "run", ty = Run)]
#[composite(command = "list", ty = List)]
#[composite(command = "diff", ty = Diff, init = "spec")]
#[composite(command = "job", ty = Job, init = "action")]
#[composite(command = "test", ty = Test)]
#[composite(command = "check", ty = Check, init = "which")]
#[composite(command = "annotate", ty = Annotate)]
#[composite(command = "features", ty = Features)]
#[composite(command = "diagnostics", ty = Diagnostics, init = "action")]
#[composite(command = "triage", ty = Triage, init = "target")]
#[composite(command = "bundle", ty = Bundle, init = "target")]
#[composite(command = "sweep", ty = Sweep, init = "dir")]
#[composite(command = "count", ty = Count)]
#[composite(command = "serve", ty = Serve)]
#[composite(command = "stop", ty = Stop)]
#[composite(command = "watch", ty = Watch)]
#[composite(command = "self", method = "self_", ty = SelfLine, init = "action")]
pub struct Custodian;

/// `custodian init [--macros]`: the plain folder (specs, readings, journal); `--macros` adds `maintenance` to the
/// workspace through code_passenger.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "init")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Init {
    #[arg(flag = "--macros")]
    pub macros: bool,
}

/// `custodian agent init [--macros]`: the folder with jobs as folders and the job kinds the agent mode knows.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "agent")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Agent {
    #[builder(init_required, into)]
    #[arg(positional, each = agent_action)]
    pub action: OsString,
    #[arg(flag = "--macros")]
    pub macros: bool,
}

/// `custodian run [SPEC|ROUTE] [--kind KIND] [--job ID] [--again]`: one spec (a file, or a queued or done spec's
/// name), a search on the line (`--kind` and a target), or every queued spec and job folder in order.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "run")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Run {
    #[builder(into)]
    #[arg(positional, each = plain)]
    pub target: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--kind", each = kind_word)]
    pub kind: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--job", each = plain)]
    pub job: Option<OsString>,
    #[arg(flag = "--again")]
    pub again: bool,
}

/// `custodian list`: the inventory, rendered from the journal.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "list")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct List;

/// `custodian diff SPEC [--from SEQ] [--to SEQ]`: what moved between two readings of one spec (the last two by default).
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "diff")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Diff {
    #[builder(init_required, into)]
    #[arg(positional, each = plain)]
    pub spec: OsString,
    #[builder(into)]
    #[arg(kv = "--from", each = number)]
    pub from: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--to", each = number)]
    pub to: Option<OsString>,
}

/// `custodian job start|close|brief|import|list [NAME|ID|BUNDLE] [--kind KIND] [--target T] [--again]`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "job")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
#[validate(with = "job_valid")]
pub struct Job {
    #[builder(init_required, into)]
    #[arg(positional, each = job_action)]
    pub action: OsString,
    #[builder(into)]
    #[arg(positional, each = plain)]
    pub name: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--kind", each = job_kind_word)]
    pub kind: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--target")]
    pub target: Option<OsString>,
    /// What the job is for, in the asker's words: its task history's intent record.
    #[builder(into)]
    #[arg(kv = "--intent", each = nonempty)]
    pub intent: Option<OsString>,
    #[arg(flag = "--again")]
    pub again: bool,
}

/// `custodian test [report] [--affected | --all] [--observe failed|progress|all] [--ref REF] [--job ID]`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "test")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Test {
    #[builder(into)]
    #[arg(positional, each = test_action)]
    pub action: Option<OsString>,
    #[arg(flag = "--affected")]
    #[conflicts_with("all")]
    pub affected: bool,
    #[arg(flag = "--all")]
    pub all: bool,
    #[builder(into)]
    #[arg(kv = "--observe", each = observe_word)]
    pub observe: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--ref", each = plain)]
    pub git_ref: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--job", each = plain)]
    pub job: Option<OsString>,
}

/// `custodian check sensitivity|mechanism|boundary|supersedes|reads_env|golden|all [--job ID]`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "check")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Check {
    #[builder(init_required, into)]
    #[arg(positional, each = check_word)]
    pub which: OsString,
    #[builder(into)]
    #[arg(kv = "--job", each = plain)]
    pub job: Option<OsString>,
}

/// `custodian annotate [--from CHECK_JSON] [--apply]`: the maintenance attributes the receipts call for, as a plan.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "annotate")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Annotate {
    #[builder(into)]
    #[arg(kv = "--from")]
    pub from: Option<OsString>,
    #[arg(flag = "--apply")]
    pub apply: bool,
}

/// `custodian features [--apply]`: code_passenger's feature-gating spec from the eligibility reading.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "features")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Features {
    #[arg(flag = "--apply")]
    pub apply: bool,
}

/// `custodian diagnostics list [CRATE] | where CODE | from LOGFILE`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "diagnostics")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
#[validate(with = "diagnostics_valid")]
pub struct Diagnostics {
    #[builder(init_required, into)]
    #[arg(positional, each = diagnostics_action)]
    pub action: OsString,
    #[builder(into)]
    #[arg(positional)]
    pub arg: Option<OsString>,
}

/// `custodian triage CODE|@LOGFILE`: the built-in triage job, started and closed.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "triage")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Triage {
    #[builder(init_required, into)]
    #[arg(positional, each = nonempty)]
    pub target: OsString,
}

/// `custodian bundle ROUTE|JOB [--no-tests] [--no-feature F] [--only src] [--redact-level secret] [--reveal]`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "bundle")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Bundle {
    #[builder(init_required, into)]
    #[arg(positional, each = plain)]
    pub target: OsString,
    #[arg(flag = "--no-tests")]
    pub no_tests: bool,
    #[builder(into)]
    #[arg(kv = "--no-feature", each = plain)]
    pub no_feature: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--only", each = only_word)]
    pub only: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--redact-level", each = redact_word)]
    pub redact_level: Option<OsString>,
    #[arg(flag = "--reveal")]
    pub reveal: bool,
    #[arg(flag = "--again")]
    pub again: bool,
}

/// `custodian sweep DIR`: list an undeclared tree and offer the routes to declare (the one walk custodian makes).
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "sweep")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Sweep {
    #[builder(init_required, into)]
    #[arg(positional, each = nonempty)]
    pub dir: OsString,
}

/// `custodian count [ROUTE] [--by crate|language] [--tests] [--again]`, `count report A B`, `count history [ROUTE]`:
/// files, lines, code, comments and blanks (custodian_kit's `count` reading kind, on tokei), per crate or language,
/// the tests' share; two counts compared; every count of a route.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "count")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
#[validate(with = "count_valid")]
pub struct Count {
    /// A route; or `report A B`; or `history [ROUTE]`.
    #[arg(positional, each = count_word)]
    pub args: Vec<OsString>,
    #[builder(into)]
    #[arg(kv = "--by", each = by_word)]
    pub by: Option<OsString>,
    #[arg(flag = "--tests")]
    pub tests: bool,
    #[arg(flag = "--again")]
    pub again: bool,
    /// The job the count is part of (a job's own counts at open and close).
    #[builder(into)]
    #[arg(kv = "--job", each = plain)]
    pub job: Option<OsString>,
}

/// `custodian serve`: the resident, on the project's owner-only socket, until `custodian stop`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "serve")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Serve;

/// `custodian stop`: ask the resident to stop.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "stop")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Stop;

/// `custodian watch [--observe failed|progress|all] [--since SEQ] [--follow]`: job state changes and test
/// progress, through the observer set.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "watch")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Watch {
    #[builder(into)]
    #[arg(kv = "--observe", each = observe_word)]
    pub observe: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--since", each = number)]
    pub since: Option<OsString>,
    #[arg(flag = "--follow")]
    pub follow: bool,
}

/// `custodian self install|uninstall|status [--from BIN] [--bin-dir DIR] [--state-dir DIR] [--dry-run] [--force]`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "self")]
#[exit_status(code = 0, word = "done")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
#[validate(with = "self_valid")]
pub struct SelfLine {
    #[builder(init_required, into)]
    #[arg(positional, each = self_action)]
    pub action: OsString,
    #[builder(into)]
    #[arg(kv = "--from")]
    pub from: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--bin-dir")]
    #[requires("state_dir")]
    pub bin_dir: Option<OsString>,
    #[builder(into)]
    #[arg(kv = "--state-dir")]
    #[requires("bin_dir")]
    pub state_dir: Option<OsString>,
    #[arg(flag = "--dry-run")]
    pub dry_run: bool,
    #[arg(flag = "--force")]
    pub force: bool,
}

// ---- the words each field takes ----

fn refuse(field: &str, msg: String) -> CommandValidationResult {
    Err(CommandValidationError::structural(msg).with_field(field))
}

fn nonempty(field: &str, word: &OsStr) -> CommandValidationResult {
    if word.is_empty() {
        return refuse(field, format!("{field} is empty"));
    }
    Ok(())
}

/// A word that never reads as an option.
fn plain(field: &str, word: &OsStr) -> CommandValidationResult {
    let t = word.to_string_lossy();
    if t.is_empty() || t.starts_with('-') {
        return refuse(field, format!("{t:?} is not a plain {field}"));
    }
    Ok(())
}

fn number(field: &str, word: &OsStr) -> CommandValidationResult {
    if word.to_str().and_then(|w| w.parse::<i64>().ok()).is_none() {
        return refuse(field, format!("{word:?} is not a number"));
    }
    Ok(())
}

fn one_of(field: &str, word: &OsStr, words: &[&str]) -> CommandValidationResult {
    match word.to_str() {
        Some(w) if words.contains(&w) => Ok(()),
        _ => refuse(field, format!("{word:?} is not a {field}: {}", words.join(", "))),
    }
}

pub const AGENT_ACTIONS: &[&str] = &["init"];
pub const JOB_ACTIONS: &[&str] = &["start", "close", "brief", "import", "list"];
pub const TEST_ACTIONS: &[&str] = &["report"];
pub const DIAGNOSTICS_ACTIONS: &[&str] = &["list", "where", "from"];
pub const SELF_ACTIONS: &[&str] = &["install", "uninstall", "status"];

fn agent_action(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, AGENT_ACTIONS)
}
fn job_action(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, JOB_ACTIONS)
}
fn test_action(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, TEST_ACTIONS)
}
fn diagnostics_action(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, DIAGNOSTICS_ACTIONS)
}
fn self_action(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, SELF_ACTIONS)
}

fn kind_word(field: &str, word: &OsStr) -> CommandValidationResult {
    let kinds: Vec<&str> = custodian_kit::api::ReadingKind::ALL.iter().map(|k| k.as_str()).collect();
    one_of(field, word, &kinds)
}

fn job_kind_word(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, &crate::words::JobKind::words())
}

fn observe_word(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, &crate::words::ObserveLevel::words())
}

fn check_word(field: &str, word: &OsStr) -> CommandValidationResult {
    let words: Vec<&str> = maintenance::runner::CheckName::ALL.iter().map(|c| c.word()).collect();
    one_of(field, word, &words)
}

fn count_word(field: &str, word: &OsStr) -> CommandValidationResult {
    if word.is_empty() || (word.to_string_lossy().starts_with('-') && word != "-") {
        return refuse(field, format!("{word:?} is not a route, `report` or `history`"));
    }
    Ok(())
}

fn count_valid(line: &Count) -> CommandValidationResult {
    match line.args.first().and_then(|a| a.to_str()) {
        Some("report") if line.args.len() != 3 => refuse("args", "count report names two counts: count report A B".into()),
        Some("history") if line.args.len() > 2 => refuse("args", "count history [ROUTE]".into()),
        Some(w) if w != "report" && w != "history" && line.args.len() > 1 => refuse("args", "count names one route".into()),
        _ => Ok(()),
    }
}

fn by_word(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, &["crate", "language"])
}

fn only_word(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, &["src"])
}

fn redact_word(field: &str, word: &OsStr) -> CommandValidationResult {
    one_of(field, word, &["secret"])
}

fn job_valid(line: &Job) -> CommandValidationResult {
    if line.action == "import" && line.name.is_none() {
        return refuse("name", "job import names the bundle: job import FILE.bundle".into());
    }
    Ok(())
}

fn diagnostics_valid(line: &Diagnostics) -> CommandValidationResult {
    if (line.action == "where" || line.action == "from") && line.arg.is_none() {
        return refuse("arg", format!("diagnostics {} names its {}", line.action.to_string_lossy(), if line.action == "where" { "code" } else { "log file (- for stdin)" }));
    }
    Ok(())
}

fn self_valid(line: &SelfLine) -> CommandValidationResult {
    if line.from.is_some() && line.action != "install" {
        return refuse("from", "--from is install's".into());
    }
    Ok(())
}

// ---- reading words into a line ----

/// The words after the subcommand, taken by the line's setters: options first (anywhere), then positionals in order.
pub struct Words {
    rest: Vec<String>,
}

impl Words {
    pub fn new(args: &[String]) -> Words {
        Words { rest: args.to_vec() }
    }

    /// `--name` present: taken.
    pub fn flag(&mut self, name: &str) -> bool {
        match self.rest.iter().position(|w| w == name) {
            Some(i) => {
                self.rest.remove(i);
                true
            }
            None => false,
        }
    }

    /// `--name VALUE`: the value, taken; `--name` last with no value is refused.
    pub fn kv(&mut self, name: &str) -> Result<Option<String>> {
        match self.rest.iter().position(|w| w == name) {
            Some(i) if i + 1 < self.rest.len() => {
                let v = self.rest.remove(i + 1);
                self.rest.remove(i);
                Ok(Some(v))
            }
            Some(_) => Err(Code::LineRefused.refuse(format!("{name} takes a value"))),
            None => Ok(None),
        }
    }

    /// The next word that is not an option.
    pub fn positional(&mut self) -> Option<String> {
        let i = self.rest.iter().position(|w| !w.starts_with("--") || w == "-")?;
        Some(self.rest.remove(i))
    }

    /// Every word was taken, or the first one no setter takes is refused.
    pub fn done(self, segment: &str) -> Result<()> {
        match self.rest.first() {
            None => Ok(()),
            Some(w) => Err(Code::LineRefused.refuse(format!("`{w}` is not a word of `custodian {segment}`"))),
        }
    }
}

macro_rules! lines {
    ($($variant:ident($ty:ident) = $seg:literal, $usage:literal => $read:ident;)*) => {
        /// A declared line, read from the words: the leaf with its values, rooted under `custodian` to run.
        #[derive(Debug, Clone)]
        pub enum Line { $($variant($ty),)* }

        /// Every subcommand and how it reads, for `custodian` with no words.
        pub const USAGE: &[(&str, &str)] = &[$(($seg, $usage),)*];

        impl Line {
            /// The subcommand's word.
            pub fn segment(&self) -> &'static str {
                match self { $(Line::$variant(_) => $seg,)* }
            }

            /// The words after the program, as the line writes them.
            pub fn argv(&self) -> Vec<OsString> {
                match self { $(Line::$variant(l) => l.clone().rooted::<Custodian>().argv(),)* }
            }

            /// `custodian run spec.toml --again`.
            pub fn display(&self) -> String {
                match self { $(Line::$variant(l) => l.clone().rooted::<Custodian>().build_display(),)* }
            }

            /// The line's own rules.
            pub fn validate(&self) -> CommandValidationResult {
                match self { $(Line::$variant(l) => l.clone().rooted::<Custodian>().validate(),)* }
            }

            /// The preview xccute makes of it: data, no side effect.
            pub fn preview(&self) -> CommandPreview {
                match self { $(Line::$variant(l) => l.clone().rooted::<Custodian>().preview(),)* }
            }

            /// What xccute decides after a status, by the statuses the line names.
            pub fn decide(&self, code: i32) -> ExitStatusDecision {
                match self {
                    $(Line::$variant(l) => {
                        let l = l.clone().rooted::<Custodian>();
                        let accepted = l.ok_codes().contains(&code);
                        ExitStatusPolicy::of(&l).decide(RuntimeExitStatus::new(Some(code), accepted))
                    })*
                }
            }

            /// The word the line names for a status.
            pub fn status_word(&self, code: i32) -> &'static str {
                match self { $(Line::$variant(l) => l.clone().rooted::<Custodian>().exit_status_of(Some(code)).map_or("unnamed", |s| s.word),)* }
            }
        }

        fn read_segment(seg: &str, rest: &[String]) -> Result<Line> {
            match seg {
                $($seg => {
                    let mut w = Words::new(rest);
                    let line = $read(&mut w)?;
                    w.done($seg)?;
                    Ok(Line::$variant(line))
                })*
                other => Err(Code::LineRefused.refuse(format!("`{other}`; the subcommands are {}", USAGE.iter().map(|(s, _)| *s).collect::<Vec<_>>().join(", ")))),
            }
        }
    };
}

lines! {
    Init(Init) = "init", "init [--macros]" => read_init;
    Agent(Agent) = "agent", "agent init [--macros]" => read_agent;
    Run(Run) = "run", "run [SPEC|ROUTE] [--kind KIND] [--job ID] [--again]" => read_run;
    List(List) = "list", "list" => read_list;
    Diff(Diff) = "diff", "diff SPEC [--from SEQ] [--to SEQ]" => read_diff;
    Job(Job) = "job", "job start|close|brief|import|list [NAME|ID|BUNDLE] [--kind KIND] [--target T] [--intent TEXT] [--again]" => read_job;
    Test(Test) = "test", "test [report] [--affected|--all] [--observe failed|progress|all] [--ref REF]" => read_test;
    Check(Check) = "check", "check sensitivity|mechanism|boundary|supersedes|reads_env|golden|all" => read_check;
    Annotate(Annotate) = "annotate", "annotate [--from CHECK_JSON] [--apply]" => read_annotate;
    Features(Features) = "features", "features [--apply]" => read_features;
    Diagnostics(Diagnostics) = "diagnostics", "diagnostics list [CRATE] | where CODE | from LOGFILE|-" => read_diagnostics;
    Triage(Triage) = "triage", "triage CODE|@LOGFILE" => read_triage;
    Bundle(Bundle) = "bundle", "bundle ROUTE|JOB [--no-tests] [--no-feature F] [--only src] [--redact-level secret] [--reveal] [--again]" => read_bundle;
    Sweep(Sweep) = "sweep", "sweep DIR" => read_sweep;
    Count(Count) = "count", "count [ROUTE] [--by crate|language] [--tests] [--again] [--job ID] | count report A B | count history [ROUTE]" => read_count;
    Serve(Serve) = "serve", "serve" => read_serve;
    Stop(Stop) = "stop", "stop" => read_stop;
    Watch(Watch) = "watch", "watch [--observe failed|progress|all] [--since SEQ] [--follow]" => read_watch;
    SelfCmd(SelfLine) = "self", "self install|uninstall|status [--from BIN] [--bin-dir DIR --state-dir DIR] [--dry-run] [--force]" => read_self;
}

/// The words after `custodian`, as a declared line, or the refusal.
pub fn read(args: &[OsString]) -> Result<Line> {
    let words: Vec<String> = args
        .iter()
        .map(|a| a.to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Code::LineRefused.refuse("a word is not UTF-8"))?;
    let (seg, rest) = words.split_first().ok_or_else(|| Code::LineRefused.refuse("no subcommand"))?;
    let line = read_segment(seg, rest)?;
    line.validate().map_err(|e| Code::LineRefused.refuse(e))?;
    Ok(line)
}

fn need(w: &mut Words, what: &str, seg: &str) -> Result<OsString> {
    w.positional().map(OsString::from).ok_or_else(|| Code::LineRefused.refuse(format!("custodian {seg} names its {what}")))
}

fn kv(w: &mut Words, name: &str) -> Result<Option<OsString>> {
    Ok(w.kv(name)?.map(OsString::from))
}

fn pos(w: &mut Words) -> Option<OsString> {
    w.positional().map(OsString::from)
}

fn read_init(w: &mut Words) -> Result<Init> {
    Ok(Init { macros: w.flag("--macros") })
}

fn read_agent(w: &mut Words) -> Result<Agent> {
    let macros = w.flag("--macros");
    Ok(Agent { macros, action: need(w, "action (init)", "agent")? })
}

fn read_run(w: &mut Words) -> Result<Run> {
    Ok(Run { kind: kv(w, "--kind")?, job: kv(w, "--job")?, again: w.flag("--again"), target: pos(w) })
}

fn read_list(_: &mut Words) -> Result<List> {
    Ok(List)
}

fn read_diff(w: &mut Words) -> Result<Diff> {
    Ok(Diff { from: kv(w, "--from")?, to: kv(w, "--to")?, spec: need(w, "spec", "diff")? })
}

fn read_job(w: &mut Words) -> Result<Job> {
    Ok(Job { kind: kv(w, "--kind")?, target: kv(w, "--target")?, intent: kv(w, "--intent")?, again: w.flag("--again"), action: need(w, "action (start, close, brief, import, list)", "job")?, name: pos(w) })
}

fn read_test(w: &mut Words) -> Result<Test> {
    Ok(Test { affected: w.flag("--affected"), all: w.flag("--all"), observe: kv(w, "--observe")?, git_ref: kv(w, "--ref")?, job: kv(w, "--job")?, action: pos(w) })
}

fn read_check(w: &mut Words) -> Result<Check> {
    Ok(Check { job: kv(w, "--job")?, which: need(w, "check (sensitivity, mechanism, boundary, supersedes, reads_env, golden, all)", "check")? })
}

fn read_annotate(w: &mut Words) -> Result<Annotate> {
    Ok(Annotate { from: kv(w, "--from")?, apply: w.flag("--apply") })
}

fn read_features(w: &mut Words) -> Result<Features> {
    Ok(Features { apply: w.flag("--apply") })
}

fn read_diagnostics(w: &mut Words) -> Result<Diagnostics> {
    Ok(Diagnostics { action: need(w, "action (list, where, from)", "diagnostics")?, arg: pos(w) })
}

fn read_triage(w: &mut Words) -> Result<Triage> {
    Ok(Triage { target: need(w, "code or @logfile", "triage")? })
}

fn read_bundle(w: &mut Words) -> Result<Bundle> {
    Ok(Bundle {
        no_tests: w.flag("--no-tests"),
        no_feature: kv(w, "--no-feature")?,
        reveal: w.flag("--reveal"),
        again: w.flag("--again"),
        only: kv(w, "--only")?,
        redact_level: kv(w, "--redact-level")?,
        target: need(w, "route or job", "bundle")?,
    })
}

fn read_sweep(w: &mut Words) -> Result<Sweep> {
    Ok(Sweep { dir: need(w, "directory", "sweep")? })
}

fn read_count(w: &mut Words) -> Result<Count> {
    let (by, tests, again, job) = (kv(w, "--by")?, w.flag("--tests"), w.flag("--again"), kv(w, "--job")?);
    let mut args = Vec::new();
    while let Some(a) = pos(w) {
        args.push(a);
    }
    Ok(Count { by, tests, again, job, args })
}

fn read_serve(_: &mut Words) -> Result<Serve> {
    Ok(Serve)
}

fn read_stop(_: &mut Words) -> Result<Stop> {
    Ok(Stop)
}

fn read_watch(w: &mut Words) -> Result<Watch> {
    Ok(Watch { observe: kv(w, "--observe")?, since: kv(w, "--since")?, follow: w.flag("--follow") })
}

fn read_self(w: &mut Words) -> Result<SelfLine> {
    Ok(SelfLine {
        from: kv(w, "--from")?,
        bin_dir: kv(w, "--bin-dir")?,
        state_dir: kv(w, "--state-dir")?,
        dry_run: w.flag("--dry-run"),
        force: w.flag("--force"),
        action: need(w, "action (install, uninstall, status)", "self")?,
    })
}

/// A field as a string.
pub fn s(v: &OsString) -> String {
    v.to_string_lossy().into_owned()
}

/// An optional field as a string.
pub fn so(v: &Option<OsString>) -> Option<String> {
    v.as_ref().map(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<OsString> {
        line.split_whitespace().map(OsString::from).collect()
    }

    /// Each line, written by its builder, reads back to the same words: the reader takes exactly what the
    /// declaration writes.
    #[test]
    fn every_line_reads_back_from_its_own_words() {
        let lines: Vec<Vec<OsString>> = vec![
            Custodian::init().macros().argv(),
            Custodian::agent("init").macros().argv(),
            Custodian::run().target("spec.toml").kind("reach").job("j-1").again().argv(),
            Custodian::run().argv(),
            Custodian::list().argv(),
            Custodian::diff("env").from("3").to("9").argv(),
            Custodian::job("start").name("batch").kind("triage").target("CUST0004").intent("find the raise").again().argv(),
            Custodian::test().action("report").argv(),
            Custodian::test().affected().observe("progress").git_ref("HEAD~1").job("j-2").argv(),
            Custodian::check("all").job("j-3").argv(),
            Custodian::annotate().from("c.json").apply().argv(),
            Custodian::features().apply().argv(),
            Custodian::diagnostics("where").arg("CUST0004").argv(),
            Custodian::triage("@box.log").argv(),
            Custodian::bundle("crate:core_lib").no_tests().no_feature("serde").only("src").redact_level("secret").again().argv(),
            Custodian::sweep("vendor").argv(),
            Custodian::count().args(["crate:core_lib"]).by("language").tests().again().job("j-4").argv(),
            Custodian::count().args(["report", "3", "9"]).argv(),
            Custodian::serve().argv(),
            Custodian::stop().argv(),
            Custodian::watch().observe("all").since("4").follow().argv(),
            Custodian::self_("install").from("target/release/custodian").bin_dir("b").state_dir("s").dry_run().force().argv(),
        ];
        let mut segments: Vec<String> = lines.iter().map(|l| l[0].to_string_lossy().into_owned()).collect();
        segments.dedup();
        assert_eq!(segments.len(), USAGE.len(), "every subcommand");
        for argv in lines {
            let back = read(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            assert_eq!(back.argv(), argv);
        }
    }

    #[test]
    fn options_read_in_any_order_and_unknown_words_are_refused() {
        let a = read(&words("run --again spec.toml --kind reach")).unwrap();
        assert_eq!(a.display(), "custodian run spec.toml --kind reach --again");
        let e = read(&words("run spec.toml --loud")).unwrap_err();
        assert_eq!(e.to_string(), "[CUST1001] the words are not a declared custodian line: `--loud` is not a word of `custodian run`");
        let e = read(&words("job explode")).unwrap_err();
        assert!(e.to_string().contains("\"explode\" is not a action: start, close, brief, import, list"), "{e}");
        assert!(read(&words("test --affected --all")).is_err());
        assert!(read(&words("run --kind nonsense x")).is_err());
        assert!(read(&words("diagnostics where")).unwrap_err().to_string().contains("names its code"));
        assert!(read(&words("self install --bin-dir b")).is_err(), "--bin-dir needs --state-dir");
        assert!(read(&words("deploy")).unwrap_err().to_string().contains("the subcommands are init, agent, run"));
    }

    #[test]
    fn every_line_names_its_statuses() {
        let l = read(&words("check all")).unwrap();
        assert_eq!((l.status_word(0), l.status_word(1), l.status_word(2)), ("done", "findings", "refused"));
        let d = l.decide(1);
        assert_eq!((d.disposition, d.reason.as_str()), (xccute_runtime::ExitDisposition::Stop, "findings"));
        assert_eq!(l.preview().display(), "custodian check all");
    }
}
