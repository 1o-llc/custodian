# How custodian composes the 1o crates

custodian is built to show that a whole program can be made from small libraries that each do one thing. Every
library below is used for what it already does; custodian declares nothing it could take from one of them. The
names are the exact Cargo package names.

| crate | what custodian uses it for | where |
|---|---|---|
| `custodian_kit` | the readings: a spec answered over klepto's scan of the workspace (`Workspace::open`, `run`, `Reading`, `Receipt`, `IndexKey`, `Refusal`, `bundle`), the declared routes (`Route`), the job lifecycle (`jobs::start`, `close`, `brief`, `run_template`), the count kind and its compare, the task records, the reading diff, the feature eligibility rule, the annotate plan, the folder's parts (`store::named`); 0.2.0: the one same-search key (`api::SearchKey`), the impact reading, the change spec and its operation vocabulary (`change`), the preflight and its gate (`preflight`), plans (`plan`), decisions (`questions`), linkage records (`linkage`), task states (`tasks`), bounded jobs (`jobs::JobMode`) | `src/readings.rs`, `src/jobs.rs`, `src/counting.rs`, `src/maint.rs`, `src/bundling.rs`, `src/layout.rs`, `src/changes.rs`, `src/agent.rs` |
| `task_history` | a job's typed 1o records (task, intent, solution, change, verification, result), read back for `job list`; through the kit, a decision asked is a `problem` record and its answer a `solution`, an impact a `diagnosis` | `src/history.rs` |
| `klepto` | (through custodian_kit) the one extractor: every item, use site, call and attribute of the workspace | |
| `simple_impl_derive` | every line declared once: the `custodian` program as a `CompositeShell`, its surfaces (`agent`, `job`, `test`, `diagnostics`, `count`, `self`, `mcp`, `plan`, `linkage`) as `CompositeSubCommand`s, one `SimpleSubCommand` leaf per line, each naming its exit statuses; the usage screen is its generated help lines (0.2.7) | `src/surface.rs` |
| `simple_impl` | (through custodian_kit and code_passenger) the toolkit a change spec's function bodies are written in: each piece one of its 0.1.11 macros, rendered as its line | |
| `xccute` | the job lock: `Hold` over a `LockFile`, walking the line it guards (0.3.1), and its named timings | `src/runtime.rs`, `src/timing.rs` |
| `xccute_contract` | the declared lines' contract: validation, preview, argv, exit statuses | `src/surface.rs`, `src/facade.rs` |
| `xccute_runtime` | every run executed through the runtime: `RuntimeOperation`, `RuntimeOperationPlan`, `ExitStatusPolicy`, `RuntimeOperationReceipt` | `src/facade.rs` |
| `xccute_commands` | the cargo test lines (`CargoBuilder`), nothing redeclared | `src/testing.rs` |
| `xccute_decisions` | the gates as declared decisions with observation tools: same search, a job running, a step's input | `src/decide.rs` |
| `uh-tom-mick` | the journal's sequence (`AtomicCounter`) and the process's one writer (`AtomicAdmission`) | `src/journal.rs`, `src/facade.rs` |
| `stable_digest` | every digest custodian takes, over one canonical form | `src/digest.rs` |
| `pathgen`, `pathgen-macros` | the `.custodian/` folder: each part an `ArtifactContract`, resolved by pathgen's dev policy | `src/layout.rs` |
| `waker_protocol`, `waker_tables` | the job queue: QoS by job kind, a checkpoint between a job's start and its close, restore after an interruption | `src/runtime.rs` |
| `ob-serve-a-bowl` | the observer set: changes published with a wake key, the `--observe` level as a cold observer | `src/observe.rs` |
| `mine` | the resident's owner-only Unix socket | `src/serve.rs` |
| `abut` | length-prefixed frames on that socket | `src/serve.rs` |
| `big-bad-bool` | option sets as packed words: the observe levels, the job kinds (the bundle filters are custodian_kit's word); through the kit, the preflight's word and the task states | `src/words.rs` |
| `flaga_solver` | each word's rules checked, a refusal naming the rule; through the kit, the rules that authorize a preflight | `src/words.rs` |
| `liaise_proc_macro` | one `liaise_codes!` table of refusals, prefix `CUST` | `src/codes.rs` |
| `installation` | `self install`: plan, install from the plan, receipt, uninstall from the receipt | `src/install.rs` |
| `maintenance` | `custodian check` as maintenance's own declared lines; the timing report | `src/maint.rs`, `src/testing.rs` |
| `code_passenger` | the writer: custodian never edits source; `dep add` (init --macros), `attr place` (annotate --apply), `gate` (features --apply), `passenger checkpoint` (a job's open and close), `transform` (a change spec made in the code, proved on a scratch copy first), as custodian_kit's declared `code-passenger --root R ...` line | `src/maint.rs`, `src/cli.rs`, `src/jobs.rs`, `src/changes.rs` |
| `bundles`, `classified` | (through custodian_kit) bundle files, and secret items held redacted | |
| `rusqlite` | the journal's SQLite file | `src/journal.rs` |

The libraries stay independently usable: custodian depends on each through its public API and couples none of
them to another.

His crates first: before anything from outside the `1o` set, the set was read for a crate that already does the job
(his crates on crates.io, read 2026-10-19). The 0.2.0 increment added no outside crate to custodian. code_passenger
added `syn` and `proc-macro2` (with `span-locations`), which klepto already builds on, to find an item's exact span
before it splices; `simple_impl` is a dev-dependency in the kit and code_passenger, so their tests compile the
toolkit lines a change spec renders.

Reuse by digest: a search is spent once per spec over one state of the code. The key is custodian_kit's
(`api::SearchKey`: the spec's hash and klepto's fingerprint of the code its scope reads); a bundle's filters are
part of its spec, so there is no second key. A preflight is found again by the change spec's digest and the code
digest, and a transform runs only on the preflight of both.
