# Changelog

## 0.3.0 (2026-10-19)

custodian on custodian_kit 0.3.0 and the history crates it split into. The
machinery custodian held for receipts and counts now lives in those crates,
and custodian calls them. What it does on the command line is unchanged.
Every link is pinned to a216685; the earlier lines are pinned to cd22817
(0.2.0).

- The journal ([src/journal.rs#L224](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/journal.rs#L224)) is receipt_history's journal with
  custodian's tables on its connection: `readings`, `events`,
  `test_runs`, `installs`, `checkpoints`.
  - Keeping a receipt ([src/journal.rs#L247](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/journal.rs#L247)) and reading receipts are
    receipt_history's. Keeping takes the number, writes the file and inserts
    the row in one writing transaction.
  - Rebuilding the `runs` from the receipt files is receipt_history's too.
    custodian indexes each rebuilt reading inside the same transaction, then
    puts back the job folders the journal does not know
    ([src/journal.rs#L452](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/journal.rs#L452)).
  - Before, all of this was custodian's own: [src/journal.rs#L245](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/journal.rs#L245),
    [src/journal.rs#L274](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/journal.rs#L274), [src/journal.rs#L534](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/journal.rs#L534) and
    [src/journal.rs#L605](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/journal.rs#L605).
  - A refusal from receipt_history reads as custodian's `CUST1301` (the
    journal) or its write code ([src/journal.rs#L153](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/journal.rs#L153)).
  - One behaviour changes. A receipt file now ends with a newline, as
    custodian_kit's receipt files always did.
- The counts are tree_history's.
  - The count table and the diff table are re-exported where they were
    ([src/counting.rs#L21](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/counting.rs#L21)).
  - Reading every count from the journal ([src/counting.rs#L49](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/counting.rs#L49)) and naming
    one by its receipt number, job, commit or date ([src/counting.rs#L54](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/counting.rs#L54))
    call tree_history's `counts_in` and `resolve`.
  - These were [src/counting.rs#L50](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/counting.rs#L50), [src/counting.rs#L87](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/counting.rs#L87),
    [src/counting.rs#L116](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/counting.rs#L116) and [src/counting.rs#L152](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/counting.rs#L152).
- `custodian count history` answers through differences_kit's
  `count_history` ([src/counting.rs#L103](https://github.com/1o-llc/custodian/blob/a2166857abd1b2611161b98d6099fdb188449163/src/counting.rs#L103)), the library the `differences`
  command line is. Before, it was [src/counting.rs#L232](https://github.com/1o-llc/custodian/blob/cd2281740ae5a79290311217c9238509dcc8cf1f/src/counting.rs#L232).
- The test `two_journals_on_one_file_never_take_one_number` moved with the
  sequence it proves to receipt_history's tests/journal.rs, where it passes.
  `a_journal_custodian_kit_made_is_extended_not_replaced` stays here: it
  proves custodian's tables sit beside receipt_history's in one file.
- New dependencies: receipt_history, tree_history and differences_kit, each
  0.1.0.

Proof: `cargo test`, 54 tests. That is every test of 0.2.0 but the one that
moved. The fixture project runs every line through the built program, the
MCP server over stdio, and the bounded-mode tests with code-passenger.## 0.2.0 (2026-10-19)

bounded and unbounded jobs, the 2026-10-19 specification's second section, on custodian_kit 0.2.0 and with code_passenger 0.4.0 as the writer. what the specification asks, row by row, and where each is met: [docs/spec-gap-2026-10-19.md](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/docs/spec-gap-2026-10-19.md). every link is pinned to 6ac3f6e.

### why

an agent that starts a change before it understands it meets its questions halfway through, and the person who handed it the task has to rebuild the context. 0.2.0 moves the understanding to the front: what a change entails and costs is one reading, a change is a spec decided before anything is written, the questions are asked together and never twice, and a job says up front what done looks like. in a bounded job the agent writes no code at all: custodian writes it through code_passenger, and every line the job touched must be one of its transforms.

### the two fixes and the brief

- a fresh checkout rebuilds its journal from the receipts and job folders it carries, at every open ([src/journal.rs#L534](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/journal.rs#L534), called at [src/facade.rs#L152](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/facade.rs#L152)); `list` says what it put back ([src/readings.rs#L252](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/readings.rs#L252)). before, the journal was opened as an empty file ([src/journal.rs#L245](https://github.com/1o-llc/custodian/blob/4180e64e612f1e41bd3c16c999049be9998e067b/src/journal.rs#L245)): the kit's checkout had 67 receipt files and a journal of 31 rows. on the kit, the first `list` put back 36 receipts, 16 readings and 2 jobs (receipt #68). `the_journal_is_rebuilt_from_the_receipts_a_checkout_carries` ([tests/modes.rs#L22](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L22)).
- `list` names the job each reading was made in and counts the specs inside job folders ([src/readings.rs#L246](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/readings.rs#L246); before, [src/readings.rs#L253](https://github.com/1o-llc/custodian/blob/4180e64e612f1e41bd3c16c999049be9998e067b/src/readings.rs#L253) read the readings table and `data/specs` only). `list_names_the_job_of_each_reading_and_counts_the_specs_in_jobs` ([tests/modes.rs#L40](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L40)).
- `custodian agent brief [--head]`, the first line an agent runs: the mode first, the decisions to make first, the open job and what it waits on, the work not completed, the latest reading of each spec with its age and whether the code moved since, the test lines, what the records saved (runs, searches reused, preflight time, decisions asked before and after a job changed code, completion), and the next lines ([src/agent.rs#L80](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/agent.rs#L80), [src/agent.rs#L191](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/agent.rs#L191)). `the_agent_brief_says_the_mode_first_and_the_guard_answers_the_hook` ([tests/modes.rs#L51](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L51)).

### modes

- `job start --mode bounded|unbounded`, and `init --job-mode` for the workspace's default. `custodian agent guard FILE` ([src/agent.rs#L279](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/agent.rs#L279)) refuses a hand edit of code in a bounded job, and the pre-edit hook asks it first and blocks the edit with its words (exit 2, [hooks/pre_edit_reach.sh#L7](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/hooks/pre_edit_reach.sh#L7)). a bounded job's close refuses lines no transform covers, CUST0031, and closes it `rejected`. `a_bounded_job_writes_its_code_through_a_change_spec_and_closes_accepted` ([tests/modes.rs#L115](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L115)), `a_bounded_job_with_a_hand_edit_is_refused_at_close_and_rejected` ([tests/modes.rs#L144](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L144)), `a_bounded_job_needs_its_writer` ([tests/modes.rs#L176](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L176)). CI builds code-passenger beside custodian so these run there ([.github/workflows/ci.yml#L46](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/.github/workflows/ci.yml#L46)).
- `job start --verify LINE` and `job verify LINE`: what done looks like. a job with none closes `pending`, never `accepted`; an unbounded job's `pending` exits 0 (help), a bounded one's 1 ([src/jobs.rs#L284](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/jobs.rs#L284)).

### change specs, plans, decisions, linkage

- `custodian preflight SPEC` ([src/changes.rs#L53](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/changes.rs#L53)) and `custodian transform SPEC [--write]`, `transform --revert RECORD` ([src/changes.rs#L76](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/changes.rs#L76)), the second run through code_passenger's declared line on the preflight receipt of this spec over this code. on the kit, a field added to `nodeplan_sdk::uds_contract::UdsRouteTable` was not authorized, its default reaching 18 files of other crates (receipts #74–80).
- `custodian plan make INTENT`, `plan approve PLAN STEP`, `plan show PLAN` ([src/changes.rs#L137](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/changes.rs#L137)). `an_intent_is_planned_and_a_step_becomes_a_change_spec` ([tests/modes.rs#L85](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L85)).
- `custodian decide [ID ANSWER --why TEXT]` ([src/changes.rs#L181](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/changes.rs#L181)): the open decisions together; one answered, kept in the task history, never asked again; the answer to a job's "what does done look like" declares its verification. `a_decision_is_asked_once_and_its_answer_declares_what_done_looks_like` ([tests/modes.rs#L67](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L67)).
- `custodian linkage add NAME --kind K --why TEXT --bind A --bind B`, `linkage check [TARGET]` ([src/changes.rs#L214](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/changes.rs#L214)). `a_linkage_record_is_added_and_checked` ([tests/modes.rs#L102](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L102)).

### one key

- the same-search key is custodian_kit's (`api::SearchKey`, [src/readings.rs#L99](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/readings.rs#L99)), looked up over the receipts ([src/journal.rs#L359](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/journal.rs#L359)); custodian's own digest of a spec and an option word is gone, the option word now part of the spec. a search on the line is the kit's one search spec ([src/readings.rs#L42](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/readings.rs#L42)), the same a preflight or a plan makes, so each reuses the others. a receipt written by 0.1.0 has the old key, so each spec is read once more under 0.2.0: on the kit, `dependents` of fleet_commands read again (#71) and reused (#72). the receipt's `spec_hash` is `sha256:`-prefixed now ([tests/cli.rs#L46](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/cli.rs#L46)).

### the lines as surfaces

- the lines are nested surfaces (simple_impl_derive 0.2.7: `agent`, `job`, `test`, `diagnostics`, `count`, `self`, `mcp`, `plan`, `linkage`), declared once ([src/surface.rs#L1343](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/surface.rs#L1343)), and the usage screen is their generated help lines ([src/surface.rs#L1389](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/surface.rs#L1389)), held by `the_help_lines_are_generated_from_the_declarations` ([tests/modes.rs#L204](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L204)). `count [ROUTE]`, `test [--all]` and `plan INTENT` still read as `count of`, `test run`, `plan make` ([src/surface.rs#L1416](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/surface.rs#L1416)).
- removed from the library's public surface, because each was a flattened 0.1.0 leaf the surfaces replace: `surface::Agent`, `Count`, `Diagnostics`, `Job`, `Mcp`, `SelfLine` and `Test` with their fields, builders, `*Rooted` types and `DECLARED`; `AGENT_ACTIONS`, `DIAGNOSTICS_ACTIONS`, `JOB_ACTIONS`, `MCP_ACTIONS`, `SELF_ACTIONS`, `TEST_ACTIONS` and `USAGE`; `Line::Agent`, `Count`, `Diagnostics`, `Job`, `Mcp`, `SelfCmd`, `Test` (one variant per line now: `Line::JobStart`, `Line::CountOf`, ...); `mcp::ToolLine.segment` and `.fixed` (a tool names its line's path now). changed, for the same reason: `Custodian::agent`, `count`, `diagnostics`, `job`, `mcp`, `self_` and `test` take no action word and return the surface; `counting::count` takes `&CountOf`, `install::self_line` a `SelfAsk`. 150 items in all, by rustdoc JSON of 4180e64 against 6ac3f6e; 1326 added. `cli::init` keeps its 0.1.0 signature ([src/cli.rs#L203](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/cli.rs#L203)); `init_with` takes the job mode. the 0.1.0 surface is kept in git at 4180e64.
- MCP tools are named by their line's path (`job_start` runs `job start`), every declared line a tool's or a person's ([src/mcp.rs#L59](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/mcp.rs#L59), [src/mcp.rs#L93](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/mcp.rs#L93)); new tools `agent_brief`, `job_verify`, `test_report`, `preflight`, `transform`, `plan_make`, `plan_approve`, `plan_show`, `decide`, `linkage_add`, `linkage_check`. `the_tool_list_is_the_declared_surface` ([src/mcp.rs#L525](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/mcp.rs#L525)).

### the rest

- `job start`, `job close` and the queue hold `.custodian/jobs.lock` through xccute 0.3.1's `Hold` over a `LockFile` ([src/runtime.rs#L37](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/runtime.rs#L37)); every wait is a named value ([src/timing.rs#L11](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/src/timing.rs#L11)). `two_job_starts_at_once_open_one_job` ([tests/modes.rs#L187](https://github.com/1o-llc/custodian/blob/6ac3f6e9f9f0d4f86cc44f8431aba9bddc42877c/tests/modes.rs#L187)).
- custodian_kit 0.2.0, simple_impl_derive 0.2.7, xccute 0.3.1, xccute_contract 0.1.3, xccute_decisions 0.2.1; `rust-version` 1.89, and CI tests on it.
- every new file carries the notice Copyright © 2026 1o. All rights reserved.; the existing notices are as they were.
- 55 tests: 19 unit, 21 in tests/cli.rs, 11 in tests/modes.rs, 2 in tests/mcp.rs, 2 in tests/runtime.rs.

## 0.1.0 (2026-10-19)

the first version (the crates.io name `custodian` held a 0.0.1 placeholder). every path below links to the file at [da30d11](https://github.com/1o-llc/custodian/tree/da30d116047660c660f2818a00f2cadb12a73696), the commit this entry describes.

### why

an agent should read a repository before it works on it, ask what it must ask before the first edit, and leave a record that does not depend on any one conversation. custodian is that record's keeper: every run a declared line with a receipt, every reading dated and kept, every job a folder with a closing receipt. the reasons, in James Hunt's words: [docs/SPEC-2026-10-19.md](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/docs/SPEC-2026-10-19.md).

### since the first draft of this entry (the same day)

- jobs run through custodian_kit's job code (`jobs::start`, `close`, `brief`, `run_template`; a1ecffd): the command line keeps its declared lines and their receipts, and adds the passenger checkpoints, the intent the asker states (`job start --intent`, an intent record beside the kit's), the job events the observer set reads, and the queue. A job is a task history in James Hunt's typed 1o records (`task_history`, `.custodian/data/history/<task>/<revision>/`), written by the kit; a job name run again opens its next revision. `custodian exec-reading` is the kit's declared reading line, read back into the kit's own declaration and answered by the kit: [src/jobs.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/jobs.rs), [src/history.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/history.rs), [src/cli.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/cli.rs).
- the queue ([src/runtime.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/runtime.rs)) schedules queued job folders on waker_protocol by class (triage interactive, specs standard, features batch), one job at a time, two stages each (the kit's start, the kit's close) with a checkpoint between them in the journal; [tests/runtime.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/runtime.rs) interrupts a job after its start and resumes it at its close. scope is no longer a dependency: the kit runs a job's steps itself.
- the `.custodian/` parts the kit declares are the kit's descriptors (`store::named`), named here as types; only the parts the command line alone keeps (a reading's record, a check's JSON, a log read from stdin) are declared in [src/layout.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/layout.rs).
- `custodian count [ROUTE] [--by crate|language] [--tests]`, `count report A B` (a count named by its receipt number, a job, `<job>:open` or `<job>:close`, a commit or a date; a name that fits several is refused, CUST1205) and `count history [ROUTE]`, over custodian_kit's `count` kind and `count::compare_counts`: [src/counting.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/counting.rs). `diff` of two counts says what grew.
- `init --macros` runs `code-passenger dep add <crate> maintenance --path ...` (and big-bad-bool for a crate that declares a level word), `annotate --apply` runs `code-passenger attr place` per plan line, `features --apply` runs `code-passenger gate --write`, each through the executor with a receipt; `job start` and `job close` take a passenger checkpoint when code-passenger is installed, kept in the folder's `passenger.json`.
- bound to custodian_kit 9a176fc and to the crates.io `maintenance` 0.1.0 and `task_history` 0.1.0 it uses. A job's readings are answered by the kit's in-process runner over the workspace custodian holds open (`jobs::start_with`, `close_with`) when no resident answers for the project, and by its line runner (`custodian exec-reading`) inside the resident; every receipt says which (`runner`: `in_process` or `line`), custodian's own lines `in_process`, a program's `line`. A passenger checkpoint code_passenger cannot take is said and does not stop the job.
- custodian_kit fed6c5d fixed what a1ecffd got wrong: a job's specs are its spec files only (`jobs::job_specs`), and a job's move (`jobs::move_job`) rewrites every receipt's paths, so a folder-of-specs job runs from start to close and a reading's receipt names where its file is now ([tests/cli.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/cli.rs) `a_folder_of_specs_is_a_job_from_start_to_close`, and every receipt of the job checked). custodian moves no job folder itself; its `job start` receipt names the job file inside the folder, so the kit's move rewrites it with the rest.

- the completeness pass against the contract (DIRECTIONS part 6), each with its test in [tests/cli.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/cli.rs):
  - a job's closing receipt lists what ran while the job was open, from the command line too: a receipt's
    `command` is custodian_kit's word (`reading` for a run that answered a spec, read or reused; `test` for a cargo
    test line; `git`, `gate`), declared once in [src/facade.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/facade.rs) (`KIT_COMMANDS`, `command_word`). Before, a
    `custodian run` inside an open job was kept as `run` and a cargo line as `cargo.test`, so the kit's close
    listed neither the reading nor the tests. A test's outcomes are kept in cargo's words (`ok`, `FAILED`,
    `ignored`), as the kit's own test runner keeps them (custodian_kit 0.1.1 reads `FAILED` as a failure). A check's
    receipt is named by the check, so its verification record reads `custodian check all`.
    `a_jobs_closing_receipt_lists_what_ran_while_it_was_open`.
  - `check` passes `--registry`, so the line writes `.maintenance/registry.json`, the one registry file the kit's
    readings read each item's level, owner and rules from, and says where it is.
  - `job brief [ID] --bundle`: the brief, and the closed job packed as `custodian bundle <job>` packs it (secret
    items held redacted), from one line; `--bundle` is refused on any other job action.
  - `bundle --no-benches` and `--no-examples`: the two filters custodian_kit's word has that the line did not take.
  - `init --macros` adds `maintenance@0.1.0` from crates.io; it named a path on one machine.
  - `features` shows each dependency's scope depth and unavailable list (the lines its gate header carries) and
    writes them beside the spec (`data/plans/features-unavailable.toml`), as custodian_kit's `features` does.
  - the pre-edit hook's filter prints a reach reading's entries section too. Its own reading is a search on the
    line, which carries no declaration sources (`[[declared_in]]`), so it finds no entries yet: where a project
    declares the key that names a binary is still open.
  - CI clones custodian_kit from github.com/1o-llc (the only crate custodian takes by path); the repository field
    names github.com/1o-llc/custodian.

### the surface

- every subcommand is a line declared once with simple_impl_derive 0.2.6 over xccute_contract 0.1.2: the `custodian` program a `CompositeShell`, one `SimpleSubCommand` leaf each, each naming its exit statuses (0 done, 1 findings, 2 refused): [src/surface.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/surface.rs). there is no hand parser and no clap: `surface::read` takes the words through the leaf's own fields and refuses a word no field takes; the line is then what runs, what is previewed and what the receipt keeps.
- a subcommand with actions (`job start`, `self install`, `diagnostics where`) is one leaf whose first word is checked by its `each` rule, the shape maintenance's `check <which>` has, because a nested surface does not yet carry a leaf's optional setters or its exit statuses.
- `tests::every_line_reads_back_from_its_own_words` builds every subcommand with its builder and reads its argv back to the same words.

### the facade and the receipt

- every run goes through xccute_runtime 0.1.2: the line's preview becomes a `RuntimeOperation` in a `RuntimeOperationPlan` (the job's, or `custodian`'s), the handler runs, the status is decided by the statuses the line names (`ExitStatusPolicy::of`), and the `RuntimeOperationReceipt` goes into custodian_kit's `Receipt` with the sequence number, the UTC time, the spec's digest, the code's index key, the outputs, the exit status and the job: [src/facade.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/facade.rs).
- a program custodian runs (cargo, git, code-passenger, custodian itself for a job's step) is an `External` line with a receipt of its own; maintenance's lines run through maintenance's runner in this process, as its runner declares them.
- every digest custodian takes is stable_digest's over one canonical form (`custodian.canonical.v1`, what is digested, the value as JSON with object keys sorted): [src/digest.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/digest.rs).
- refusals are one `liaise_codes!` table, prefix `CUST`, from 1001, so they never share a number with custodian_kit's `CUST0001` to `CUST0099`: [src/codes.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/codes.rs).

### the journal and the folder

- one SQLite file, `.custodian/state/journal.sqlite`, custodian_kit's journal extended: the kit's `runs` (the receipts), `jobs` and `tests` made by its `JOURNAL_SCHEMA` first, then custodian's readings index, events, test lines, install receipts and the queue's checkpoints (`journal::SCHEMA`); a `jobs` table the kit made gains three columns, nothing is dropped: [src/journal.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/journal.rs). `tests::a_journal_custodian_kit_made_is_extended_not_replaced`. the sequence is uh-tom-mick's `AtomicCounter`, brought up to the file's last receipt inside the writing transaction (`BEGIN IMMEDIATE`), so two processes never take one number; `tests::two_journals_on_one_file_never_take_one_number`.
- the `.custodian/` parts are pathgen-macros `ArtifactContract`s placed by one descriptor each and resolved through pathgen's dev policy: [src/layout.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/layout.rs). the templates are custodian_kit's, including the 1o job file (`v1.job.1o.job`) and paths table (`v1.paths.1o.paths`); `tests::the_kit_names_the_same_places` fails the moment the two drift.
- the two modes: walking up for `.custodian/`, a run is journaled; with none it is free mode and says so: [src/mode.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/mode.rs).

### readings

- `run` answers a spec file, a queued or done spec by name, or a search on the line (`--kind`), through custodian_kit's `Workspace` held open and reopened only when the index key moves; journaled, the reading is written dated beside the specs and indexed: [src/readings.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/readings.rs).
- the same-search check is an xccute decision ([src/decide.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/decide.rs)): the same spec digest over the same index key is a reuse that says `search #N from <time>, same spec, same code: reused (--again reads again)`. the guides cover their paths; `tests::every_gate_covers_its_path`.
- `diff` compares two readings of a spec with custodian_kit's diff; `list` renders the inventory from the journal into `data/inventory.md`.

### jobs and the runner

- jobs are folders in `state/jobs/{queued,running,done}`: a folder of specs, or a template's kind (`triage` is custodian_kit's, `test` and `check` ship here); `job start`, `close`, `brief`, `import`, `list`: [src/jobs.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/jobs.rs). readings run while a job is open are tagged with it; the closing receipt (`closing.md`, `closing.json`) lists the steps, the specs read, the reuses, what changed since each spec was last read, and the tests run.
- the runner: each job a waker_protocol 0.3.2 task, its QoS its kind's (triage interactive, specs standard, test and check batch); each poll one stage, then a checkpoint saved through the journal's `checkpoints` table, the task restored from those bytes for the next stage: [src/runtime.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/runtime.rs). a stage's steps are child processes polled as scope 0.3.1 tasks, so they run at once and are recorded in step order. a step that reads an earlier step's receipt runs only when that step gave it something (the step-input decision). a template step's spec is written into the job's `steps/` in 1o (`v1.<name>.1o.reading`), as custodian_kit writes one; a value 1o cannot carry keeps it in TOML.
- [tests/runtime.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/runtime.rs) stops the runner after one stage, drops it, starts again in a new context, and checks the job resumes at stage 2 of 3 without running stage 1 again.

### the observer set, the resident

- ob-serve-a-bowl 0.2.2: a run's state is a cold observable, each change published with a wake key whose field mask says what changed, the `--observe` level the cold observer that decides what is shown: [src/observe.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/observe.rs). `watch` replays the journal's events through it.
- `serve` binds mine 0.3.2's owner-only socket and answers abut 0.2.2 frames of JSON: one request kind (`run_spec`, `run_job`, `import_bundle`, `status`, `stop`), one receipt in the reply, the workspace held open between requests: [src/serve.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/serve.rs). the command line tries the socket first and does the work itself when nothing answers. [tests/golden/request_run_spec.frame](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/golden/request_run_spec.frame) pins one request's bytes.

### the MCP server

- `custodian mcp serve [--root DIR]` (feature `mcp`, on by default) serves MCP on stdin and stdout with rmcp 3.5.1, the official Rust SDK: [src/mcp.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/mcp.rs). why: an agent's host speaks MCP, and the lines it should run are already declared; the tools are made from those declarations so there is no second surface to drift.
- each leaf in [src/surface.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/surface.rs) is now declared inside `leaf!`, which hands the struct to simple_impl_derive unchanged and keeps what it says as a `Declared` table (the doc, the segment, each field's name, type, `///`, how it is written and its `each` check); the word-set checks are declared with `word_sets!`, which also answers `words_of(check)`. a tool's input schema is that table read field by field; `surface::tests::every_leaf_keeps_its_declaration` holds the tables to the leaves.
- `mcp::TOOLS` names the agent's lines (19 tools, with the action words a tool fixes and the fields it leaves out: `annotate` and `features` are plans only) and `mcp::A_PERSONS` the lines that are a person's (`init`, `agent`, `sweep`, `serve`, `stop`, `watch`, `self`, `mcp`). `mcp::tests::the_tool_list_is_the_declared_surface` refuses a segment that is neither or both, a fixed or hidden name that is not a field, and a list that is not [tests/golden/mcp_tools.json](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/golden/mcp_tools.json).
- a call is written as the declared line's words, read back through `surface::read`, sent to the resident when one answers (`serve::ask_resident`, the socket path without printing: stdout is the protocol), else run through `cli::run_line` on the one thread that holds the project's `Ctx`. the result is the run's JSON (receipt, receipt file, the reading's path and `custodian://readings/<seq>`, said, live, refusal) and the reading's markdown inline up to `mcp::INLINE_CAP` (64 KiB). a refused run's `Outcome` now carries its `refusal` (the liaise code), also over the socket; the arguments that do not make a line are an MCP error with the code in `data` (CUST1001 from the reader, CUST1903 to CUST1906 for the server's own: no such tool, arguments, resource, connection).
- resources: `custodian://readings/<seq>` (the journal's reading index) and `custodian://job/<id>/brief` (a closed job's brief, through `jobs::brief`), listed and as templates.
- not installed or registered anywhere; the README's MCP section has the stdio command and an example host entry.

### tests, checks, the maintenance loop

- `test`: the change since a ref through custodian_kit's declared git line, the affected_tests reading, one xccute_commands `CargoBuilder` line per package and target, each with a receipt, its outcomes through the observer, its counts and wall time in the journal; `test report` joins maintenance's timed report when the workspace has one: [src/testing.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/testing.rs).
- `check` runs maintenance's line; `annotate` writes the plan from the latest check receipt and the timings; `features` writes code_passenger's `klepto.toml` from custodian_kit's eligibility reading and `--apply` runs `code-passenger gate --write`: [src/maint.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/maint.rs).
- `diagnostics` and `triage` read custodian_kit's `diagnostics` kind and run the triage template: [src/diag.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/diag.rs). `bundle` hands a route to custodian_kit's bundle with its filters word, or a job to its job bundler: [src/bundling.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/bundling.rs).
- `self install` plans, installs from the plan and keeps installation 0.2.2's receipt, in the journal too: [src/install.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/src/install.rs).
- [hooks/pre_edit_reach.sh](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/hooks/pre_edit_reach.sh) and [hooks/settings.snippet.json](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/hooks/settings.snippet.json): before an edit, the reach of the file. not enabled anywhere.

### what is refused, and why

- `init --macros`: code_passenger's cargo editing marks existing dependencies optional and sets features; it has no line that adds one, and custodian never edits a manifest.
- `annotate --apply`: code_passenger's writers are file headers, `#[cfg]` gates and manifest features; none places an attribute from a plan. the plan is kept.

### proof

- `cargo test`: 43 tests pass on stable 1.97.1 against custodian_kit 9a176fc: 19 unit tests, [tests/mcp.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/mcp.rs) (2), [tests/cli.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/cli.rs) (20, every subcommand on the fixture project in tests/fixtures/ws, through the built program; the writing tests need code-passenger on PATH), [tests/runtime.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/runtime.rs) (2).
- the MCP server (2026-10-19, against custodian_kit fed6c5d): 4 more unit tests (`surface::tests::every_leaf_keeps_its_declaration`, `mcp::tests::the_tool_list_is_the_declared_surface`, `arguments_are_written_as_the_declared_line`, `resource_addresses_read_back`) and [tests/mcp.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/mcp.rs) (2: rmcp's client in this process lists the tools, calls `count` and `run --kind reach` on the fixture, reads the reading as a resource, and gets CUST1001, CUST1904, CUST1905 and a refused `diff`'s CUST1202 back; and the built program's `mcp serve --root` over its stdio, its `run` answered by a running `custodian serve`). the whole suite: 19 unit, [tests/cli.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/cli.rs) 20, [tests/mcp.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/mcp.rs) 2, [tests/runtime.rs](https://github.com/1o-llc/custodian/blob/da30d116047660c660f2818a00f2cadb12a73696/tests/runtime.rs) 2, all pass; `cargo build --no-default-features` builds without rmcp.
- `cargo clippy --all-targets`: no warnings.
