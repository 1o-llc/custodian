//! Every line custodian answers to, declared once with simple_impl_derive over xccute_contract: the `custodian`
//! program (a `CompositeShell` root) and one `SimpleSubCommand` leaf per subcommand, each naming its exit statuses
//! (0 `done`, 1 `findings`, 2 `refused`).
//!
//! A subcommand with actions (`job start`, `self install`, `diagnostics where`) is one leaf whose first word is the
//! action, checked by its `each` rule, because a nested surface does not yet carry a leaf's optional setters or its
//! exit statuses (simple_impl_derive 0.2.6; maintenance's `check <which>` is the same shape).
//!
//! Each leaf is declared inside [`leaf!`], which hands the struct to the derive unchanged and keeps what the
//! declaration says (its doc, its segment, each field's name, type, docs, how it is written and the words it
//! takes) as the leaf's [`Declared`] table. The table is what the MCP server's tools are made from (`crate::mcp`),
//! so a tool's schema is the declaration's, never a second copy of it.
//!
//! [`read`] reads the words after `custodian` into the declared line through the line's own setters and refuses a
//! word no setter takes; the line is then the authority: its `argv()` is what runs, what is previewed and what the
//! receipt keeps. `tests::every_line_reads_back_from_its_own_words` holds the reader and the declaration together.

use crate::codes::{Code, Result};
use simple_impl_derive::{CompositeShell, SimpleSubCommand};
use std::ffi::{OsStr, OsString};
use xccute_contract::{CommandPreview, CommandValidationError, CommandValidationResult, CompositeShellCommand, ValidatedCommand};
use xccute_runtime::{ExitStatusDecision, ExitStatusPolicy, RuntimeExitStatus};

// ---- what a declaration says, kept as a table ----

/// How a field is written on the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// A bare word, in the declaration's order.
    Positional,
    /// `--name` when true.
    Flag(&'static str),
    /// `--name VALUE`.
    Kv(&'static str),
}

/// One attribute of a declaration, as [`leaf!`] reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attr {
    /// A `///` line.
    Doc(&'static str),
    /// `#[subcommand(segment = "...")]`.
    Segment(&'static str),
    /// `#[arg(...)]`: how the field is written, and the name of its `each` check.
    Arg(Written, Option<&'static str>),
    /// `#[builder(init_required, ...)]`.
    Required,
    /// Anything else (the derive's own, the builder's `into`, `conflicts_with`, ...).
    Other,
}

/// A declared field: its name, the outer word of its type (`OsString`, `Option`, `Vec`, `bool`) and its attributes.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub ty: &'static str,
    pub attrs: &'static [Attr],
}

/// A declared leaf: its attributes and its fields, in the declaration's order.
#[derive(Debug, Clone, Copy)]
pub struct Declared {
    pub attrs: &'static [Attr],
    pub fields: &'static [Field],
}

fn docs(attrs: &[Attr]) -> String {
    attrs.iter().filter_map(|a| if let Attr::Doc(d) = a { Some(d.trim()) } else { None }).collect::<Vec<_>>().join(" ")
}

impl Field {
    /// How it is written; every field the derive takes has an `#[arg(...)]`.
    pub fn written(&self) -> Option<Written> {
        self.attrs.iter().find_map(|a| if let Attr::Arg(w, _) = a { Some(*w) } else { None })
    }

    /// The name of its `each` check.
    pub fn each(&self) -> Option<&'static str> {
        self.attrs.iter().find_map(|a| if let Attr::Arg(_, e) = a { *e } else { None })
    }

    /// The words the field takes, when its check is a word set.
    pub fn words(&self) -> Option<Vec<&'static str>> {
        self.each().and_then(words_of)
    }

    pub fn doc(&self) -> String {
        docs(self.attrs)
    }

    /// `#[builder(init_required)]`: the line is not made without it.
    pub fn required(&self) -> bool {
        self.attrs.contains(&Attr::Required)
    }
}

impl Declared {
    pub fn segment(&self) -> &'static str {
        self.attrs.iter().find_map(|a| if let Attr::Segment(s) = a { Some(*s) } else { None }).unwrap_or_default()
    }

    pub fn doc(&self) -> String {
        docs(self.attrs)
    }

    pub fn field(&self, name: &str) -> Option<&'static Field> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// One attribute's tokens, read into an [`Attr`]. A new `#[arg(...)]` form is a compile error here until it is read.
