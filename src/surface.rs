//! Every line custodian answers to, declared once with simple_impl_derive over xccute_contract: the `custodian`
//! program (a `CompositeShell` root), its surfaces (`job`, `agent`, `test`, `count`, `diagnostics`, `self`, `mcp`,
//! `plan`, `linkage`: each a `CompositeSubCommand`) and one `SimpleSubCommand` leaf per line, each naming its exit
//! statuses (0 `done`, 1 `findings`, 2 `refused`). A line under a surface is its leaf's nested wrapper
//! (simple_impl_derive 0.2.7, `xccute_contract::RootableNestedCommand`), so it keeps its leaf's setters, statuses
//! and rules at every depth.
//!
//! Each leaf is declared inside [`leaf!`], which hands the struct to the derive unchanged and keeps what the
//! declaration says (its doc, its segment, each field's name, type, docs, how it is written and the words it
//! takes) as the leaf's [`Declared`] table. The table is what the MCP server's tools are made from (`crate::mcp`),
//! so a tool's schema is the declaration's, never a second copy of it. The usage screen is the root's help lines
//! (`xccute_contract::CommandHelp`), generated from the same declarations.
//!
//! [`read`] reads the words after `custodian` into the declared line through the line's own setters and refuses a
//! word no setter takes; the line is then the authority: its `argv()` is what runs, what is previewed and what the
//! receipt keeps. `tests::every_line_reads_back_from_its_own_words` holds the reader and the declaration together.
//! Three spellings from before the surfaces are still read: `count [ROUTE]` is `count of [ROUTE]`, `test [--all]`
//! is `test run [--all]`, `plan INTENT` is `plan make INTENT`.

use crate::codes::{Code, Result};
use simple_impl_derive::{CompositeShell, CompositeSubCommand, SimpleSubCommand};
use std::ffi::{OsStr, OsString};
use xccute_contract::{CommandHelp, CommandPreview, CommandValidationError, CommandValidationResult, CompositeShellCommand, RootableNestedCommand, ValidatedCommand};
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
#[composite(surface = "agent", ty = Agent)]
#[composite(command = "run", ty = Run)]
#[composite(command = "list", ty = List)]
#[composite(command = "diff", ty = Diff, init = "spec")]
#[composite(surface = "job", ty = Job)]
#[composite(surface = "test", ty = Test)]
#[composite(command = "check", ty = Check, init = "which")]
#[composite(command = "annotate", ty = Annotate)]
#[composite(command = "features", ty = Features)]
#[composite(surface = "diagnostics", ty = Diagnostics)]
#[composite(command = "triage", ty = Triage, init = "target")]
#[composite(command = "bundle", ty = Bundle, init = "target")]
#[composite(command = "sweep", ty = Sweep, init = "dir")]
#[composite(surface = "count", ty = Count)]
#[composite(command = "serve", ty = Serve)]
#[composite(command = "stop", ty = Stop)]
#[composite(command = "watch", ty = Watch)]
#[composite(surface = "self", method = "self_", ty = SelfSurface)]
#[composite(surface = "mcp", ty = Mcp)]
#[composite(command = "preflight", ty = Preflight, init = "spec")]
#[composite(command = "transform", ty = Transform)]
#[composite(surface = "plan", ty = Plan)]
#[composite(command = "decide", ty = Decide)]
#[composite(surface = "linkage", ty = Linkage)]
#[composite(surface = "exercise", ty = Exercise)]
#[composite(command = "handoff", ty = Handoff)]
#[composite(command = "why", ty = Why, init = "route")]
#[composite(command = "cost", ty = Cost)]
pub struct Custodian;

/// `custodian agent init|brief|guard`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "agent")]
#[composite(command = "init", ty = AgentInit)]
#[composite(command = "brief", ty = AgentBrief)]
#[composite(command = "guard", ty = AgentGuard, init = "file")]
pub struct Agent;

/// `custodian job start|close|brief|import|list|verify|allow`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "job")]
#[composite(command = "start", ty = JobStart)]
#[composite(command = "close", ty = JobClose)]
#[composite(command = "brief", ty = JobBrief)]
#[composite(command = "import", ty = JobImport, init = "bundle")]
#[composite(command = "list", ty = JobList)]
#[composite(command = "verify", ty = JobVerify, init = "line")]
#[composite(command = "allow", ty = JobAllow, init = "glob")]
pub struct Job;

/// `custodian test run|report`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "test")]
#[composite(command = "run", ty = TestRun)]
#[composite(command = "report", ty = TestReport)]
pub struct Test;

/// `custodian diagnostics list|where|from`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "diagnostics")]
#[composite(command = "list", ty = DiagnosticsList)]
#[composite(command = "where", method = "where_", ty = DiagnosticsWhere, init = "code")]
#[composite(command = "from", ty = DiagnosticsFrom, init = "log")]
pub struct Diagnostics;

/// `custodian count of|report|history`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "count")]
#[composite(command = "of", ty = CountOf)]
#[composite(command = "report", ty = CountReport, init = "a,b")]
#[composite(command = "history", ty = CountHistory)]
pub struct Count;

/// `custodian self install|uninstall|status`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "self")]
#[composite(command = "install", ty = SelfInstall)]
#[composite(command = "uninstall", ty = SelfUninstall)]
#[composite(command = "status", ty = SelfStatus)]
pub struct SelfSurface;

/// `custodian mcp serve`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "mcp")]
#[composite(command = "serve", ty = McpServe)]
pub struct Mcp;

/// `custodian plan make|approve|show`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "plan")]
#[composite(command = "make", ty = PlanMake, init = "intent")]
#[composite(command = "approve", ty = PlanApprove, init = "plan,step")]
#[composite(command = "show", ty = PlanShow, init = "plan")]
pub struct Plan;

/// `custodian exercise run|report`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "exercise")]
#[composite(command = "run", ty = ExerciseRun)]
#[composite(command = "report", ty = ExerciseReport)]
pub struct Exercise;

/// `custodian linkage add|check`.
#[derive(CompositeSubCommand, Debug, Clone, Copy, Default)]
#[subcommand(segment = "linkage")]
#[composite(command = "add", ty = LinkageAdd, init = "name")]
#[composite(command = "check", ty = LinkageCheck)]
pub struct Linkage;

leaf! {
    /// The plain folder (specs, readings, journal); `--macros` adds `maintenance` to the workspace through
    /// code_passenger; `--job-mode` is the workspace's job mode when a job names none.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "init")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Init {
        #[arg(flag = "--macros")]
        pub macros: bool,
        /// The workspace's job mode: `bounded` (change specs only) or `unbounded` (help, nothing refused).
        #[builder(into)]
        #[arg(kv = "--job-mode", each = mode_word)]
        pub job_mode: Option<OsString>,
    }
}

