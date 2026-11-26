# Unit-level scheduling: implementation design

- **Input plan:** the owner's unit-level scheduling plan, kept in the owner's project (Vietnamese, sections 1-7).
- **Applies to:** this repository (dtech-aidlc 1.0.0 Rust source: `rust-cli/**`, `core/**`, `harness/**`, `tests/**`).
- **Owner decisions already made:** implement all milestones M1-M5. Scheduling is unit-level only: there is no `--strict-order` or stage-major option, and in-flight 1.0.0 intents migrate to unit-level scheduling. The auto-park threshold N defaults to 3 and can be set in `.aidlc/config.json`.
- **Evidence used:** the Rust source at the current `main` (b3e97b0), and a read-only inspection of the owner's intent `<real-intent>` (its `state.json`, `audit.jsonl`, `work-units.json`, `work-units.md`, `test-strategy.md`, `ids.json` and `build-and-verify/WU-06/regression-report.md`). Nothing under the owner's project was written, run or touched with git.

---

## 1. Verification of the plan's "current behaviour" claims (section 1, A-G)

Line numbers refer to the files as they are at b3e97b0.

| # | Claim (plan) | Verdict | Evidence |
|---|---|---|---|
| A | The selector works stage-first, then unit. A blocked unit keeps `currentStage` still. `doctor` reports "currentStage build-and-verify is not the first unfinished stage". | **Partly.** | The repeat span is already **unit-major**: `select_next_stage` (`rust-cli/orchestrate.rs:535-586`) takes the first unit in `state.units` whose `status` is unfinished (`:555`), then the first repeating stage that unit has not finished (`:558-579`). So WU-01 runs red-tests and then build-and-verify before WU-02 starts. Two parts of the claim do hold. (1) `dependsOn` is never read when selecting. Only the array order of `work-units.json` counts (`apply_work_units` copies `dependsOn` into state at `:656-666`, but nothing reads it). (2) A blocked unit is never "finished", because `stage_finished` only accepts `complete` and `skipped` (`rust-cli/workflow/mod.rs:1122-1124`). It is therefore selected again on every `next`, and one blocked unit holds the whole intent. The doctor message has a different cause from the one the plan gives. `check_workflow` (`rust-cli/doctor.rs:1024-1035`) compares `currentStage` with the first unfinished stage *row*, and the `red-tests` row stays `in_progress` until every unit has finished red (`orchestrate.rs:2558-2577`). So doctor **fails** whenever `currentStage` is `build-and-verify` for any unit except the last. It is a false failure caused by normal unit-major interleaving, not by blocking. |
| B | `orchestrate report` only has `--stage`. The current unit is implicit. | **Confirmed.** | `ORCHESTRATE_REPORT_FLAGS` (`rust-cli/args.rs:136-165`) has no unit option. The report writes to `unit_key(current_unit(&state))` (`orchestrate.rs:2545-2556`). There is no way to name or park a different unit. |
| C | `--needs` is only for environment prerequisites. There is no "waiting for another unit" or "foreign test red" kind. | **Confirmed.** | `--needs` is accepted only with `--result blocked` (`orchestrate.rs:2303-2306`), and every name must be a declared prerequisite (`rust-cli/prerequisites.rs:93`, `require_known_prerequisites`). The workaround is visible in the real `work-units.json`: WU-06 `dependsOn` lists `["WU-03","WU-04","WU-16"]`, and WU-16 was moved ahead of WU-06 in the array. |
| D | Regression is workspace-wide and charged to each unit, so 13 + 3 WU-16 tests block WU-06. | **Partly.** | The CLI has **no** regression concept. It runs no tests, and report validation only checks artifacts and identifier citations (`orchestrate.rs:2365-2452`). The per-unit blocking came from the methodology and the agent. `builder-verifier.md:46` says "Run the full existing suite after the slices are complete … A baseline-passing test that now fails blocks completion", and `develop.md:45` says "A regression blocks the release candidate". WU-06's own "Done when" in `work-units.md` adds the requirement that the full normal and ignored workspace runs pass after WU-16. The real reports (`build-and-verify` 2026-11-25T02:25:18Z and 2026-11-26T02:17:13Z) were blocked by 13 normal and 3 ignored WU-16 tests. Strictly, those tests are *new* tests owned by WU-16, not baseline-passing ones, so the methodology rule did not even require the block. There is no attribution mechanism to make that distinction. |
| E | `intent rework` sends the whole intent back to `implementation-design`, and red-tests/build-and-verify then reset every unit to `pending`. | **Confirmed.** | `request_design_rework` (`rust-cli/intent_commands.rs:383-489`) resets every unit of every downstream Develop row to `pending` (`:446-462`), removes `state.units` (`:466`) and sets `designReworkPending` (`:467`). After that, `select_next_stage` picks the first unit whose stage entry is not finished, which is WU-01 again. The real state agrees. After the 2026-11-26T02:17:13Z rework, every `red-tests`/`build-and-verify` unit entry is `pending` and `currentUnit` is `WU-01`. The replays are visible in the reports: 6 `skipped` red-tests reports (WU-01 twice, WU-02, WU-03, WU-04, WU-05), 1 `skipped` build-and-verify report (WU-01), and 4 "refresh" build-and-verify `complete` reports (WU-02 to WU-05) on 2026-11-24. |
| F | Loading `work-units.json` can fall back to a 1-unit placeholder. | **Confirmed, root cause found.** | `work_units_of` returns `[{"id": null, "status": "in_progress"}]` whenever `state.units` is missing or empty (`orchestrate.rs:477-482`). Its key is `"_"` (`:494-498`). `apply_work_units` returns silently while `designReworkPending` is true (`:608-610`), and also when the file is missing, unreadable, unparseable or empty (`:643-650`). **Root cause in the real run.** `designReworkPending`/`designRework` are cleared only in the *not-blocked* branch of `orchestrate_next` (`:1539-1555`). When `implementation-design` itself was last reported `blocked`, the `was_blocked` branch (`:1508-1538`) runs instead and never clears them. Audit sequence: 2026-11-25T06:56:01Z `implementation-design` blocked → 06:56:23Z `design.rework.requested` (fromStage `implementation-design`) → 06:56:52Z `stage.selected` with rework:true (the blocked branch) → 06:58:20Z complete → `red-tests` runs with the placeholder. The real `state.json` still has `stages.red-tests.units["_"]` reported at 2026-11-25T07:16:17.519Z, which is the report that describes the placeholder. |
| G | No per-unit attempt counter, and a repeated blocker is not detected. | **Confirmed.** | `attempt` is `reports.len().max(1) + 1` over **all** reports of the stage row, for every unit (`orchestrate.rs:1534`). Nothing compares blockers. In the real run the 2026-11-23T10:40:30Z red-tests report says "Attempt 20" (19 earlier red-tests reports across WU-01 to WU-06). WU-06 alone has 16 `blocked` red-tests reports, all with different free text. |

**Further facts that shape the design:**

1. `ids.json` TS entries never trace to a WU. In the real registry, 0 of 69 TS entries have a `WU-` trace. Test ownership exists only as Markdown prose and tables: `test-strategy.md` "Work unit to test mapping and exit", and `work-units.md` "Work unit mapping" and "Coverage cross-check". These use ranges (`TS-20..TS-29`) and letter suffixes (`TS-27a`, `TS-44b`) that `ids.rs::valid_id` rejects (`rust-cli/ids.rs:23-27`). The registry note says suffixed tests "are registered under their base number".
2. Real failing test names carry the TS number as a lowercase prefix of the last path segment, for example `suite_a::tests::ts27a_case`. Some do not, for example `suite_b::tests::unregistered_case`. Several TS numbers have more than one owner (TS-42, TS-44, TS-56, TS-65, TS-30, TS-32).
3. `normalise_state` forces `"version": 2` on every load (`rust-cli/workflow/mod.rs:1155-1160`), and `validate_intent_state` does not check `version` (`:1186-1260`). A 1.0.0 binary therefore loads any version number and rewrites it as 2. It keeps unknown top-level keys, because `serde_json` `Value` maps with `preserve_order` round-trip them.
4. The 1.0.0 reports have no `unit` field. Each per-unit stage entry `stages[s].units[U] = {status, reportedAt}` keeps `reportedAt` even after a rework resets `status` to `pending`. Joining `reportedAt` with the row's `reports[].at` recovers the pre-reset result. For example, red-tests WU-02 `reportedAt` 2026-11-24T09:58:42.653Z matches the `skipped` report at the same instant.
5. There is no serde derive in `Cargo.toml` (only `serde_json` with `preserve_order`, and `sha2`). Builds are offline, so no dependency may be added. New code stays `Value`-based, with small hand-written views.
6. 1.0.0 unit attribution of old reports is unreliable. The last `build-and-verify` report (2026-11-26T02:17:13.868Z) is stored under `units["WU-01"]`, because WU-01 was the implicit current unit after the 2026-11-25 rework, but its text is about the WU-06 release regression ("13 normal-workspace and 3 ignored-workspace failures, all WU-16"). So a foreign regression blocked WU-01 as well as WU-06, and `priorResults` recovered by the `reportedAt` join (section 3.6) must be treated as information, never as evidence about a unit.
7. `implementation-design` completion already validates `work-units.json` with `ids::work_unit_problems` at full and standard depth (`artifacts.rs:285-287`). A missing or invalid file at the repeat span therefore means either the file was changed after the stage completed, or the state predates that validation.
8. 1.0.0 never checks a gate when moving between phases. Gates are enforced only in `orchestrate report` (`is_phase_gate`). `select_next_stage` falls through to the stages after the repeat span as soon as the first unfinished entry of `state.units` has no unfinished repeating stage (`orchestrate.rs:554-584`). The 1.0.0 compatibility mirrors (section 3.4) must take this into account.
9. Unit ids may be legacy `U-01`; `ids::normalize_id` maps them to `WU-01` (`ids.rs:30-37`), but 1.0.0 keeps the id as written in `state.units`, in `stages[s].units` keys, in `currentUnit` and therefore in artifact folder names.

---

## 2. Scope and module layout

| New or changed | Responsibility |
|---|---|
| **new** `rust-cli/work_units.rs` | Loads `work-units.json` strictly, plus test ownership. Holds the ledger model (read, write, sync), migration from 1.0.0, the selector, park, revisit, auto-park signatures, fingerprints, rework/split reconciliation, waves, and the queue view. Pure functions over `Value` plus file reads; no state writes. |
| **new** `rust-cli/regression.rs` | Parses and validates the `regression.json` verification artifact, infers TS numbers from test names, attributes failures to owners, and keeps the deferred-regressions ledger. Pure. |
| **new** `rust-cli/unit_commands.rs` | Handlers for `orchestrate park`, `orchestrate queue` and `orchestrate revisit`. They load state, call `work_units`, persist through the same `ensure_unchanged`/`save_intent` path as `orchestrate.rs`, and audit. |
| `rust-cli/orchestrate.rs` | `orchestrate_next` and `orchestrate_report` use the ledger when it exists. Adds the `designRework` cleanup fix, per-unit rework info, the sweep step, the gate rule and the directive additions. `apply_work_units` is retired (section 4.4). `persist_active`, `ensure_unchanged`, `load_intent`/`save_intent` and `audit` become `pub(crate)` so `unit_commands.rs` reuses them. |
| `rust-cli/intent_commands.rs` | `intent rework --unit`, `intent split`, and rework without `--unit` for ledger intents. |
| `rust-cli/doctor.rs` | Relaxed currentStage rule, work-unit load errors, inverted dependency, unassigned tests, waves, parked units, stale ledger. |
| `rust-cli/status.rs` | Adds a `units` summary to `status`. |
| `rust-cli/ids.rs` | `work_unit_problems` validates the new optional `tests` field. Adds a helper `ts_owners(registry)` for `traces` that point to WU ids. |
| `rust-cli/artifacts.rs` | Cross-checks `tests` against the registry when `implementation-design` is reported. |
| `rust-cli/scope.rs`, `rust-cli/config_commands.rs`, `rust-cli/doctor.rs` | Validate the `autoParkThreshold` key (three validators, next to `unitSplitThreshold`). |
| `rust-cli/args.rs`, `rust-cli/main.rs`, `rust-cli/help.txt`, `rust-cli/completion.zsh` | CLI surface. |
| `rust-cli/lib.rs` | `pub mod work_units; pub mod regression; pub mod unit_commands;` |
| `core/methodology/**`, `harness/*/skills/aidlc/SKILL.md`, `core/data/projection-history.json`, `README.md`, `CHANGELOG.md` | M5 documentation. |

The phase graph (`core/data/phase-graph.json`) is **not** changed: no new output, no `schemaVersion` bump. The `regression.json` path is derived by the CLI, see section 8.

---

## 3. State schema

### 3.1 Placement and version

The intent `state.json` keeps `"version": 2` and gains one optional top-level key, **`workUnits`** (the "ledger"), with its own `schemaVersion: 1`. No existing key is removed or renamed. See Decision D1 for why the top-level version is not bumped.

The ledger exists only when the intent has real work units: a `work-units.json` that lists one or more units at `full` or `standard` depth. `fast` depth (where `work-units-json` is not produced) and a `{"status": "not-applicable", "reason": …}` file keep the 1.0.0 implicit single-unit path unchanged. That path still uses `select_next_stage` and `work_units_of`, and it creates no ledger.

### 3.2 Ledger shape

```jsonc
"workUnits": {
  "schemaVersion": 1,
  "source": {
    "path": ".aidlc/intents/<id>/develop/implementation-design/work-units.json",
    "sha256": "<hex of the bytes read>",
    "registrySha256": "<hex of ids.json bytes or null>"
  },
  "order": ["WU-01", "WU-02", "…"],          // array order of work-units.json (tie-break order)
  "progressSeq": 7,                          // +1 each time any unit becomes satisfied (see 3.3)
  "syncedAt": "<ISO>",                       // last write by a unit-aware binary
  "units": {
    "WU-06": {
      "title": "Allocation reconciliation and retention store contracts",
      "dependsOn": ["WU-03", "WU-04"],
      "tests": ["TS-42", "TS-44", "TS-56", "TS-65"],   // owned TS ids (union of sources, 3.5)
      "status": "parked",                    // pending|red|blocked|green|verified|done|parked
      "selectedAt": "<ISO>",                 // last time next selected a step of this unit (regression.json freshness, 8.2)
      "steps": { "red-tests": "complete", "build-and-verify": null },  // complete|skipped|blocked|null
      "attempts": { "red-tests": 1, "build-and-verify": 2 },           // reports for this unit and stage since its last reset
      "blockedStreak": { "signature": "<hex>", "count": 2 },            // consecutive same-signature blocked reports
      "park": {                              // open park record, or null
        "kind": "foreign-regression",
        "blockedBy": ["TS-20"],              // as given
        "blockedByUnits": ["WU-16"],         // resolved owners (dep-unit: the same as blockedBy)
        "signature": "<hex>",
        "attempts": 2,                       // attempts[step] when parked
        "detail": "…",
        "at": "<ISO>",
        "step": "build-and-verify",
        "auto": false,                       // true when auto-parked by signature
        "resumeStatus": "red",               // status to restore on revisit
        "seq": 5                             // progressSeq when parked (drain rule, 4.4)
      },
      "parkHistory": [ { "...closed park records...", "revisitedAt": "<ISO>", "revisitReason": "blocker-done|drain|manual|rework|resync", "revisitDetail": null, "decidedBy": null } ],
      "inputFingerprint": "sha256:<hex>",    // section 7.2
      "priorResults": [ { "stage": "red-tests", "result": "complete", "at": "<ISO>" } ],  // migration only (3.6)
      "history": [ { "stage": "red-tests", "result": "complete", "at": "<ISO>" } ],      // every finished/blocked step, never reset
      "reopened": [ { "at": "<ISO>", "by": "sweep|rework|split", "tests": ["…"] } ],
      "removed": false,                      // true when a later work-units.json dropped it (kept for history)
      "splitInto": null,                     // ["WU-06","WU-17"] when replaced by intent split
      "updatedAt": "<ISO>"
    }
  },
  "deferredRegressions": [
    { "test": "suite_a::tests::ts20_case",
      "ts": ["TS-20"], "owner": "WU-16", "unowned": false,
      "status": "open",                      // open|resolved
      "firstSeen": "<ISO>", "lastSeen": "<ISO>", "reportedBy": ["WU-06"], "resolvedAt": null, "suite": "normal" }
  ],
  "sweep": { "status": "pending", "selectedAt": null, "at": null, "failures": [], "attempts": 0, "via": null },  // pending|red|green; via: null|"1.0.0-gate" (3.6)
  "baselineTests": [],                       // test names declared baseline by an accepted unit regression.json (10)
  "rework": null,                            // pending rework/split request, see 7.1
  "migration": { "from": "1.0.0-stage-rows", "at": "<ISO>", "ignoredKeys": ["_"], "priorResults": 12, "preSha256": "<hex of state.json bytes before migration>" }
}
```

**Unit keys.** `units` keys, `order`, mirrors, `currentUnit` and artifact folders use the id **as written** in `work-units.json`, which is what 1.0.0 used (fact 9). Every comparison (`dependsOn`, `--unit`, `--blocked-by`, `--into`, ids.json traces, ownership) goes through `ids::normalize_id`, so `U-01` and `WU-01` name the same unit. `--unit` accepts either spelling.

**Authority.** Between ledger creation and the next reconcile (section 7.1), the ledger is the only source for `order`, `dependsOn` and `tests` (ownership). `work-units.json` and `ids.json` are not re-read; a change of their sha256 against `source` is reported as drift (section 9) and takes effect only through `intent rework` / `intent split`.

### 3.3 Unit statuses (exact meaning)

| Status | Meaning | Set by | Satisfies `dependsOn`? | In queue? |
|---|---|---|---|---|
| `pending` | No repeating step finished since creation or the last reset. | creation, rework/split reset | no | ready when deps satisfied |
| `red` | `red-tests` finished (`complete` or `skipped`); `build-and-verify` is next. Also the state of a unit reopened by the sweep. | report, sweep reopen | no | ready when deps satisfied |
| `blocked` | The last report for the unit's current step was `blocked` and it is not parked. It is retried. | report blocked | no | ready (retry) |
| `green` | `build-and-verify` was accepted for the unit, but the regression evidence still had foreign (deferred) or unowned failures, or a non-`full` scope, or the step was `skipped`. | report complete/skipped | **yes** | no |
| `verified` | `build-and-verify` was accepted and a `full`-scope regression run had no failures outside the baseline. Either it was the unit's own run, or a later full run upgraded every `green` unit to `verified` (section 8.4). | report complete, upgrade | **yes** | no |
| `done` | The final strict sweep passed. Every satisfied unit becomes `done` there. | sweep green | **yes** | no |
| `parked` | Set aside with a park record. It is not retried until it is revisited. | `orchestrate park`, auto-park | no | no (shown as parked) |