macro_rules! attr {
    (doc = $d:literal) => { Attr::Doc($d) };
    (subcommand(segment = $s:literal)) => { Attr::Segment($s) };
    (arg(positional)) => { Attr::Arg(Written::Positional, None) };
    (arg(positional, each = $e:ident)) => { Attr::Arg(Written::Positional, Some(stringify!($e))) };
    (arg(flag = $f:literal)) => { Attr::Arg(Written::Flag($f), None) };
    (arg(kv = $k:literal)) => { Attr::Arg(Written::Kv($k), None) };
    (arg(kv = $k:literal, each = $e:ident)) => { Attr::Arg(Written::Kv($k), Some(stringify!($e))) };
    (arg($($other:tt)*)) => { compile_error!(concat!("an #[arg(", stringify!($($other)*), ")] form surface::attr! does not read")) };
    (builder(init_required $($rest:tt)*)) => { Attr::Required };
    ($($other:tt)*) => { Attr::Other };
}

/// A leaf, declared: the struct handed to the derive as written, and its [`Declared`] table as `DECLARED`.
macro_rules! leaf {
    ($(#[$($sa:tt)*])* pub struct $name:ident;) => {
        $(#[$($sa)*])*
        pub struct $name;

        impl $name {
            pub const DECLARED: Declared = Declared { attrs: &[$(attr!($($sa)*)),*], fields: &[] };
        }
    };
    ($(#[$($sa:tt)*])* pub struct $name:ident { $($(#[$($fa:tt)*])* pub $f:ident: $t:ident $(<$i:ident>)?,)* }) => {
        $(#[$($sa)*])*
        pub struct $name { $($(#[$($fa)*])* pub $f: $t $(<$i>)?,)* }

        impl $name {
            pub const DECLARED: Declared = Declared {
                attrs: &[$(attr!($($sa)*)),*],
                fields: &[$(Field { name: stringify!($f), ty: stringify!($t), attrs: &[$(attr!($($fa)*)),*] }),*],
            };
        }
    };
}

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
#[composite(command = "mcp", ty = Mcp, init = "action")]
pub struct Custodian;

leaf! {
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
}

leaf! {
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
}

leaf! {
    /// `custodian run [SPEC|ROUTE] [--kind KIND] [--job ID] [--again]`: one spec (a file, or a queued or done spec's
    /// name), a search on the line (`--kind` and a target), or every queued spec and job folder in order.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "run")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Run {
        /// A spec file, a queued or done spec's name, or (with `kind`) a route such as `crate:core_lib`; none runs the queue.
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub target: Option<OsString>,
        /// The reading kind of a search on the line.
        #[builder(into)]
        #[arg(kv = "--kind", each = kind_word)]
        pub kind: Option<OsString>,
        /// The job the run is part of (the running job by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
        /// Read again even when the same spec ran over the same code.
        #[arg(flag = "--again")]
        pub again: bool,
    }
}

leaf! {
    /// `custodian list`: the inventory, rendered from the journal.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "list")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct List;
}

leaf! {
    /// `custodian diff SPEC [--from SEQ] [--to SEQ]`: what moved between two readings of one spec (the last two by default).
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "diff")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Diff {
        /// The spec's name (or its file).
        #[builder(init_required, into)]
        #[arg(positional, each = plain)]
        pub spec: OsString,
        /// The earlier reading's receipt number.
        #[builder(into)]
        #[arg(kv = "--from", each = number)]
        pub from: Option<OsString>,
        /// The later reading's receipt number.
        #[builder(into)]
        #[arg(kv = "--to", each = number)]
        pub to: Option<OsString>,
    }
}

leaf! {
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
        /// The job's name (start), its id (close, brief; the running or last one by default), or a bundle file (import).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub name: Option<OsString>,
        /// What the job is: a folder of specs, or one of the templates.
        #[builder(into)]
        #[arg(kv = "--kind", each = job_kind_word)]
        pub kind: Option<OsString>,
        /// What a template job works on (a triage's code, a route).
        #[builder(into)]
        #[arg(kv = "--target")]
        pub target: Option<OsString>,
        /// What the job is for, in the asker's words: its task history's intent record.
        #[builder(into)]
        #[arg(kv = "--intent", each = nonempty)]
        pub intent: Option<OsString>,
        /// Read every step again.
        #[arg(flag = "--again")]
        pub again: bool,
    }
}

leaf! {
    /// `custodian test [report] [--affected | --all] [--observe failed|progress|all] [--ref REF] [--job ID]`.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "test")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Test {
        /// `report`: the last runs' outcomes, nothing run.
        #[builder(into)]
        #[arg(positional, each = test_action)]
        pub action: Option<OsString>,
        /// Only the tests the change since `git_ref` reaches.
        #[arg(flag = "--affected")]
        #[conflicts_with("all")]
        pub affected: bool,
        /// Every test line.
        #[arg(flag = "--all")]
        pub all: bool,
        /// What the run shows while it runs.
        #[builder(into)]
        #[arg(kv = "--observe", each = observe_word)]
        pub observe: Option<OsString>,
        /// The git ref the change is measured from.
        #[builder(into)]
        #[arg(kv = "--ref", each = plain)]
        pub git_ref: Option<OsString>,
        /// The job the run is part of.
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// `custodian check sensitivity|mechanism|boundary|supersedes|reads_env|golden|all [--job ID]`.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "check")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Check {
        /// The maintenance check to run.
        #[builder(init_required, into)]
        #[arg(positional, each = check_word)]
        pub which: OsString,
        /// The job the run is part of.
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// `custodian annotate [--from CHECK_JSON] [--apply]`: the maintenance attributes the receipts call for, as a plan.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "annotate")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Annotate {
        /// A check's JSON to plan from (the last check's by default).
        #[builder(into)]
        #[arg(kv = "--from")]
        pub from: Option<OsString>,
        /// Place the plan's lines through code_passenger.
        #[arg(flag = "--apply")]
        pub apply: bool,
    }
}

leaf! {
    /// `custodian features [--apply]`: code_passenger's feature-gating spec from the eligibility reading.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "features")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Features {
        /// Gate the eligible dependencies through code_passenger.
        #[arg(flag = "--apply")]
        pub apply: bool,
    }
}

leaf! {
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
        /// A crate (list), a diagnostic code (where), or a log file, `-` for stdin (from).
        #[builder(into)]
        #[arg(positional)]
        pub arg: Option<OsString>,
    }
}

leaf! {
    /// `custodian triage CODE|@LOGFILE`: the built-in triage job, started and closed.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "triage")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Triage {
        /// A diagnostic code, or `@` and a log file.
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub target: OsString,
    }
}