leaf! {
    /// The folder with jobs as folders and the job kinds the agent mode knows.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "init")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct AgentInit {
        #[arg(flag = "--macros")]
        pub macros: bool,
        /// The workspace's job mode: `bounded` (change specs only) or `unbounded` (help, nothing refused).
        #[builder(into)]
        #[arg(kv = "--job-mode", each = mode_word)]
        pub job_mode: Option<OsString>,
    }
}

leaf! {
    /// The first line an agent runs in a project: the mode, the open jobs and what they wait on, the decisions to
    /// make first, the latest reading of each spec and whether the code moved since, the test lines' last results,
    /// and the exact next lines to run.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "brief")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct AgentBrief {
        /// Only the head: the mode, the open job, the decisions and the next line (what the pre-edit hook prints).
        #[arg(flag = "--head")]
        pub head: bool,
    }
}

leaf! {
    /// May this file be edited by hand now? A bounded job refuses it (its code is written through change specs);
    /// otherwise yes. What the pre-edit hook asks before an edit.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "guard")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct AgentGuard {
        /// The file about to be edited.
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub file: OsString,
    }
}

leaf! {
    /// One spec (a file, or a queued or done spec's name), a search on the line (`--kind` and a target), or every
    /// queued spec and job folder in order.
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
    /// The inventory, rendered from the journal (rebuilt from the receipts a checkout carries).
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "list")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct List;
}

leaf! {
    /// What moved between two readings of one spec (the last two by default).
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
    /// Opens a job: a queued folder (by name or id), else a new one for an agentic task; its mode, target and
    /// verification recorded at open.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "start")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct JobStart {
        /// The job's name (a queued folder, or a new one).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub name: Option<OsString>,
        /// What the job is: a folder of specs, or one of the templates.
        #[builder(into)]
        #[arg(kv = "--kind", each = job_kind_word)]
        pub kind: Option<OsString>,
        /// What the job works on: a route or an item (its target is known when it resolves).
        #[builder(into)]
        #[arg(kv = "--target")]
        pub target: Option<OsString>,
        /// What the job is for, in the asker's words: its task history's intent record.
        #[builder(into)]
        #[arg(kv = "--intent", each = nonempty)]
        pub intent: Option<OsString>,
        /// `bounded` (the agent writes no code: change specs only) or `unbounded` (help, nothing refused).
        #[builder(into)]
        #[arg(kv = "--mode", each = mode_word)]
        pub mode: Option<OsString>,
        /// What done looks like: `test:<name>`, `reading:<spec>` (a reading that must change), `check:<which>`.
        #[builder(into)]
        #[arg(kv = "--verify", each = verify_word)]
        pub verify: Vec<OsString>,
        /// Read every step again.
        #[arg(flag = "--again")]
        pub again: bool,
        /// Run the tests the target reaches at open, and keep them as the job's baseline: a test failing the same way
        /// there is not the job's failure at close.
        #[arg(flag = "--baseline")]
        pub baseline: bool,
        /// A file the job may touch that no operation writes (a test's expected output, a fixture), as a glob.
        #[builder(into)]
        #[arg(kv = "--allow-file", each = nonempty)]
        pub allow_file: Vec<OsString>,
    }
}

leaf! {
    /// Closes the running job: its closing receipt, its task history's result (`accepted` only on evidence), its
    /// folder moved to done; a bounded job with lines no transform covers is refused, `rejected`.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "close")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct JobClose {
        /// The job's id (the running one by default).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub name: Option<OsString>,
    }
}

leaf! {
    /// The facts of a job: open decisions first, the result, the mode and when its target and outcome became
    /// known, the readings, the lines touched; `--bundle` packs a closed job as one file too.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "brief")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct JobBrief {
        /// The job's id or name (the last closed one by default).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub name: Option<OsString>,
        /// The closed job packed as one bundle too (its receipts, readings and the windows around its sites, secret
        /// items held redacted), as `custodian bundle <job>` makes it.
        #[arg(flag = "--bundle")]
        pub bundle: bool,
    }
}

leaf! {
    /// A bundled job unbundled into the queue and started under a new id, its origin the bundle's digest.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "import")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct JobImport {
        /// The bundle file.
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub bundle: OsString,
    }
}

leaf! {
    /// Every job folder by state, each with its task state (proposed, ready, running, blocked, paused, verifying,
    /// completed, failed).
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "list")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct JobList;
}

leaf! {
    /// Declares what done looks like for the running job; the closing receipt records when the outcome became known.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "verify")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct JobVerify {
        /// `test:<name>`, `reading:<spec>` (a reading that must change), `check:<which>`.
        #[builder(init_required, into)]
        #[arg(positional, each = verify_word)]
        pub line: OsString,
        /// The job (the running one by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// Lets the running job touch a file no operation writes (a test's expected output, a fixture): a glob added to
    /// its job file's `allow_file`, with a receipt; the closing receipt names each such file.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "allow")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct JobAllow {
        /// The file, or a glob of files, from the root.
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub glob: OsString,
        /// The job (the running one by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// The tests the change since `--ref` reaches (or every test line), each cargo line with its receipt and times.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "run")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct TestRun {
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
    /// The last runs' outcomes: the slowest lines, the flakiest tests, maintenance's timed report; nothing run.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "report")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct TestReport;
}

leaf! {
    /// A maintenance check, through maintenance's own line, its JSON kept.
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
    /// The maintenance attributes the receipts call for, as a plan.
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
    /// code_passenger's feature-gating spec from the eligibility reading.
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
    /// Every liaise code table: each code's meaning and declaration, and the sites raising it.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "list")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct DiagnosticsList {
        /// A crate (every crate by default).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub krate: Option<OsString>,
    }
}

leaf! {
    /// Every site raising a diagnostic code, with its function.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "where")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct DiagnosticsWhere {
        /// The diagnostic code.
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub code: OsString,
    }
}

leaf! {
    /// Each code a log names: its declaration and its sites.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "from")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct DiagnosticsFrom {
        /// The log file, `-` for stdin.
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub log: OsString,
    }
}

leaf! {
    /// The built-in triage job, started and closed.
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
    /// A route, a declared set or a job as one annotated file, its filters a checked word.
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
        /// Leave `benches/` out.
        #[arg(flag = "--no-benches")]
        pub no_benches: bool,
        /// Leave `examples/` out.
        #[arg(flag = "--no-examples")]
        pub no_examples: bool,
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
    /// List an undeclared tree and offer the routes to declare (the one walk custodian makes).
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
    /// Files, lines, code, comments and blanks of a route (custodian_kit's `count` reading kind, on tokei), per crate
    /// or language, and the tests' share.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "of")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct CountOf {
        /// The route (the workspace by default).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub route: Option<OsString>,
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
    /// Two counts compared: per crate and language, files added and removed, both digests.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "report")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct CountReport {
        /// The earlier count: a receipt number, a job id (`job:open`, `job:close`), a commit or a date.
        #[builder(init_required, into)]
        #[arg(positional, each = plain)]
        pub a: OsString,
        /// The later count.
        #[builder(init_required, into)]
        #[arg(positional, each = plain)]
        pub b: OsString,
    }
}

