# custodian

A command line that makes a coding agent read a Rust repository before it works on it, and keeps a record of
what it read.

## Why it exists

custodian began as the answer to two problems that come from working with coding agents.

The first is planning debt. An agent starts a substantial task before it understands the repository: where a
thing is used, what depends on it, which features gate it, what a change would reach. Partway through, it meets a
question it cannot answer or a decision it needs from the person who gave it the task. Until then the person
believes the work is moving toward completion, when it was contingent on questions that could have been found
before the first edit.

The second is the cost of context switching. A person who has explained a problem in depth hands it off and moves
to another agent, another repository, another problem. When the first agent stops with a late question, the person
has to rebuild the context of a task they had already delegated: the architecture, the decisions, the state of the
work, what is being asked. Across several agents and projects this becomes a pile of partly finished work that was
believed to be handled.

The principle custodian is built on: **the user should not have to serve as the persistent memory, task
coordinator, or recovery mechanism for the agents.** custodian moves that responsibility into durable, inspectable,
repository-aware software. It is a repository-awareness and pre-execution intelligence system first, a records
custodian: what was read, when, over which code, with what result. Maintenance and construction extend that
purpose; they do not replace it.

What that means in practice:

- An agent reads the codebase before it changes it: definitions, use sites, readers of an env key, dependents,
  declarations, sockets, what a change reaches, the tests that cover it.
- A question answered once is not asked again: the same spec over the same code is a reuse, and says so
  (`search #12 from 2026-10-19T08:20:00Z, same spec, same code: reused`).
- Work is a job with a closing receipt: what was read, what was reused, what changed since it was last read, which
  tests ran. A job can be bundled and picked up in another session or another project.
- Every run is a declared line with a receipt in a journal, so the record does not depend on any one conversation.

The full specification behind this, in James Hunt's words, is [docs/SPEC-2026-10-19.md](docs/SPEC-2026-10-19.md).

## What it does today, and what is planned

Built and tested in this version:

- readings: one question per spec file (or one on the command line), answered by
  [custodian_kit](../custodian_kit) over klepto's scan of the workspace: `use_sites`, `readers_of_env`,
  `duplicates`, `dependents`, `declared_in`, `sockets`, `reach`, `constructs`, `matches_on`, `public_api`,
  `tests_covering`, `affected_tests`, `sensitivity`, `diagnostics`, `bundle`, `sweep`
- the two modes: inside an initialized `.custodian/` folder every run is journaled; elsewhere it is free mode,
  printed, nothing stored
- the same-search check before a search is spent, and the diff of two readings of one spec
- jobs as folders: queued, running, done; a folder of specs, or a template (`triage`, `features`), run by
  custodian_kit's job code; the closing receipt; each job a task history in James Hunt's typed 1o records
  (task, intent, a solution per step, changes, verifications, result), a job run again its next revision; bundles
  of jobs and their import elsewhere
- counts: files, lines, code, comments and blanks by tokei, the tests' share by klepto; two counts compared; a
  counting job's lines touched
- the queue: queued job folders scheduled by kind, a checkpoint between a job's start and its close so an
  interrupted job resumes at its close
- `custodian test`: the tests a change reaches (or every package), each cargo line with its receipt and times
- the maintenance loop: `check` through maintenance's own lines, `annotate` as a plan, `features` as
  code_passenger's gating spec
