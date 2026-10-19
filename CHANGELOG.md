# Changelog

## 0.1.0 (unreleased)

the first version, not published (the crates.io name `custodian` holds a 0.0.1 placeholder). the line links below are paths in this repository; they get pinned to a commit once it has a home on GitHub.

### why

an agent should read a repository before it works on it, ask what it must ask before the first edit, and leave a record that does not depend on any one conversation. custodian is that record's keeper: every run a declared line with a receipt, every reading dated and kept, every job a folder with a closing receipt. the reasons, in James Hunt's words: docs/SPEC-2026-10-19.md.

### since the first draft of this entry (the same day)

- jobs run through custodian_kit's job code (`jobs::start`, `close`, `brief`, `run_template`; a1ecffd): the command line keeps its declared lines and their receipts, and adds the passenger checkpoints, the intent the asker states (`job start --intent`, an intent record beside the kit's), the job events the observer set reads, and the queue. A job is a task history in James Hunt's typed 1o records (`task_history`, `.custodian/data/history/<task>/<revision>/`), written by the kit; a job name run again opens its next revision. `custodian exec-reading` is the kit's declared reading line, read back into the kit's own declaration and answered by the kit: src/jobs.rs, src/history.rs, src/cli.rs.
- the queue (src/runtime.rs) schedules queued job folders on waker_protocol by class (triage interactive, specs standard, features batch), one job at a time, two stages each (the kit's start, the kit's close) with a checkpoint between them in the journal; tests/runtime.rs interrupts a job after its start and resumes it at its close. scope is no longer a dependency: the kit runs a job's steps itself.
- the `.custodian/` parts the kit declares are the kit's descriptors (`store::named`), named here as types; only the parts the command line alone keeps (a reading's record, a check's JSON, a log read from stdin) are declared in src/layout.rs.
- `custodian count [ROUTE] [--by crate|language] [--tests]`, `count report A B` (a count named by its receipt number, a job, `<job>:open` or `<job>:close`, a commit or a date; a name that fits several is refused, CUST1205) and `count history [ROUTE]`, over custodian_kit's `count` kind and `count::compare_counts`: src/counting.rs. `diff` of two counts says what grew.
- `init --macros` runs `code-passenger dep add <crate> maintenance --path ...` (and big-bad-bool for a crate that declares a level word), `annotate --apply` runs `code-passenger attr place` per plan line, `features --apply` runs `code-passenger gate --write`, each through the executor with a receipt; `job start` and `job close` take a passenger checkpoint when code-passenger is installed, kept in the folder's `passenger.json`.
- found in custodian_kit a1ecffd, the kit's to fix: a folder-of-specs job writes `counts.json` into the folder before listing its specs and reads it as a JSON spec (CUST0002), so such a job is refused (tests/cli.rs `a_folder_of_specs_is_a_job_from_start_to_close` is ignored with that reason); a reading's receipt keeps the path it had in `jobs/running`, which moves to `jobs/done` at close, so a later reuse of it fails to copy (an imported job reads again).

### the surface

- every subcommand is a line declared once with simple_impl_derive 0.2.6 over xccute_contract 0.1.2: the `custodian` program a `CompositeShell`, one `SimpleSubCommand` leaf each, each naming its exit statuses (0 done, 1 findings, 2 refused): src/surface.rs. there is no hand parser and no clap: `surface::read` takes the words through the leaf's own fields and refuses a word no field takes; the line is then what runs, what is previewed and what the receipt keeps.
- a subcommand with actions (`job start`, `self install`, `diagnostics where`) is one leaf whose first word is checked by its `each` rule, the shape maintenance's `check <which>` has, because a nested surface does not yet carry a leaf's optional setters or its exit statuses.
- `tests::every_line_reads_back_from_its_own_words` builds every subcommand with its builder and reads its argv back to the same words.

### the facade and the receipt