leaf! {
    /// Every count of a route (of every route by default).
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "history")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct CountHistory {
        /// The route.
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub route: Option<OsString>,
    }
}

leaf! {
    /// The resident, on the project's owner-only socket, until `custodian stop`.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "serve")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Serve;
}

leaf! {
    /// Ask the resident to stop.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "stop")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Stop;
}

leaf! {
    /// Job state changes and test progress, through the observer set.
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
    /// Installs custodian through the installation crate: a plan, the install from it, its receipt in the journal.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "install")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct SelfInstall {
        /// The binary to install (this program by default).
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
    /// Uninstalls custodian from its install receipt.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "uninstall")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct SelfUninstall {
        #[builder(into)]
        #[arg(kv = "--bin-dir")]
        #[requires("state_dir")]
        pub bin_dir: Option<OsString>,
        #[builder(into)]
        #[arg(kv = "--state-dir")]
        #[requires("bin_dir")]
        pub state_dir: Option<OsString>,
        #[arg(flag = "--force")]
        pub force: bool,
    }
}

leaf! {
    /// What is installed, from its receipt.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "status")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct SelfStatus {
        #[builder(into)]
        #[arg(kv = "--bin-dir")]
        #[requires("state_dir")]
        pub bin_dir: Option<OsString>,
        #[builder(into)]
        #[arg(kv = "--state-dir")]
        #[requires("bin_dir")]
        pub state_dir: Option<OsString>,
    }
}

leaf! {
    /// An MCP server on stdin and stdout whose tools are these lines, each call run as the command line runs it.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "serve")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct McpServe {
        /// The project the server answers for (where it starts by default).
        #[builder(into)]
        #[arg(kv = "--root")]
        pub root: Option<OsString>,
    }
}

leaf! {
    /// What a change spec entails over the code as it is, and whether it is authorized: where it lands, its impact
    /// and cost, its preconditions, the files it writes against the scope it may write, the decisions it leaves
    /// open; before anything is written.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "preflight")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Preflight {
        /// The change spec (`v1.<name>.1o.change`, or its TOML twin).
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub spec: OsString,
        /// The job it is part of (the running one by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// A change spec made in the code by code_passenger, on the preflight receipt of this spec over this code:
    /// proved on a scratch copy, written with `--write`, its record kept; or a record's files put back (`--revert`).
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "transform")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    #[validate(with = "transform_valid")]
    pub struct Transform {
        /// The change spec.
        #[builder(into)]
        #[arg(positional, each = nonempty)]
        pub spec: Option<OsString>,
        /// Write it (else prove it on a scratch copy only).
        #[arg(flag = "--write")]
        pub write: bool,
        /// A transform record whose files are put back.
        #[builder(into)]
        #[arg(kv = "--revert", each = nonempty)]
        pub revert: Option<OsString>,
        /// The job it is part of (the running one by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// An intent's plan: where the work is, what can be reused, what is new, the top candidates' impact and the
    /// cost, the decisions to make before any step, and the steps proposed; nothing written.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "make")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct PlanMake {
        /// The intent, in the asker's words.
        #[builder(init_required, into)]
        #[arg(positional, each = nonempty)]
        pub intent: OsString,
        /// The plan's name (from the intent's words by default).
        #[builder(into)]
        #[arg(kv = "--name", each = plain)]
        pub name: Option<OsString>,
        /// The job it is part of (the running one by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// A plan's step written as a change spec (`data/specs/changes/`), proposed until its preflight authorizes it.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "approve")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct PlanApprove {
        /// The plan's name.
        #[builder(init_required, into)]
        #[arg(positional, each = plain)]
        pub plan: OsString,
        /// The step's number.
        #[builder(init_required, into)]
        #[arg(positional, each = number)]
        pub step: OsString,
        /// The operation (the step's first by default).
        #[builder(into)]
        #[arg(kv = "--operation", each = operation_word)]
        pub operation: Option<OsString>,
        /// The job it is part of (the running one by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// A plan's page.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "show")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct PlanShow {
        /// The plan's name.
        #[builder(init_required, into)]
        #[arg(positional, each = plain)]
        pub plan: OsString,
        /// The job it is part of (the running one by default).
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// The decisions still open, asked together; with an id and an answer, one decision made (its reason with it),
    /// kept in the task history so it is never asked again.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "decide")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    #[validate(with = "decide_valid")]
    pub struct Decide {
        /// The decision's id (`q-…`).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub id: Option<OsString>,
        /// The answer.
        #[builder(into)]
        #[arg(positional, each = nonempty)]
        pub answer: Option<OsString>,
        /// Why: the reasoning kept with the decision.
        #[builder(into)]
        #[arg(kv = "--why", each = nonempty)]
        pub why: Option<OsString>,
    }
}

leaf! {
    /// A linkage record: why declarations are bound together, each bound declaration resolved now.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "add")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct LinkageAdd {
        /// The record's name.
        #[builder(init_required, into)]
        #[arg(positional, each = plain)]
        pub name: OsString,
        /// Why they are bound: responsibility, invariant, connected, compatibility, synchronized, guarded, semantics.
        #[builder(into)]
        #[arg(kv = "--kind", each = link_word)]
        pub kind: Option<OsString>,
        /// The reason, in the author's words.
        #[builder(into)]
        #[arg(kv = "--why", each = nonempty)]
        pub why: Option<OsString>,
        /// A declaration bound: an item path, a crate:/decl: route, dep:<crate>.
        #[builder(into)]
        #[arg(kv = "--bind", each = nonempty)]
        pub bind: Vec<OsString>,
    }
}

leaf! {
    /// The linkage records naming a target (every record by default), each checked against the code: stale, to
    /// review, or holding; and the comment anchors no record backs.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "check")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct LinkageCheck {
        /// An item, a route or a record's name.
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub target: Option<OsString>,
    }
}