- a resident (`custodian serve`) on an owner-only socket that holds the parsed workspace between runs; with none, the
  parsed index kept once per code digest under `.custodian/state/index/` and read back by the next run (each
  receipt's reason says `index parsed`, `index from cache` or `index from daemon`, with its milliseconds)
- `self install` through the installation crate, and a pre-edit hook script (not enabled)
- bounded and unbounded jobs (below), the `impact` reading (one target's uses, constructions, matches, impls,
  dependents, reach, tests, features and the cost of changing it, each dimension measured, heuristic or unknown),
  change specs with their preflight and code_passenger's `transform`, the plan of an intent, decisions asked once,
  linkage records, and `agent brief`, the handoff an agent reads first
- a fresh checkout rebuilds its journal from the receipts it carries

Planned, not built: the enterprise edition (several daemons over a fleet of workspaces); core-1o 0.2.0 for every
document (docs/spec-gap-2026-10-19.md says what it drops).

## Modes

A job is bounded or unbounded. The mode is in the job document (`mode`), `job start --mode` sets it, and
`job_mode` in `config/custodian.toml` is the default when a job names none (unbounded when unset). `agent brief`
says the mode first.

**Bounded is strict.** The agent writes no code. It writes a change spec (`.custodian/data/specs/changes/`: one
operation, such as `add_field` or `make_fn`, its target, its boundaries, and any function body as simple_impl's
toolkit pieces), and custodian does the rest:

1. `custodian preflight SPEC` reads where the change lands, its impact and cost, its preconditions, the files it
   would write against the scope it may write, and the decisions it leaves open. It is authorized only when its
   rules hold (a big-bad-bool word over flaga's rules: the target is known, the outcome is known, the scope holds,
   no decision is open, the name is free). Nothing is written. Its receipt carries the spec's digest and the
   code's.
2. `custodian transform SPEC --write` runs code_passenger's `transform` line on that preflight receipt. A spec or
   code that moved since the preflight is refused. code_passenger proves the change on a scratch copy first (cargo
   check over each affected crate's feature configurations, then a fresh scan for the facts the operation
   promises), then writes it and keeps a transform record. A test is code too: it is written with `add_test`.
3. `custodian job close` checks every Rust and TOML line the job touched against its transform records. A line no
   transform covers refuses the close (CUST0031): the job is still closed, its result `rejected`, the lines named
   by file, because a partial task must not look finished.

With the pre-edit hook enabled, a bounded job refuses a hand edit before it happens (`custodian agent guard
FILE`, exit 2). A bounded job needs code-passenger installed: it is the writer.

**Unbounded is help.** Everything above is there to use: readings, `plan make`, impact and cost, the decisions
list, reuse by digest. The agent writes code itself and nothing is refused. The closing receipt still records what
ran and the lines touched.

## Jobs: what is known at the start

Two facts are recorded for every job when it opens, whatever its mode:

- **target known**: the job's `--target` resolves to a route or an item through the index.
- **outcome known**: the job says what done looks like up front, `--verify LINE`: `test:<name>`, `reading:<spec>`
  (a reading that must change) or `check:<which>`. `custodian job verify LINE` declares it later.

The closing receipt records the receipt number at which each became true. A bounded job needs both before a
preflight authorizes anything. When the outcome is not known, the job's first decision is "what does done look
like for <job>?" (`custodian decide` lists it; answering it with a verify line declares it). `job close` writes
`accepted` only when every verify line was satisfied by the job's own runs; with none it writes `pending`, with the
reason, never `accepted`.

A job opens at a commit (its base). A test that fails the same way at the base is not the job's failure: at close
each failing test is run again and looked up at the base (in the baseline `job start --baseline` ran at open, else on
a worktree of the base commit), and one with the same first failing line there is named `pre-existing` in the
closing receipt, with that line; the job can be `accepted`. The closing receipt lists every file the job touched
since its base, and a bounded job's file that no operation writes (a test's expected output) needs a person's
`custodian job allow GLOB`, else the close is refused (`[CUST0041]`). `test run` measures the change from the
running job's base, new files included.

Decisions are task-history records: a question asked is a `problem`, its answer a `solution` with the reason
(`custodian decide ID ANSWER --why TEXT`). A question answered anywhere in the project is never asked again. The
task states (proposed, ready, running, blocked, paused, verifying, completed, failed) are derived from the
records, never stored twice.

## Using it