`satisfied(u) := status ∈ {green, verified, done}`. Where the plan says a dependency must be "`done`" (3.1, 3.2), this design reads it as *satisfied* (Decision D3). `done` itself is reserved for "confirmed by the release sweep".

`progressSeq` increments each time a unit goes from not satisfied to satisfied.

### 3.4 Mirrors kept for 1.0.0 compatibility (rollback safety)

Each ledger mutation calls `work_units::sync_mirrors(state)`, which rewrites only these keys:

- `stages[s].units[U] = {status, reportedAt}` for every repeating stage `s`. `status` is `steps[s]`, or `"pending"` when it is null (the same value 1.0.0 rework writes). `reportedAt` is preserved, and updated when the step changes.
- `state.units = [{id, status, dependsOn}]` in `order`. `status` is `"complete"` when the unit is satisfied, otherwise `"in_progress"`. This is what 1.0.0's selector reads.
- `stages[s].status` for repeating stages, derived as in section 5. While a ledger rework or split is pending (`workUnits.rework` set), every repeating row is mirrored `in_progress` with `reworkResetAt` = the request time, as 1.0.0 rework does. Otherwise doctor's "recorded complete although the earlier stage is not finished" rule would fail while `implementation-design` is back in progress.
- **Release gate projection.** When every non-removed unit is satisfied and `sweep.status != "green"`, the *gate unit* (the last non-removed unit in `order`) is mirrored as not finished: `state.units[gate].status = "in_progress"` and `stages["build-and-verify"].units[gate].status = "pending"` (its `reportedAt` is kept). Without this, a 1.0.0 binary would see every unit complete and fall through past `build-and-verify` with no release-candidate gate (fact 8). With it, 1.0.0 selects the gate unit's `build-and-verify` as its final unit, which in 1.0.0 *is* the release-candidate phase gate. The projection only affects mirrors; the ledger keeps the true step.
- `currentStage` and `currentUnit` are set by `next`. `report` clears `currentStage` and keeps `currentUnit`, as 1.0.0 does, so `orchestrate park` right after a blocked report defaults to that unit. `park` of the current step's unit clears `currentStage`.

Reports appended to `stages[s].reports[]` gain `"unit": "<id>"`, and for parks `"parked": {kind, blockedBy, auto}`. The key is added only on ledger intents, so 1.0.0 golden snapshots are not affected.

### 3.5 Test ownership sources

`work_units::ownership(work_units_json, registry) -> BTreeMap<TS, BTreeSet<WU>>` is the union of:

1. **`work-units.json` per-unit `tests`.** This is a new, optional array of `TS-` registry ids (`^TS-\d{1,5}$`). `ids::work_unit_problems` validates it: "work-units.json WU-06 tests must be an array of TS- identifiers". When `implementation-design` is reported, `artifacts.rs` cross-checks it against `ids.json`: "work-units.json WU-06 tests cite unregistered TS-99". A suffixed test is listed under its base number, following the existing registry convention.
2. **`ids.json` TS entries whose `traces` contain `WU-` ids.** The registry already allows such traces (`ids.rs:310-334`).

The Markdown tables are **not** parsed (Decision D6). M5 changes the work-units template and the implementation-planner instructions so the planner writes `tests`.

The union is computed when the ledger is created, migrated or reconciled and stored as each unit's `tests`. Attribution, park resolution and doctor read only the stored `tests` (see "Authority" in 3.2).

**No declared ownership.** The owner's real intent has none: no `tests` field and 0 TS→WU traces. If attribution treated every failure as `unowned` there, a unit's own failing tests would never block it, and the per-unit check would mean nothing. So when the stored ownership map is empty, a failure with no explicit `owner` is attributed to the **reporter** (Decision D17). The agent can still defer a failure by naming another unit as `owner` in `regression.json`. That is how WU-06 defers its 16 WU-16 failures today.

### 3.6 Migration from 1.0.0 state

`work_units::ensure_ledger(root, graph, state) -> Result<LedgerChange, String>` runs at the start of `orchestrate next`, `orchestrate report`, `orchestrate park`, `orchestrate revisit`, `intent rework` and `intent split`. Read-only commands (`orchestrate queue`, `status`, `doctor`) call it on an in-memory copy and never persist the result.

It runs only when all of the following hold:
- `state.workUnits` is absent.
- Every stage before the repeat span is finished. A repeating row that already exists is **not** enough. A 1.0.0 `intent rework` that is still open (`implementation-design` in progress, `designReworkPending`, `state.units` removed) has repeating rows while the agent may be rewriting `work-units.json`. Migrating then would read a half-edited file.
- `load_work_units` (section 9) returns units. A load error is returned as the command's error.

**A 1.0.0 rework that is still open at upgrade time** is therefore not migrated on `next`. `next` selects `implementation-design`, a `before` stage, as today. The ledger is built at that stage's `complete` report, at the same point where a new intent creates it (section 7.1). Because repeating rows exist, it is built with the migration table below and audited as `units.migrated`, not `units.created`, so `priorResults` and `ignoredKeys` are still recorded.

**Order with the v1 migration.** A legacy v1 project state is first migrated by the existing `state_migration::migrate_legacy_state` (unchanged, with its own backup). The ledger migration only ever sees a v2 intent state.

**Purity guarantee.** `ensure_ledger`'s output differs from its input only in the new `workUnits` key, plus `currentStage`/`currentUnit` when they are cleared as described below. Every other effect of the first write comes from the command itself (`next` selecting a step, `report` recording one). A unit test asserts this by diffing `ensure_ledger` input and output on `state-1.0.0.json`. Because of this guarantee, "remove `workUnits`" is a real rollback, and `migration.preSha256` records the original bytes' hash for forensic comparison.

**Mapping for each unit id `U` in `work-units.json` order:**

| 1.0.0 source | Ledger field |
|---|---|
| `stages["red-tests"].units[U].status ∈ {complete, skipped}` | `steps["red-tests"] = that` |
| `… == "blocked"` | `steps["red-tests"] = "blocked"` |
| `… == "pending"` or absent | `steps["red-tests"] = null` |
| same for `build-and-verify` | `steps["build-and-verify"]` |
| `state.units[i].status ∈ {complete, skipped}` (set by 1.0.0 at the last repeat of the phase) | unit is satisfied → `green` |
| otherwise, first match wins: b&v step finished → `green`; b&v step `blocked` → `blocked`; red finished → `red`; red step `blocked` → `blocked`; else `pending` | `status` |
| `stages[s].units[U].reportedAt` == `stages[s].reports[k].at` while `status == "pending"` (a reset by 1.0.0 rework) | `priorResults += {stage, result: reports[k].result, at}`. Information only: it never restores status, and the unit it names may be wrong (fact 6) |
| unit keys in `stages[s].units` that are not in `work-units.json` (for example the placeholder `"_"`) | left untouched in the stage row and listed in `migration.ignoredKeys` |
| 1.0.0 reports cannot be attributed to a unit | `attempts = {}`, `blockedStreak = null`, `history` = finished steps with their `reportedAt` |
| — | `inputFingerprint` computed from the current design files (section 7.2) as the baseline |

`currentStage` and `currentUnit` are kept if they name a unit and the step the new selector would pick for that unit. Otherwise both are cleared. For a cleared state, `orchestrate report` says "No current stage; run orchestrate next first", as today.

**Real intent, as computed during this design (re-checked in review):** the real state has no `designReworkPending` or `designRework`, `implementation-design` is complete, and `state.units` holds the 16 units, so it migrates on the first command. Every unit migrates to `pending`, because the last rework reset all unit entries. 12 `priorResults` are recovered by joining `reportedAt` with the reports: red-tests WU-01 to WU-05 `skipped` and WU-06 `complete` (2026-11-24T17:06:28.829Z); build-and-verify WU-01 `blocked`, WU-02 to WU-05 `complete`, and WU-06 `blocked`. `"_"` goes to `ignoredKeys`. `currentStage: red-tests` / `currentUnit: WU-01` is consistent with the new selector, so it is kept.

**Persistence and audit.** The migration is persisted only as part of the command's normal state write: `next` persists the directive state, `report` and `park` persist their change. After that write it appends one audit row:
`{"event":"units.migrated","intentId":…,"units":16,"from":"1.0.0-stage-rows","ignoredKeys":["_"],"priorResults":12}`.
Older rows in `audit.jsonl` are never rewritten.

**Idempotence.** Migration is a pure function of (state, `work-units.json`, `ids.json`, `work-units.md`). It runs only when `workUnits` is absent, so a second run is a no-op and writes no second `units.migrated` row. A failed command writes nothing (validate first, then `preflight_audit`, then `persist_active` with `ensure_unchanged`), so the next run migrates again from the same input.

**Rollback safety.**
1. Migration only adds `workUnits` (purity guarantee above), and every later write keeps the mirrors (section 3.4) up to date. Deleting the `workUnits` key gives back a state that 1.0.0 drives correctly, including its release-candidate gate (gate projection, section 3.4). No backup file is written (Decision D2).
2. A 1.0.0 binary can still load and drive the migrated state. It ignores `workUnits`, selects by array order from `state.units` and `stages[s].units`, and keeps `workUnits` untouched, since unknown keys survive its load and save. It applies 1.0.0 rules: a parked unit is simply retried.
3. If a 1.0.0 binary changed the state afterwards, the unit-aware binary detects it in `ensure_ledger`. Every command writes `syncedAt` with the same `now` it uses for `reportedAt`, so its own writes never look newer than the ledger. The ledger is stale when any of these is later than `workUnits.syncedAt`:
   - a `stages[s].units[U].reportedAt`;
   - a repeating row's `reworkResetAt`, or `stages["implementation-design"].reworkRequestedAt`. This covers a 1.0.0 `intent rework`, which resets entries without touching `reportedAt`. That rework is followed by a 1.0.0 `next` that clears `designReworkPending`, so the flag alone is not enough;
   - `stages["implementation-design"].reportedAt` while `workUnits.rework` is pending;
   - `gates["release-candidate"].at`.

   It is also stale when `designReworkPending: true` exists without a ledger `rework` record. Re-sync then runs in this order:
   - **Legacy rework.** A 1.0.0 `intent rework` newer than `syncedAt` becomes a ledger rework request with no named units (section 7.1), and `designReworkPending` is removed. If `implementation-design` has already been reported complete after that request, by 1.0.0 or by a ledger rework completed under 1.0.0, the reconcile of section 7.1 runs **now**, using the stored fingerprints. So only units whose design changed are reset, and the 1.0.0 whole-intent reset of the mirrors is not taken literally.
   - **Per unit.** The steps and status of each unit with newer entries are re-derived from the stage entries with the migration table, and its open park is closed with `revisitReason: "resync"`. Units without newer entries keep their ledger fields.
   - **Legacy gate.** A `gates["release-candidate"]` that 1.0.0 recorded after `syncedAt` with `decision: "approved"` was approved under 1.0.0 rules on the gate unit (projection). It is accepted: `sweep = {status: "green", via: "1.0.0-gate"}`, and every satisfied unit becomes `done`. Human gate decisions are never discarded.
   - Audit: `units.resynced {intentId, units:[…], legacyRework: bool, legacyGate: bool}`.

---

## 4. Selector

### 4.1 Entry point

`orchestrate_next` (`orchestrate.rs:1366-1376`) changes from
`apply_work_units(...); select_next_stage(&mut state, &stages)`
to:

```text
ensure_ledger(...)?                                   // may migrate, may error (section 9)
match state.workUnits {
  None    => select_next_stage(&mut state, &stages)   // unchanged 1.0.0 path (fast / not-applicable)
  Some(_) => work_units::select(&mut state, &stages, &config) -> Selection
}
```

`Selection` is one of these:
- `Stage(stage)`: a stage outside the repeat span.
- `Unit{stage, unit, revisits}`: a work unit's next step.
- `Sweep{stage}`: the final strict sweep.
- `Stalled{parked, waiting}`: nothing can be selected.
- `Done`: everything is finished.

### 4.2 Algorithm (`work_units::select`)

1. `before` = stages before the first repeating stage (as today, `:547-553`). If one is unfinished, return `Stage`, and clear `currentUnit`.
2. **Automatic revisit.** For each parked unit, in `order`:
   - kind `dep-unit` or `foreign-regression` with every `blockedByUnits` satisfied → close the park (`revisitReason: "blocker-done"`), set `status = park.resumeStatus`, and queue a `unit.revisited` audit row.
   - Other kinds are never revisited automatically.
3. **Ready set**: units with `!removed`, `status ∈ {pending, red, blocked}`, and every `dependsOn` satisfied. Choose the **first in `order`**. Because `dependsOn` is acyclic and readiness requires satisfied dependencies, this is a valid dependency order. The array order is the planner's tie-break. A `blocked` unit stays in place and is retried, and auto-park (section 6) stops endless retries.
4. If a unit was chosen, its step is the first repeating stage `s`, in graph order, with `steps[s]` not in {complete, skipped}. Set `currentUnit` and `selectedAt`, and return `Unit`.
5. **Drain** (no ready unit, but some parked unit is drain-eligible). A parked unit is drain-eligible when its kind is `dep-unit` or `foreign-regression` and `park.seq < progressSeq`, meaning something became satisfied after it was parked. Revisit the first such unit in `order` (`revisitReason: "drain"`) and restart from step 3. This is the plan's "when all workable units are finished, return to the parked ones" (section 2). Termination:
   - Each restart un-parks one unit, so the loop runs at most once per parked unit.
   - Across commands, a unit that is parked again gets `seq = progressSeq`, so drain revisits it at most once per progress event.
6. **Sweep.** If `sweep.status == "green"`, skip to step 8. Otherwise, when every non-removed unit is satisfied (a satisfied unit is never parked), return `Sweep` on the last repeating stage (`build-and-verify`), clear `currentUnit` and set `sweep.selectedAt`.
7. **Stalled.** Otherwise → `Stalled`. Because `dependsOn` is acyclic, the earliest unsatisfied unit in dependency order always has satisfied dependencies, so it is ready unless it is parked. A stall therefore always names at least one parked unit: `owner-decision`, `design-defect`, or a `dep-unit`/`foreign-regression` park that is not drain-eligible. Units waiting on parked units are listed as `waiting`. `orchestrate next` prints a hold payload and exits with `BLOCKED_EXIT_CODE` (3).
   - If steps 2 or 5 revisited a unit during this call, those revisits are persisted and audited (validate → `preflight_audit` → `persist_active` → `unit.revisited`) before the payload is printed.
   - Otherwise nothing is written. `currentStage` and `currentUnit` are left untouched by a stall.

```json
{ "complete": false, "blocked": true, "reason": "queue-stalled", "intentId": "…",
  "parked": [ {"unit":"WU-10","kind":"owner-decision","blockedBy":["CFG-17","CFG-18"],"detail":"…","at":"…"} ],
  "waiting": [ {"unit":"WU-15","waitingOn":["WU-10"]} ],
  "hint": "No work unit can run. WU-10 waits for an owner decision: record it, then run 'dtech-aidlc orchestrate revisit --unit WU-10 --detail \"<decision>\"'. A design defect needs 'dtech-aidlc intent rework --unit <id> --detail \"<reason>\"'." }
```

The hint is built per root park kind: `owner-decision` → revisit with the decision; `design-defect` → `intent rework --unit`; a non-eligible `dep-unit`/`foreign-regression` → "WU-06 waits on WU-16, which cannot run; revisit WU-06 with 'orchestrate revisit --unit WU-06' or resolve WU-16 first".

8. After the repeat span (the gate has passed) → the first unfinished stage of `after`, as today.

### 4.3 Prerequisites and rework info per unit

The current code reads the stage row's last report (`:1380-1395`). On ledger intents, "last report" means the last entry of `stages[s].reports` with `unit == U`. A migrated unit with no tagged report falls back to `steps[s] == "blocked"`, with `needs` taken from the last untagged report only when it is the row's last report. With that substitution, the existing code then works per unit:
- `was_blocked` is computed per unit.
- `--needs` prerequisite holds work per unit.
- `stage.rework` gains `{"unit": U, "attempt": attempts[s] + 1, "blockedStreak": n, "autoParkThreshold": N}` in place of the stage-wide `reports.len() + 1`. Non-ledger intents keep the old field so `rust_cli.rs` (`attempt == 2`) and the goldens stay unchanged.
- A revisited unit gets `stage.rework.revisit = {kind, blockedBy, detail, parkedAt, reason}`.

### 4.4 Fix for the F root cause (applies to every intent)