leaf! {
    /// Custodian's own change path tried on copies of the project, every result a receipt. For each operation and
    /// each kind of unit it applies to, N targets picked from the index, each a case run on a copy in the job's data
    /// folder (spec, preflight, transform, cargo check, the tests with `--tests`, the closing check); every refusal,
    /// compile error or uncovered file a finding with its receipt; with `--intents`, each line through the cheap
    /// model first.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "run")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct ExerciseRun {
        /// The operations, comma-separated (every one by default).
        #[builder(into)]
        #[arg(kv = "--ops", each = ops_words)]
        pub ops: Option<OsString>,
        /// The unit kinds, comma-separated: item, function, impl, module, file, crate (every one by default).
        #[builder(into)]
        #[arg(kv = "--units", each = unit_words)]
        pub units: Option<OsString>,
        /// Targets per operation and unit kind (1 by default).
        #[builder(into)]
        #[arg(kv = "--n", each = number)]
        pub n: Option<OsString>,
        /// Run the landing crate's tests after each transform.
        #[arg(flag = "--tests")]
        pub tests: bool,
        /// Only this crate's units.
        #[builder(into)]
        #[arg(kv = "--crate", each = plain)]
        pub krate: Option<OsString>,
        /// One intent a line, each turned into a change spec by the cheap model.
        #[builder(into)]
        #[arg(kv = "--intents", each = nonempty)]
        pub intents: Option<OsString>,
        /// The pick's seed (0 by default).
        #[builder(into)]
        #[arg(kv = "--seed", each = number)]
        pub seed: Option<OsString>,
    }
}

leaf! {
    /// An exercise job's summary, the newest when none is named. Cases run and passed, the findings by code, what to
    /// fix first, the operation × unit matrix, the slowest cases, every finding with its receipt.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "report")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct ExerciseReport {
        /// The exercise job: its id or name.
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub job: Option<OsString>,
    }
}

leaf! {
    /// The message a person reads after a job, from the records only: what waits on a person first, then what the
    /// job was for, what changed, what was decided and by whom, what was verified, and what runs next; short, every
    /// list cut with "… and N more".
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "handoff")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Handoff {
        /// The job's id or name (the job closed last by default; with `--since`, every job that ran after it).
        #[builder(into)]
        #[arg(positional, each = plain)]
        pub job: Option<OsString>,
        /// Who reads it: `james` (sentences) or `agent` (the exact lines to run and the routes too).
        #[builder(into)]
        #[arg(kv = "--for", each = reader_word)]
        pub reader: Option<OsString>,
        /// Only what happened after this receipt.
        #[builder(into)]
        #[arg(kv = "--since", each = number)]
        pub since: Option<OsString>,
    }
}

leaf! {
    /// Who introduced a crate: or decl: route and for what: the task histories whose changes touch it, the linkage
    /// records binding it, the readings about it, and git's first and last commit touching its files (a git log
    /// line with its receipt); "no record" where the records say nothing.
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "why")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Why {
        /// A `crate:` or `decl:` route.
        #[builder(init_required, into)]
        #[arg(positional, each = route_word)]
        pub route: OsString,
    }
}