```text
custodian init                         # a plain folder: specs, readings, journal
custodian agent init                   # the same, with jobs as folders and the job templates
custodian run env.toml                 # one spec; again: "search #N ... reused"; --again reads again
custodian run crate:core_lib --kind reach
custodian run                          # every queued spec, then every queued job folder
custodian list                         # the inventory, rendered from the journal
custodian agent brief                  # the first line an agent runs: mode, open job, decisions, next lines
custodian run core_lib::Config --kind impact
custodian plan make "add a digest to the config"
custodian job start digest --mode bounded --target core_lib::Config --verify test:core_lib
custodian preflight config-digest      # then: custodian transform config-digest --write
custodian handoff                      # after a job: what waits on a person, what it was for, what changed, what runs next
custodian why crate:core_lib           # who introduced a route and for what, from the records and git
custodian cost                         # the tokens readings stood for, read and saved by reuse
```

A spec is a small TOML file:

```toml
name = "env_socket"
question = "who reads FIX_SOCKET?"
kind = "readers_of_env"
target = "FIX_SOCKET"
```

Every run ends by saying what it did and where: the reading's path, the receipt's path, the journal line, what
moved to done. Exit status 0 is done, 1 is findings (a check or a test failed), 2 is refused, with a liaise code
(`[CUST1402] a job is already running; close it first: custodian job close`).

