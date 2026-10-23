# Changelog

## 0.3.9 (2026-10-23)

On custodian_kit 0.3.10: what an intended change needs from the manifests, read once, before anything is changed. An
agent integrating a multi-crate package failed on a dependency six times in a row, each error fixed alone and the
next one found by the next build, a person running every build. Each of those was in the manifests before the first
one. `custodian requirements` asks custodian_kit's `requirements` reading and keeps it as every reading is kept: a
receipt, a dated page, reused while the manifests, the lock and the intent are the same. It never compiles and never
writes a manifest, a lock or a source file. Nothing is removed from the API: the rustdoc JSON of 0.3.8 and 0.3.9
differ by 143 additions (26 without the derived and blanket trait impls) and no removal. Every link is pinned to
74aa195; the lines it replaces to bc351a9 (0.3.8).

- **`custodian requirements [ROUTE] [--intent TEXT] [--spec FILE] [--faces WORDS] [--job JOB] [--again]`**
  (`surface::Requirements`, [surface.rs L1340](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/src/surface.rs#L1340); `readings::requirements`, [readings.rs L210](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/src/readings.rs#L210)): the
  route is `.` (every workspace under the root), `crate:<name>` or a directory; the intent's words and a change
  spec's dependency and feature operations are the needs; the faces are custodian_kit's word, checked on the line
  ([surface.rs L1424](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/src/surface.rs#L1424)). It is the search `run ROUTE --kind requirements` with those in its spec, so the one
  same-search key finds either. A reading exits 0 with its findings on its page, so the journal can reuse it.
- **No workspace is opened for a kind that reads no scan.** `read_spec` answers a sweep or a requirements reading at
  the root through custodian_kit's `api::run_at` ([readings.rs L145-L150](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/src/readings.rs#L145-L150)); 0.3.8 opened the workspace for
  every kind ([readings.rs L143 at 0.3.8](https://github.com/1o-llc/custodian/blob/bc351a9/src/readings.rs#L143)), and klepto's scan stops at a manifest cargo refuses, which is exactly
  the state this reading is for.
- **An MCP tool, from its declaration** ([mcp.rs L94](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/src/mcp.rs#L94)): the agent asks for the same report over MCP; the golden
  tool list and the help screen are made again from the declarations.
- **The agent brief carries what is unresolved** ([agent.rs L188](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/src/agent.rs#L188)): the newest requirements reading's unsatisfied
  findings, and whether the manifests and the lock it read still hold the same bytes; with no job open, the line to run
  is among the next ones ([agent.rs L239](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/src/agent.rs#L239)). A job's handoff carries them too (custodian_kit's). The README has a section for it, "Before a change: requirements".

Proof: tests/requirements.rs on two fixtures (tests/fixtures/req_inherit, the `hex` case; tests/fixtures/req_add):
`a_broken_manifest_is_read_without_a_scan_and_its_finding_names_the_root` ([L40](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/tests/requirements.rs#L40)) (cases 1 and 9 at the line);
`the_report_is_reused_until_a_manifest_or_the_lock_changes_and_the_workspace_is_never_written` ([L63](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/tests/requirements.rs#L63)) (case 7: no
file of the workspace written, reused, a source edit still reused, a manifest edit read again, the brief seeing it move);
`a_change_spec_gives_the_needs_and_its_preflight_names_the_line` ([L88](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/tests/requirements.rs#L88));
`a_plan_folds_the_requirements_into_its_reuse_and_steps` ([L105](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/tests/requirements.rs#L105)) (case 6 through `plan make`);
`the_faces_word_is_checked_on_the_line` ([L120](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/tests/requirements.rs#L120)); and tests/mcp.rs
`the_requirements_report_over_mcp_is_the_one_the_handoff_carries` ([L170](https://github.com/1o-llc/custodian/blob/74aa1958947eabceddacb6689f581ef1c2a91865/tests/mcp.rs#L170)) (case 8: the page an agent gets
over MCP is the page on disk, and the job's handoff carries the same unresolved lines). Case 10: every existing test
passes unchanged, the bounded and unbounded ones in tests/modes.rs included; `cargo test --all-features` is 0 failed
and `cargo clippy --all-targets --all-features -- -D warnings` is clean.

## 0.3.8 (2026-10-20)

On custodian_kit 0.3.9: what last night's fleet_ops swaps cost, read from their receipts, each made to cost once. The
295 members of the largest shape batch (receipt #1762) took 1,849 seconds, 253 of them the batch's compile (#1477)
and the other 1,595 the members' preflights: 0.3.7's `--preflight` ran a whole preflight per member, the code read
again each time. On a copy of the kit, 0.3.7 preflights those 295 in 1,743 seconds and 0.3.8 in 55 (101 for the
whole line, the index parse included), every member's verdict the same. Then a test run after the swaps built every
dependent's dependent from an empty build in the worktree, for six hours on battery. Measured on a copy of the kit
after a body swap in `sql_executor`: 0.3.7 compiled 33 crates into the worktree's own build and ran 3 test lines, and
3 crates again after the next swap; 0.3.8 compiled 6 into the proof's kept build and ran 2 lines, and 2 crates after
the next swap. Nothing is removed from the API: the rustdoc JSON of 0.3.7 and 0.3.8 differ by 10 additions and no
removal. Every link is pinned to 6491cb8; the lines it replaces to 1a1fe24 (0.3.7).

- **`custodian preflight --batch LIST`** (`changes::preflight_batch`, [changes.rs L89](https://github.com/1o-llc/custodian/blob/6491cb8/src/changes.rs#L89)): every
  spec LIST names, preflighted by custodian_kit's `run_preflight_batch` over the one workspace the run holds open: one
  scan, one code digest, one code graph, each member checked by its own preflight's rules, one `preflight-batch`
  receipt listing each member and its verdict. `transform --batch --preflight` runs it, where 0.3.7 ran a whole
  preflight per member ([changes.rs L168 at 0.3.7](https://github.com/1o-llc/custodian/blob/1a1fe24/src/changes.rs#L168)), and `transform --batch` takes its receipt
  as each member's gate, the journal read once (`gate_all`, [changes.rs L231](https://github.com/1o-llc/custodian/blob/6491cb8/src/changes.rs#L231)). Proof:
  tests/modes.rs `a_batch_preflight_is_one_receipt_and_the_batch_transform_goes_on_it` ([L240](https://github.com/1o-llc/custodian/blob/6491cb8/tests/modes.rs#L240)),
  each member's verdict the one it gets alone, and custodian_kit's `a_batch_preflight_gives_each_member_the_verdict_its_own_preflight_gives`.
- **`test run` builds on the kept build.** Every line runs on the workspace's one copy and kept build, custodian_kit's
  `proof_place` (the proof's own), synced first ([testing.rs L188](https://github.com/1o-llc/custodian/blob/6491cb8/src/testing.rs#L188)); 0.3.7 ran cargo in the
  worktree ([testing.rs L182 at 0.3.7](https://github.com/1o-llc/custodian/blob/1a1fe24/src/testing.rs#L182)). `--affected` is the proof's rule: the crates the
  change is in, their direct dependents, and the readers of a public signature it alters, found from the diff
  ([testing.rs L103](https://github.com/1o-llc/custodian/blob/6491cb8/src/testing.rs#L103)). Proof: tests/kept_build.rs
  `a_test_run_after_a_swap_compiles_the_crate_and_its_direct_dependents_on_the_kept_build` ([L47](https://github.com/1o-llc/custodian/blob/6491cb8/tests/kept_build.rs#L47)):
  base, mid, far; a swap in base compiles base and mid, not far, nothing in the workspace, and the next run compiles
  nothing.
- **No cargo line on a dying battery.** Under 20% and not charging, a cargo line custodian starts is refused with
  `[CUST0042]` and the job stays open ([facade.rs L440](https://github.com/1o-llc/custodian/blob/6491cb8/src/facade.rs#L440)). Proof: tests/kept_build.rs
  `a_dying_battery_refuses_the_cargo_line_and_the_job_stays_open` ([L70](https://github.com/1o-llc/custodian/blob/6491cb8/tests/kept_build.rs#L70)).
- **custodian reads its own cost.** `job start` and `agent brief` say what waits will cost, from this workspace's
  receipts: N preflights × their measured time, a compile and a test line per crate, the kept build's size, each figure
  measured or unknown ([budget.rs L13](https://github.com/1o-llc/custodian/blob/6491cb8/src/budget.rs#L13), [jobs.rs L246](https://github.com/1o-llc/custodian/blob/6491cb8/src/jobs.rs#L246),
  [agent.rs L152](https://github.com/1o-llc/custodian/blob/6491cb8/src/agent.rs#L152)); the handoff's "what runs next" says it too. `job start --budget
  hours=N,battery=P` (or `.custodian/config/budget.toml`): a step whose measured time would take the job past its
  hours is refused before it starts with `[CUST0043]` ([budget.rs L22](https://github.com/1o-llc/custodian/blob/6491cb8/src/budget.rs#L22)), and the battery is the
  floor its cargo lines keep. The closing receipt keeps the time per step kind and names the kinds paid one member at a
  time where a batch form exists ([custodian_kit jobs.rs L1114](https://github.com/1o-llc/custodian_kit/blob/2899848/src/jobs.rs#L1114)). Proof: tests/costs.rs
  `the_planned_cost_is_measured_or_unknown_and_the_closing_names_steps_paid_one_at_a_time` ([L19](https://github.com/1o-llc/custodian/blob/6491cb8/tests/costs.rs#L19))
  and `a_step_past_the_hours_budget_is_refused_before_it_starts_and_the_job_stays_open` ([L45](https://github.com/1o-llc/custodian/blob/6491cb8/tests/costs.rs#L45)).

`cargo test --all-features` passes; `cargo clippy --all-targets --all-features -- -D warnings` is clean.

## 0.3.7 (2026-10-20)

One finding from the kit's first batch on 0.3.6. A batch's members each need a preflight receipt over the code as it
is, and `custodian preflight` opens the workspace anew each time: about 45 s on the kit, the index parsed again for
every member, so fleet_ops' 640 swaps would spend 8 hours on preflights before one batch was made. James's rule says
one preflight listing them all. Nothing is removed from the API: the rustdoc JSON of 0.3.6 and 0.3.7 differ by
4 additions and no removal. Every link is pinned to 6156da5; the lines it replaces to d1cc51d (0.3.6).

- **`custodian transform --batch LIST --preflight`** (`changes::transform_batch_with`,
  [changes.rs L155](https://github.com/1o-llc/custodian/blob/6156da5/src/changes.rs#L155)): the batch preflights each member itself first, in the one run, over the
  workspace the run holds open (`Ctx::workspace` keeps it while the code does not move), each member with its own
  preflight receipt as `custodian preflight` leaves it ([changes.rs L164](https://github.com/1o-llc/custodian/blob/6156da5/src/changes.rs#L164)); a member the
  preflight refuses is left out and named, the rest go as the batch. The line says how many were authorized and how
  long the preflights took. Proof: tests/modes.rs `a_batch_preflights_its_members_in_one_run`
  ([L211](https://github.com/1o-llc/custodian/blob/6156da5/tests/modes.rs#L211)): two members authorized and one whose target is not in the code refused, in one
  run; the two written, the job closed accepted.

`cargo test --all-features` and `cargo clippy --all-targets --all-features -- -D warnings` pass.

## 0.3.6 (2026-10-20)

On custodian_kit 0.3.8 and code_passenger 0.4.6: James's rule for a repeated shape as a custodian line. Prove one
member of a shape fully, then batch the rest of that shape in the crate under one compile, one receipt per member, a
failing member excluded and named; it holds for every operation with repeated shapes, not only a swap. Nothing is
removed from the API: the rustdoc JSON of 0.3.5 and 0.3.6 differ by 11 additions and no removal. Every link is pinned to
3c23186; the lines it replaces to eb9c2eb (0.3.5).

- **`custodian transform --batch LIST [--after-proof N] [--write] [--tests]`** (`changes::transform_batch`,
  [changes.rs L148](https://github.com/1o-llc/custodian/blob/3c23186/src/changes.rs#L148)): each spec of LIST goes on its own preflight receipt over this code, a
  spec with none left out and named; the rest are made by code_passenger's `transform --batch` under one compile;
  then one `transform` receipt per member, named `<spec> (batch of N after proof #S #<the batch's receipt>)`, its
  record its output ([changes.rs L221](https://github.com/1o-llc/custodian/blob/3c23186/src/changes.rs#L221)), so a bounded job's close covers each member's lines
  as it covers a single transform's. 0.3.5 took one spec ([changes.rs L80 at 0.3.5](https://github.com/1o-llc/custodian/blob/eb9c2eb/src/changes.rs#L80)).
  Proof: tests/modes.rs `a_batch_of_one_shape_goes_under_one_proof_with_a_receipt_per_member_and_closes_accepted`
  ([L169](https://github.com/1o-llc/custodian/blob/3c23186/tests/modes.rs#L169)): one attribute made alone, two more as a batch after it, a spec with no preflight
  left out, a receipt per member, the job closed accepted.
- **`--tests`** on a transform (`changes::transform_with`; `transform` keeps its signature): the crate's tests run on the proof against the same tests before the change
  (code_passenger 0.4.6), for a change no job's test line covers. The MCP tool `transform` takes `batch`,
  `after_proof` and `tests` (tests/golden/mcp_tools.json).

`cargo test --all-features` passes.

## 0.3.5 (2026-10-20)

On custodian_kit 0.3.7: the command line's half of three findings custodian's exercise made with 0.3.4. Nothing is
removed from the API: the rustdoc JSON of 0.3.4 and 0.3.5 differ by 2 additions and no removal. Every link is pinned
to 9f8859d; the lines it replaces to 11db2de (0.3.4).

- **One refusal, the one line.** `exercise run` in a folder with no `.custodian` named `custodian init` (CUST1002),
  and after it, CUST1003 asked for `custodian agent init`. `custodian agent init` makes the folder and makes it an
  agent's, so a line that needs an agent's folder now says only that, when there is no folder too:
  `Mode::need_agent_project` ([mode.rs L106](https://github.com/1o-llc/custodian/blob/9f8859d/src/mode.rs#L106)), used by every such line, where 0.3.4 asked for any folder
  first ([mode.rs L101 at 0.3.4](https://github.com/1o-llc/custodian/blob/11db2de/src/mode.rs#L101)). Proof: tests/cli.rs
  `a_folder_with_no_custodian_is_told_agent_init_and_a_new_job_needs_its_name` ([L120](https://github.com/1o-llc/custodian/blob/9f8859d/tests/cli.rs#L120)).
- **A new job needs its name.** A bare `custodian job start` with nothing queued opened an unbounded job named by
  the time ([jobs.rs L178 at 0.3.4](https://github.com/1o-llc/custodian/blob/11db2de/src/jobs.rs#L178)). It is now refused, CUST1408, and asks for the name; nothing is
  opened ([jobs.rs L177](https://github.com/1o-llc/custodian/blob/9f8859d/src/jobs.rs#L177), [codes.rs L41](https://github.com/1o-llc/custodian/blob/9f8859d/src/codes.rs#L41)). A queued folder is still started by a bare `job start`,
  the queue's own way. Proof: the same test.
- **A spec already in place waits on no one.** The handoff exited 1 for the pantry's four gate specs, each refused
  because its item was gated already ([records.rs L37 at 0.3.4](https://github.com/1o-llc/custodian/blob/11db2de/src/records.rs#L37)). It counts what waits by
  custodian_kit's `Handoff::waiting` now ([records.rs L38](https://github.com/1o-llc/custodian/blob/9f8859d/src/records.rs#L38)), so such a spec is told as done and the
  handoff exits 0. Proof: tests/records.rs `a_spec_already_in_place_is_told_as_done_and_waits_on_no_one`
  ([L120](https://github.com/1o-llc/custodian/blob/9f8859d/tests/records.rs#L120)): an item gated by `json` gated again in a job; the preflight's top line is
  "already in place", the handoff says nothing waits and exits 0. The handoff's pinned golden says where its line
  count comes from (tests/golden/handoff_fixture.txt).

`cargo test --all-features` and `cargo clippy --all-targets --all-features -- -D warnings` pass.

## 0.3.4 (2026-10-20)

On custodian_kit 0.3.6: the operations and the reading a program built through change specs asked for
(`declare_feature`, `make_lib`, `optional`, a body with `change_signature`, a module made by `add_test`, the
`patterns` reading), and two of its findings that are the command line's own. Nothing is removed from the API: the
rustdoc JSON of 0.3.3 and 0.3.4 differ by 10 additions and no removal. Every link is pinned to 838a701.

- **Features in a test run** (the pantry's tenth finding: `test run` tested the default features only, so the
  `json` and `chrono` configurations were run by hand). `custodian test run --features F` runs each line whose crate
  declares the features with them on, and the others as they are (cargo refuses a feature a package does not
  have); `--all-features` every feature; `--matrix` each line once per configuration of its crate's check matrix,
  the transform's N of N: [surface.rs L580-L597](https://github.com/1o-llc/custodian/blob/838a701/src/surface.rs#L580-L597), [testing.rs L126-L155](https://github.com/1o-llc/custodian/blob/838a701/src/testing.rs#L126-L155). The configuration is
  carried in the line's target word (`core_lib all [no-default-features, features json]`,
  custodian_kit's `testrun::Configuration`), so the cargo line is built from it ([testing.rs L37-L40](https://github.com/1o-llc/custodian/blob/838a701/src/testing.rs#L37-L40)), its
  outcomes are told apart from the default's, and a job's close reruns it the same way.
- **init, then a job** (the twelfth): `custodian init` makes a person's folder, and `job start` there was refused
  with "jobs are agent mode: custodian agent init: .custodian". The detail now says what happened and the exact line
  on: ".custodian was made by `custodian init`, a person's folder; run exactly `custodian agent init` here to make it
  an agent's" ([mode.rs L52](https://github.com/1o-llc/custodian/blob/838a701/src/mode.rs#L52)).
- The help and the tool list carry `test run`'s three new words, and `make_lib` and `declare_feature` among the
  operations (tests/golden/help.txt, tests/golden/mcp_tools.json); the `patterns` kind and `inline`, `wrap` came
  with the `patterns` branch.

Proof: tests/features.rs `a_test_run_takes_features_all_features_and_the_matrix` ([L19](https://github.com/1o-llc/custodian/blob/838a701/tests/features.rs#L19)): a
fixture whose `core_lib` declares `json` and holds a test that exists only with it; `--features json` runs
`core_lib` with it and `app` as it is; `--matrix` runs `core_lib` twice and the json line passes one test more;
`--features` with `--matrix` is refused. tests/cli.rs `a_refused_line_says_its_code_and_is_kept`
([L99](https://github.com/1o-llc/custodian/blob/838a701/tests/cli.rs#L99)): the refusal's text, then `agent init` and `job list` succeed. `cargo test
--all-features` and `cargo clippy --all-targets --all-features -- -D warnings` pass.

## 0.3.3 (2026-10-20)

Three lines that only read the records, on custodian_kit 0.3.5: `handoff`,
`why` and `cost`. Each is a declared line with its receipt and an MCP tool.
Nothing is removed from the API. Every link is pinned to d61e248; the
earlier line is pinned to 45642e2 (0.3.2).

- `custodian handoff [JOB] [--for james|agent] [--since SEQ]`
  ([src/surface.rs#L1230](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/surface.rs#L1230), [src/records.rs#L19](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/records.rs#L19)):
  the message a person reads after a job, made by custodian_kit's `handoff`
  from the job's records. What waits on a person comes first. With no job
  named it is the job closed last; `--since` tells every job that ran after
  that receipt. Exit 1 when something waits on a person.
- `custodian why ROUTE` ([src/surface.rs#L1256](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/surface.rs#L1256),
  [src/records.rs#L50](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/records.rs#L50)): a `crate:` or `decl:` route's task
  histories, linkage records and readings, and git's first and last commit
  touching its files. The `git log` is custodian_kit's declared line, run
  with its own receipt (`command` `git`). Another route is refused by the
  line's own word check ([src/surface.rs#L1316](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/surface.rs#L1316)).
- `custodian cost [--job ID] [--since SEQ]` ([src/surface.rs#L1274](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/surface.rs#L1274),
  [src/records.rs#L95](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/records.rs#L95)): the cost records summed, per reading
  kind and per job. A reading run here keeps its cost record under its
  receipt's number ([src/readings.rs#L173](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/readings.rs#L173), kept
  by the facade: [src/facade.rs#L311](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/facade.rs#L311)),
  and a reuse keeps the same number as `saved`
  ([src/readings.rs#L132](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/readings.rs#L132)).
- `agent brief` says `tokens: ~N read, ~M saved by reuse (estimate: bytes/4
  of the files each reading touched)` ([src/agent.rs#L198](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/agent.rs#L198));
  before it said "tokens: not measured here (no source in the records)"
  ([src/agent.rs#L198](https://github.com/1o-llc/custodian/blob/45642e295a72c78952ee85aa4f85979f8a306faf/src/agent.rs#L198)).
- The three lines are tools: `handoff`, `why`, `cost`
  ([src/mcp.rs#L91](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/src/mcp.rs#L91)). The help lines and the tool list's
  goldens have them; both diffs only add.

Proof: `cargo test` has 63 tests, the 58 of 0.3.2 and five new in
`tests/records.rs`. `a_handoff_after_a_finished_job_is_its_records_in_order`
([tests/records.rs#L68](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/tests/records.rs#L68)) pins a fixture
job's handoff whole (`tests/golden/handoff_fixture.txt`, its times, job id
and run count read as words).
`a_handoff_says_what_waits_on_a_person_first_and_the_agent_gets_the_lines`
([tests/records.rs#L98](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/tests/records.rs#L98)),
`why_says_who_introduced_a_route_and_for_what_from_the_records_and_git`
([tests/records.rs#L118](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/tests/records.rs#L118)) and
`a_readings_cost_is_its_files_a_reuse_saves_it_and_the_brief_says_both`
([tests/records.rs#L146](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/tests/records.rs#L146)): the cost is the
sum of the bytes on disk of the files the reading names, and the reuse's
`saved` equals it. `the_real_jobs_read_as_pinned`
([tests/records.rs#L181](https://github.com/1o-llc/custodian/blob/d61e248cdf3d9d32292d387f7223f30f6ad792c4/tests/records.rs#L181)) runs `handoff`
over real projects' records and compares each text with its pinned one; the
records and the texts are private, so it runs only when
`CUSTODIAN_PRIVATE_CORPUS` names them. Over three real jobs (a bounded job
closed failed_test, an unbounded job pending on two decisions, a reading job
accepted), five texts read as pinned. The rustdoc JSON of 0.3.2 against
this: 2,737 public items before, 2,844 after, 107 added, 0 removed or
changed.

## 0.3.2 (2026-10-20)

On custodian_kit 0.3.3 and code_passenger 0.4.3, the fixes two real runs called for: the first bounded job in the
kit closed `failed_test` on 26 tests that fail the same way at its base, and the exercise found 22 findings in the
change path. The exercise itself ships here (built on the branch `exercise`, 93a4c59). Nothing public removed or
changed: the rustdoc JSON of 0.3.1 and 0.3.2 differ only by added items. New lines are pinned to 3d1d5d5 (the tool's status to 6f66987).

- `custodian exercise run|report`: the kit's exercise job from the command line (`exercise [--ops ...] [--units ...]
  [--n N] [--tests] [--crate R] [--intents FILE] [--seed S]` reads as `exercise run`), its options one checked word;
  `exercise` is a job kind; `exercise report` is a tool, `exercise run` a person's line, because it runs for minutes
  ([src/exercise.rs#L15](https://github.com/1o-llc/custodian/blob/93a4c5987b5e4e92aa18ba69c48382e4f863d957/src/exercise.rs#L15), [src/exercise.rs#L41](https://github.com/1o-llc/custodian/blob/93a4c5987b5e4e92aa18ba69c48382e4f863d957/src/exercise.rs#L41)).
- `job start --baseline`: the tests the target reaches run at open and kept as the job's baseline, so a test failing
  the same way there is named pre-existing at close ([src/surface.rs#L434](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/surface.rs#L434), [src/jobs.rs#L208](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/jobs.rs#L208)).
- `job start --allow-file GLOB` and `custodian job allow GLOB [--job ID]`: a file a bounded job may touch that no
  operation writes (a test's expected output), with an `allow` receipt; a person's line, never a tool: an agent does
  not widen its own bounds ([src/surface.rs#L537](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/surface.rs#L537), [src/jobs.rs#L359](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/jobs.rs#L359), [src/mcp.rs#L99](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/mcp.rs#L99); the
  `job_start` tool leaves `allow_file` out, [src/mcp.rs#L64](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/mcp.rs#L64)).
- `test run` measures the change from the running job's base (the commit `job start` recorded), not HEAD, and the
  files git does not track yet are part of the change (was: HEAD, tracked files only, [src/testing.rs#L117 at 0.3.1](https://github.com/1o-llc/custodian/blob/633c596fc18251a72c1089558c823845da011ef3/src/testing.rs#L117),
  [src/testing.rs#L79 at 0.3.1](https://github.com/1o-llc/custodian/blob/633c596fc18251a72c1089558c823845da011ef3/src/testing.rs#L79); now [src/testing.rs#L124](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/testing.rs#L124), [src/testing.rs#L79-L80](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/testing.rs#L79-L80)).
- A run's receipt says where its parsed index came from: `done · index parsed (35901 ms)`, `index from cache (3314
  ms)`, `index from daemon` (custodian serve held it from a run before) ([src/facade.rs#L219](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/facade.rs#L219),
  [src/facade.rs#L289](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/facade.rs#L289), [src/readings.rs#L148](https://github.com/1o-llc/custodian/blob/3d1d5d5d8144d1fee5630b1c661949a11ad3f679/src/readings.rs#L148)).
- An MCP tool's `status` is still the line's word (`done`); where the index came from is beside it, `index`
  ([src/mcp.rs#L303-L310](https://github.com/1o-llc/custodian/blob/6f66987ed0eb5cf8b4158c9dcade87f3e7e19896/src/mcp.rs#L303-L310), pinned to 6f66987).
- The tool list's golden carries `inline`, `wrap` and `job_start`'s `baseline`; the help's golden `job start
  --baseline --allow-file` and `job allow`; the closing line says the two facts as set at open
  (`outcome_known: false at open, known at #6`).

### Proof

- The exercise again on the three projects it found 22 findings on (the kit's fixtures `ws` and `features_ws`, and a
  copy of `access_vocabulary`), `custodian exercise --n 2`, built from these sources: 172 cases, 164 passed, 8
  findings, every one a refusal that is right: `add_variant` with a match that has no catch-all (2), a private field
  a struct literal elsewhere cannot name (1), a `gate` while code outside the item uses it (2), and `#[bitpack]`
  refusing `add_field` and `add_constructor` by name, `[CUST0040]` (3). No `transform-unproved` is left. Before: 162
  cases, 140 passed, 22 findings, 10 not run (the cheap model).
- `inline` and `wrap` ran 10 cases, all passed (inline on `access_vocabulary` and the kit fixture `ws`; the
  `features_ws` fixture has no function that can be inlined: each is called from another crate).
- tests/cli.rs, the surface tests and the tool list's golden pass; the free-mode run says `index parsed (`.

## 0.3.1 (2026-10-19)

One rule, one place, on custodian_kit 0.3.2 and differences_kit 0.1.1. The
`.custodian` folder is declared once, in custodian_kit. A spec's readings
come from one source, the receipts. A plan can be made in a named job, and
it targets the crates its intent names. Nothing is removed from the API.
Every link is pinned to 1bc55b5; the earlier lines are pinned to 02b908a
(0.3.0).

- `layout` declares no part of its own. `FOLDER` is custodian_kit's
  ([src/layout.rs#L12](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/layout.rs#L12)). Every part type hands back the kit's
  descriptor, and its `ArtifactContract` is read from that descriptor
  ([src/layout.rs#L75](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/layout.rs#L75)). The reading record, the check run and the log
  were declared here before ([src/layout.rs#L92](https://github.com/1o-llc/custodian/blob/02b908ad1e58b721fbc4b0617b53d568b6384544/src/layout.rs#L92)); they are the kit's
  now, with the same schema, route and place. New part types name the kit's
  other parts: `WriterLock`, `ChangeSpec`, `PreflightRecord`,
  `TransformRecord`, `Linkage`, `IntentPlan` and `Tree`. The change specs'
  folder was a joined path before; it is `ChangeSpec`'s now.
  `custodian list` prints the kit's `named::ALL` ([src/layout.rs#L155](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/layout.rs#L155)).
- `custodian diff` picks its two readings from the receipts. It uses
  differences_kit's `reading_pair`, the same pair `differences readings`
  compares ([src/readings.rs#L221](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/readings.rs#L221)). Before, it read the readings index
  ([src/readings.rs#L218](https://github.com/1o-llc/custodian/blob/02b908ad1e58b721fbc4b0617b53d568b6384544/src/readings.rs#L218)). The index is a cache. It is rebuilt from the
  receipts by custodian_kit's `receipts::reading_of`
  ([src/journal.rs#L529](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/journal.rs#L529)); before, this crate kept its own copy of that
  rule ([src/journal.rs#L535](https://github.com/1o-llc/custodian/blob/02b908ad1e58b721fbc4b0617b53d568b6384544/src/journal.rs#L535)).
- `plan make`, `plan approve` and `plan show` take `--job`
  ([src/changes.rs#L147](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/changes.rs#L147), [src/changes.rs#L172](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/changes.rs#L172),
  [src/changes.rs#L192](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/changes.rs#L192)). A plan made in a job has its impact readings
  in the job, and its receipt carries the job. Before, `plan make --job` was
  refused (`CUST1001`) and a plan used the running job only
  ([src/changes.rs#L139](https://github.com/1o-llc/custodian/blob/02b908ad1e58b721fbc4b0617b53d568b6384544/src/changes.rs#L139)). A job that is not in the journal is refused
  (`CUST1401`, [src/changes.rs#L33](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/changes.rs#L33)).
- custodian_kit 0.3.2 is required. Its two new operations, `replace_body`
  and `change_dependency`, are in the operations the MCP tools offer, and
  the tool list's golden says so.
- From custodian_kit 0.3.1: a plan targets the crates, files and
  declarations its intent names before it matches words. A `reading:<spec>`
  verification is satisfied by the job's first reading of a spec never read
  before, or by a reading that differs from the latest one before the job
  opened.

Proof: the rustdoc JSON of 0.3.0 compared with this release: 2,520 public
items before, 2,589 after. `layout::FOLDER` is now a re-export of the same
value; nothing is removed. `cargo test` has 60 tests, the 56 of 0.3.0 and
four new: `every_path_the_command_line_names_is_the_kits`
([src/layout.rs#L200](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/src/layout.rs#L200)),
`custodian_diff_and_differences_readings_pick_the_same_pair_from_the_receipts`
([tests/readings_pair.rs#L23](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/tests/readings_pair.rs#L23)),
`a_plan_belongs_to_its_job_and_a_first_reading_verifies_it`
([tests/modes.rs#L104](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/tests/modes.rs#L104)), and
`on_a_projects_committed_folder_both_lines_pick_the_same_pair`
([tests/readings_pair.rs#L63](https://github.com/1o-llc/custodian/blob/1bc55b50a117cc7e4ae24ee327625a635f802dee/tests/readings_pair.rs#L63)). The last one runs only when
`CUSTODIAN_PAIR_DATA` names a copy of a project's folder. On a copy of the
kit's committed `.custodian` (123 receipts), it compared 7 specs, and both
lines picked the same pair for each.

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