leaf! {
    /// `custodian bundle ROUTE|JOB [--no-tests] [--no-feature F] [--only src] [--redact-level secret] [--reveal]`.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "bundle")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Bundle {
        /// A route, or a job's id.
        #[builder(init_required, into)]
        #[arg(positional, each = plain)]
        pub target: OsString,
        /// Leave the tests out.
        #[arg(flag = "--no-tests")]
        pub no_tests: bool,
        /// Leave out the code behind this feature.
        #[builder(into)]
        #[arg(kv = "--no-feature", each = plain)]
        pub no_feature: Option<OsString>,
        /// Only these files.
        #[builder(into)]
        #[arg(kv = "--only", each = only_word)]
        pub only: Option<OsString>,
        /// Redact items at this sensitivity level.
        #[builder(into)]
        #[arg(kv = "--redact-level", each = redact_word)]
        pub redact_level: Option<OsString>,
        /// Show what redaction would hide.
        #[arg(flag = "--reveal")]
        pub reveal: bool,
        /// Bundle again even when the same bundle exists over the same code.
        #[arg(flag = "--again")]
        pub again: bool,
    }
}

leaf! {
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
}

leaf! {
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
        /// The table's rows.
        #[builder(into)]
        #[arg(kv = "--by", each = by_word)]
        pub by: Option<OsString>,
        /// The tests' share.
        #[arg(flag = "--tests")]
        pub tests: bool,
        /// Count again even when the code has not moved.
        #[arg(flag = "--again")]
        pub again: bool,
        /// The job the count is part of (a job's own counts at open and close).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// `custodian serve`: the resident, on the project's owner-only socket, until `custodian stop`.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "serve")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Serve;
}