- every run goes through xccute_runtime 0.1.2: the line's preview becomes a `RuntimeOperation` in a `RuntimeOperationPlan` (the job's, or `custodian`'s), the handler runs, the status is decided by the statuses the line names (`ExitStatusPolicy::of`), and the `RuntimeOperationReceipt` goes into custodian_kit's `Receipt` with the sequence number, the UTC time, the spec's digest, the code's index key, the outputs, the exit status and the job: src/facade.rs.
- a program custodian runs (cargo, git, code-passenger, custodian itself for a job's step) is an `External` line with a receipt of its own; maintenance's lines run through maintenance's runner in this process, as its runner declares them.
- every digest custodian takes is stable_digest's over one canonical form (`custodian.canonical.v1`, what is digested, the value as JSON with object keys sorted): src/digest.rs.
- refusals are one `liaise_codes!` table, prefix `CUST`, from 1001, so they never share a number with custodian_kit's `CUST0001` to `CUST0099`: src/codes.rs.

### the journal and the folder

- one SQLite file, `.custodian/state/journal.sqlite`, custodian_kit's journal extended: the kit's `runs` (the receipts), `jobs` and `tests` made by its `JOURNAL_SCHEMA` first, then custodian's readings index, events, test lines, install receipts and the queue's checkpoints (`journal::SCHEMA`); a `jobs` table the kit made gains three columns, nothing is dropped: src/journal.rs. `tests::a_journal_custodian_kit_made_is_extended_not_replaced`. the sequence is uh-tom-mick's `AtomicCounter`, brought up to the file's last receipt inside the writing transaction (`BEGIN IMMEDIATE`), so two processes never take one number; `tests::two_journals_on_one_file_never_take_one_number`.
- the `.custodian/` parts are pathgen-macros `ArtifactContract`s placed by one descriptor each and resolved through pathgen's dev policy: src/layout.rs. the templates are custodian_kit's, including the 1o job file (`v1.job.1o.job`) and paths table (`v1.paths.1o.paths`); `tests::the_kit_names_the_same_places` fails the moment the two drift.
- the two modes: walking up for `.custodian/`, a run is journaled; with none it is free mode and says so: src/mode.rs.

### readings

- `run` answers a spec file, a queued or done spec by name, or a search on the line (`--kind`), through custodian_kit's `Workspace` held open and reopened only when the index key moves; journaled, the reading is written dated beside the specs and indexed: src/readings.rs.
- the same-search check is an xccute decision (src/decide.rs): the same spec digest over the same index key is a reuse that says `search #N from <time>, same spec, same code: reused (--again reads again)`. the guides cover their paths; `tests::every_gate_covers_its_path`.
- `diff` compares two readings of a spec with custodian_kit's diff; `list` renders the inventory from the journal into `data/inventory.md`.

### jobs and the runner

- jobs are folders in `state/jobs/{queued,running,done}`: a folder of specs, or a template's kind (`triage` is custodian_kit's, `test` and `check` ship here); `job start`, `close`, `brief`, `import`, `list`: src/jobs.rs. readings run while a job is open are tagged with it; the closing receipt (`closing.md`, `closing.json`) lists the steps, the specs read, the reuses, what changed since each spec was last read, and the tests run.
- the runner: each job a waker_protocol 0.3.2 task, its QoS its kind's (triage interactive, specs standard, test and check batch); each poll one stage, then a checkpoint saved through the journal's `checkpoints` table, the task restored from those bytes for the next stage: src/runtime.rs. a stage's steps are child processes polled as scope 0.3.1 tasks, so they run at once and are recorded in step order. a step that reads an earlier step's receipt runs only when that step gave it something (the step-input decision). a template step's spec is written into the job's `steps/` in 1o (`v1.<name>.1o.reading`), as custodian_kit writes one; a value 1o cannot carry keeps it in TOML.
- tests/runtime.rs stops the runner after one stage, drops it, starts again in a new context, and checks the job resumes at stage 2 of 3 without running stage 1 again.

### the observer set, the resident

- ob-serve-a-bowl 0.2.2: a run's state is a cold observable, each change published with a wake key whose field mask says what changed, the `--observe` level the cold observer that decides what is shown: src/observe.rs. `watch` replays the journal's events through it.
- `serve` binds mine 0.3.2's owner-only socket and answers abut 0.2.2 frames of JSON: one request kind (`run_spec`, `run_job`, `import_bundle`, `status`, `stop`), one receipt in the reply, the workspace held open between requests: src/serve.rs. the command line tries the socket first and does the work itself when nothing answers. tests/golden/request_run_spec.frame pins one request's bytes.

### tests, checks, the maintenance loop

- `test`: the change since a ref through custodian_kit's declared git line, the affected_tests reading, one xccute_commands `CargoBuilder` line per package and target, each with a receipt, its outcomes through the observer, its counts and wall time in the journal; `test report` joins maintenance's timed report when the workspace has one: src/testing.rs.
- `check` runs maintenance's line; `annotate` writes the plan from the latest check receipt and the timings; `features` writes code_passenger's `klepto.toml` from custodian_kit's eligibility reading and `--apply` runs `code-passenger gate --write`: src/maint.rs.
- `diagnostics` and `triage` read custodian_kit's `diagnostics` kind and run the triage template: src/diag.rs. `bundle` hands a route to custodian_kit's bundle with its filters word, or a job to its job bundler: src/bundling.rs.
- `self install` plans, installs from the plan and keeps installation 0.2.2's receipt, in the journal too: src/install.rs.
- hooks/pre_edit_reach.sh and hooks/settings.snippet.json: before an edit, the reach of the file. not enabled anywhere.

### what is refused, and why

- `init --macros`: code_passenger's cargo editing marks existing dependencies optional and sets features; it has no line that adds one, and custodian never edits a manifest.
- `annotate --apply`: code_passenger's writers are file headers, `#[cfg]` gates and manifest features; none places an attribute from a plan. the plan is kept.

### proof

- `cargo test`: 36 tests pass, 1 ignored (the kit's folder-of-specs refusal), on stable 1.97.1 against custodian_kit a1ecffd and task_history f0677e4: 15 unit tests, tests/cli.rs (19 of 20, every subcommand on the fixture project in tests/fixtures/ws, through the built program; the writing tests need code-passenger on PATH), tests/runtime.rs (2).
- `cargo clippy --all-targets`: no warnings.