In `orchestrate_next`, the `designRework` / `designReworkPending` cleanup (`:1547-1555`) moves out of the `else` branch. It now runs whenever `next_id == "implementation-design"`. When both `was_blocked` and `designRework` are true, the `rework` info merges the blocked info with `requestedAt`, `fromStage` and `detail` (the request's keys win).

`apply_work_units` is **retired**, not patched. Only removing its `designReworkPending` early return would be a new bug: `apply_work_units` would then fill `state.units` from the *old* `work-units.json` while `implementation-design` is being reworked, and its "already filled" early return would hide the redesigned file afterwards. Its job moves to `load_work_units`, which `ensure_ledger` calls only when the ledger is built or reconciled. The non-ledger path, at fast depth or with a `not-applicable` file, by definition has no units to load, so `work_units_of` returns the implicit unit there as before.

Regression test: "implementation-design blocked → rework → next → complete → next selects red-tests with `currentUnit: WU-01` and no `_` key".

---

## 5. Intent-level `currentStage`, stage rows, gates and doctor

`work_units::sync_mirrors` derives these after every ledger change:

- `stages["red-tests"].status`:
  - `"complete"` when every non-removed unit has `steps["red-tests"] ∈ {complete, skipped}` and at least one of them is `complete`.
  - `"skipped"` when all of them are `skipped`.
  - otherwise `"in_progress"`, provided the row exists or any unit has started.
  - The row is created on the first selection, as today, with `startedAt`.
- `stages["build-and-verify"].status`: `"complete"` (or `"skipped"`) **only** when the sweep report passes the release-candidate gate (section 10). Until then it is `"in_progress"`. A repeating row is never `"blocked"` on ledger intents, because blocking is recorded per unit.
- `currentStage` is the stage of the selected step: a unit's step, or `build-and-verify` for the sweep.
- `derive_status`, `progress_of`, `complete_payload` and `intent list` progress keep working on the rows unchanged.

`is_final_unit` / `is_phase_gate` (`:505-522`): for a ledger intent and a repeating stage, `is_phase_gate` is true **only for the sweep step**. It is false for every per-unit step, including the last unit. Non-ledger intents keep the 1.0.0 rule.

`report` has no `Selection`, so the sweep step is identified from persisted state. It is the sweep when all of these hold:
- a ledger exists;
- `currentStage` is the last repeating stage;
- `currentUnit` is absent;
- `sweep.selectedAt` is set.

`currentStage` set to a repeating stage with no `currentUnit` in any other case is refused ("No current work unit; run orchestrate next").

Doctor rules (`doctor.rs:1007-1036`), on ledger intents only:
- "stage X is recorded complete although the earlier stage is not finished": unchanged.
- "currentStage … is not the first unfinished stage": accepted when every stage before the span is finished and `currentStage` is a repeating stage of the span. This removes the false failure from claim A.

---

## 6. Blocked-kind taxonomy, park, revisit and auto-park

### 6.1 Kinds

| Kind | How it is declared | Validation | Handling |
|---|---|---|---|
| `env-prerequisite` | `orchestrate report --result blocked --needs NAME` (unchanged) | declared prerequisite (`prerequisites.rs:93`) | Not a park. `next` holds the unit while the check fails, as today. Excluded from auto-park. `orchestrate park --kind env-prerequisite` is refused: "use orchestrate report --result blocked --needs NAME". |
| `dep-unit` | `orchestrate park --kind dep-unit --blocked-by WU-16` | at least one id; each a non-removed unit ≠ the parked unit; **refused when any blocker (transitively) `dependsOn` the parked unit** ("inverted dependency: WU-16 depends on WU-06, so WU-06 cannot wait for it; fix the order with intent rework --unit"); **refused when every blocker is already satisfied** (see the park rules below); **refused when the park would close a wait cycle** (see the park rules below); a warning when the blocker is not in `dependsOn` | Parked. Revisited automatically when every blocker is satisfied (step 2), or by drain. |
| `foreign-regression` | `orchestrate park --kind foreign-regression --blocked-by TS-20 [--blocked-by WU-16]` | each id a `TS-` (resolved through the stored ownership, section 3.5) or a unit id; the resolved owners must be non-empty ("TS-20 has no owning unit in work-units.json; name the unit with --blocked-by WU-xx") and must not include the parked unit; **refused when an owner (transitively) depends on the parked unit**, with a hint to "report complete; failures owned by WU-16 are deferred (attributed regression)"; the satisfied-blocker and wait-cycle rules below apply as for `dep-unit` | Parked. Revisited automatically when every owner is satisfied, or by drain. With attribution (M3) the agent normally needs no park, because foreign failures do not block the unit. |
| `owner-decision` | `orchestrate park --kind owner-decision [--blocked-by CFG-17]...`, or **auto-park** | `--blocked-by` tokens match `^[A-Za-z][A-Za-z0-9]*-[A-Za-z0-9.]{1,32}$` (accepts `RK-16`, `CFG-17`, `Q-04`), max 20 | Parked, never revisited automatically or by drain. `orchestrate revisit --unit U --detail "<decision>"` is required, and the decision is recorded. Listed in the stalled payload and the sweep summary. |
| `design-defect` | `orchestrate park --kind design-defect` | `--blocked-by` optional (same token rule) | Parked. `orchestrate revisit` refuses it ("use dtech-aidlc intent rework --unit U"). Cleared by `intent rework --unit` / `intent split`, which reset the unit. |

**Park rules that prevent revisit loops and deadlocks:**
- **Satisfied blocker.** A `dep-unit` or `foreign-regression` park whose resolved blockers are all already satisfied is refused: "WU-16 is already done; a failure owned by a finished unit blocks WU-06 — fix it, or park it as design-defect and run intent rework --unit". Such a park would be revisited automatically on the very next `next`, and the agent could repeat park → revisit forever.
- **Wait cycle.** A park is refused when a resolved blocker is itself parked and waits, directly or through other open parks' `blockedByUnits`, on the unit being parked: "WU-16 is parked waiting on WU-06; parking WU-06 on WU-16 would deadlock". The `dependsOn` check above cannot see these cycles.
- **Pending rework.** `park` and `revisit` are refused while a ledger rework or split is pending ("A design rework is pending; complete implementation-design first"), and before the repeat span is reached.

### 6.2 `orchestrate park`

```text
orchestrate park [--unit ID] --kind KIND [--blocked-by ID]... --detail TEXT
```

- Requires an active intent with a ledger (`ensure_ledger`). Otherwise: "orchestrate park needs work units (Develop with work-units.json)".
- `--unit` defaults to `currentUnit`. Otherwise it must be a non-removed, non-satisfied, non-parked unit. Error for an already parked unit: "WU-06 is already parked (dep-unit); run orchestrate revisit first".
- `--detail` is required (trimmed, non-empty).
- Effects:
  - Append a report `{at, result: "blocked", detail, unit, parked: {kind, blockedBy, auto: false}}` to `stages[step].reports`, where `step` is the unit's next step.
  - Set `steps[step] = "blocked"`.
  - **Attempt counting.**
    - If the unit is `currentUnit` and `currentStage` is set, the park is the report of the current attempt: `attempts[step] += 1`, and `currentStage` is cleared.
    - Otherwise, for example right after a `report --result blocked` or for a unit that is not current, the park only classifies an attempt that was already counted, and `attempts` is unchanged. This avoids double counting the usual "report blocked, then park" sequence.
  - Write the park record with `resumeStatus` = status before the park, `seq = progressSeq`, and `signature` (section 6.4). Set `status = "parked"`.
  - Run `sync_mirrors`.
- Order: validate → `preflight_audit` → `persist_active` → audit `unit.parked {intentId, unit, stage, kind, blockedBy, blockedByUnits, signature, attempts, detail, auto:false}`.
- Output: `{"parked": true, "intentId", "unit", "stage", "kind", "blockedBy", "blockedByUnits", "next": "run dtech-aidlc orchestrate next"}`.

### 6.3 `orchestrate revisit`

```text
orchestrate revisit [--unit ID]... [--detail TEXT] [--approved-by NAME]
```

- Without `--unit`: apply only the automatic revisit rule (every blocker satisfied) now, and output the list. Drain is not applied here, because it is defined relative to "no ready unit" and belongs to `next`. This is idempotent; an empty list writes nothing.
- With `--unit`: revisit the named parked units regardless of kind, except `design-defect`, which is refused. `owner-decision` requires `--detail` (the decision).
- **Who decided.** For `owner-decision`, `decidedBy` is resolved with `identity::resolve_approver(root, --approved-by)`, the same resolution gates use, and stored in the closed park record. A decision recorded by "AI Orchestrator" is listed in the release summary as **unconfirmed** (section 10). This reconciles plan 3.7, which wants the gate to require no parked units *and* the final report to list the "still open" owner decisions.
- Effects: close the park (`revisitReason: "manual"` or `"blocker-done"`, `revisitDetail`, `decidedBy`), set `status = resumeStatus` and `blockedStreak = null`, then `sync_mirrors`.
- Audit: `unit.revisited {intentId, unit, kind, reason, detail, decidedBy?}` per unit.

### 6.4 Auto-park by signature (solves G)

- **Signature of a blocked report** on unit `U` and step `s`:
  `sha256(canonical_json({"stage": s, "unit": U, "kind": "unclassified", "needs": sorted(needs), "blockedBy": []}))`.
  For a park, `kind` and `blockedBy` are filled in.
- `detail` is **deliberately excluded**. In the real run, WU-06's 16 blocked red-tests reports had 16 different texts for one unchanged blocker, so a detail-based signature would never repeat (Decision D7). For a plain blocked report (no `needs`), the signature is therefore constant per (unit, step). In practice the streak counts **consecutive plain blocked reports on the same step of the same unit**. That is the intended behaviour, and it is stated plainly so nobody expects the hash to tell different blockers apart.
- **Streak.**
  - A `blocked` report whose signature equals `blockedStreak.signature` increments `count`. Otherwise the streak becomes `{signature, count: 1}`.
  - A `complete` or `skipped` report, a revisit, or a reset clears it.
  - A blocked report with `--needs` (env-prerequisite) leaves the streak unchanged.
- **Threshold.** `N = config.autoParkThreshold`: a positive integer, default `DEFAULT_AUTO_PARK_THRESHOLD = 3`. `orchestrate report` reads the configuration unvalidated (`main.rs:405-408`), so an invalid value falls back to 3 there; the three validators reject it elsewhere.
- **Trigger.** When `count >= N` and `needs` is empty, the same report also parks the unit: kind `owner-decision`, `auto: true`, `blockedBy: []`, and detail `"Auto-parked after N consecutive blocked reports with the same blocker on <stage>: <last detail>"`.
  - The report output gains `"parked": {"kind":"owner-decision","auto":true,"count":N}`.
  - Audit: `unit.parked {…, auto: true, count: N}`.
  - The next `orchestrate next` picks another ready unit instead of returning `attempt: N+1`.
- Env-prerequisite blocks (with `needs`) never auto-park; their streak is not counted.
- **Loop bound.** An auto-parked unit comes back only through an explicit, audited `orchestrate revisit --unit U --detail` (owner-decision kind). Each further cycle costs N blocked reports plus one recorded decision. The harness instructions (section 15) tell the agent not to revisit an auto-park on its own in human mode.

### 6.5 What the harness sees

- `orchestrate queue` (read-only, section 11).
- Directive additions (section 12).
- The stalled payload (section 4.2).

---

## 7. Scoped rework, fingerprints and split (solves E and the rest of F)

### 7.1 `intent rework [--unit ID]... --detail TEXT`

- **Without a ledger** (fast, not-applicable, or no work units yet): 1.0.0 behaviour, byte for byte. The golden `rust_intent_commands/rework_returns_blocked_develop_stage_to_design.json` stays unchanged.
- **With a ledger:**
  - **Precondition:**
    - With `--unit`: every named unit exists and is not removed, and at least one of them is `blocked` or `parked`, **or** the last sweep report is `blocked`. The last case covers a red sweep and an owner's `--approval rejected` on the release gate: after either, no unit is blocked, yet the owner must be able to send named units back to design.
    - Without `--unit`: any unit is `blocked` or `parked`, the last sweep report is `blocked`, or the 1.0.0 condition holds (a blocked Develop row or report).
    - Refused while a rework or split is already pending: "A design rework is already pending; complete implementation-design first".
    - Otherwise: "Design rework is only available after a blocked or parked work unit or a blocked release sweep".
  - **Without a ledger, one extra precondition.** `intent rework` is also allowed when `load_work_units` fails (section 9). That is the way back to `implementation-design` for an in-flight intent whose `work-units.json` is missing or invalid while nothing is blocked. Without it, the strict-loading error would leave the intent stuck.
  - **Effects:**
    - `stages["implementation-design"]` becomes `in_progress`, and `reworkRequestedAt` is set.
    - `state.designRework = {requestedAt, fromStage, detail, units: [..]}`. `units` is empty without `--unit`.
    - `workUnits.rework = {requestedAt, units, kind: "rework", fingerprints: {U: inputFingerprint}}`.
    - `currentStage` and `currentUnit` are cleared.
    - The command does **not** remove `state.units`, does **not** set `designReworkPending`, and does **not** reset unit entries.
  - **Audit:** `design.rework.requested {intentId, fromStage, detail, units}`. The `units` key appears only on ledger intents.
  - **Output:** `{"reworkRequested": true, "intentId", "fromStage", "nextStage": "implementation-design", "units": [...]}`.
- `orchestrate next` selects `implementation-design` (a `before` stage that is unfinished). `stage.rework` carries `units`, and its text says: change only the design of the named units, and keep every other unit's row and section byte-identical.
- **Reconcile** when `implementation-design` is reported `complete`. This happens in `orchestrate_report`, after artifact validation and before any write:
  1. Load the new `work-units.json` strictly.
  2. Compute every unit's new `inputFingerprint`.
  3. `changed = named units ∪ {u : fp_new(u) ≠ fp_old(u)}`. A rework **without** `--unit`, including a converted 1.0.0 rework, has no named units: only fingerprint changes count. That is what lets "rework without --unit keeps unchanged units" hold. `reset = changed ∪ {d : d.dependsOn ∩ changed ≠ ∅ and d has a finished or blocked step}`. A direct dependent that has not started has nothing to reset.
  4. Reset each unit in `reset`:
     - `steps = {}`, `attempts = {}`, `blockedStreak = null`.
     - Parks:
       - a **changed** unit's open park is closed with `revisitReason: "rework"`;
       - a reset **dependent** keeps an open park with `resumeStatus = "pending"`, so an owner decision on a dependent is not lost because its dependency was redesigned.
     - Set `status = pending` (or keep `parked`, as above) and append `reopened {by: "rework"}`.
     - Keep `history` and `parkHistory`.
  5. New ids → new `pending` units.
  6. Missing ids → `removed: true`. They are kept and excluded from scheduling, and their open deferred regressions are reassigned as `unowned`.
  7. Every other unit keeps its status. **No report is appended for kept units**, which is what plan scenario "WU-01 to WU-05 stay done without skip reports" requires.
  8. Replace `order`, `dependsOn` and `tests`. Set `rework = null` and clear `state.designRework`. Run `sync_mirrors`.
  9. Audit after the write: `units.reconciled {intentId, kept:[…], reset:[…], added:[…], removed:[…], changed:[…]}`.
- The first `implementation-design` completion of an intent (no ledger yet) creates the ledger at the same point. Every unit starts `pending`, and the audit row is `units.created {intentId, units}`.

### 7.2 `inputFingerprint`

```text
sha256( "aidlc-unit-fp/1\n"
      + canonical_json(unit row from work-units.json, keys sorted, "title" included)
      + "\n" + normalised Markdown section "## <id> …" of work-units.md
      + "\n" + canonical_json(ids.json entries of every FN/IF/TS/CFG id cited in that section or listed in "tests", sorted by id) )
```

- **Section extraction:** from the line matching `^##\s+<id>\b` up to the next `^## `. The match uses the normalised id, so a legacy `U-01` heading matches `WU-01`.
- **Normalisation:**
  - CRLF becomes LF and trailing spaces are trimmed.
  - List items whose label is `Status` are dropped (`^\s*[-*]\s*(\*\*)?Status\b[^:]*:`). The real planner rewrites those lines on every pass, which would otherwise invalidate kept units.
- **Missing section** (the Markdown file has none): the section term is the empty string.
- **Code and tests are not hashed** (Decision D8). The CLI does not know the code layout. The final strict sweep (section 10) catches code-level fallout.

### 7.3 `intent split ID --into ID... --detail TEXT`

- **Arity:** 1 positional. `--into` is a list, repeated or comma-separated.
- **Validation:**
  - `ID` is a non-removed unit that is not satisfied.
  - `--into` holds at least 2 distinct valid `WU-` ids. It may include `ID` itself.
  - New ids must not collide with any other existing unit, including removed ones.
  - The ledger must exist.
  - A satisfied unit is refused: "WU-06 is done; use intent rework --unit".
- **Effects:** as rework, with `workUnits.rework = {kind: "split", unit: ID, into: [...]}` and `designRework.split = {unit, into}`.
- **Audit:** `intent.split.requested {intentId, unit, into, detail}`.
- **At the `implementation-design` complete report**, it is validated in addition. A failure refuses the report with no write:
  - Every `into` id appears in `work-units.json`.
  - `ID` is absent unless it is listed in `into`.
  - `ids.json`'s WU list matches, which `registry_problems` (`ids.rs:254`) already enforces.
- **Reconcile** as in section 7.1:
  - Every `into` unit is reset or added as `pending`.
  - `ID` is reset when kept, or marked `removed: true` with `splitInto` when dropped.
  - Kept units are untouched.

---

## 8. Attributed regression protocol (solves D)

The CLI never runs tests. The agent reports regression evidence as a structured **verification artifact**, which the CLI validates and attributes.

### 8.1 Location

- **Per unit:** `regression.json` in the same directory as the unit's expected `regression-report` artifact:
  `.aidlc/intents/<id>/develop/build-and-verify/<WU>/regression.json`.
  It is derived from `expected_artifacts` the same way `apply_work_units` derives `work-units.json` from `work-units.md` (`orchestrate.rs:626-640`).
- **Sweep:** `.aidlc/intents/<id>/develop/build-and-verify/regression.json`, because there is no unit folder (`currentUnit` is absent).
- The directive names the path (section 12). It is **required** for `--result complete` on `build-and-verify` of a ledger intent, for both a unit and the sweep. It is not read for `blocked` or `skipped` reports.

### 8.2 Schema (`schemaVersion: 1`)

```json
{
  "schemaVersion": 1,
  "unit": "WU-06",                    // the reported unit id, or "sweep"
  "scope": "full",                    // "full" = whole existing suite(s); "affected" = a declared subset
  "commands": ["cargo test --manifest-path Cargo.toml --workspace --no-fail-fast",
               "cargo test --manifest-path Cargo.toml --workspace --no-fail-fast -- --ignored"],
  "baseline": ".aidlc/intents/<id>/develop/baseline.md",
  "summary": { "passed": 369, "failed": 16, "ignored": 133 },
  "failures": [
    { "test": "suite_a::tests::ts20_case",
      "suite": "normal", "ts": ["TS-20"], "owner": "WU-16", "baseline": false }
  ]
}
```

**Validation** (`regression::parse`):
- An error makes the report fail with "Stage build-and-verify cannot be completed: regression.json <problem>" and nothing is written.
- `schemaVersion == 1`.
- `unit` equals the reported unit, or `"sweep"`.
- `scope ∈ {full, affected}`. The sweep requires `full`.
- `commands` is a non-empty array of non-empty strings.
- `failures` is an array, at most 2000 entries. For each entry:
  - `test` is a non-empty string ≤ 512 characters, unique.
  - optional `suite` string; `ts` an array of `TS-` registry ids; `owner` a unit id; `baseline` a boolean.
- `summary` is optional. When present, its fields are non-negative integers.
- The file must be a regular file, not a symlink, read through the existing `artifacts::safe_path` checks.
- **Freshness.** The file's modification time must not be earlier than the step's selection time: the unit's `selectedAt`, or `sweep.selectedAt` for the sweep. Otherwise: "regression.json predates this step (written <mtime>, step selected <selectedAt>); run the suites again". This stops a reopened unit or a repeated sweep from passing on an old green file. The CLI cannot prove that tests ran, but it can refuse evidence that is obviously stale.

### 8.3 Attribution (`regression::attribute(failures, ownership, ledger, reporter)`)

Each failure is classified in this order:

1. `baseline: true` → **baseline**. It is never gating and is only counted.
2. **TS set**: the explicit `ts`, or else a TS number inferred from the test name. The last `::`/`.`/`/` segment must match, case-insensitively, `^ts(\d{1,5})[a-z]?(_|$)`, which gives `TS-<n>`. Real example: `ts27a_case` → `TS-27`.
3. **Explicit `owner`.** It must be a non-removed unit. When the TS set is non-empty and ownership knows those TS, `owner` must be one of their owners. Otherwise the report fails with "regression.json: lifecycle::…ts20… owner WU-07 does not own TS-20 (owners: WU-16)". The owner is used.
4. **Without `owner`:**
   - the stored ownership map is empty (no unit declares `tests` and no TS→WU traces exist) → the **reporter** (Decision D17). At the sweep it is **unowned**;
   - one owner across the TS set → that unit;
   - several owners and the reporter is one of them → the **reporter**. This is conservative: the reporter is blocked until it names `owner`;
   - several owners and the reporter is not one of them → **unowned** (ambiguous);
   - no owners, or an empty TS set → **unowned**.
5. **Gating for the reporting unit `R`:**
   - owner `R` → **own**, blocking.
   - owner `O ≠ R` and `O` **not satisfied** → **deferred**, non-blocking. It is recorded and `O` keeps working.
   - owner `O ≠ R` and `O` **satisfied** → **blocking** for `R`, because `R` most likely broke a finished unit (Decision D9). The error says so and suggests fixing it, or `intent rework --unit` when ownership is wrong.
   - **unowned** → recorded with `unowned: true`, non-blocking for `R`, blocking at the sweep (plan 3.3).
6. For the sweep (reporter `"sweep"`), every non-baseline failure is gating. The sweep reopens the owner units (section 10).

When any blocking failure exists, `--result complete` is refused with no write:

```text
Stage build-and-verify cannot be completed for WU-06: 2 failing test(s) are owned by WU-06 or by a finished unit.
  WU-06: suite_b::tests::ts56_… (TS-56)
Fix them, or report --result blocked --detail "…". Name the owner in regression.json when a test belongs to another unit.
```

### 8.4 Acceptance and the deferred-regressions record

On an accepted `complete`:
- test names marked `baseline: true` are added to `workUnits.baselineTests` (section 10 uses them).
- deferred and unowned failures are upserted into `workUnits.deferredRegressions`, keyed by `test`. New items get `status: open` and `firstSeen`; every item seen gets `lastSeen` and `reportedBy += R`.
- if `scope == "full"`, every open item **not** listed in this run becomes `resolved` (`resolvedAt`). If, in addition, the run has no failures outside the baseline, every `green` unit is upgraded to `verified` and `R` becomes `verified`.
- otherwise `R` becomes `green`.
- audit: `regression.attributed {intentId, unit: R|"sweep", stage, own, deferred, unowned, baseline, resolved, scope}`.

**Real scenario (M3 acceptance).** WU-06 reports `complete` with the 16 real WU-16 failures: the 13 `suite_a::tests::ts…` tests plus 3 `suite_b::tests::…` tests, each with `owner: "WU-16"`. All 16 become deferred, and WU-06 becomes `green` instead of blocked. When WU-16 later reports a clean `full` run, the 16 items are resolved and WU-06 becomes `verified`.

**Feasibility with the real data (checked in review against `test-strategy.md` "Work unit to test mapping").**
- 15 of the 16 names give a TS number by name inference. `suite_b::tests::unregistered_case` does not, so its `owner` is accepted without a cross-check.
- Mapped to base numbers, TS-27 (WU-16 `TS-20..TS-29`; WU-08 `TS-27b`), TS-30 (WU-04, WU-16), TS-32 (WU-04, WU-16), TS-42 (WU-06, WU-16), TS-56 (WU-04, WU-06, WU-16) and TS-65 (WU-06, WU-16) each have several owners. WU-16 is always among them, so `owner: "WU-16"` passes the check in step 3.
- Without `owner`, TS-42/56/65 would be charged to the reporter WU-06, and TS-30/32 would be unowned.
- On the owner's intent as it is now (no `tests`, empty ownership), all 16 need an explicit `owner`, or D17 charges them to WU-06.
- **Limit.** An explicit `owner` on a test with no TS number cannot be verified, so an agent could defer such a failure to any unfinished unit. The final sweep still gates every non-baseline failure, so this can only delay a failure, never let it through the release gate.

---

## 9. Loading `work-units.json` without the 1-unit fallback

`work_units::load_work_units(root, graph, state) -> Result<Option<WorkUnitsInput>, String>`:

- **Path:** the `work-units-json` artifact, resolved through `expected_artifacts` on a copy of the state without `currentUnit` (the same lookup as `orchestrate.rs:619-640`). `Ok(None)` when the depth's `produces` list for `implementation-design` does not include `work-units-json` (fast).
- **Errors** (each starts "Develop cannot select a work unit: "):
  - missing file: "<rel path> is missing; it is required at <depth> depth. Restore it, or run 'dtech-aidlc intent rework --detail …' and write it in implementation-design, or replace it with {\"status\":\"not-applicable\",\"reason\":…} to keep running Develop as one implicit unit." The `intent rework` route always works here, because of the extra precondition in 7.1. The `not-applicable` route keeps an in-flight 1.0.0 intent that ran as the implicit unit on its existing `"_"` entries, without a reset.
  - symlink or non-regular file: "<rel path> is not a regular file".
  - unreadable or not UTF-8.
  - invalid JSON or any `ids::work_unit_problems` result: "<rel path>: <first problem> (and N more)".
- **`Ok(None)`** for `{"status": "not-applicable", "reason": …}`. That is the declared implicit unit, with no warning.
- **Ok(Some)** otherwise: units, `order`, `dependsOn`, `tests`, title, plus the registry and the work-units.md text for fingerprints and ownership.
- **When it is called:** only to build the ledger (the first selection in the repeat span, migration, or the first `implementation-design` complete) and to reconcile it (an `implementation-design` complete after a rework or split). It is **not** called while `implementation-design` is unfinished, so a broken file during design rework is reported at the design stage, not here. It is also not called on ordinary commands of a ledger intent: the ledger is authoritative (section 3.2), so a file that is later deleted or broken does not stop scheduling.
- **Drift.** On ledger intents, `next`, `queue`, `status` and `doctor` compare the sha256 of `work-units.json` and `ids.json` with `workUnits.source`. On a mismatch they warn: "work-units.json changed since the work-unit ledger was built (sha …); the change takes effect only through 'dtech-aidlc intent rework' or 'intent split'". In `next` this is a directive warning; in doctor it is a `warn` row. Nothing is reloaded silently. The real run shows why this matters: the agent edited `dependsOn` mid-Develop to reorder units.
- **Removed:** the old warning "No work-units.json was read for this intent, so Develop runs as one implicit work unit …" (`orchestrate.rs:1485-1487`). No golden or test contains it (checked with grep).
- `work_units_of` and `select_next_stage` stay for the non-ledger path only.

---

## 10. Final strict sweep and the `release-candidate` gate

- **The sweep step.** The selector returns `Sweep` when every non-removed unit is satisfied and none is parked. The directive is a `build-and-verify` directive without `currentUnit`:
  - `stage.phaseGate: true`.
  - `approvalRequiredAfter: "build-and-verify"` (unchanged).
  - artifacts at the stage root: implementation, green-verification-report, regression-report and known-limitations, which are the release-candidate evidence.
  - a `sweep` object (section 12).
- **Gate rule** (in `orchestrate_report`, before any write). `--result complete --approval approved` on the sweep requires:
  1. the queue is empty: every non-removed unit is satisfied;
  2. no unit is parked;
  3. a valid, fresh `regression.json` with `unit: "sweep"`, `scope: "full"`, and **no** non-baseline failure;
  4. every failure marked `baseline: true` in the sweep file is already in `workUnits.baselineTests`, meaning an earlier accepted unit run declared it baseline. A failure first declared baseline at the sweep is refused: "regression.json marks <test> as baseline, but no earlier unit run did; baseline failures must be declared before the release sweep". The CLI cannot check the baseline itself, so this stops a red sweep from being made green by relabelling failures at the last moment.

  Each violation has its own error. Existing gate checks (approver identity, gate mode and allowed approvers, AI recommendation, open items, citations, `uncovered_functions` warning) are unchanged, so human gates behave exactly as in 1.0.0.
- **Other results on the sweep.**
  - `--result skipped` is **refused**: "The release regression sweep cannot be skipped; report complete with regression.json, or blocked". In 1.0.0 a phase gate may be skipped with approval. Allowing that here would open release-candidate without a green sweep, against plan 3.7.
  - `--approval rejected --result blocked` is recorded as in 1.0.0 (`gates["release-candidate"].decision = "rejected"`). The sweep stays not green and is selected again. `intent rework --unit` is available afterwards (7.1).
  - `--result blocked` **without** a sweep `regression.json`, for example an environment problem or a rejection, changes no unit. `sweep.attempts += 1`, and the next `next` selects the sweep again with `stage.rework` carrying the detail.
- **On success:**
  - every satisfied unit becomes `done` and `sweep.status = green`;
  - open deferred items not listed become `resolved`;
  - `stages["build-and-verify"].status = "complete"`;
  - `gates["release-candidate"]` is recorded as today.
  - The report output and the `gate.decided` audit row gain these keys, only on ledger intents. Together they are the "final report" of plan 3.7:
    - `ownerDecisions`: closed `owner-decision` park records with their decision, `decidedBy`, and `confirmed: false` when it was decided by "AI Orchestrator". These are the "owner decisions still open" for the owner;
    - `deferredRegressions`: all items with their final status;
    - `sweep: {commands, baselineFailures: [...]}`.
  - The same three keys appear in the sweep directive (section 12), so an approver sees them **before** approving, not only in the audit afterwards.
- **Sweep with failures.** `--result complete` is refused, listing the failures. A `--result blocked` sweep report **with** a valid sweep `regression.json` gets attribution:
  - for each owner unit `O` with failures: `status = red`, `steps["build-and-verify"] = null`, `reopened {by: "sweep", tests}`, audit `unit.reopened`. A reopened unit is no longer satisfied, so its dependents simply wait; nothing else is reset;
  - unowned failures → `sweep.status = red` and `sweep.failures` recorded;
  - the next `orchestrate next` selects the reopened units first, then the sweep again.
  - If only unowned failures remain, the sweep is selected again, and `stage.rework` lists them with the hint "name `owner` in regression.json to reopen that unit, or assign owners in work-units.json `tests` through `intent rework --unit` (available after a blocked sweep, 7.1)".
- `--approval` on any per-unit step of a ledger intent is refused: "Stage build-and-verify is not a phase gate here; --approval … are only accepted on the final regression sweep of build-and-verify, after every work unit is done".
- The sweep is never auto-parked (Decision D10).

---

## 11. CLI surface

### 11.1 Commands and flags

| Command | Flags (args.rs) | Positional max | Notes |
|---|---|---|---|
| `orchestrate report` | existing + `unit` (List) | 0 | `--unit` takes exactly one value here ("--unit takes one work unit for orchestrate report"). It must equal `currentUnit` ("Only the current work unit (WU-01) can be reported"). It is allowed only on a repeating stage of a ledger intent ("--unit applies only to a work-unit stage"). |
| `orchestrate park` | `unit` (List, ≤1), `kind` (String), `blocked-by` (List), `detail` (String) | 0 | section 6.2 |
| `orchestrate queue` | none | 0 | read-only (`load_workflow(..., migrate=false)`, migration preview in memory) |
| `orchestrate revisit` | `unit` (List), `detail` (String), `approved-by` (String, existing) | 0 | section 6.3 |
| `intent rework` | `detail` (String), `unit` (List) | 0 | section 7.1 |
| `intent split` | `into` (List), `detail` (String) | 1 | section 7.3 |

- **Shared option table.** `parse_args` merges every command's options into one table by name, and the first definition wins (`args.rs:290-301`). So `unit` must have **one** type everywhere: List. `kind` already exists as String (knowledge sources). `blocked-by` and `into` are new List options. `detail` is String everywhere.
- **List values** for `unit`, `into` and `blocked-by` are also split on `,` and trimmed, so `--unit WU-06,WU-16` and repeated flags both work. Empty items are an error.
- **`COMMANDS`** (`args.rs:391-429`) gains `orchestrate park`, `orchestrate queue`, `orchestrate revisit` and `intent split`. `command_flags` and `command_max_args` get the rows above.
- **`main.rs` routing.** `orchestrate park|queue|revisit` go to `unit_commands::run`, with the same root resolution and safety checks as `run_orchestrate`. The fallback message becomes "Use orchestrate next, report, park, queue or revisit". `intent split` goes to `intent_commands::run`. `USE_MESSAGE` is unchanged (Decision D12).
- **`help.txt`:**

```text
  orchestrate report --result complete|blocked|skipped [--approval approved] [--approved-by NAME] [--approval-note TEXT] [--detail TEXT] [--stage ID] [--unit ID] [--needs NAME]...
                          --needs names a declared prerequisite; the stage resumes once its check passes
                          --unit names the work unit being reported (default: the current unit)
  orchestrate park [--unit ID] --kind dep-unit|foreign-regression|owner-decision|design-defect [--blocked-by ID]... --detail TEXT
                          Set a blocked work unit aside; orchestrate next continues with the next ready unit
  orchestrate queue       List ready, waiting, blocked, parked and done work units
  orchestrate revisit [--unit ID]... [--detail TEXT] [--approved-by NAME]
                          Return parked units to the queue (without --unit: those whose blockers are done)
  intent rework [--unit ID]... --detail TEXT  Return blocked Develop work to implementation-design (only the named units with --unit)
  intent split ID --into ID... --detail TEXT  Replace a work unit with several units through implementation-design
```

- **`completion.zsh`:**
  - `orchestrate` actions: `next report park queue revisit`, with their flags. `--kind` completes the 4 park kinds.
  - `intent` actions: `list pause resume abandon depth rework split assign`. `rework` was missing in 1.0.0.
- **`tests/repo_content.rs` `COMMAND_KEYS`** gains the 4 keys, so `help_completion_and_the_command_table_stay_in_step` keeps guarding them.
- **Configuration key** in `.aidlc/config.json`:
  - `"autoParkThreshold": <positive integer>` (default 3).
  - It is validated in `scope.rs` (next to `:955`), `config_commands.rs` (next to `:525`) and `doctor.rs` (next to `:341`) with the message "Invalid autoParkThreshold: expected a positive integer".
  - There is no `config` subcommand; the owner edits the file (Decision D11).

### 11.2 `orchestrate queue` output

```json
{
  "intentId": "…", "currentStage": "red-tests", "currentUnit": "WU-07",
  "ready":    [ {"unit": "WU-07", "step": "red-tests", "status": "pending"} ],
  "waiting":  [ {"unit": "WU-08", "waitingOn": ["WU-06", "WU-16"]} ],
  "blocked":  [ {"unit": "WU-03", "step": "build-and-verify", "attempts": 2, "blockedStreak": 2, "autoParkThreshold": 3} ],
  "parked":   [ {"unit": "WU-06", "kind": "foreign-regression", "blockedBy": ["TS-20"], "blockedByUnits": ["WU-16"],
                 "detail": "…", "at": "…", "revisit": "auto when WU-16 is done"} ],
  "done":     [ {"unit": "WU-01", "status": "verified"} ],
  "sweep":    {"status": "pending"},
  "deferredRegressions": {"open": 16, "byOwner": {"WU-16": 16}, "unowned": 0},
  "ownerDecisions": [],
  "drift": null,                  // or {"workUnitsJson": true, "idsJson": false} (section 9)
  "migration": "pending"          // present only when the ledger was previewed, not stored
}
```

### 11.3 `status`

The `status` payload gains `"units": {"total": 16, "byStatus": {"pending": 10, …}, "rows": [{"id", "status", "step", "park": {kind, blockedBy}|null}]}` when a ledger exists or would be created (a preview, with `"migration": "pending"` inside `units`). Without work units the key is absent, so status goldens are unchanged. This is M1's "status shows each unit's state".

---

## 12. Directive JSON additions (ledger intents only)

On a unit step, next to the existing `currentUnit`:

```json
"unit": {
  "id": "WU-07", "title": "Idempotency, atomic acceptance", "status": "pending", "step": "red-tests",
  "dependsOn": ["WU-03"], "tests": ["TS-13", "TS-14", "TS-15", "TS-16", "TS-44"],
  "attempt": 1, "blockedStreak": 0, "autoParkThreshold": 3,
  "revisit": null
},
"queue": { "ready": ["WU-07", "WU-11", "WU-16"], "parked": [{"unit": "WU-06", "kind": "foreign-regression", "blockedBy": ["TS-20"]}],
           "waiting": 8, "satisfied": 5, "total": 16 },
"verification": {                       // build-and-verify steps only
  "path": ".aidlc/intents/<id>/develop/build-and-verify/WU-07/regression.json",
  "schemaVersion": 1, "requiredFor": "complete", "scope": "full",
  "deferredOpen": 16, "ownership": "work-units.json tests and ids.json TS traces"
}
```

On the sweep step, `currentUnit` is absent and these are added:

```json
"sweep": { "path": ".aidlc/intents/<id>/develop/build-and-verify/regression.json", "scope": "full",
           "units": 16, "deferredOpen": 0, "unowned": [], "reopenedBefore": [],
           "baselineTests": ["…declared by earlier unit runs…"],
           "deferredRegressions": [ {"test": "…", "owner": "WU-16", "status": "resolved"} ],
           "ownerDecisions": [ {"unit": "WU-10", "decision": "…", "at": "…", "decidedBy": "AI Orchestrator", "confirmed": false} ] }
```

Existing fields keep their meaning. `stage.phaseGate` is true only on the sweep. `stage.rework` is per unit (section 4.3). `warnings` gains the wave suggestion line next to the unchanged split warning (`orchestrate.rs:1493-1503`). The existing sentence is kept verbatim, because `repo_content.rs::red_tests_warns_above_the_unit_split_threshold…` asserts it. The extra line reads: "Suggested waves of at most 6 units: 1) WU-01, WU-02, WU-03, WU-04, WU-07, WU-11; 2) …".

`complete_payload` gains `units` (byStatus), `ownerDecisions` and `deferredRegressions` only when a ledger exists.

---

## 13. Audit events

Every event is appended after a successful state write, with `crate::audit::audit`. `crate::audit::preflight_audit` runs before the first write of each command.

| Event | When | Fields |
|---|---|---|
| `units.created` | first ledger, at implementation-design complete | intentId, units |
| `units.migrated` | 1.0.0 state migrated | intentId, units, from, ignoredKeys, priorResults |
| `units.resynced` | stale ledger after a 1.0.0 write | intentId, units, legacyRework, legacyGate |
| `units.reconciled` | implementation-design complete after rework/split | intentId, kept, reset, added, removed, changed |
| `unit.parked` | park or auto-park | intentId, unit, stage, kind, blockedBy, blockedByUnits, signature, attempts, detail, auto, count? |
| `unit.revisited` | manual, automatic or drain revisit | intentId, unit, kind, reason, detail, decidedBy? |
| `unit.reopened` | sweep failure | intentId, unit, by, tests |
| `regression.attributed` | every accepted regression.json | intentId, unit, stage, own, deferred, unowned, baseline, resolved, scope |
| `intent.split.requested` | intent split | intentId, unit, into, detail |
| existing `stage.selected` / `stage.reported` | as today | + `unit` key when a ledger unit is involved |
| existing `design.rework.requested` | as today | + `units` key on ledger intents |
| existing `gate.decided` | release-candidate on the sweep | + `ownerDecisions`, `deferredRegressions` on ledger intents |

---

## 14. Doctor additions (`check_workflow`, ledger or would-be-ledger intents)

Doctor computes everything on a preview copy and never writes.

| Status | Message (examples) | Rule |
|---|---|---|
| fail | `Intent X: Develop cannot select a work unit: … is missing …` | `load_work_units` error while the repeat span is reached |
| (removed fail) | currentStage rule | relaxed as in section 5 |
| warn | `Intent X: WU-06 dependsOn WU-16, but WU-06 already has finished work (red-tests 2026-11-24T17:06:28.829Z) and WU-16 has none; the dependency may be inverted. Prefer attributed regression (deferred failures) to reordering.` | A `dependsOn` B, A has a finished step in `history`/`priorResults`, B has none. **This fires on the real intent.** |
| warn | `Intent X: WU-09 is listed before its dependency WU-12 in work-units.json` | array order contradicts `dependsOn` |
| warn | `Intent X: 12 registered tests are not owned by any work unit: TS-03, TS-07, … (add "tests" to work-units.json)` | ownership is non-empty and some registered TS has no owner |
| warn | `Intent X: no test ownership is declared, so failing tests cannot be attributed to work units` | ≥ 2 units and the ownership map is empty |
| warn | `Intent X: 16 failing tests already in the tree belong to WU-16, which has not started; consider running it earlier` | open deferred items whose owner has no finished step (the plan's "tests already in the tree but unassigned to a finished unit") |
| warn | `Intent X: 2 failing tests have no owning work unit and will block the release sweep: …` | open unowned deferred items |
| warn | `Intent X: 16 work units exceed the split threshold 6. Suggested waves: 1) …; 2) …; 3) …. Or split the intent.` | units > `unitSplitThreshold` |
| warn | `Intent X: WU-10 is parked (owner-decision) since … : …` | each open park |
| warn | `Intent X: work-unit ledger is older than stage records for WU-03 (written by an older dtech-aidlc); it is re-synced by the next orchestrate command` | stale ledger |
| warn | `Intent X: parked WU-06 waits on WU-16, which depends on WU-06` | park or wait cycle. `park` refuses these (6.1), so it appears only after manual edits or a reconcile that changed `dependsOn` |
| warn | `Intent X: work-units.json changed since the work-unit ledger was built; the change takes effect only through intent rework or split` | drift (section 9) |
| warn | `Intent X: stage red-tests holds a placeholder entry "_" from an earlier one-unit fallback; it is ignored` | key listed in `migration.ignoredKeys` (plan 3.5: a placeholder is reported, not silently used) |
| warn | `Intent X: 1 owner decision was recorded by AI Orchestrator and awaits owner confirmation: WU-10 …` | closed `owner-decision` parks with `confirmed: false` |

**Wave algorithm.**
1. `level(u) = 0` when it has no dependencies, else `1 + max(level(dep))`.
2. Walk the levels in order and add each whole level to the current wave while the wave size stays ≤ the threshold. A level larger than the threshold becomes its own wave.
3. Within a wave, keep `order`.

On the real graph with threshold 6 the result is:
- `1) WU-01, WU-02, WU-03, WU-04, WU-07, WU-11`
- `2) WU-05, WU-16, WU-06, WU-08, WU-09, WU-12`
- `3) WU-10, WU-13, WU-14, WU-15`

**Not implemented from plan section 6:** the plan's "doctor warns when code imports across units" needs a mapping from code to units, which the CLI does not have (the same reason as D8). Instead, `dependsOn` must be declared (the methodology rule in section 15), the inverted-dependency warning applies, and the final sweep remains the safety net. This is a deliberate deviation, recorded here so that it is not lost.

---

## 15. Methodology, harness and documentation (M5)

- **`core/methodology/phases/develop.md`:**
  - Stages 2 and 3: "Repeat once per work unit, in the order the CLI selects (dependency order; parked units are skipped and revisited)".
  - Stage 3 is no longer "the phase gate … after the last unit". The release-candidate gate is presented on the **final regression sweep**, after every unit is done.
  - The regression rule becomes the attribution rule: own failures block the unit, failures owned by unfinished units are deferred, unowned failures block the sweep.
  - The rework paragraph documents `intent rework --unit`, `intent split`, and the park kinds.
- **`core/methodology/agents/builder-verifier.md`:**
  - Write `regression.json` (schema in section 8) beside the regression report, after running the suites for this step. Set `owner` for multi-owner or unprefixed tests, and for every failure when the work units declare no `tests`. Set `baseline: true` for baseline failures, and declare them in the first unit run, since the sweep refuses new baseline claims.
  - Use `orchestrate park` for dep-unit, foreign-regression, owner-decision and design-defect blockers, instead of repeated blocked reports.
  - Stop after N same-blocker reports; the CLI auto-parks anyway.
- **`core/methodology/agents/implementation-planner.md` and `templates/work-units.md`:**
  - Each unit lists `"tests": ["TS-…"]` in `work-units.json`.
  - Array order is dependency order.
  - Do not invert `dependsOn` to reorder work; use attribution.
  - On a scoped rework, change only the named units' rows and sections.
- **`templates/test-strategy.md`:** note that the "work unit to test mapping" must be mirrored in `work-units.json` `tests`.
- **`harness/claude/skills/aidlc/SKILL.md` and `harness/deepseek/skills/aidlc/SKILL.md`:**
  - Report lines with `--unit <id>`.
  - New `orchestrate park`, `queue` and `revisit` bullets.
  - Exit 3 with `reason: "queue-stalled"` → show the parked list and stop.
  - Never revisit an `owner-decision` park (including auto-parks) without a decision from the owner in human mode. In AI mode, record the decision with `--detail`; it is reported as unconfirmed at the release gate.
  - The sweep is the gate.
  - Rework with `--unit`.
- **`core/data/projection-history.json`:** append the 1.0.0 sha256 of every changed projected file (claude `SKILL.md` `b0ccee48…`, deepseek `SKILL.md` `debd57b6…`) and of every changed methodology file listed under `methodology` (for example `phases/develop.md`, `agents/builder-verifier.md`, `agents/implementation-planner.md`). Projects projected by 1.0.0 then read as pristine-but-outdated, not "modified". `repo_content.rs::projection_history_has_the_required_hashes…` keeps passing, because hashes are only added.
- **`README.md`:**
  - "Evidence-driven Develop" and "Run an intent": unit queue, park/queue/revisit, attributed regression, final sweep gate.
  - "Scopes, depth, mode…": `autoParkThreshold`.
- **`CHANGELOG.md`:** a new `## Unreleased` section that lists every behaviour change a user or script can notice (sections 3-14), including the migration, the removed implicit-unit fallback, the extra sweep directive, and the `designRework` fix. The crate version is not bumped (Decision D13).

---

## 16. Golden-test impact

Re-blessing (`DTECH_AIDLC_BLESS=1`) is limited to the files below. Each diff is inspected and listed in the implementation report.

| Golden | Expected change | Cause |
|---|---|---|
| `rust_utility_parity/help-and-completion.json` | help and completion text | sections 11.1, M5 |
| `rust_routing_parity/validation.json` | "Use orchestrate next, report, park, queue or revisit"; allowed-option lists of `orchestrate report` (`--unit`) and `intent rework` (`--unit`) | section 11.1 |
| `rust_routing_parity/lifecycle.json`, `lifecycle-embedded.json` | help output lines (`intent rework …`) if they embed help | help.txt |
| `rust_intent_commands/rework_error_paths_match_golden.json` | usage text "Usage: intent rework [--unit ID]... --detail \"<reason>\"" and allowed options | section 7.1 |
| 15 goldens containing the projected `SKILL.md` hash/size: `rust_config_commands/{config_agents,config_gate_mode,config_mode,config_prerequisite,config_scope}_parity.json`, `rust_harness_commands/{harness_add,harness_refresh_multiple_harnesses,harness_refresh,harness_status}_parity.json`, `rust_knowledge_commands/{onboarded_project_commands,records_list_show_check_and_index,source_management_and_errors}_match_golden.json`, `rust_routing_parity/{fixture-onboarded-claude,fixture-onboarded-deepseek,harness}.json` | sha256 and byte size of `SKILL.md` | M5 skill text |

**Expected to stay unchanged:**
- every state snapshot golden: no top-level version bump; a ledger appears only at the repeat span with real units, which no golden reaches;
- `rust_orchestration_parity/*`;
- `rust_status_parity/*`;
- `rust_scope_parity/scope-output.json` (no graph change);
- `rust_doctor/*` (no unit intents).

Any other golden diff is a defect to fix, not to bless.

Existing tests that must keep passing unchanged:
- `repo_content.rs::red_tests_warns_above_the_unit_split_threshold_and_points_at_one_baseline`: the ledger is created and WU-01 is first; the split warning text is unchanged.
- `rust_cli.rs::orchestration_lifecycle_reports_blocks_reworks_and_passes_a_gate`: non-ledger `attempt: 2`.
- `rust_intent_commands.rs` rework tests: non-ledger path.
- `orchestrate.rs` unit tests.

---

## 17. Milestones, tasks and acceptance tests

Shared test infrastructure (M1):
- `tests/rust_unit_scheduling.rs` drives the binary in a `TempDir`, reusing the patterns of `tests/repo_content.rs`: `init_project`, `json_ok`, `write_artifacts`.
- `tests/fixtures/unit_scheduling/` holds the replay fixtures described here. They are **structural copies** of the real intent's `work-units.json` and `ids.json`: the same 16 unit ids, the same `dependsOn` edges and array order, the same 69 TS ids, and `tests` ownership taken from the real `test-strategy.md` "Work unit to test mapping" table, with suffixes reduced to base numbers. Titles are neutral placeholders, so no project-specific prose is committed to this repository (Decision D14). Variants:
  - `work-units.real.json`: as in the owner's project now (WU-16 before WU-06; WU-06 `dependsOn` WU-16).
  - `work-units.uninverted.json`: WU-16 removed from WU-06's `dependsOn` and WU-16 moved after WU-11. This is the graph before the plan 1.C workaround.
  - `ids.json`: the 69 TS ids, WU-01..WU-16, plus a few FN/IF/CFG ids.
  - `work-units.md`: a `## WU-xx` section per unit, with a `Status:` line to exercise normalisation.
  - `regression.wu06.json`: the 16 real failing test names from `build-and-verify/WU-06/regression-report.md`, `owner: "WU-16"`, 13 `suite: normal` and 3 `suite: ignored`.
  - `state-1.0.0.json`: a synthetic state with the real state's *shape*:
    - unit entries `pending` with `reportedAt` that join to reports;
    - a `"_"` key;
    - `reworkResetAt`;
    - `designRework` absent;
    - 33/17 reports;
    - `currentStage: red-tests`, `currentUnit: WU-01`.
- A helper `seed_develop(root, variant)` creates an intent at `full` depth with every stage before Develop complete, the gates approved, and the fixtures copied into `develop/implementation-design/`. Then `orchestrate next` reaches the repeat span.
- A **manual replay** is run once during M5 and its results are recorded in the implementation report, never committed. It copies the real `.aidlc` folder (config, intents, audit; no credentials or `.env`) into a scratch directory outside both repositories, then runs the newly built `target/debug/dtech-aidlc` there: `status`, `orchestrate queue`, `doctor`, `orchestrate next`. Expected:
  - `units.migrated` with 16 units, 12 `priorResults` and `ignoredKeys: ["_"]`;
  - the WU-06 → WU-16 inverted-dependency warning;
  - the 3 waves;
  - no doctor fail;
  - the next directive is red-tests WU-01.

### M1: Per-unit state, `report --unit`, migration

**Tasks:**
1. `work_units.rs`: `load_work_units` (section 9), `ownership`, ledger read and write helpers, `ensure_ledger` + migration (section 3.6), `sync_mirrors`, `select` steps 1, 3, 4, 6 and 8 (no parks yet), fingerprints (section 7.2; needed as the migration baseline), and the queue view.
2. `orchestrate.rs`:
   - wire `ensure_ledger` and `select`;
   - the F root-cause fix (section 4.4);
   - per-unit `was_blocked`, needs and rework info;
   - `report --unit` validation and unit tagging;
   - ledger step/status updates for red-tests and build-and-verify. For M1, build-and-verify complete still requires no regression.json, and a complete unit becomes `green`;
   - `is_phase_gate` for the sweep, with a minimal sweep step: in M1 the sweep needs only approval; regression.json comes in M3;
   - directive `unit`/`queue` fields.
3. `ids.rs`: `tests` validation. `artifacts.rs`: registry cross-check.
4. `status.rs`: `units`. `doctor.rs`: relaxed currentStage rule and load-error fail.
5. `args.rs` / `main.rs` / `help.txt` / `completion.zsh`: `--unit` on report.

**Acceptance tests:**
- `migrates_1_0_0_state_losslessly`: load `state-1.0.0.json`, run `orchestrate next`, then check:
  - every original key and value is still present (deep-subset compare), apart from `updatedAt`, `currentStage`/`currentUnit` and the documented mirror keys;
  - `audit.jsonl` keeps its old bytes as a prefix, plus exactly one `units.migrated`;
  - `priorResults` and `ignoredKeys` are as expected;
  - a second `next` adds no second migration row (idempotent);
  - `validate_intent_state` still passes;
  - deleting `workUnits` from the file leaves a state that the old selector `select_next_stage` handles (rollback);
  - a unit test diffs `ensure_ledger` input and output: only `workUnits` is added (purity guarantee, 3.6).
- `open_1_0_0_rework_is_migrated_at_design_complete`: a 1.0.0 state with `designReworkPending: true`, `implementation-design` in progress and repeating rows → `next` selects `implementation-design` and writes no ledger; the `complete` report builds it and audits `units.migrated` (not `units.created`).
- `rollback_projection_keeps_the_release_gate`: every unit satisfied, sweep pending → the mirrors show the gate unit's `build-and-verify` as `pending`. On the state with `workUnits` removed, 1.0.0's `select_next_stage` + `is_phase_gate` select that step as the release-candidate phase gate instead of falling through to the next phase.
- `status_shows_each_unit_state`: `units.rows` has 16 entries with statuses and steps.
- `report_unit_must_match_current_unit`: wrong unit → error and byte-identical state; omitted `--unit` → current unit; `--unit` on a non-repeating stage → error.
- `selector_follows_dependencies_not_array_order`: a small graph where WU-02 is listed first but depends on WU-03 → WU-01, WU-03, WU-02.
- `design_rework_after_blocked_design_does_not_fall_back_to_one_unit`: the F reproduction (section 4.4).
- `missing_or_invalid_work_units_json_errors_without_writes`: missing, invalid JSON, cycle and `"units": []` each give the documented error with no state or audit change; `not-applicable` → implicit unit; `intent rework --detail` is accepted in the missing-file case although nothing is blocked (7.1).
- `ledger_ignores_later_work_units_json_edits_and_warns`: edit `dependsOn` after the ledger exists → the selection is unchanged and the directive carries the drift warning; deleting the file does not stop `next`.
- `legacy_u_ids_keep_their_spelling`: `work-units.json` with `U-01`/`U-02` and `dependsOn: ["WU-01"]` → the ledger keys, `currentUnit` and artifact folders use `U-01`; `--unit WU-01` is accepted.
- `doctor_accepts_build_and_verify_current_while_red_tests_in_progress`.

### M2: Taxonomy, park, queue, revisit, auto-park

**Tasks:**
- `select` steps 2, 5 and 7, plus the stalled outcome (exit 3).
- `unit_commands.rs`: park, queue, revisit.
- Auto-park in `orchestrate_report` (section 6.4).
- `autoParkThreshold` validation ×3.
- Audit events.
- `args.rs`, help and completion rows.

**Acceptance tests:**
- `replay_park_foreign_regression_moves_on_and_reopens` (plan 5.1 / M2), with `work-units.uninverted.json`:
  1. Complete WU-01 to WU-05 (red then build) and WU-06 red.
  2. `orchestrate park --kind foreign-regression --blocked-by TS-20 --detail …`.
  3. Expect `blockedByUnits: ["WU-16"]`.
  4. `next` selects WU-07, then WU-11, then WU-16.
  5. After WU-16 is satisfied, the next `next` audits `unit.revisited` (blocker-done) and selects WU-06 **build-and-verify** (its red step is kept).
- `park_refuses_waiting_on_a_dependent_unit`: with `work-units.real.json`, `park WU-16 --kind dep-unit --blocked-by WU-06` → inverted-dependency error and no write.
- `third_identical_blocked_report_auto_parks` (plan 5.3), with `work-units.real.json`, WU-01 and WU-02 satisfied: three `--result blocked` reports with different `--detail` on WU-03 build-and-verify → the third output has `parked.auto: true`, and `next` selects **WU-11**. WU-04, WU-05 and WU-07 depend on WU-03, so WU-11 (which depends only on WU-01) is the first ready unit. `queue.parked[0].kind == "owner-decision"`. With `autoParkThreshold: 5` → parks on the fifth. With `--needs docker` → never auto-parks, and the streak is unchanged.
- `report_then_park_counts_one_attempt`: `report --result blocked` followed by `park` (no `--unit`) parks the same unit, and `attempts` grows by 1, not 2.
- `park_refuses_satisfied_blockers_and_wait_cycles`: `park WU-06 --kind dep-unit --blocked-by WU-03` with WU-03 satisfied → refused. With WU-16 parked `dep-unit` on WU-06 (uninverted graph), `park WU-06 --kind dep-unit --blocked-by WU-16` → refused as a deadlock. Both leave the state byte-identical.
- `stall_after_drain_persists_the_revisit`: a drain revisit whose unit is still not ready ends in exit 3; the revisit and its `unit.revisited` row are persisted once, and a second `next` writes nothing.
- `owner_decision_stalls_until_revisited_with_detail`: everything else satisfied → exit 3 `queue-stalled` with nothing written; `revisit --unit WU-10` without `--detail` → error; with `--detail` → resumes.
- `drain_revisits_once_per_progress`: a dep-unit blocker that is itself parked (owner-decision) → drain revisits at most once until `progressSeq` changes, so no loop.
- `queue_is_read_only`: snapshot of `.aidlc` before and after `orchestrate queue` on a 1.0.0 state is identical, and `migration: "pending"` is shown.
- `park_and_revisit_fail_closed`: a broken audit log (directory in place of `audit.jsonl`) and a concurrently changed state each produce an error with byte-identical state.
- CLI validation: unknown kind, `env-prerequisite`, missing `--detail`, two `--unit` on report, comma lists.

### M3: Attributed regression, deferred regressions, final sweep

**Tasks:**
- `regression.rs` (parse, infer, attribute, upsert or resolve deferred items).
- The report gate for unit build-and-verify (section 8).
- The sweep rule, reopen and gate (section 10).
- Directive `verification`/`sweep`.
- `complete_payload` and `gate.decided` additions.
- Doctor deferred and unowned warnings.

**Acceptance tests:**
- `wu16_failures_do_not_block_wu06` (M3 criterion): `work-units.uninverted.json`; WU-06 build-and-verify `complete` with `regression.wu06.json` → accepted, WU-06 `green`, 16 open deferred items owned by WU-16, `regression.attributed {deferred: 16}`. Then WU-16 complete with a clean `full` run → 16 resolved and WU-06 `verified`.
- `owner_must_own_the_test`: `owner: "WU-07"` on a `ts20_…` test → error and no write.
- `multi_owner_test_without_owner_blocks_the_reporter`: `ts56_…` reported by WU-06 with no owner → refused.
- `failure_of_a_finished_unit_blocks_the_reporter`.
- `name_inference`: `ts27a_x` → TS-27; `job_store_x` → unowned.
- `release_gate_requires_empty_queue_and_green_sweep`:
  - `--approval` on the last unit's step is refused;
  - the sweep is offered only when all units are satisfied;
  - a sweep `complete` with a failure is refused;
  - a sweep `blocked` with a failure owned by WU-04 → WU-04 reopened (`red`), and `next` selects WU-04 build-and-verify, then the sweep again;
  - an unowned failure keeps the sweep red;
  - a green sweep with approval → gate recorded, every unit `done`, `build-and-verify` row complete, `gate.decided.deferredRegressions` present.
- `parked_unit_blocks_release_gate`.
- `sweep_cannot_be_skipped_or_relabelled`: `--result skipped --approval approved` on the sweep → refused. A sweep `regression.json` that marks a failure `baseline: true` that no earlier unit run declared → refused. A stale sweep `regression.json` (mtime before `sweep.selectedAt`) → refused.
- `rejected_release_gate_allows_scoped_rework`: in human mode, `--approval rejected --result blocked` on the sweep records the rejection; `intent rework --unit WU-04 --detail …` is then accepted although no unit is blocked or parked.
- `empty_ownership_charges_the_reporter`: work units without `tests`, and a failure with no `owner` → WU-06's `complete` is refused as its own failure; with `owner: "WU-16"` it is deferred.

### M4: `intent rework --unit`, fingerprints, `split`, strict loading

**Tasks:**
- `intent_commands.rs`: rework with a ledger (section 7.1), `intent split` (section 7.3).
- Reconcile in `orchestrate_report` at implementation-design complete.
- Legacy `designReworkPending` conversion and re-sync (section 3.6).
- `args.rs`, help and completion.

**Acceptance tests:**
- `rework_one_unit_keeps_green_units` (plan 5.2 / M4 criterion):
  1. WU-01 to WU-05 satisfied, WU-06 parked `design-defect`.
  2. `intent rework --unit WU-06 --detail …`.
  3. Edit only WU-06's section and row, and change a `Status:` line of WU-02.
  4. Complete implementation-design.
  5. Expect: WU-01 to WU-05 keep their status and the report counts of `red-tests` and `build-and-verify` for them are unchanged (no skip reports); `units.reconciled.kept` ⊇ WU-01..WU-05; `reset == ["WU-06"]` plus direct dependents of WU-06 that had started; `next` selects WU-06 red-tests.
- `fingerprint_change_resets_unit_and_direct_dependents_only`: small graph A ← B ← C; change A's section → A and B reset, C kept.
- `rework_without_unit_on_ledger_keeps_unchanged_units`.
- `rework_without_ledger_is_unchanged`: the existing goldens.
- `split_replaces_a_unit`: `intent split WU-06 --into WU-06,WU-17` → impl-design; missing WU-17 → report refused, no write; with WU-17 added (json, md, ids) → WU-17 `pending`, WU-06 reset, others kept, and `dependsOn` on a removed id is rejected by validation.
- `legacy_rework_marker_is_converted`: a state with a ledger plus `designReworkPending: true` and `state.units` missing (a 1.0.0 rework after the upgrade) → treated as a ledger rework with no named units; no `"_"` unit appears.
- `legacy_rework_already_completed_by_1_0_0_is_reconciled`: a ledger state on which 1.0.0 ran `intent rework`, then `next` (which clears `designReworkPending`), then `implementation-design` complete, with only WU-06's section changed → the next unit-aware command detects it through `reworkResetAt`/`reworkRequestedAt`, reconciles, resets only WU-06 and its started direct dependents, and audits `units.resynced {legacyRework: true}` plus `units.reconciled`.
- `stale_ledger_is_resynced`: the ledger's `syncedAt` is older than a unit entry's `reportedAt` → `units.resynced`.
- `legacy_release_gate_is_accepted`: 1.0.0 approved release-candidate on the projected gate unit → `sweep.via == "1.0.0-gate"`, every unit `done`, and the gate row untouched.
- `rework_keeps_owner_decision_parks_on_dependents`: a direct dependent of the reworked unit is parked `owner-decision` and had started → after reconcile it is still parked, with `resumeStatus: "pending"`.

### M5: Methodology, doctor completion, docs, end-to-end

**Tasks:**
- Section 15 edits and projection-history hashes.
- Doctor inverted dependency, unassigned tests, waves, parks, stale.
- README and CHANGELOG.
- Golden re-bless (section 16).
- Manual real-state replay.

**Acceptance tests:**
- `doctor_reports_inverted_dependency_waves_and_unassigned_tests`: with `work-units.real.json` and a history where WU-06 finished red before WU-16 started → the inverted warning; the waves equal the 3 lists in section 14; registered TS without owners are listed.
- `end_to_end_unit_scheduling`: plan section 5 scenarios 1-3 chained in one intent through to `complete: true`.
- Compatibility (plan 5.4) is covered by M1 `migrates_1_0_0_state_losslessly`.
- **Metrics** (plan 5.5): a helper computes reports per unit and the share of `skipped` reports whose detail starts with "Skip redundant" from the end-to-end state. Assert ≤ 2 reports per unit on the happy path, 0 skip-redundant reports after a scoped rework, and print the values in the test output.
- Repository checks: `repo_content.rs` command table, methodology lint, projection history.
- `cargo fmt --check`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo test --offline --no-fail-fast`.

---

## 18. Decisions (ambiguities resolved with the simplest safe choice)

- **D1. No top-level `version` bump.** `normalise_state` already forces 2 and 1.0.0 ignores the value, so a bump would protect nothing. It would also rewrite about 30 state goldens. The ledger carries its own `schemaVersion: 1`, and staleness after an old-binary write is detected from timestamps (section 3.6).
- **D2. No backup file during migration.** Migration only adds a key, and this is enforced by a test (the purity guarantee in 3.6). The mirrors, including the release gate projection, keep 1.0.0 able to drive the state, so removing `workUnits` is the rollback. Writing a backup would leave a file behind on a failed command, against the "unchanged on error" rule. This differs from the v1→v2 migration, which rewrites the state shape and therefore needs `state.v1.bak.json`. `migration.preSha256` lets anyone confirm which bytes were migrated.
- **D3. "done" in the plan's dependency rule means *satisfied* (`green | verified | done`).** `done` itself means "confirmed by the release sweep". Otherwise no dependent could start before the sweep.
- **D4. Ledger only for real work units.** Fast depth and `not-applicable` keep the 1.0.0 implicit unit, with no ledger and no fallback warning. The "no 1-unit fallback" rule applies wherever `work-units.json` is required.
- **D5. The release-candidate gate moves from "last unit's build-and-verify" to a dedicated sweep step.** This costs one extra directive per intent, and it is the only place where "queue empty + no parked + strict sweep green" can be checked.
- **D6. Ownership comes from machine-readable sources only** (`work-units.json` `tests` and `ids.json` TS→WU traces). The real Markdown uses ranges, suffixes and prose that cannot be parsed reliably. Explicit `owner` in `regression.json` and the `ts\d+_` name inference cover the rest.
- **D7. The signature excludes the free-text detail.** It is not stored separately; the report keeps the detail. Evidence: 16 different texts for one WU-06 blocker.
- **D8. The fingerprint covers design inputs only** (unit row, unit section without `Status:` lines, registry entries). Code is not hashed. The final sweep is the safety net.
- **D9. A failure owned by an already satisfied unit blocks the reporter.** The reporter most likely broke it. Failures of unfinished units are deferred, and unowned ones are deferred per unit but block the sweep.
- **D10. The sweep is never auto-parked.** It is one owner-visible gate per intent. Repeated sweep blocks show the unowned list in `stage.rework`.
- **D11. `autoParkThreshold` is a config-file key without a `config` subcommand,** like `unitSplitThreshold`. An invalid value falls back to 3 where the configuration is read unvalidated (`orchestrate report`).
- **D12. `--unit` is a List option everywhere** because of the shared parser table, and commands that take one unit enforce exactly one value. `intent`'s `USE_MESSAGE` is left unchanged to avoid unrelated golden churn; help and completion list the new commands.
- **D13. No crate version bump.** The CHANGELOG gets an `## Unreleased` section. Bumping `CARGO_PKG_VERSION` would rewrite every golden with an engine version or projection stamp. The release number is the owner's call.
- **D14. Committed fixtures are structural copies.** They keep the ids, edges, ownership and test names, with neutral titles. Full real state and prose are only used in a manual, uncommitted replay in a scratch folder, so the owner's project text does not land in this repository.
- **D15. `report --unit` is limited to the current unit.** Parking or revisiting other units goes through `park` and `revisit`. This keeps the "only the current stage can be reported" invariant.
- **D16. No automatic restore of results reset by a 1.0.0 whole-intent rework.** They are recorded as `priorResults` and shown by `queue` and `doctor`. Restoring them without fingerprints from before the rework could keep units whose design changed. For the owner's in-flight intent this means the WU-01 to WU-05 replay of plan section 7 still has to happen once, recorded as `skipped` with a reason. Every later rework is scoped. Migration loses no progress that 1.0.0 had not already reset, and the recovered `priorResults` may name the wrong unit (fact 6), which is one more reason not to restore from them.
- **D17. With no declared ownership, failures default to the reporter.** When the stored ownership map is empty, a failure without `owner` is the reporter's own failure. Treating it as `unowned` would make every unit pass its own regression check. The plan's "unowned does not block the unit" still holds wherever ownership is declared and a test is simply missing from it.
- **D18. The ledger is authoritative between reconciles.** `work-units.json` and `ids.json` are read only when the ledger is built or reconciled. Later edits are reported as drift, never applied silently. Applying them would bypass scoped rework, and erroring on them would leave the intent stuck when the file is damaged.
- **D19. The release sweep cannot be skipped, and baseline claims must predate it.** These are the two ways a red sweep could otherwise open release-candidate. A rejection by a human approver works as in 1.0.0.
- **D20. The 1.0.0 mirrors project the sweep onto the gate unit.** This is the only representation under which a 1.0.0 binary stops at the release-candidate gate (fact 8). A release-candidate approval recorded by 1.0.0 on that projection is accepted as the sweep result, so a human gate decision is never discarded.

## 19. Open issues (for the owner, not blocking implementation)

1. Whether suffixed tests (`TS-27a`) should become first-class registry ids. Today the registry rejects them, and this design maps them to the base number.
2. Whether `intent split` and `intent rework --unit` should require `--approved-by` (an explicit owner identity), like `intent depth`. This design follows the current `intent rework`, which requires only `--detail`.
3. Whether to add an owner-only `orchestrate restore --unit` for intents migrated after a 1.0.0 whole-intent rework (D16). It was left out to keep the surface small.
4. Whether `orchestrate revisit` of an `owner-decision` park should *require* a human approver in human mode, rather than recording AI decisions as unconfirmed (6.3). The design keeps it permissive, because plan 3.7 asks for open owner decisions to be listed, not enforced.

---

## Review changes

An adversarial review checked this design against the plan, the source at b3e97b0 and the owner's real intent (read-only). It fixed the following. The section where each fix lives is given in brackets.

1. **The rollback could bypass the release gate** [3.4, 3.6, D20]. 1.0.0 enforces gates only at report time and falls through past `build-and-verify` once every `state.units` entry is complete (fact 8). The old mirrors marked units complete as soon as they were satisfied, so a downgraded binary, or a state with `workUnits` removed, would skip release-candidate. Added the gate-unit projection and the acceptance of a 1.0.0-recorded gate.
2. **Migration during an open 1.0.0 rework** [3.6]. The precondition "or a repeating row exists" let migration read `work-units.json` while `implementation-design` was being reworked, which contradicts section 9. Migration now waits until every `before` stage is finished, and the ledger is built as `units.migrated` at the design complete report.
3. **Stale-ledger detection missed a 1.0.0 rework** [3.6]. A 1.0.0 `intent rework` resets entries without changing `reportedAt`, and the following 1.0.0 `next` clears `designReworkPending`, so neither old signal fired. Added `reworkResetAt`/`reworkRequestedAt`/design `reportedAt`/gate timestamps, the same-`now` rule for `syncedAt`, and a reconcile when 1.0.0 has already completed the redesign.
4. **Retiring `apply_work_units`** [4.4]. Removing only its `designReworkPending` early return would fill `state.units` from the pre-rework file and then never reload it. The function is now retired, and loading happens only in `ensure_ledger`.
5. **Strict loading could leave an intent stuck** [7.1, 9]. Implementation-design completion already validates the file (fact 7). A missing file at the repeat span therefore has no way back, because 1.0.0 `intent rework` needs a blocked stage. Rework is now allowed on a load failure, and the error names three escapes, including `not-applicable` for implicit-unit 1.0.0 intents.
6. **Ledger authority and drift** [3.2, 3.5, 9, D18]. It was unspecified whether `work-units.json` edits after ledger creation were applied. The ledger is now authoritative and edits are reported as drift. The real agent did edit `dependsOn` mid-Develop.
7. **Empty ownership made per-unit regression meaningless** [3.5, 8.3, D17]. The real intent declares no ownership, so every failure, including a unit's own, would have been `unowned` and non-blocking. Without ownership, failures now default to the reporter. Also recorded the real ownership check of the 16 WU-16 failures [8.4].
8. **Gate holes on the sweep** [10, D19]:
   - `--result skipped --approval approved` would open release-candidate without a sweep;
   - `baseline: true` could relabel a red sweep;
   - a stale `regression.json` could be reused.

   All three are now refused, and baseline failures, commands and owner decisions are shown to the approver in the sweep directive.
9. **A rejected or red sweep left no way to rework** [7.1]. After a rejection or a red sweep no unit is blocked, so `intent rework --unit` would have been refused. A blocked sweep now satisfies the precondition. A second rework while one is pending is refused.
10. **Revisit loop and park deadlocks** [6.1]. Parking on an already satisfied blocker caused park → auto-revisit → park forever. Mutual `dep-unit` parks could deadlock; the doctor row claimed they "can only appear after manual edits", which was wrong. Both are now refused. `park` and `revisit` are refused during a pending rework.
11. **A stall after a drain revisit wrote nothing, yet changed the ledger** [4.2]. The stall now persists and audits the revisits it made, and is a no-op otherwise. Also proved that a stall always names a parked unit, gave the hint per kind, and made the sweep step check `sweep.status` so a green sweep is never selected again.
12. **Double attempt counting on "report blocked, then park"** [6.2]. Also corrected the claim that `report` clears `currentUnit`; 1.0.0 keeps it [3.4].
13. **Rework semantics** [7.1]. Without `--unit`, "named = all" contradicted "keeps unchanged units"; named is now empty. Direct dependents are reset only if they had started, and an `owner-decision` park on a dependent survives a reconcile.
14. **Plan 3.7 contradiction** [6.3, 10]. The gate needs no parked units, yet the final report lists "open" owner decisions. Decisions now record `decidedBy`, and AI decisions are listed as unconfirmed.
15. **Migration mapping order** [3.6]. "red finished → red" shadowed a blocked `build-and-verify`. Also documented that `priorResults` may name the wrong unit: the real 02:17:13Z WU-01 entry is the WU-06/WU-16 regression report [fact 6, D16].
16. **Legacy `U-01` ids** [3.2, fact 9]. Keys stay as written, and comparisons are normalised. Otherwise migration lookups and artifact folders break.
17. **Corrected test expectations** [17]:
    - the auto-park test expected "WU-04's chain", but WU-04 depends on WU-03; it is WU-11;
    - the wave lists did not follow the stated "keep `order`" rule [12, 14].

    Added tests for every fix above.
18. **Trimmed over-engineering:**
    - dropped `detailDigest` [6.4] and the `waves` array in `queue` [11.2];
    - `revisit` without `--unit` no longer runs the drain rule [6.3];
    - stated plainly that the auto-park "signature" is a per-step streak [6.4].
19. **Completeness** [14]:
    - the plan 6 "cross-unit import" doctor check is now an explicit, reasoned deviation;
    - the plan 3.5 placeholder report is now a doctor warning;
    - drift and unconfirmed-decision warnings were added.

---

## Implementation notes

### M1 (per-unit state, `report --unit`, migration, strict loading)

Implemented in `rust-cli/work_units.rs` (new), `orchestrate.rs`, `intent_commands.rs`, `status.rs`, `doctor.rs`, `ids.rs`, `artifacts.rs`, `args.rs`, `main.rs`, `help.txt`, `completion.zsh`; tests in `tests/rust_unit_scheduling.rs`, fixtures in `tests/fixtures/unit_scheduling/` (generated from the real intent's structure with neutral text; `regression.wu06.json` is left for M3).

Deviations and additions, with reasons:

1. **`ensure_ledger` also skips intents whose repeat span is already finished** (every repeating row `complete`/`skipped`). Migrating such an intent would offer the release sweep again, and a later deletion of `work-units.json` would make reports on Launch/Archive stages fail. Those intents keep the 1.0.0 path.
2. **Sweep reports are tagged `"sweep": true`** (stage row report, `stage.reported`, `stage.selected`, report output) because they have no unit. `last_sweep_report` finds them; sweep reports never write a `stages[s].units` entry (no `"_"`).
3. **Interim ledger rework (until M4).** `intent rework` on a ledger intent follows section 7.1's state effects (ledger `rework` record, `designRework` with `units: []`, no `state.units` removal, mirrors projected `in_progress` with `reworkResetAt`, second request refused while pending). The reconcile at `implementation-design` complete resets **every** unit to `pending` (the 1.0.0 whole-intent semantics, history kept, new ids added, missing ids `removed: true`) and audits `units.reconciled` with `kept: []` and `changed` computed from fingerprints for information. M4 replaces the reset set with the fingerprint/named-unit rule. Without this, a 1.0.0-style rework on a ledger intent would be silently ignored by the ledger.
4. **`intent rework` migrates first** (as section 3.6 lists it) but never fails on a load error: a load error selects the 1.0.0 path with the extra precondition of section 7.1, with `fromStage` = the first repeating stage.
5. **`Stalled`** cannot occur in M1 (no parks); `next` returns an error naming the parked units if it ever does. M2 replaces it with the exit-3 hold payload.
6. **Gate rule (M1 minimum):** the sweep refuses `--result skipped` and refuses `complete` while a unit is not satisfied. `regression.json` is not read yet (M3), so the skip message does not mention it.
7. **Drift warning text** omits the sha values: "work-units.json changed since the work-unit ledger was built; the change takes effect only through 'dtech-aidlc intent rework' or 'intent split'." (same for `ids.json`). A deleted file counts as drift.
8. **`blockedStreak`** is maintained from M1 (signature of section 6.4); auto-park itself is M2. `autoParkThreshold` is read with the fallback 3 but not yet validated.
9. **`status` preview errors** are shown as `"units": {"error": "Develop cannot select a work unit: …"}` instead of failing `status`.
10. **`intent list`** rows of ledger intents gain `workUnits: {total, byStatus, sweep}` (stored ledger only, no preview) — the task asked for per-unit state in `intent list`; the design only covered `status`.
11. **The rollback tests** (`rollback_projection_keeps_the_release_gate`, `migrated_state_without_the_ledger_is_driven_by_the_1_0_0_selector`) are unit tests in `orchestrate.rs`, because the 1.0.0 selector and `is_phase_gate` are private there.
12. **One existing golden case changed its fixture:** `rust_intent_commands/rework_error_paths_match_golden.json` case `rework-reset-after-report` now writes a `not-applicable` `work-units.json`. Without it, the state (full depth, at the repeat span, no `work-units.json`) qualifies for the new load-failure rework escape, so the case would no longer test "a reset newer than the last report is not blocked". The escape itself is tested in `rust_unit_scheduling.rs`.

Golden files re-blessed in M1 (each diff inspected):
- `rust_routing_parity/lifecycle.json`, `lifecycle-embedded.json`: the F fix (section 4.4). After "implementation-design blocked → intent rework → next", `stage.rework` now carries `requestedAt`, `fromStage` and the request's `detail` (which wins over the blocked report's), and `designRework`/`designReworkPending` are removed from the state at that `next`. This change was not listed in section 16.
- `rust_routing_parity/validation.json`: `--unit` in the allowed options of `orchestrate report`, and the help/completion text.
- `rust_routing_parity/unsafe.json`, `rust_utility_parity/help-and-completion.json`: help and completion text (`--unit`).
- `rust_intent_commands/rework_error_paths_match_golden.json`: the fixture file of note 12.

Manual replay (scratch copy of the owner's `.aidlc` config, pointer, audit log and intent folder, with the `prerequisites` key removed from the copied config so no owner check command ran): `status` previewed 16 `pending` units with `"migration": "pending"`; `doctor` had no workflow failure and warned about the `"_"` placeholder (its only failures were the uncopied harness projection files); `orchestrate next` wrote `units.migrated {units: 16, ignoredKeys: ["_"], priorResults: 12}` and selected red-tests WU-01.

### M2 (blocked-kind taxonomy, park, queue, revisit, auto-park)

Implemented in `rust-cli/work_units.rs` (park, auto-park, close/automatic/drain revisits, `select`, stalled payload, queue view, owner decisions), the new `rust-cli/unit_commands.rs` (`orchestrate park|queue|revisit`), `orchestrate.rs` (selector wiring, exit-3 stall, `stage.rework.revisit`, auto-park in `report`; `persist_active`/`audit` made `pub(crate)`, new `audit_revisits`), `main.rs` (routing), `args.rs`, `help.txt`, `completion.zsh`, and the `autoParkThreshold` validators in `scope.rs`, `config_commands.rs` and `doctor.rs`. Tests: 15 acceptance tests appended to `tests/rust_unit_scheduling.rs`, 3 unit tests in `work_units.rs`, validator cases in `scope.rs`; `tests/repo_content.rs` `COMMAND_KEYS` gained the three commands.

Deviations and additions, with reasons:

1. **"Config command support" is validation, not a subcommand.** Following D11 there is no `config auto-park-threshold` command; the owner sets `autoParkThreshold` in `.aidlc/config.json`, and `orchestrate next`, `config agents` (the only `config` command that validates the file; `config scope` and `config mode` rewrite it unvalidated, as for `unitSplitThreshold`) and `doctor` reject a value that is not a positive integer ("Invalid autoParkThreshold: expected a positive integer"). `orchestrate report` reads the file unvalidated and falls back to 3.
2. **Wait-cycle check is a superset.** Besides open parks' `blockedByUnits`, the walk also follows unsatisfied `dependsOn` edges (a blocker that depends on a unit parked on the parked unit is also a deadlock). Removed units never block (they count as done for automatic revisit and for the "already done" rule).
3. **`foreign-regression` on a test the parked unit owns itself is refused** ("TS-42 is owned by WU-06 itself (owners: WU-06, WU-16) ... name the other owner with --blocked-by WU-xx"), which is how "resolved owners must not include the parked unit" reads for multi-owner tests.
4. **Park writes like a report:** it creates the step's stage row when missing, sets `stages[step].units[U] = {status: "blocked", reportedAt}` and the row's `reportedAt`/`detail`, and appends a `history` row only when it counts the attempt (the unit's selected step is being parked).
5. **`revisit` without `--unit`** refuses `--detail`/`--approved-by`, and an empty result writes nothing, also on a 1.0.0 state (the migration then waits for the next writing command). `decidedBy` is resolved only for `owner-decision` parks; other kinds record `null`.
6. **Revisit information** (`stage.rework.revisit`, `unit.revisit`) also carries the recorded `decision` and `decidedBy`; it is shown until the unit's next report. `ownerDecisions` rows carry `parkedFor` (the park detail). The auto-park report output also carries `detail`.
7. **Queue view:** `ready` lists blocked units too (they are retried, as in the directive's `queue.ready`), and `blocked` repeats them with attempts and streak; parked rows carry `auto` and a `revisit` route. On an intent without work units `orchestrate queue` is an error ("orchestrate queue needs work units ...").
8. **Prerequisite hold after a revisit:** when `next` revisits a unit and then holds on a failing prerequisite, the revisit is not saved; it is recomputed identically by the next `next`. The stall path does save its revisits, as designed.
9. **Interim rework (M1 note 3) closes open parks** with `revisitReason: "rework"`, and `intent rework` already accepts a parked unit as its precondition. Without this the whole-intent reset would leave a `pending` unit with an open park record.
10. **Design-defect message** names `intent rework --unit` (available from M4). Doctor park warnings stay in M5 as planned.
11. **Test seams:** `unit_commands::park_loaded` / `revisit_loaded` take an already loaded workflow, so a test can change `state.json` between load and write and check that nothing is overwritten.

Golden files re-blessed in M2 (each diff inspected; only help/completion/routing text changed):
- `rust_utility_parity/help-and-completion.json`: the three new help entries and the completion rows (`park queue revisit` actions, their flags, the `orchestrate` description).
- `rust_routing_parity/validation.json`: the fallback "Use orchestrate next, report, park, queue or revisit" (two cases), and the embedded help and completion text.
- `rust_routing_parity/unsafe.json`: the embedded completion text.

Acceptance replay (plan M2): `replay_park_foreign_regression_moves_on_and_reopens` uses the uninverted structural copy of the real graph. After WU-01..WU-05 and WU-06 red, `orchestrate park --kind foreign-regression --blocked-by TS-20` resolves to WU-16; `next` then selects WU-07, WU-11 and WU-16 (red and build each), and the `next` after WU-16 is satisfied audits `unit.revisited` (`blocker-done`) and selects WU-06 `build-and-verify` with its red step kept. A manual run on a scratch copy of the owner's intent (prerequisites removed from the copied config) showed `queue` as a `"migration": "pending"` preview (ready WU-01, 15 waiting), `next` migrating (16 units, 12 prior results, `ignoredKeys: ["_"]`) and selecting WU-01 red, `park --unit WU-06 --kind dep-unit --blocked-by WU-16` accepted with `revisit: "auto when WU-16 is done"`, `revisit` (no unit) returning an empty list, and `doctor` failing only on the uncopied harness projection files. The copy was deleted afterwards.

### M3 (attributed regression, deferred regressions, final strict sweep)

Implemented in the new `rust-cli/regression.rs` (location, read with freshness, parse/validate, TS inference, attribution, deferred-regressions upsert/resolve, `verified` upgrade, sweep reopen, directive/doctor views), `orchestrate.rs` (unit build-and-verify gate, sweep gate rule, blocked-sweep reopen, directive `verification`/`sweep`, sweep `stage.rework` additions, `complete_payload`, report output and `gate.decided` additions, `regression.attributed`/`unit.reopened` audit rows), `work_units.rs` (`registered_tests`, `apply_selection` keeps `selectedAt` on re-selection, reconcile reassigns removed owners' open items, a few helpers made `pub`), `doctor.rs` (deferred and unowned warnings), `lib.rs`. Tests: 7 acceptance tests in `tests/rust_unit_scheduling.rs` (`wu16_failures_do_not_block_wu06`, which also covers owner_must_own_the_test, multi_owner_test_without_owner_blocks_the_reporter, failure_of_a_finished_unit_blocks_the_reporter and the unowned/name-inference path; `migrated_1_0_0_state_needs_regression_evidence_and_defers_foreign_failures`; `release_gate_requires_empty_queue_and_green_sweep`; `parked_unit_blocks_release_gate`; `sweep_cannot_be_skipped_or_relabelled`; `rejected_release_gate_allows_rework`; `empty_ownership_charges_the_reporter`), 5 unit tests in `regression.rs` (`name_inference` among them), and the fixture `tests/fixtures/unit_scheduling/regression.wu06.json` (the 16 real failing test names, `owner: "WU-16"`, 13 `normal` + 3 `ignored`).

Deviations and additions, with reasons:

1. **`sweepRun` instead of `sweep`** for `{commands, baselineFailures}` in the approved sweep's report output and `gate.decided` row. M1 already tags sweep reports with `"sweep": true` (output, `stage.reported`, `stage.selected`); reusing the key for an object would break that contract.
2. **Blocked sweep and a stale file.** A `regression.json` older than `sweep.selectedAt` on a `--result blocked` sweep is ignored with a report warning (nothing is reopened), so an environment block is never refused because of an old file. A fresh but invalid file is an error; a missing file means "no attribution", as designed.
3. **A failing blocked sweep always turns `sweep.status` red** and records every failure outside the baseline in `sweep.failures` (`{test, ts, owner, suite}`), owned or not. Selection depends only on unit satisfaction, so this does not change scheduling; it keeps the evidence. The failures are also upserted into `deferredRegressions` with `reportedBy: ["sweep"]`, so `queue`, `doctor` and the next sweep directive see them until a full run no longer lists them. A reopened unit also gets `blockedStreak = null`.
4. **`selectedAt` is kept when `next` re-selects the step that is already current** (no report or park in between, i.e. the same attempt). Otherwise a repeated `orchestrate next` (a resumed session) would make the evidence of that attempt stale. Every report and park clears `currentStage`, so the next attempt is stamped anew and the "old green file" protection holds.
5. **No freshness check when the step has no `selectedAt`**: only a 1.0.0 step that migration kept current at `orchestrate report` lacks one.
6. **Validation details.** An explicit `owner` must look like a work-unit id (`WU-`/legacy `U-`) and resolve to a non-removed unit ("… owner WU-99 is not a work unit of this intent"). Explicit `ts` ids are checked against `ids.json` when it is readable ("cites unregistered TS-21"). The owner check uses the union of the owners of every TS id of the failure. TS ids match by number (`TS-5` = `TS-05`); an inferred id is written `TS-NN`.
7. **Audit and output.** `regression.attributed` is also written for a blocked sweep that brought fresh evidence (with `reopened`), and unit rows carry `verified` (the units upgraded by that run); `own` counts own plus finished-unit failures (always 0 on an accepted unit run). The report output gains a `regression` object with the same details. The directive `sweep` object keeps M1's `units`/`satisfied` and adds `schemaVersion`; `verification.ownership` says "none declared: …" when D17 applies.
8. **Removed owners (design 7.1 step 6)** are handled now: the reconcile at `implementation-design` complete turns open deferred items of a unit that disappeared into unowned items.
9. **Parked gate check** comes before the "not done" check with its own message ("needs no parked work unit; parked: WU-01 (owner-decision)"). The CLI cannot park a unit while the sweep is current (a satisfied unit cannot be parked), so the test uses a hand-edited state; the selector never offers the sweep while a unit is parked.
10. **Doctor "has not started"** means the owning unit has no finished step in `steps`.
11. **Scoped rework after a rejected gate** is tested with `intent rework --detail` (no `--unit`), which the M1/M2 precondition already accepts after a blocked sweep report; `--unit` arrives in M4. For the same reason the finished-unit hint names `intent rework --detail`.
12. **Changed expectations in M1/M2 tests:** the shared `write_artifacts` test helper now also writes a clean full `regression.json` for build-and-verify directives, and three assertions that expected `green` after a unit's complete report now expect `verified` (a clean full run verifies the unit, section 8.4). No golden file changed in M3.

Manual replay (scratch copy of the owner's `.aidlc` config, pointer, audit log and intent folder, `prerequisites` removed from the copied config; deleted afterwards): `queue` previewed the migration with `deferredRegressions.open: 0`; `next` migrated and selected WU-01 red; the existing red artifacts reported complete; the WU-01 `build-and-verify` directive named `.../build-and-verify/WU-01/regression.json` with ownership "none declared"; `complete` without the file was refused; the 16 real WU-16 failures without `owner` were refused as WU-01's own (D17, the real intent declares no `tests`); with `owner: "WU-16"` they were deferred (`regression.deferred: 16`), WU-01 became `green`, `queue` showed 16 open items owned by WU-16, `doctor` warned "16 failing test(s) already in the tree belong to WU-16, which has not started", and `next` moved on to WU-02 red. This is the 2026-11-26T02:17:13Z report of the real run (fact 6) that 1.0.0 recorded as a WU-01 block.

### M4 (scoped rework, fingerprints, split, re-sync after a 1.0.0 write)

Implemented in `rust-cli/work_units.rs` (`request_rework` with named units and split requests, `rework_source` for named units, `resolve_named_units`, `validate_split`, `rework_instruction`, the fingerprint reconcile in `reconcile_rework`, `resync` called from `ensure_ledger` for ledger states, `LedgerChange::Resynced` and `audit_rows`, `reconciled_row`, `clear_stale_current`; `preview` re-syncs in memory), `intent_commands.rs` (`intent rework --unit`, `intent split`), `orchestrate.rs` (audit rows of every ledger change, `stage.rework.split`/`instruction` on the implementation-design directive), `unit_commands.rs`, `ids.rs` (`valid_unit_id`), `regression.rs` (the finished-unit hint names `intent rework --unit`), `args.rs`, `help.txt`, `completion.zsh`. Tests: 13 acceptance tests in `tests/rust_unit_scheduling.rs` (`rework_one_unit_keeps_green_units`, `fingerprint_change_resets_unit_and_direct_dependents_only`, `rework_without_unit_on_ledger_keeps_unchanged_units`, `rework_unit_is_validated_without_writes`, `rework_unit_on_a_1_0_0_state_migrates_first`, `rejected_release_gate_allows_scoped_rework`, `rework_keeps_owner_decision_parks_on_dependents`, `split_replaces_a_unit`, `split_can_drop_the_replaced_unit`, `legacy_rework_marker_is_converted`, `legacy_rework_already_completed_by_1_0_0_is_reconciled`, `stale_ledger_is_resynced`, `legacy_release_gate_is_accepted`) and 3 unit tests in `work_units.rs`. `rework_without_ledger_is_unchanged` is covered by the existing `rust_intent_commands` rework goldens, which keep their 1.0.0 output (only the usage text changed, see below).

Deviations and additions, with reasons:

1. **Named-unit precondition message.** When `--unit` names units of which none is blocked or parked (and the last sweep report is not blocked), the error is "Design rework of WU-02 is only available when one of the named units is blocked or parked, or after a blocked release sweep", so it is clear that another blocked unit does not count. Unknown and removed units have their own errors. Without `--unit` the design's message is kept. `--unit` values are deduplicated (either spelling) and stored as ledger keys; `fromStage` is the blocked (or next) step of the first named blocked or parked unit, or `build-and-verify` after a blocked sweep.
2. **`--unit` without a ledger is refused** ("intent rework --unit needs work units: ..."), also when `work-units.json` cannot be loaded; the load-failure escape stays the plain `intent rework --detail`.
3. **Directive text.** The implementation-design directive's `stage.rework` gains `instruction` (what to change and what to keep byte-identical) and, for a split, `split: {unit, into}`. Both are added only on ledger intents (requests with a `units` key), so 1.0.0 directives and goldens are unchanged.
4. **Sweep record at reconcile.** The interim M1 reconcile reset the whole sweep record. Now the record (attempts, failures, a red status) is kept, and only a green sweep goes back to `pending` when the reconcile reset, added or removed a unit. Selection depends only on unit satisfaction, so a kept red sweep is simply offered again once the reset units are done.
5. **Reconcile details.** A unit that a previous rework removed and that reappears in `work-units.json` counts as changed (reset, `removed: false`). A unit removed now has its open park closed (`revisitReason: "rework"` or `"split"`). Kept units get their (identical) fingerprint, title, `dependsOn` and `tests` refreshed and nothing else. The `units.reconciled` row of a split also carries `split: {unit, into}`.
6. **Split validation.** `--into` ids must be work unit ids (`WU-` or legacy `U-`) and are deduplicated; the split unit must not be satisfied (the error names its status and points to `intent rework --unit`); a split needs no blocked or parked unit, as section 7.3 says. The output is `{"splitRequested": true, intentId, unit, into, fromStage, nextStage}` and the audit row also carries `fromStage`. A split unit that is still listed without being in `--into` is refused at reconcile ("intent split replaces WU-06, but work-units.json still lists it ..."); a `dependsOn` on the dropped id is refused by the existing `work-units.json` validation.
7. **Re-sync is per step, not per unit.** For a unit with newer entries, only the steps whose stage entry has a `reportedAt` later than `syncedAt` are re-derived (complete, skipped or blocked; anything else clears the step); its other steps keep their ledger value. Re-deriving the whole unit with the migration table would take a 1.0.0 rework's reset entries ("pending" with an old `reportedAt`) literally, which section 3.6 forbids. A unit whose last step stays finished keeps a satisfied status (`verified` stays `verified`); `attempts` are unchanged (1.0.0 reports cannot be attributed), `blockedStreak` is cleared, and the new `history` rows carry `via: "1.0.0"`. Timestamps are compared as ISO-8601 strings, the format both binaries write.
8. **Re-sync triggers and read-only views.** The legacy-rework trigger is the `designReworkPending` marker or a newer `reworkRequestedAt`/`reworkResetAt` while no ledger rework is pending; the 1.0.0 `designRework` record, if still present, gains `units: []` and is consumed by the next `orchestrate next` as usual. `queue`, `status` and `doctor` show the re-synced view through `preview` (in memory, never written); when the re-sync cannot run there (for example a reconcile whose `work-units.json` no longer loads), they show the stored ledger. A re-sync that leaves `currentStage` pointing at a step the selector would not pick clears it, as migration does; that check now also keeps a current sweep step.
9. **Audit plumbing.** `LedgerChange::audit_row` became `audit_rows` (a re-sync writes `units.resynced` and, when it reconciled, `units.reconciled`). The "everything finished" path of `orchestrate next` now also appends the ledger change rows (it previously appended only revisits).
10. **Changed expectations in existing tests.** `ledger_rework_resets_units_at_design_complete_and_keeps_history` (M1) now changes WU-01's row in the redesign, because a rework without `--unit` and without design changes keeps every unit in M4; it also asserts `reset` and `kept`. `design_defect_park_is_cleared_by_rework` (M2) names the parked unit with `--unit WU-01` (the design files do not change there). `rust_intent_commands::rework_error_paths_match_golden` expects the new usage text `Usage: intent rework [--unit ID]... --detail "<reason>"` (section 16).

Golden files re-blessed in M4 (each diff inspected; only these lines changed):
- `rust_intent_commands/rework_error_paths_match_golden.json`: the usage text of `intent rework` without `--detail`.
- `rust_routing_parity/lifecycle.json`, `lifecycle-embedded.json`: the same usage text (step 13).
- `rust_routing_parity/validation.json`: `--unit` in the allowed options of `intent rework`, and the embedded help (`intent rework [--unit ID]...`, `intent split`) and completion text.
- `rust_routing_parity/unsafe.json`, `rust_utility_parity/help-and-completion.json`: the help and completion text (`intent` description, `rework split` actions and their flags).

Manual replay (scratch copy of the owner's `.aidlc` config, pointer, audit log, knowledge and intent folder, `prerequisites` removed from the copied config; deleted afterwards): `orchestrate next` migrated (16 units, 12 prior results, `ignoredKeys: ["_"]`) and selected WU-01 red; `orchestrate park --kind design-defect` parked WU-01; `intent rework --unit WU-01` was accepted (`fromStage: red-tests`); the next directive was implementation-design with `stage.rework.units: ["WU-01"]` and the instruction; reporting it complete with the real, unchanged design files audited `units.reconciled` with `reset: ["WU-01"]`, `changed: ["WU-01"]` and the other 15 units kept; `orchestrate next` selected WU-01 red again; `doctor` failed only on the uncopied harness projection files and warned about the `"_"` placeholder.

### M5 (doctor early warnings, waves, methodology, harness skills, documentation, end to end)

Implemented in `rust-cli/work_units.rs` (`waves`, `waves_text`, `wave_warning`, `doctor_warnings`, `stale_warning`, `preview_with_change`; `directive_unit` gains `commands`), `rust-cli/doctor.rs` (stale, early warnings, the split threshold read from the configuration), `rust-cli/orchestrate.rs` (wave line next to the unchanged split warning; `unit_split_threshold` made `pub`), `core/methodology/phases/develop.md` (new "Work-unit scheduling" section, stages 2 and 3, attribution rule, scoped rework), `agents/builder-verifier.md`, `agents/implementation-planner.md`, `templates/work-units.md`, `templates/test-strategy.md`, the methodology `README.md` and `contracts.md`, `harness/{claude,deepseek}/skills/aidlc/SKILL.md` (new identical section "Work units, park, queue and revisit"), `core/data/projection-history.json` (1.0.0 hashes of every changed projected and methodology file), `README.md` ("Work units in Develop", the Evidence-driven Develop highlight, the two thresholds) and `CHANGELOG.md`. Tests: 5 acceptance tests in `tests/rust_unit_scheduling.rs` (`doctor_reports_inverted_dependency_waves_and_unassigned_tests`, `first_unit_directive_suggests_waves_beside_the_split_warning`, `doctor_reports_parks_cycles_unconfirmed_decisions_and_stale_ledger`, `end_to_end_unit_scheduling` with the metrics, `replay_of_a_real_intent_copied_to_a_temp_fixture`), 3 unit tests in `work_units.rs`, and `repo_content.rs::skills_and_methodology_explain_unit_scheduling`.

Deviations and additions, with reasons:

1. **Waves cut an oversized level into chunks.** Section 14 makes a level larger than the threshold its own wave, which would break "at most N units". Units of one level never depend on each other, so the level is cut into chunks of N in `order`, and the last chunk stays open for the next levels. The real graph is unaffected (the three waves of section 14 are asserted).
2. **Wave line text.** "Suggested waves of at most N units: 1) …; 2) …. The CLI runs units in dependency order either way; the owner may split the intent along these waves, or review the work wave by wave." It is added only on ledger intents, next to the verbatim split warning, on the same directive (first unit, first repeating stage). Waves are advisory; the selector is unchanged.
3. **`unit.commands`** in unit directives (the "directive additions for harnesses"): the `orchestrate report --unit`, `orchestrate park --unit` and `orchestrate queue` lines for the unit. Section 12 does not list it; it is additive and ledger-only, so no golden changes.
4. **Doctor rows.** The inverted-dependency rule reads `history` and migrated `priorResults` (complete or skipped; the earliest one is quoted) and fires only while the dependency has no finished step and is not satisfied. The "not owned" rule needs non-empty ownership and a readable `ids.json` (the current file, read only for this warning). The wave row is shown while the sweep is not green. The cycle rows reuse `park`'s checks (`depends on` first, then `waits on`). The unconfirmed-decision row counts closed `owner-decision` parks decided by "AI Orchestrator". The stale row comes from the re-sync a preview would make (`preview_with_change`), so it names the units, a legacy rework and a legacy gate; a re-sync that cannot run in the preview shows no stale row (the stored ledger is shown, as in M4).
5. **The no-ownership row** says what happens instead of only that attribution is impossible: "…a failure without an explicit owner in regression.json is charged to the unit that reports it" (D17).
6. **Onboarded routing fixtures updated, not re-recorded.** `rust_routing_parity/fixture-onboarded-{claude,deepseek}.json` hold the reference onboarding with the 1.0.0 skill. Their `SKILL.md` text and `.projection.json` hash were replaced with the new skill (as the 1.0.0 release did), and nothing else; re-recording from the binary would change unrelated files (the known onboarding gap). Without this, `lifecycle.json`, `lifecycle-embedded.json` and `harness.json` would gain a "projection behind" warning in every step, because the crate version is not bumped (D13).
7. **Methodology files are written with LF line endings** (Git normalises them; `hash_text` folds CRLF), like the M1-M4 files.
8. **The real-intent replay is an opt-in test.** `replay_of_a_real_intent_copied_to_a_temp_fixture` copies the active intent of `DTECH_AIDLC_REPLAY_PROJECT` (config without `prerequisites`, pointer, audit log, intent folder; no `.env`, credential, secret or symlinked files) into a temporary folder and runs there; without the variable it prints "skipped" and passes. The real text never lands in the repository (D14).
9. **Not implemented (as recorded in section 14):** the plan's "code imports across units" doctor check.

Golden files re-blessed in M5 (each diff decoded and checked; only the projected skill changed):
- `rust_config_commands/{config_agents,config_gate_mode,config_mode,config_prerequisite,config_scope}_parity.json`, `rust_harness_commands/{harness_add,harness_refresh,harness_refresh_multiple_harnesses,harness_status}_parity.json`, `rust_knowledge_commands/{onboarded_project_commands,records_list_show_check_and_index,source_management_and_errors}_match_golden.json`: the projected `SKILL.md` sha256 and byte size, and the stamp hash in `.projection.json`.
- `rust_routing_parity/harness.json`, `onboard.json`: the projected `SKILL.md` text and its stamp hash.
- `rust_routing_parity/fixture-onboarded-claude.json`, `fixture-onboarded-deepseek.json`: note 6.

Replay on a copy of the owner's intent (`DTECH_AIDLC_REPLAY_PROJECT=<owner project>`, temporary folder deleted by the test): `status` previewed 16 `pending` units; `queue` showed WU-01 ready; `doctor` had no workflow failure and warned about the `"_"` placeholder, the WU-06 → WU-16 inverted dependency (red-tests 2026-11-24T17:06:28.829Z), the missing test ownership and the three waves of section 14; `orchestrate next` wrote `units.migrated {units: 16, ignoredKeys: ["_"], priorResults: 12}` (old audit bytes kept as a prefix) and selected red-tests WU-01; after WU-01's red step, its build step deferred the 16 real WU-16 failures (`regression.deferred: 16`), WU-01 became `green`, doctor warned that 16 failing tests belong to WU-16, which has not started, and `next` moved on to WU-02 red.

### Review fixes (after M5)

Each fix has a regression test at the end of `tests/rust_unit_scheduling.rs`.

1. **A skipped design cannot strand a ledger rework.** `orchestrate report --result skipped` on `implementation-design` is refused while `workUnits.rework` is pending ("cannot be skipped while a design rework or split is pending"). Only a `complete` report reconciles the request; before, a skip left the request pending with the design finished, so nothing selected the design again and park, revisit, rework and split all refused. A 1.0.0 skip under a ledger rework is still reconciled by the re-sync (section 3.6, M4). Test: `skipped_design_while_a_ledger_rework_is_pending_is_refused`.
2. **The current unit is re-validated (section 5).** `ensure_ledger` on a stored ledger now clears `currentStage`/`currentUnit` when the recorded unit step cannot run (`work_units::unit_step_problem`: a pending rework, a removed, parked or satisfied unit, unsatisfied dependencies, or a step that is not the unit's next one). A 1.0.0 `orchestrate next` moves only the pointer, so the re-sync saw nothing stale and the new binary recorded reports on a parked unit. `report` then refuses with "No current stage; run orchestrate next first (the recorded step, <stage> of <unit>, cannot be reported: <reason>)". The check accepts any runnable step, not only the selector's exact pick, so a manual revisit of an earlier unit does not invalidate the step in progress. A unit counts as parked when its status is `parked` **or** it holds an open park record (`work_units::is_parked`): such a unit is not ready, is listed as parked in a stall, keeps the sweep from being selected, and refuses the release gate. Test: `report_on_a_parked_unit_selected_by_an_older_binary_is_refused`.
3. **Owner decisions follow the gates' mode rule (section 6.3).** In an AI-mode intent `orchestrate revisit` of an `owner-decision` park records `decidedBy: "AI Orchestrator"` (with or without that `--approved-by`) and refuses any other `--approved-by`, so the release summary lists the decision as unconfirmed. Human-mode intents are unchanged. Defaulting to "AI Orchestrator" instead of refusing a flag-less revisit is the safe direction: the decision is shown as unconfirmed. Test: `ai_mode_owner_decisions_are_recorded_as_unconfirmed`.
4. **An environment prerequisite holds its unit only (sections 4.3 and 6.1).** When the selected unit's blocked step names `--needs` whose check fails, `orchestrate next` passes over it (`work_units::peek_skipping`) to the next ready unit, adds a "Work unit <id> is held: …" warning and audits `prerequisite.checked` with `unit`. When no other unit can run, the held unit is selected and the 1.0.0 hold (exit 3) applies as before; `--force-resume` still overrides it. Preflight prerequisites attached to a stage or phase still hold every unit of that stage. Test: `env_prerequisite_holds_only_its_unit`.
5. **`regression.json` summary consistency (section 8.2).** A file whose `summary.failed` is above 0 while `failures` is empty is refused ("summary.failed is N, but failures lists no failing test"), for unit runs and the sweep. A `failed` count larger than the failures list is still accepted, because suites can count one test more than once. Test: `sweep_summary_that_contradicts_its_failures_is_refused`.
6. **`priorResults` are shown (D16).** `orchestrate queue` gains `priorResults` (`{unit, stage, result, at}` rows in unit order, empty when there are none) and `doctor` warns "N results from before a 1.0.0 whole-intent rework are kept as priorResults (…): WU-01 red-tests skipped <at>, …" while the sweep is not green. `status` rows are unchanged. Test: `migrated_prior_results_are_shown_by_queue_and_doctor`.
7. **Documentation.** The CHANGELOG and M2 note 1 now say that `config agents`, not every `config` command, validates `autoParkThreshold`.

Open: a 1.0.0 `orchestrate report` on a parked unit is re-synced by closing its park with `revisitReason: "resync"`; such an owner-decision park is not listed in `ownerDecisions` (no decision was recorded). No golden file changed for these fixes.

---

## Parallel work units (amends D15)

The owner decided to let several work units be in flight at once, with `unitWorkers` defaulting to 3. A pre-implementation analysis measured the one real intent (16 units, critical path 12, maximum antichain 3): it allows about 1.33x at best. It named three things that parallel units would break: the single current-unit pointer, `report --unit` being limited to the current unit, and regression attribution, which assumes each run covers a tree holding every satisfied unit. The design keeps the 1.1.0 invariants for intents that never claim and adds only what parallel units need:

- **Pool.** `currentStage`/`currentUnit` stay the primary unit in flight. `workUnits.claims` (`[{unit, stage, claimedAt}]`) holds the others, up to `unitWorkers` in all. The key is absent when empty, so state goldens and one-worker intents keep their shape.
- **Claiming is opt-in.** Only `orchestrate next --claim` adds a unit. It uses `peek_skipping` with the in-flight units as the skip list, so it takes the first ready unit in order that is not in flight and never the sweep. Plain `next` never claims, so a harness that does not know about claims (DeepSeek, older skills) runs one unit at a time as in 1.1.0. A `--claim` with nothing to claim prints a Busy payload (exit 0, no writes), never the stall exit.
- **D15 amended.** `report --unit` and `park --unit` accept any unit in flight, and `--unit` is required while more than one is. A claimed unit is selected and reported through a view whose current step is the claim. Artifact paths, the `regression.json` location, attempts and mirrors are computed exactly as for the current unit, and the current unit is put back before the state is written.
- **Freshness (keeps D9, resolve-unlisted and the verified upgrade sound).** A unit's `regression.json` must postdate `max(unit.selectedAt, workUnits.lastSatisfiedAt)`, where `lastSatisfiedAt` is stamped whenever a unit becomes satisfied during a parallel run. A unit without `selectedAt` keeps the 1.1.0 behaviour (no check). With one unit in flight the selection is always later, so nothing changes. With several, each unit's final run covers a tree that holds every satisfied unit's code.
- **Parallel run.** `workUnits.parallelSince` is set when the first claim is made and removed once nothing is in flight. While it is set, plain `next` resumes the current unit (or promotes the oldest claim) instead of re-selecting, so a promoted or continuing unit is never replaced by the selector's first pick. `resync` keeps a runnable current unit instead of clearing a non-first one. `lastSatisfiedAt` is stamped only during a run, so intents that never claim gain no keys. `--claim` runs the automatic and drain revisits before looking for a unit, as `select` does.
- **Continuity.** During a parallel run, a unit reported complete or skipped on `red-tests` stays in flight on `build-and-verify`, and its `selectedAt` is stamped then. It leaves the pool on its last step, on blocked, on park, or when `ensure_ledger` finds it can no longer run (`drop_unrunnable_claims`, which also clears every claim while a design rework is pending).
- **Not enforced by the CLI.** File-disjointness of in-flight units, worktree isolation and running the suites on the integrated tree are left to the harness. The Claude skill requires them and falls back to one unit at a time when they cannot be met.