leaf! {
    /// The tokens readings stood for: read against reused, the tokens an agent would have spent reading the same
    /// files by hand against the tokens reuse saved, per reading kind and per job (an estimate: bytes/4 of the
    /// files each reading touched).
    #[allow(clippy::duplicated_attributes)]
    #[derive(SimpleSubCommand, Debug, Clone, Default)]
    #[subcommand(segment = "cost")]
    #[exit_status(code = 0, word = "done")]
    #[exit_status(code = 1, word = "findings")]
    #[exit_status(code = 2, word = "refused")]
    pub struct Cost {
        /// One job's runs only.
        #[builder(into)]
        #[arg(kv = "--job", each = plain)]
        pub job: Option<OsString>,
        /// Only the runs after this receipt.
        #[builder(into)]
        #[arg(kv = "--since", each = number)]
        pub since: Option<OsString>,
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

/// A route `why` reads: `crate:<name>[/<file>]` or `decl:<name>`.
fn route_word(field: &str, word: &OsStr) -> CommandValidationResult {
    match word.to_str() {
        Some(w) if (w.starts_with("crate:") && w.len() > 6) || (w.starts_with("decl:") && w.len() > 5) => Ok(()),
        _ => refuse(field, format!("{word:?} is not a crate: or decl: route")),
    }
}

fn one_of(field: &str, word: &OsStr, words: &[&str]) -> CommandValidationResult {
    match word.to_str() {
        Some(w) if words.contains(&w) => Ok(()),
        _ => refuse(field, format!("{word:?} is not a {field}: {}", words.join(", "))),
    }
}

/// Words of a set, comma-separated.
fn each_of(field: &str, word: &OsStr, words: &[&str]) -> CommandValidationResult {
    let t = word.to_string_lossy();
    match t.split(',').map(str::trim).find(|w| !words.contains(w)) {
        Some(w) => refuse(field, format!("{w:?} is not one of the {field}: {}", words.join(", "))),
        None => Ok(()),
    }
}

fn ops_words(field: &str, word: &OsStr) -> CommandValidationResult {
    each_of(field, word, &custodian_kit::change::Operation::ALL.iter().map(|o| o.as_str()).collect::<Vec<_>>())
}

fn unit_words(field: &str, word: &OsStr) -> CommandValidationResult {
    each_of(field, word, &custodian_kit::exercise::Unit::ALL.iter().map(|u| u.as_str()).collect::<Vec<_>>())
}

/// A verification line: `test:<name>`, `reading:<spec>`, `check:<which>`.
fn verify_word(field: &str, word: &OsStr) -> CommandValidationResult {
    match word.to_str().and_then(|w| w.split_once(':')) {
        Some((form, what)) if custodian_kit::jobs::VERIFY_FORMS.contains(&form) && !what.is_empty() => Ok(()),
        _ => refuse(field, format!("{word:?} is not a verification: test:<name>, reading:<spec>, check:<which>")),
    }
}

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
    kind_word => custodian_kit::api::ReadingKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<&'static str>>();
    job_kind_word => crate::words::JobKind::words();
    observe_word => crate::words::ObserveLevel::words();
    check_word => maintenance::runner::CheckName::ALL.iter().map(|c| c.word()).collect::<Vec<&'static str>>();
    mode_word => custodian_kit::jobs::JobMode::ALL.iter().map(|m| m.as_str()).collect::<Vec<&'static str>>();
    operation_word => custodian_kit::change::Operation::ALL.iter().map(|o| o.as_str()).collect::<Vec<&'static str>>();
    link_word => custodian_kit::linkage::LinkKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<&'static str>>();
    by_word => ["crate", "language"];
    only_word => ["src"];
    redact_word => ["secret"];
    reader_word => custodian_kit::handoff::Reader::WORDS;
}

fn transform_valid(line: &Transform) -> CommandValidationResult {
    match (&line.spec, &line.revert) {
        (Some(_), Some(_)) => refuse("revert", "a spec or --revert RECORD, not both".into()),
        (None, None) => refuse("spec", "transform SPEC [--write], or transform --revert RECORD".into()),
        (None, Some(_)) if line.write => refuse("write", "--revert writes; --write is a spec's".into()),
        _ => Ok(()),
    }
}

fn decide_valid(line: &Decide) -> CommandValidationResult {
    if line.answer.is_some() && line.id.is_none() || line.why.is_some() && line.answer.is_none() {
        return refuse("answer", "decide (the open decisions), or decide ID ANSWER [--why TEXT]".into());
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

    /// Every `--name VALUE`, in order.
    pub fn kvs(&mut self, name: &str) -> Result<Vec<String>> {
        let mut out = Vec::new();
        while let Some(v) = self.kv(name)? {
            out.push(v);
        }
        Ok(out)
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

/// A leaf as the full line: directly under the root, or under a surface (its nested wrapper).
macro_rules! full {
    ($l:expr, root) => {
        $l.clone().rooted::<Custodian>()
    };
    ($l:expr, $surface:ident) => {
        <_ as RootableNestedCommand<Custodian, $surface>>::nested($l.clone(), $surface)
    };
}

macro_rules! lines {
    ($($variant:ident($ty:ident) = [$($word:literal),+] in $under:ident => $read:ident;)*) => {
        /// A declared line, read from the words: the leaf with its values, rooted under `custodian` (or nested under
        /// its surface) to run.
        #[derive(Debug, Clone)]
        pub enum Line { $($variant($ty),)* }

        /// Every leaf's place (its words after `custodian`) and its declaration table, in order.
        pub const DECLARED: &[(&str, Declared)] = &[$((concat!($($word, " "),+), $ty::DECLARED),)*];

        /// Every surface the root has, by its segment.
        pub const SURFACES: &[&str] = &["agent", "job", "test", "diagnostics", "count", "self", "mcp", "plan", "linkage", "exercise"];

        impl Line {
            /// The words that name the line: `job start`.
            pub fn path(&self) -> &'static str {
                match self { $(Line::$variant(_) => concat!($($word, " "),+).trim_end(),)* }
            }

            /// The subcommand's word (the first of its path).
            pub fn segment(&self) -> &'static str {
                self.path().split(' ').next().unwrap_or_default()
            }

            /// The words after the program, as the line writes them.
            pub fn argv(&self) -> Vec<OsString> {
                match self { $(Line::$variant(l) => full!(l, $under).argv(),)* }
            }

            /// `custodian run spec.toml --again`.
            pub fn display(&self) -> String {
                match self { $(Line::$variant(l) => full!(l, $under).build_display(),)* }
            }

            /// The line's own rules.
            pub fn validate(&self) -> CommandValidationResult {
                match self { $(Line::$variant(l) => full!(l, $under).validate(),)* }
            }

            /// The preview xccute makes of it: data, no side effect.
            pub fn preview(&self) -> CommandPreview {
                match self { $(Line::$variant(l) => full!(l, $under).preview(),)* }
            }

            /// What xccute decides after a status, by the statuses the line names.
            pub fn decide(&self, code: i32) -> ExitStatusDecision {
                match self {
                    $(Line::$variant(l) => {
                        let l = full!(l, $under);
                        let accepted = l.ok_codes().contains(&code);
                        ExitStatusPolicy::of(&l).decide(RuntimeExitStatus::new(Some(code), accepted))
                    })*
                }
            }

            /// The word the line names for a status.
            pub fn status_word(&self, code: i32) -> &'static str {
                match self { $(Line::$variant(l) => full!(l, $under).exit_status_of(Some(code)).map_or("unnamed", |s| s.word),)* }
            }
        }

        fn read_path(path: &str, rest: &[String]) -> Result<Line> {
            match path {
                $(p if p == concat!($($word, " "),+).trim_end() => {
                    let mut w = Words::new(rest);
                    let line = $read(&mut w)?;
                    w.done(p)?;
                    Ok(Line::$variant(line))
                })*
                other => Err(Code::LineRefused.refuse(format!("`{other}`; the lines are: {}", usage_words().join(" · ")))),
            }
        }
    };
}

lines! {
    Init(Init) = ["init"] in root => read_init;
    AgentInit(AgentInit) = ["agent", "init"] in Agent => read_agent_init;
    AgentBrief(AgentBrief) = ["agent", "brief"] in Agent => read_agent_brief;
    AgentGuard(AgentGuard) = ["agent", "guard"] in Agent => read_agent_guard;
    Run(Run) = ["run"] in root => read_run;
    List(List) = ["list"] in root => read_list;
    Diff(Diff) = ["diff"] in root => read_diff;
    JobStart(JobStart) = ["job", "start"] in Job => read_job_start;
    JobClose(JobClose) = ["job", "close"] in Job => read_job_close;
    JobBrief(JobBrief) = ["job", "brief"] in Job => read_job_brief;
    JobImport(JobImport) = ["job", "import"] in Job => read_job_import;
    JobList(JobList) = ["job", "list"] in Job => read_job_list;
    JobVerify(JobVerify) = ["job", "verify"] in Job => read_job_verify;
    JobAllow(JobAllow) = ["job", "allow"] in Job => read_job_allow;
    TestRun(TestRun) = ["test", "run"] in Test => read_test_run;
    TestReport(TestReport) = ["test", "report"] in Test => read_test_report;
    Check(Check) = ["check"] in root => read_check;
    Annotate(Annotate) = ["annotate"] in root => read_annotate;
    Features(Features) = ["features"] in root => read_features;
    DiagnosticsList(DiagnosticsList) = ["diagnostics", "list"] in Diagnostics => read_diagnostics_list;
    DiagnosticsWhere(DiagnosticsWhere) = ["diagnostics", "where"] in Diagnostics => read_diagnostics_where;
    DiagnosticsFrom(DiagnosticsFrom) = ["diagnostics", "from"] in Diagnostics => read_diagnostics_from;
    Triage(Triage) = ["triage"] in root => read_triage;
    Bundle(Bundle) = ["bundle"] in root => read_bundle;
    Sweep(Sweep) = ["sweep"] in root => read_sweep;
    CountOf(CountOf) = ["count", "of"] in Count => read_count_of;
    CountReport(CountReport) = ["count", "report"] in Count => read_count_report;
    CountHistory(CountHistory) = ["count", "history"] in Count => read_count_history;
    Serve(Serve) = ["serve"] in root => read_serve;
    Stop(Stop) = ["stop"] in root => read_stop;
    Watch(Watch) = ["watch"] in root => read_watch;
    SelfInstall(SelfInstall) = ["self", "install"] in SelfSurface => read_self_install;
    SelfUninstall(SelfUninstall) = ["self", "uninstall"] in SelfSurface => read_self_uninstall;
    SelfStatus(SelfStatus) = ["self", "status"] in SelfSurface => read_self_status;
    McpServe(McpServe) = ["mcp", "serve"] in Mcp => read_mcp_serve;
    Preflight(Preflight) = ["preflight"] in root => read_preflight;
    Transform(Transform) = ["transform"] in root => read_transform;
    PlanMake(PlanMake) = ["plan", "make"] in Plan => read_plan_make;
    PlanApprove(PlanApprove) = ["plan", "approve"] in Plan => read_plan_approve;
    PlanShow(PlanShow) = ["plan", "show"] in Plan => read_plan_show;
    Decide(Decide) = ["decide"] in root => read_decide;
    LinkageAdd(LinkageAdd) = ["linkage", "add"] in Linkage => read_linkage_add;
    LinkageCheck(LinkageCheck) = ["linkage", "check"] in Linkage => read_linkage_check;
    ExerciseRun(ExerciseRun) = ["exercise", "run"] in Exercise => read_exercise_run;
    ExerciseReport(ExerciseReport) = ["exercise", "report"] in Exercise => read_exercise_report;
    Handoff(Handoff) = ["handoff"] in root => read_handoff;
    Why(Why) = ["why"] in root => read_why;
    Cost(Cost) = ["cost"] in root => read_cost;
}

/// The usage screen's lines: the root's help lines, generated from the declarations.
pub fn usage_lines() -> Vec<xccute_contract::HelpLine> {
    Custodian::help_lines()
}

/// Every line's words after `custodian`, for a refusal that names them.
fn usage_words() -> Vec<String> {
    DECLARED.iter().map(|(p, _)| p.trim_end().to_string()).collect()
}

/// The commands a surface holds.
fn commands_of(surface: &str) -> Vec<&'static str> {
    DECLARED.iter().filter_map(|(p, _)| p.trim_end().split_once(' ').filter(|(s, _)| *s == surface).map(|(_, c)| c)).collect()
}