| subcommand | what it does |
|---|---|
| `init [--macros]` | makes `.custodian/` (plain); `--macros` adds `maintenance` 0.1.0 from crates.io to every crate through `code-passenger dep add` |
| `agent init [--macros]` | the same, with `state/jobs/{queued,running,done}` and the job templates in `config/jobs/` |
| `run [SPEC\|ROUTE] [--kind K] [--job ID] [--again]` | one spec (a file, or a queued or done spec's name), a search on the line, or the queue |
| `list` | the inventory: specs, the latest reading of each, jobs, test lines, installs, the folder's parts |
| `diff SPEC [--from N] [--to N]` | what moved between two readings of one spec |
| `agent brief [--head]`, `agent guard FILE` | the handoff an agent reads first; whether a file may be edited by hand now (the hook's question) |
| `job start [NAME] [--kind K] [--target T] [--intent TEXT] [--mode bounded\|unbounded] [--verify LINE]... [--again] [--baseline] [--allow-file GLOB]...`, `job close\|brief\|import\|list\|verify\|allow` | jobs as folders, run by custodian_kit; the closing receipt (every reading, reuse, test and check run while the job was open) and the task history; the brief (lines touched, revision), with `--bundle` the job packed as one file too; a bundled job imported under a new id |
| `count of [ROUTE] [--by crate\|language] [--tests]`, `count report A B`, `count history [ROUTE]` | a count as a reading; two counts compared (by receipt number, job, `job:open`/`job:close`, commit or date); every count of a route |
| `test run\|report [--affected\|--all] [--observe failed\|progress\|all] [--ref REF] [--features F\|--all-features\|--matrix]` | the affected tests (or all), a receipt per cargo line; `--features F` runs each line whose crate declares them with them on, `--all-features` with every feature, `--matrix` each line once per configuration of its crate's check matrix (the configuration in the line's target word, `lib [features json]`); `report`: slowest lines, flakiest tests, maintenance's timed report |
| `check sensitivity\|mechanism\|boundary\|supersedes\|reads_env\|golden\|all` | maintenance's check line with `--registry`, its JSON kept; the line writes `.maintenance/registry.json`, the one registry the readings carry |
| `annotate [--from CHECK_JSON] [--apply]` | the plan of maintenance attributes the receipts call for; `--apply` places each through `code-passenger attr place` |
| `features [--apply]` | primary and feature-eligible dependencies, each with its scope depth and the unavailable list; code_passenger's `klepto.toml` spec and the gate-header lines (`features-unavailable.toml`); `--apply` runs `code-passenger gate --write` |
| `diagnostics list [CRATE] \| where CODE \| from LOGFILE` | every liaise code table: meanings, declarations, the sites raising them |
| `triage CODE\|@LOGFILE` | the triage job: the code's sites, what they reach, the tests that prove a fix, the marks there |
| `bundle ROUTE\|JOB [--no-tests] [--no-benches] [--no-examples] [--no-feature F] [--only src] [--redact-level secret] [--reveal]` | a crate, a declared set or a job as one annotated file |
| `sweep DIR` | the one walk: an undeclared tree listed, routes offered to declare |
| `serve` / `stop` | the resident on the project's socket |
| `watch [--observe L] [--since N] [--follow]` | job state changes and test progress, through the observer set |
| `self install\|uninstall\|status` | install custodian through the installation crate, the receipt in the journal |
| `preflight SPEC [--job ID]` | what a change spec entails over the code as it is, and whether it is authorized; nothing written |
| `transform SPEC [--write]`, `transform --revert RECORD` | the change spec made by code_passenger on its preflight receipt; a record's files put back |
| `plan make INTENT [--name N]`, `plan approve PLAN STEP [--operation OP]`, `plan show PLAN` | an intent's plan (where, reuse, what is new, impact, cost, decisions, steps); a step as a change spec |
| `decide [ID ANSWER] [--why TEXT]` | the open decisions together; one answered, never asked again |
| `linkage add NAME --kind K --why TEXT --bind A --bind B`, `linkage check [TARGET]` | why declarations are bound together, each bind resolved; stale, to review or holding |
| `exercise [--ops OPS] [--units UNITS] [--n N] [--tests] [--crate R] [--intents FILE] [--seed S]`, `exercise report [JOB]` | custodian_kit's `exercise` job: each operation tried on N targets of each unit kind it applies to, each on a copy in the job's data folder (spec, preflight, transform, cargo check, tests, the closing check); every refusal, compile error or uncovered file a finding with its receipt; `--intents`: one intent a line through the cheap model (`claude -p --model claude-haiku-4-5-20251001`), `not run: no model` without one; `report`: the summary, what to fix first |
| `handoff [JOB] [--for james\|agent] [--since SEQ]` | the message a person reads after a job (the last closed one by default; with `--since`, every job that ran after that receipt), from the records only; exit 1 when something waits on a person |
| `why ROUTE` | for a `crate:` or `decl:` route: the task histories whose changes touch it, the linkage records binding it, the readings about it, git's first and last commit touching its files |
| `cost [--job ID] [--since SEQ]` | readings against reuses, the tokens they stood for against the tokens reuse saved, per reading kind and per job |

`custodian` with no words prints these lines with their help, generated from the declarations
(`tests/golden/help.txt` holds them). The 0.1.0 spellings `count [ROUTE]`, `test [--all]` and `plan INTENT` still
read.

## After a job: handoff, why, cost

These three lines only read. Nothing in what they print is written by hand.

`custodian handoff [JOB]` reads a job's records: its job file, its closing receipt (`closing.json`), the receipts
it ran, its task history (intent, change, verification, problem, solution and result records), its transform
records and its count diff. It says, in this order:

1. what is waiting on a person: open decisions, change specs a preflight refused that never ran, a verification
   not satisfied, a refused close, files touched by hand in a bounded job;
2. what the job was for: the last intent record of its task, and its target;
3. what changed: lines added and removed in all and per crate (from the count diff), and each change spec a
   transform wrote, with its operation, target, time and receipt;
4. what was decided: each decision of the job's task, its answer, its reason and who gave it (the record's `by`;
   when the record names no one, it says so), with the decide line's receipt;
5. what was verified: each declared verification, the test lines with passed, failed and ignored counts, a line
   that stopped before any test reported, which failures are pre-existing (a failure is pre-existing when a
   closed job's test run before this job's first transform failed the same test; otherwise it says it is not
   known), and the readings made, reused and refused;
6. what runs next, in sentences.

