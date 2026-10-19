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
- a resident (`custodian serve`) on an owner-only socket that holds the parsed workspace between runs
- `self install` through the installation crate, and a pre-edit hook script (not enabled)


Planned, not built (from the specification): bounded and unbounded modes; structured change-impact and cost
analysis; SimpleImpl transformation contracts; specification files as the central contract for transformations;
architectural linkage records and comments; durable task handoffs beyond what a job's closing receipt carries
now.

## Using it

```text
custodian init                         # a plain folder: specs, readings, journal
custodian agent init                   # the same, with jobs as folders and the job templates
custodian run env.toml                 # one spec; again: "search #N ... reused"; --again reads again
custodian run crate:core_lib --kind reach
custodian run                          # every queued spec, then every queued job folder
custodian list                         # the inventory, rendered from the journal
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
| `job start\|close\|brief\|import\|list [NAME\|ID\|BUNDLE] [--intent TEXT] [--bundle]` | jobs as folders, run by custodian_kit; the closing receipt (every reading, reuse, test and check run while the job was open) and the task history; the brief (lines touched, revision), with `--bundle` the job packed as one file too; a bundled job imported under a new id |
| `count [ROUTE] [--by crate\|language] [--tests]`, `count report A B`, `count history [ROUTE]` | a count as a reading; two counts compared (by receipt number, job, `job:open`/`job:close`, commit or date); every count of a route |
| `test [report] [--affected\|--all] [--observe failed\|progress\|all] [--ref REF]` | the affected tests (or all), a receipt per cargo line; `report`: slowest lines, flakiest tests, maintenance's timed report |
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

## The folder

```text
.custodian/config/custodian.toml          mode (plain or agent), globs no reading reads
.custodian/config/v1.paths.1o.paths       the declared routes (decl:<name>, path:<raw>), in 1o
.custodian/config/{features,annotate}.toml the rules features and annotate read
.custodian/config/jobs/<kind>.toml        the job templates (agent)
.custodian/data/specs/{queued,done}/      specs
.custodian/data/readings/<name>/<stamp>.{md,json}   every reading, dated, none overwritten
.custodian/data/receipts/<seq>.json       one receipt per run
.custodian/data/inventory.md              the inventory
.custodian/state/journal.sqlite           the journal
.custodian/state/jobs/<state>/<job>/      a job's folder: its specs, readings, closing.md, closing.json
.custodian/runtime/serve.sock             the resident's socket
```

The parts are custodian_kit's (`store::named`); the few only the command line keeps are declared in
`src/layout.rs` (pathgen-macros' `ArtifactContract`). A job's task history is under
`.custodian/data/history/<task>/<revision>/`.

## The receipt and the journal

A receipt is custodian_kit's `Receipt` (`custodian.receipt.v1`): `seq` (the journal's number, from
uh-tom-mick's checked counter inside the writing transaction), `utc`, `command` (in the kit's words: `reading`
for a run that answered a spec, read or reused; `test` for a cargo test line; `git`, `gate`; else the subcommand
or the program line's id: `src/facade.rs` `command_word`), `name`, `spec`, `spec_hash` (the
stable_digest of the spec and its option word), `index_key` (the code's index key), `code_digest`, `plan_id`,
`plan_digest`, `operation_digest`, `receipt_digest` (xccute's runtime receipt), `line` (the declared line as it
ran), `exit_code`, `accepted`, `disposition`, `reason` (the status's word), `outputs`, `job`, `reused_seq`,
`millis`.

The journal is custodian_kit's file, extended. The kit's tables (`custodian_kit::store::JOURNAL_SCHEMA`): `runs`
(one row per receipt, the fields above), `jobs` (custodian adds `origin`, `opened_seq`, `closed_seq`), `tests`
(one row per test outcome). custodian's (`src/journal.rs`, `SCHEMA`): `readings` (seq, name, kind, target,
spec_digest, index_key, word, stamp, page, record, reading_digest, job), indexed on (spec_digest, index_key) for
the same-search check; `events` (job states, steps, test progress); `test_runs` (one row per cargo line: counts,
exit status, wall time); `installs`; `checkpoints` (the job queue's).

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
boolean, a list an array, the rest strings, a field's word set its `enum`, its `///` its description). The tools:
`run` (a search on the line is `target` with `kind`, as `reach`), `list`, `diff`, `job_start`, `job_close`,
`job_brief`, `job_list`, `test`, `check`, `annotate` and `features` (their plans only: `--apply` is a person's),
`diagnostics_list`, `diagnostics_where`, `diagnostics_from`, `triage`, `bundle`, `count`, `count_report` and
`count_history`. `init`, `agent`, `sweep`, `serve`, `stop`, `watch`, `self` and `mcp` are a person's and are never
tools; `tests/golden/mcp_tools.json` is the list a host sees, and a new subcommand fails the tests until it is
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

The intended notice, once the assignments are documented, is: Copyright © 2026 1o. All rights reserved. It is not
applied here yet. The `LICENSE-MIT` in this repository, like those of the published crates, reads
`Copyright (c) 2026 1o LLC`; it is left as it was found.

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
`maintenance` 0.1.0 and `task_history` 0.1.0 the kit uses (custodian_kit 0.1.1: a job's close reads cargo's test
outcomes, the words custodian writes). `cargo test` runs 44 tests:
the unit tests, `tests/cli.rs` (every subcommand on the fixture project in `tests/fixtures/ws`) and
`tests/runtime.rs` (checkpoint resume, the job queue). Rust 1.89 or later.