/// The words after `custodian`, as a declared line, or the refusal.
pub fn read(args: &[OsString]) -> Result<Line> {
    let words: Vec<String> = args
        .iter()
        .map(|a| a.to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Code::LineRefused.refuse("a word is not UTF-8"))?;
    let (seg, rest) = words.split_first().ok_or_else(|| Code::LineRefused.refuse("no subcommand"))?;
    let line = if SURFACES.contains(&seg.as_str()) {
        let commands = commands_of(seg);
        match rest.first() {
            Some(c) if commands.contains(&c.as_str()) => read_path(&format!("{seg} {c}"), &rest[1..])?,
            // the spellings from before the surfaces: `count [ROUTE]`, `test [--all]`, `plan INTENT`
            _ if seg == "count" => read_path("count of", rest)?,
            _ if seg == "test" => read_path("test run", rest)?,
            Some(_) if seg == "plan" => read_path("plan make", rest)?,
            // `exercise [--ops ...]` is `exercise run`
            _ if seg == "exercise" && rest.first().is_none_or(|w| w.starts_with("--")) => read_path("exercise run", rest)?,
            Some(c) => return Err(Code::LineRefused.refuse(format!("`custodian {seg} {c}`: {seg} takes {}", commands.join(", ")))),
            None => return Err(Code::LineRefused.refuse(format!("`custodian {seg}` names its action: {}", commands.join(", ")))),
        }
    } else {
        read_path(seg, rest)?
    };
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
    Ok(Init { macros: w.flag("--macros"), job_mode: kv(w, "--job-mode")? })
}

fn read_agent_init(w: &mut Words) -> Result<AgentInit> {
    Ok(AgentInit { macros: w.flag("--macros"), job_mode: kv(w, "--job-mode")? })
}

fn read_agent_brief(w: &mut Words) -> Result<AgentBrief> {
    Ok(AgentBrief { head: w.flag("--head") })
}