`--for agent` adds the exact lines to run (the `decide`, `preflight`, `job start`, `run`, `cargo test` and
`job close` lines) and the routes and targets the job read. Every time is UTC from a receipt. Every list stops
at five with "… and N more", the next steps at eight, and the whole message at sixty lines; `job brief` has every
fact. `--since SEQ` keeps only what came after that receipt, and with no job named it tells every job that ran
after it.

`custodian why ROUTE` takes a `crate:` or `decl:` route. It lists the task revisions whose change records touch
the route's files (with each one's intent, result and job), the linkage records that bind it (and what else each
binds), and the readings whose target is the route, its crate, or an item of the crate. git's first and last
commit touching the files come from custodian_kit's declared `git log` line, run with its own receipt. A section
the records leave empty says "no record".

`custodian cost` sums the cost records. A reading's cost is the files its answer names: each row's file, else,
for a row that names a crate and no file, that crate's `Cargo.toml`. Their bytes are read when the reading is
made, and tokens are estimated as bytes / 4. No tokenizer runs; the number says how much there was to read by
hand, not what a model would bill. A reuse saves the cost of the reading it reused, the same number. Each run's
record is `.custodian/data/costs/<seq>.json`, under its receipt's number; the receipt itself is unchanged.
Readings made before costs were kept are counted as not measured. `agent brief` says
`tokens: ~N read, ~M saved by reuse (estimate: bytes/4 of the files each reading touched)`.

## The folder

```text
.custodian/config/custodian.toml          mode (plain or agent), globs no reading reads
.custodian/config/v1.paths.1o.paths       the declared routes (decl:<name>, path:<raw>), in 1o
.custodian/config/{features,annotate}.toml the rules features and annotate read
.custodian/config/jobs/<kind>.toml        the job templates (agent)
.custodian/data/specs/{queued,done}/      specs
.custodian/data/specs/changes/            change specs (`v1.<name>.1o.change`, or the TOML twin)
.custodian/data/preflight/<name>/         every preflight, dated
.custodian/data/plans/, linkage/          plans, linkage records
.custodian/jobs.lock                      held by job start, job close and the queue
.custodian/data/readings/<name>/<stamp>.{md,json}   every reading, dated, none overwritten
.custodian/data/receipts/<seq>.json       one receipt per run
.custodian/data/costs/<seq>.json          a reading's cost, or a reuse's saving, under its receipt's number
.custodian/data/inventory.md              the inventory
.custodian/state/journal.sqlite           the journal
.custodian/state/jobs/<state>/<job>/      a job's folder: its specs, readings, closing.md, closing.json
.custodian/runtime/serve.sock             the resident's socket
```