leaf! {
    /// `custodian stop`: ask the resident to stop.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "stop")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Stop;
}

leaf! {
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
}

leaf! {
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
}

leaf! {
    /// `custodian mcp serve [--root DIR]`: an MCP server on stdin and stdout whose tools are these lines, each call
    /// run as the command line runs it.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "mcp")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Mcp {
        #[builder(init_required, into)]
        #[arg(positional, each = mcp_action)]
        pub action: OsString,
        /// The project the server answers for (where it starts by default).
        #[builder(into)]
        #[arg(kv = "--root")]
        pub root: Option<OsString>,
    }
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
pub const MCP_ACTIONS: &[&str] = &["serve"];

/// The checks that take one word of a set: each check is `one_of` its set, and [`words_of`] names the set by the
/// check's name (what a field's `each` says), for the tool schemas.
macro_rules! word_sets {
    ($($check:ident => $words:expr;)*) => {
        $(fn $check(field: &str, word: &OsStr) -> CommandValidationResult {
            one_of(field, word, &$words)
        })*

        /// The words a check takes, by the check's name.
        pub fn words_of(check: &str) -> Option<Vec<&'static str>> {
            match check {
                $(stringify!($check) => Some($words.to_vec()),)*
                _ => None,
            }
        }
    };
}

word_sets! {
    agent_action => AGENT_ACTIONS;
    job_action => JOB_ACTIONS;
    test_action => TEST_ACTIONS;
    diagnostics_action => DIAGNOSTICS_ACTIONS;
    self_action => SELF_ACTIONS;
    mcp_action => MCP_ACTIONS;
    kind_word => custodian_kit::api::ReadingKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<&'static str>>();
    job_kind_word => crate::words::JobKind::words();
    observe_word => crate::words::ObserveLevel::words();
    check_word => maintenance::runner::CheckName::ALL.iter().map(|c| c.word()).collect::<Vec<&'static str>>();
    by_word => ["crate", "language"];
    only_word => ["src"];
    redact_word => ["secret"];
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

        /// Every leaf's declaration table, in the same order.
        pub const DECLARED: &[Declared] = &[$($ty::DECLARED,)*];

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
    Mcp(Mcp) = "mcp", "mcp serve [--root DIR]" => read_mcp;
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

fn read_mcp(w: &mut Words) -> Result<Mcp> {
    Ok(Mcp { root: kv(w, "--root")?, action: need(w, "action (serve)", "mcp")? })
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
            Custodian::mcp("serve").root("/tmp/ws").argv(),
        ];
        let mut segments: Vec<String> = lines.iter().map(|l| l[0].to_string_lossy().into_owned()).collect();
        segments.dedup();
        assert_eq!(segments.len(), USAGE.len(), "every subcommand");
        for argv in lines {
            let back = read(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            assert_eq!(back.argv(), argv);
        }
    }

    /// The declaration tables are the leaves' own: one per subcommand, under the segment `lines!` reads it by,
    /// every field written some way.
    #[test]
    fn every_leaf_keeps_its_declaration() {
        assert_eq!(DECLARED.len(), USAGE.len());
        for (d, (seg, _)) in DECLARED.iter().zip(USAGE) {
            assert_eq!(d.segment(), *seg);
            assert!(!d.doc().is_empty(), "{seg} has a doc");
            assert!(d.fields.iter().all(|f| f.written().is_some()), "{seg}");
        }
        let run = Run::DECLARED;
        let kind = run.field("kind").unwrap();
        assert_eq!((kind.ty, kind.written(), kind.each()), ("Option", Some(Written::Kv("--kind")), Some("kind_word")));
        assert!(kind.words().unwrap().contains(&"reach"));
        assert_eq!(run.field("again").unwrap().written(), Some(Written::Flag("--again")));
        assert!(Diff::DECLARED.field("spec").unwrap().required() && !Diff::DECLARED.field("from").unwrap().required());
        assert_eq!(Count::DECLARED.field("args").unwrap().ty, "Vec");
        assert_eq!(Job::DECLARED.field("intent").unwrap().doc(), "What the job is for, in the asker's words: its task history's intent record.");
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
