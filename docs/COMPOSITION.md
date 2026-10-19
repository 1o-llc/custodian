# How custodian composes the 1o crates

custodian is built to show that a whole program can be made from small libraries that each do one thing. Every
library below is used for what it already does; custodian declares nothing it could take from one of them. The
names are the exact Cargo package names.

| crate | what custodian uses it for | where |
|---|---|---|
| `custodian_kit` | the readings: a spec answered over klepto's scan of the workspace (`Workspace::open`, `run`, `Reading`, `Receipt`, `IndexKey`, `Refusal`, `bundle`), the declared routes (`Route`), the job templates and job files, the reading diff, the feature eligibility rule, the annotate plan | `src/readings.rs`, `src/jobs.rs`, `src/maint.rs`, `src/bundling.rs` |
| `klepto` | (through custodian_kit) the one extractor: every item, use site, call and attribute of the workspace | |
| `simple_impl_derive` | every subcommand declared once: the `custodian` program as a `CompositeShell`, one `SimpleSubCommand` leaf per subcommand, each naming its exit statuses | `src/surface.rs` |
| `xccute_contract` | the declared lines' contract: validation, preview, argv, exit statuses | `src/surface.rs`, `src/facade.rs` |
| `xccute_runtime` | every run executed through the runtime: `RuntimeOperation`, `RuntimeOperationPlan`, `ExitStatusPolicy`, `RuntimeOperationReceipt` | `src/facade.rs` |
| `xccute_commands` | the cargo test lines (`CargoBuilder`), nothing redeclared | `src/testing.rs` |
| `xccute_decisions` | the gates as declared decisions with observation tools: same search, a job running, a step's input | `src/decide.rs` |
| `uh-tom-mick` | the journal's sequence (`AtomicCounter`) and the process's one writer (`AtomicAdmission`) | `src/journal.rs`, `src/facade.rs` |
| `stable_digest` | every digest custodian takes, over one canonical form | `src/digest.rs` |
| `pathgen`, `pathgen-macros` | the `.custodian/` folder: each part an `ArtifactContract`, resolved by pathgen's dev policy | `src/layout.rs` |
| `scope` | a stage's steps as structured tasks on one thread, recorded in a fixed order | `src/runtime.rs` |
| `waker_protocol`, `waker_tables` | the job queue: QoS by job kind, a checkpoint after each stage, restore after an interruption | `src/runtime.rs` |
| `ob-serve-a-bowl` | the observer set: changes published with a wake key, the `--observe` level as a cold observer | `src/observe.rs` |
| `mine` | the resident's owner-only Unix socket | `src/serve.rs` |
| `abut` | length-prefixed frames on that socket | `src/serve.rs` |
| `big-bad-bool` | option sets as packed words: the observe levels, the job kinds (the bundle filters are custodian_kit's word) | `src/words.rs` |
| `flaga_solver` | each word's rules checked, a refusal naming the rule | `src/words.rs` |
| `liaise_proc_macro` | one `liaise_codes!` table of refusals, prefix `CUST` | `src/codes.rs` |
| `installation` | `self install`: plan, install from the plan, receipt, uninstall from the receipt | `src/install.rs` |
| `maintenance` | `custodian check` as maintenance's own declared lines; the timing report | `src/maint.rs`, `src/testing.rs` |
| `code_passenger` | the writer: custodian never edits source; `features --apply` runs `code-passenger gate` | `src/maint.rs` |
| `bundles`, `classified` | (through custodian_kit) bundle files, and secret items held redacted | |
| `rusqlite` | the journal's SQLite file | `src/journal.rs` |

The libraries stay independently usable: custodian depends on each through its public API and couples none of
them to another.