The parts are custodian_kit's (`store::named`); the few only the command line keeps are declared in
`src/layout.rs` (pathgen-macros' `ArtifactContract`). A job's task history is under
`.custodian/data/history/<task>/<revision>/`.

## The receipt and the journal

A receipt is receipt_history's `Receipt` (`custodian.receipt.v1`, re-exported by custodian_kit): `seq` (the
journal's number, from uh-tom-mick's checked counter inside the writing transaction), `utc`, `command` (in the kit's words: `reading`
for a run that answered a spec, read or reused; `test` for a cargo test line; `git`, `gate`; else the subcommand
or the program line's id: `src/facade.rs` `command_word`), `name`, `spec`, `spec_hash` (the
stable_digest of the spec and its option word), `index_key` (the code's index key), `code_digest`, `plan_id`,
`plan_digest`, `operation_digest`, `receipt_digest` (xccute's runtime receipt), `line` (the declared line as it
ran), `exit_code`, `accepted`, `disposition`, `reason` (the status's word), `outputs`, `job`, `reused_seq`,
`millis`.

The journal is receipt_history's file, extended. Its tables (`receipt_history::JOURNAL_SCHEMA`, which custodian_kit
re-exports): `runs` (one row per receipt, the fields above), `jobs` (custodian adds `origin`, `opened_seq`,
`closed_seq`), `tests` (one row per test outcome). Keeping a receipt (the number, the file, the row, in one
writing transaction) and rebuilding the `runs` from the receipt files are receipt_history's; custodian indexes each
rebuilt reading in the same transaction. custodian's (`src/journal.rs`, `SCHEMA`): `readings` (seq, name, kind, target,
spec_digest, index_key, word, stamp, page, record, reading_digest, job), indexed on (spec_digest, index_key) for
the same-search check; `events` (job states, steps, test progress); `test_runs` (one row per cargo line: counts,
exit status, wall time); `installs`; `checkpoints` (the job queue's).

The counts over time are tree_history's (what a count is, two compared, a count named by its receipt number, job,
commit or date, the tables); `count history` answers through differences_kit, the library the
[differences](https://crates.io/crates/differences) command line is. `differences` shows every kind of history the
folder keeps side by side: the tree at two points, two counts, the receipts since a point, a task's revisions,
the lines jobs touched, two readings of one spec.

## The socket

```text
client → server   [u32 little-endian length][Request JSON]
server → client   [u32 little-endian length][Reply JSON]
Request { "schema": "custodian.serve.v1", "kind": "run_spec" | "run_job" | "import_bundle" | "status" | "stop", "argv": [...] }
Reply   { "schema": "custodian.serve.v1", "outcome": { "code", "receipt", "receipt_file", "printed", "said", "refusal"? }, "live": [...] }
```

`argv` is the declared line's words; the resident reads them through the same surface and runs them through the
same facade, so a run over the socket leaves the same receipt. The command line tries the socket first for `run`,
`job start`, `job import` and `stop`, and does the work itself when nothing answers. A path longer than a Unix
socket allows goes to `<runtime dir>/custodian/<digest>.sock`. `tests/golden/request_run_spec.frame` pins the
bytes of one request.

## MCP

`custodian mcp serve` is an MCP server on stdin and stdout (built with the `mcp` feature, on by default; the SDK is
rmcp, the official Rust one). Its tools are custodian's own lines, made from their declarations in
`src/surface.rs`: one tool per line an agent runs, each tool's input schema the line's declared fields (a flag is a
boolean, a list an array, the rest strings, a field's word set its `enum`, its `///` its description). Each tool is named
by its line's path (`job_start` runs `job start`): `agent_brief`, `run` (a search on the line is `target` with
`kind`, as `reach` or `impact`), `list`, `diff`, `job_start`, `job_close`, `job_brief`, `job_list`, `job_verify`,
`test`, `test_report`, `check`, `annotate` and `features` (their plans only: `--apply` is a person's),
`diagnostics_list`, `diagnostics_where`, `diagnostics_from`, `triage`, `bundle`, `count`, `count_report`,
`count_history`, `preflight`, `transform`, `plan_make`, `plan_approve`, `plan_show`, `decide`, `linkage_add`,
`linkage_check`, `exercise_report`, `handoff` (its `reader` `agent` adds the lines to run), `why` and `cost`. `init`, `agent init`, `agent guard`, `job import`, `job allow`, `sweep`, `serve`, `stop`,
`watch`, `self *`, `mcp serve` and `exercise run` are a person's and are never tools; `tests/golden/mcp_tools.json` is the list a host sees, and a new line fails the tests until it is
declared one or the other in `src/mcp.rs`.

A call runs the way the command line runs: the arguments become the declared line's words, read back through the
same reader (so a bad call is the line's own refusal), sent to `custodian serve` when one answers on the project's
socket, else run in the server's process through the facade. Every call leaves a receipt. The result is the JSON
of the run (`code`, `status`, `receipt`, `receipt_file`, `reading` with its `path` and `resource`, `said`, `live`,
`refusal`), then the reading's markdown when it is at most 64 KiB (`mcp::INLINE_CAP`). Arguments that do not make
the line are an MCP error whose `data.code` is the liaise code (`CUST1001`, `CUST1904`); a line that ran and was
refused is a tool error whose JSON names the code and the receipt. Resources: `custodian://readings/<seq>` is the
page of the reading kept under receipt `seq`, and `custodian://job/<id>/brief` is a closed job's brief.

The command an MCP host runs (the root is the project; without `--root` it is where the server starts):

```text
custodian mcp serve --root /path/to/your/workspace
```

An example of what a host's configuration holds for it (nothing here installs or registers it; where this goes is
the host's own file):

```json
{
  "mcpServers": {
    "custodian": {
      "command": "custodian",
      "args": ["mcp", "serve", "--root", "/path/to/your/workspace"]
    }
  }
}
```

## How it is built

custodian is an early reference application for the `1o` development ecosystem: a useful program made by
composing independently published Rust libraries, each keeping its own responsibility. Which crate does what is on
one page: [docs/COMPOSITION.md](docs/COMPOSITION.md).

## Releasing

Bump `version` in Cargo.toml and give CHANGELOG.md a dated `## <version> (YYYY-MM-DD)` entry, then push to main.
CI (`.github/workflows/ci.yml`) tests on stable and Rust 1.89, runs clippy, and publishes to crates.io
through trusted publishing; a version already on crates.io, or an entry still marked `(unreleased)`, publishes nothing.
A custodian_kit version custodian needs is released first: the published crate takes the kit from crates.io.

## 1o, ownership and copyright

The organization and ecosystem name is `1o`, lowercase. Its GitHub organization is
[github.com/1o-llc](https://github.com/1o-llc).

The libraries custodian is built from were developed and published by James Hunt individually, through his
crates.io account (`jameshuntt`) and his GitHub account. He intends to give them, and the rights in them, to the
`1o` organization, so that `1o` owns, maintains, distributes and licenses them. That transfer is not completed. A
repository moved to the organization does not move the copyright in its code; the assignments are to be
documented separately. Copyright ownership, crates.io publishing permissions, repository administration and the
software's license are four separate things.

The notice is: Copyright © 2026 1o. All rights reserved. It heads every file new in 0.2.0; the notices already in
the repository are left as they were found (the `LICENSE-MIT` here, like those of the published crates, reads
`Copyright (c) 2026 1o LLC`).

## Licensing

Not decided here. The recommendation: MIT OR Apache-2.0, the same as every crate custodian depends on from the
`1o` set (each one's manifest says `MIT OR Apache-2.0`) and the same as custodian_kit and maintenance. Every
dependency in the build is under a permissive license or offers one (MIT, Apache-2.0, BSD-3-Clause, Zlib, Unlicense
OR MIT, Unicode-3.0, CC0 options); the one file-level copyleft is `option-ext` (MPL-2.0, through `dirs` in
installation), which permits distribution under either of these. A dual permissive license keeps the
open-source release usable by anyone, and leaves a commercial or enterprise edition free to be licensed
differently, because the rights holder can license its own later work as it chooses. The final choice is James
Hunt's.

## Building

custodian depends by path on [custodian_kit](../custodian_kit), which sits beside it, and on the crates.io
`maintenance` 0.1.0 and `task_history` 0.1.0 the kit uses (custodian_kit 0.2.0: the impact reading, change specs
and their preflight, plans, decisions, linkage records, bounded jobs). The bounded mode's writer is
[code_passenger](../code_passenger) 0.4.0, a program custodian runs, not a dependency. `cargo test` runs 55 tests:
the unit tests, `tests/cli.rs` (every subcommand on the fixture project in `tests/fixtures/ws`), `tests/modes.rs`
(bounded and unbounded jobs, the brief, decisions, plans, linkage, the journal rebuilt, the job lock, the help
lines), `tests/mcp.rs` and `tests/runtime.rs` (checkpoint resume, the job queue). The bounded tests run
code-passenger built from `../code_passenger` (or the directory `CUSTODIAN_TEST_PASSENGER` names) and say when
they skip. Rust 1.89 or later.