fn read_agent_guard(w: &mut Words) -> Result<AgentGuard> {
    Ok(AgentGuard { file: need(w, "file", "agent guard")? })
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

fn read_job_start(w: &mut Words) -> Result<JobStart> {
    Ok(JobStart {
        kind: kv(w, "--kind")?,
        target: kv(w, "--target")?,
        intent: kv(w, "--intent")?,
        mode: kv(w, "--mode")?,
        verify: w.kvs("--verify")?.into_iter().map(OsString::from).collect(),
        again: w.flag("--again"),
        baseline: w.flag("--baseline"),
        allow_file: w.kvs("--allow-file")?.into_iter().map(OsString::from).collect(),
        name: pos(w),
    })
}

fn read_job_close(w: &mut Words) -> Result<JobClose> {
    Ok(JobClose { name: pos(w) })
}

fn read_job_brief(w: &mut Words) -> Result<JobBrief> {
    Ok(JobBrief { bundle: w.flag("--bundle"), name: pos(w) })
}

fn read_job_import(w: &mut Words) -> Result<JobImport> {
    Ok(JobImport { bundle: need(w, "bundle (job import FILE.bundle)", "job import")? })
}

fn read_job_list(_: &mut Words) -> Result<JobList> {
    Ok(JobList)
}

fn read_job_verify(w: &mut Words) -> Result<JobVerify> {
    Ok(JobVerify { job: kv(w, "--job")?, line: need(w, "verification (test:<name>, reading:<spec>, check:<which>)", "job verify")? })
}

fn read_job_allow(w: &mut Words) -> Result<JobAllow> {
    Ok(JobAllow { job: kv(w, "--job")?, glob: need(w, "file or glob (job allow GLOB)", "job allow")? })
}

fn read_test_run(w: &mut Words) -> Result<TestRun> {
    Ok(TestRun { affected: w.flag("--affected"), all: w.flag("--all"), observe: kv(w, "--observe")?, git_ref: kv(w, "--ref")?, job: kv(w, "--job")? })
}

fn read_test_report(_: &mut Words) -> Result<TestReport> {
    Ok(TestReport)
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

fn read_diagnostics_list(w: &mut Words) -> Result<DiagnosticsList> {
    Ok(DiagnosticsList { krate: pos(w) })
}

fn read_diagnostics_where(w: &mut Words) -> Result<DiagnosticsWhere> {
    Ok(DiagnosticsWhere { code: need(w, "code", "diagnostics where")? })
}

fn read_diagnostics_from(w: &mut Words) -> Result<DiagnosticsFrom> {
    Ok(DiagnosticsFrom { log: need(w, "log file (- for stdin)", "diagnostics from")? })
}

fn read_triage(w: &mut Words) -> Result<Triage> {
    Ok(Triage { target: need(w, "code or @logfile", "triage")? })
}

fn read_bundle(w: &mut Words) -> Result<Bundle> {
    Ok(Bundle {
        no_tests: w.flag("--no-tests"),
        no_benches: w.flag("--no-benches"),
        no_examples: w.flag("--no-examples"),
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

fn read_count_of(w: &mut Words) -> Result<CountOf> {
    Ok(CountOf { by: kv(w, "--by")?, tests: w.flag("--tests"), again: w.flag("--again"), job: kv(w, "--job")?, route: pos(w) })
}

fn read_count_report(w: &mut Words) -> Result<CountReport> {
    let a = need(w, "two counts: count report A B", "count report")?;
    let b = need(w, "two counts: count report A B", "count report")?;
    Ok(CountReport { a, b })
}

fn read_count_history(w: &mut Words) -> Result<CountHistory> {
    Ok(CountHistory { route: pos(w) })
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

fn read_self_install(w: &mut Words) -> Result<SelfInstall> {
    Ok(SelfInstall { from: kv(w, "--from")?, bin_dir: kv(w, "--bin-dir")?, state_dir: kv(w, "--state-dir")?, dry_run: w.flag("--dry-run"), force: w.flag("--force") })
}

fn read_self_uninstall(w: &mut Words) -> Result<SelfUninstall> {
    Ok(SelfUninstall { bin_dir: kv(w, "--bin-dir")?, state_dir: kv(w, "--state-dir")?, force: w.flag("--force") })
}

fn read_self_status(w: &mut Words) -> Result<SelfStatus> {
    Ok(SelfStatus { bin_dir: kv(w, "--bin-dir")?, state_dir: kv(w, "--state-dir")? })
}

fn read_mcp_serve(w: &mut Words) -> Result<McpServe> {
    Ok(McpServe { root: kv(w, "--root")? })
}

fn read_preflight(w: &mut Words) -> Result<Preflight> {
    Ok(Preflight { job: kv(w, "--job")?, spec: need(w, "change spec", "preflight")? })
}

fn read_transform(w: &mut Words) -> Result<Transform> {
    Ok(Transform { write: w.flag("--write"), revert: kv(w, "--revert")?, job: kv(w, "--job")?, spec: pos(w) })
}

fn read_plan_make(w: &mut Words) -> Result<PlanMake> {
    let name = kv(w, "--name")?;
    let job = kv(w, "--job")?;
    // the intent is every word left, as one
    let mut words = Vec::new();
    while let Some(p) = w.positional() {
        words.push(p);
    }
    if words.is_empty() {
        return Err(Code::LineRefused.refuse("custodian plan make names its intent"));
    }
    Ok(PlanMake { intent: words.join(" ").into(), name, job })
}

fn read_plan_approve(w: &mut Words) -> Result<PlanApprove> {
    let operation = kv(w, "--operation")?;
    let job = kv(w, "--job")?;
    let plan = need(w, "plan", "plan approve")?;
    let step = need(w, "step", "plan approve")?;
    Ok(PlanApprove { plan, step, operation, job })
}

fn read_plan_show(w: &mut Words) -> Result<PlanShow> {
    let job = kv(w, "--job")?;
    Ok(PlanShow { plan: need(w, "plan", "plan show")?, job })
}

fn read_decide(w: &mut Words) -> Result<Decide> {
    let why = kv(w, "--why")?;
    let id = pos(w);
    let mut rest = Vec::new();
    while let Some(p) = w.positional() {
        rest.push(p);
    }
    Ok(Decide { id, answer: (!rest.is_empty()).then(|| rest.join(" ").into()), why })
}

fn read_linkage_add(w: &mut Words) -> Result<LinkageAdd> {
    Ok(LinkageAdd { kind: kv(w, "--kind")?, why: kv(w, "--why")?, bind: w.kvs("--bind")?.into_iter().map(OsString::from).collect(), name: need(w, "record's name", "linkage add")? })
}

fn read_linkage_check(w: &mut Words) -> Result<LinkageCheck> {
    Ok(LinkageCheck { target: pos(w) })
}

fn read_exercise_run(w: &mut Words) -> Result<ExerciseRun> {
    Ok(ExerciseRun { ops: kv(w, "--ops")?, units: kv(w, "--units")?, n: kv(w, "--n")?, tests: w.flag("--tests"), krate: kv(w, "--crate")?, intents: kv(w, "--intents")?, seed: kv(w, "--seed")? })
}

fn read_exercise_report(w: &mut Words) -> Result<ExerciseReport> {
    Ok(ExerciseReport { job: pos(w) })
}

fn read_handoff(w: &mut Words) -> Result<Handoff> {
    Ok(Handoff { reader: kv(w, "--for")?, since: kv(w, "--since")?, job: pos(w) })
}

fn read_why(w: &mut Words) -> Result<Why> {
    Ok(Why { route: need(w, "route (crate:<name> or decl:<name>)", "why")? })
}

fn read_cost(w: &mut Words) -> Result<Cost> {
    Ok(Cost { job: kv(w, "--job")?, since: kv(w, "--since")? })
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
            Custodian::init().macros().job_mode("bounded").argv(),
            Custodian::agent().init().macros().argv(),
            Custodian::agent().brief().head().argv(),
            Custodian::agent().guard("src/lib.rs").argv(),
            Custodian::run().target("spec.toml").kind("reach").job("j-1").again().argv(),
            Custodian::run().argv(),
            Custodian::list().argv(),
            Custodian::diff("env").from("3").to("9").argv(),
            Custodian::job().start().name("batch").kind("triage").target("CUST0004").intent("find the raise").mode("bounded").verify(["test:a", "check:all"]).again().baseline().allow_file(["tests/ui/*.stderr"]).argv(),
            Custodian::job().close().name("batch").argv(),
            Custodian::job().brief().name("batch").bundle().argv(),
            Custodian::job().import("x.bundle").argv(),
            Custodian::job().list().argv(),
            Custodian::job().verify("test:config_names").job("j-1").argv(),
            Custodian::job().allow("crates/*/tests/ui/*.stderr").job("j-1").argv(),
            Custodian::test().run().affected().observe("progress").git_ref("HEAD~1").job("j-2").argv(),
            Custodian::test().report().argv(),
            Custodian::check("all").job("j-3").argv(),
            Custodian::annotate().from("c.json").apply().argv(),
            Custodian::features().apply().argv(),
            Custodian::diagnostics().list().krate("core_lib").argv(),
            Custodian::diagnostics().where_("CUST0004").argv(),
            Custodian::diagnostics().from("box.log").argv(),
            Custodian::triage("@box.log").argv(),
            Custodian::bundle("crate:core_lib").no_tests().no_benches().no_examples().no_feature("serde").only("src").redact_level("secret").again().argv(),
            Custodian::sweep("vendor").argv(),
            Custodian::count().of().route("crate:core_lib").by("language").tests().again().job("j-4").argv(),
            Custodian::count().report("3", "9").argv(),
            Custodian::count().history().route("crate:core_lib").argv(),
            Custodian::serve().argv(),
            Custodian::stop().argv(),
            Custodian::watch().observe("all").since("4").follow().argv(),
            Custodian::self_().install().from("target/release/custodian").bin_dir("b").state_dir("s").dry_run().force().argv(),
            Custodian::self_().uninstall().force().argv(),
            Custodian::self_().status().argv(),
            Custodian::mcp().serve().root("/tmp/ws").argv(),
            Custodian::preflight("add-digest.toml").job("j-5").argv(),
            Custodian::transform().spec("add-digest.toml").write().job("j-5").argv(),
            Custodian::transform().revert("r.json").argv(),
            Custodian::plan().make("carry_a_digest").name("digest").argv(),
            Custodian::plan().make("carry_a_digest").name("digest").job("j-5").argv(),
            Custodian::plan().approve("digest", "1").operation("add_field").argv(),
            Custodian::plan().approve("digest", "1").job("j-5").argv(),
            Custodian::plan().show("digest").argv(),
            Custodian::plan().show("digest").job("j-5").argv(),
            Custodian::decide().id("q-1").answer("sqlite").why("the_journal_is").argv(),
            Custodian::decide().argv(),
            Custodian::linkage().add("x").kind("synchronized").why("same_keys").bind(["core_lib::Config", "app::Settings"]).argv(),
            Custodian::linkage().check().target("core_lib::Config").argv(),
            Custodian::exercise().run().ops("add_field,rename").units("item,function").n("2").tests().krate("core_lib").intents("intents.txt").seed("7").argv(),
            Custodian::exercise().run().argv(),
            Custodian::exercise().report().job("exercise-1").argv(),
            Custodian::handoff().job("hold").reader("agent").since("107").argv(),
            Custodian::handoff().argv(),
            Custodian::why("crate:core_lib").argv(),
            Custodian::cost().job("j-1").since("4").argv(),
            Custodian::cost().argv(),
        ];
        let mut paths: Vec<String> = lines.iter().map(|l| read(l).unwrap_or_else(|e| panic!("{l:?}: {e}")).path().to_string()).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), DECLARED.len(), "every line");
        for argv in lines {
            let back = read(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            assert_eq!(back.argv(), argv);
        }
    }

    /// The declaration tables are the leaves' own, one per line under the place `lines!` reads it by; the usage
    /// screen is the root's generated help lines, one per line.
    #[test]
    fn every_leaf_keeps_its_declaration_and_its_help_line() {
        for (path, d) in DECLARED {
            assert_eq!(path.trim_end().rsplit(' ').next(), Some(d.segment()), "{path}");
            assert!(!d.doc().is_empty(), "{path} has a doc");
            assert!(d.fields.iter().all(|f| f.written().is_some()), "{path}");
        }
        let help = usage_lines();
        assert_eq!(help.len(), DECLARED.len(), "{help:?}");
        assert!(help.iter().any(|h| h.words.starts_with("custodian job start [NAME] [--kind KIND]")), "{help:?}");
        assert!(help.iter().any(|h| h.words == "custodian count report A B"), "{help:?}");
        let run = &DECLARED.iter().find(|(p, _)| p.trim_end() == "run").unwrap().1;
        let kind = run.field("kind").unwrap();
        assert_eq!((kind.ty, kind.written(), kind.each()), ("Option", Some(Written::Kv("--kind")), Some("kind_word")));
        assert!(kind.words().unwrap().contains(&"impact"));
        assert_eq!(JobStart::DECLARED.field("verify").unwrap().ty, "Vec");
    }

    #[test]
    fn options_read_in_any_order_and_unknown_words_are_refused() {
        let a = read(&words("run --again spec.toml --kind reach")).unwrap();
        assert_eq!(a.display(), "custodian run spec.toml --kind reach --again");
        let e = read(&words("run spec.toml --loud")).unwrap_err();
        assert_eq!(e.to_string(), "[CUST1001] the words are not a declared custodian line: `--loud` is not a word of `custodian run`");
        let e = read(&words("job explode")).unwrap_err();
        assert!(e.to_string().contains("job takes start, close, brief, import, list, verify"), "{e}");
        assert!(read(&words("test --affected --all")).is_err());
        assert!(read(&words("run --kind nonsense x")).is_err());
        assert!(read(&words("diagnostics where")).unwrap_err().to_string().contains("names its code"));
        assert!(read(&words("self install --bin-dir b")).is_err(), "--bin-dir needs --state-dir");
        assert!(read(&words("deploy")).unwrap_err().to_string().contains("the lines are: init"));
        assert!(read(&words("job start --verify nonsense")).is_err());
        assert!(read(&words("transform")).is_err() && read(&words("transform s.toml --revert r.json")).is_err());
        assert!(read(&words("why core_lib")).unwrap_err().to_string().contains("not a crate: or decl: route"));
        assert!(read(&words("handoff --for everyone")).is_err() && read(&words("cost --since ten")).is_err());
        assert_eq!(read(&words("handoff --since 7 --for agent j-1")).unwrap().display(), "custodian handoff j-1 --for agent --since 7");
        // the spellings from before the surfaces
        assert_eq!(read(&words("count crate:core_lib --by language")).unwrap().display(), "custodian count of crate:core_lib --by language");
        assert_eq!(read(&words("test --all")).unwrap().display(), "custodian test run --all");
        assert_eq!(read(&words("plan carry a digest")).unwrap().display(), "custodian plan make carry a digest");
        assert_eq!(read(&words("exercise --n 2 --tests")).unwrap().display(), "custodian exercise run --n 2 --tests");
        assert_eq!(read(&words("exercise")).unwrap().display(), "custodian exercise run");
        assert!(read(&words("exercise --ops add_field,unroll")).unwrap_err().to_string().contains("\"unroll\" is not one of the ops"));
        assert!(read(&words("exercise --units item,trait")).is_err() && read(&words("exercise --n two")).is_err());
    }

    #[test]
    fn every_line_names_its_statuses() {
        let l = read(&words("check all")).unwrap();
        assert_eq!((l.status_word(0), l.status_word(1), l.status_word(2)), ("done", "findings", "refused"));
        let d = l.decide(1);
        assert_eq!((d.disposition, d.reason.as_str()), (xccute_runtime::ExitDisposition::Stop, "findings"));
        assert_eq!(l.preview().display(), "custodian check all");
        let j = read(&words("job close j-1")).unwrap();
        assert_eq!((j.status_word(2), j.path(), j.segment()), ("refused", "job close", "job"));
    }
}
