# Problem statement: `dtech-aidlc monitor --port <n>`

Intent: add-a-port-n-option-to-dtech-aidlc-monitor-so-th (intent 3). Depth: fast. Mode: ai. Stage: analyze/problem-framing.

## Problem

`dtech-aidlc monitor` (intent 2, unreleased 1.3.0) binds `127.0.0.1:0`, so the OS picks a new port on every run (`rust-cli/monitor/mod.rs`). A user cannot bookmark the dashboard, pin a browser tab or write a stable local firewall or wrapper rule, because the port changes each run. (Source: User-provided request in `intent.md`; the firewall or wrapper rule use is AI-inferred, not in the request.)

Plain statement for the owner: this feature gives a stable address, not a working bookmark. The token is new each run and the fragment is dropped on reload (README), so a fixed-port bookmark always lands on `Token missing` until the full URL from stderr is opened. Approving this intent means approving that.

## Who

- Developer who runs the monitor often on one machine and wants a stable `http://127.0.0.1:<n>/` address. (US-001)
- Developer, script or AI agent that starts the monitor without caring about the port and must keep today's behaviour. (US-002)

User stories (template-lite, fast depth):
- US-001: As a developer, I want to start the monitor with `--port <n>` so that the dashboard is always on the same loopback port.
- US-002: As an existing user or script, I want `monitor` without `--port`, and every other command, to behave as before, except for the merged-flag-table effects of OQ-1.

## Desired outcome (success measures)

- `dtech-aidlc monitor --port 8787` listens on `127.0.0.1:8787` only and prints the same startup line shape with 8787.
- Without `--port`, behaviour is unchanged: a free port is chosen.
- A bad or unavailable port is refused clearly (exit 1, one fixed stderr line, nothing on stdout); the monitor never falls back to another port.
- Help, completion, validation goldens, README Monitor section and the CHANGELOG 1.3.0 entry agree on `--port`; the full test sweep stays green (baseline 657 passed, 0 failed, 1 ignored) and golden diffs touch only `monitor`/`--port` text (checked word-wise, AC-007).

## Scope

In:
- `--port <n>` (also `--port=<n>`, the parser already accepts both) on `monitor` only.
- Validation, bind-failure message, help line, zsh completion, command flag table, README "## Monitor", one CHANGELOG 1.3.0 line.
- Tests: `tests/monitor_http.rs` (bind, Host), unit tests for the validator, `tests/repo_content.rs` (help/completion/flag-table agreement and README needles), scoped golden re-bless.

Out:
- Any bind address other than 127.0.0.1 (no `--host`, no `0.0.0.0`, no `localhost`/IPv6).
- A persistent token or token in a config file; the token stays per run.
- Port ranges, `--port 0` as "auto", retry or fallback to another port, environment variable or config-file port.
- Version bump, tag, release, merge to main (owner: main is fast-forwarded only after this intent).
- New crates; changes to MCP server, telemetry, installers, or any other command.

## Constraints

- Binds 127.0.0.1 only; Host check stays exactly `127.0.0.1:<bound port>` (`rust-cli/monitor/http.rs`, intent 2 decision) and uses the chosen port.
- Token is generated per run and printed once on stderr; nothing on stdout (intent 2 decision).
- No new crates (intent 2 Q3, still binding); std only.
- Fold into unreleased 1.3.0: no version bump, one CHANGELOG 1.3.0 line. Same branch `feature/please-implement-plans-mcp-server-enhancement-sc`.
- Golden re-bless only for the monitor/`--port` help, completion and validation lines, via `DTECH_AIDLC_BLESS=1`, verified by diff. Other commands unchanged except the OQ-1 parser effects.
- Must work on macOS, Linux and Windows.

## Decisions made in this framing (AI conductor, for owner review at the gate)

- D1 Valid range is 1..=65535 as plain ASCII digits. Reason: 0 means "pick for me" in the OS and is already the no-option behaviour, so accepting it would make two spellings of one thing; `+5`, `-5`, spaces, hex, and leading `+` are refused by the digits-only rule. Leading zeros (`08080`) are digits and parse to 8080; accepted (decision, low risk), not echoed differently.
- D2 Ports 1..1023 are not refused up front. Reason: whether an unprivileged bind works is an OS matter (macOS 10.14+ lets unprivileged processes bind low ports on wildcard but behaviour on 127.0.0.1 differs by version; Linux needs `net.ipv4.ip_unprivileged_port_start` or a capability; Windows has no such rule). The program does not guess; a failed bind gives the same fixed bind-failed message as "in use". Documented in README.
- D3 Messages (fixed text, prefix `dtech-aidlc: ` as for other CLI errors):
  - Invalid value: `Option --port requires a whole number from 1 to 65535`.
  - Empty value, missing value, and `--port` followed by another option reuse the existing parser message `Option --port requires a non-empty value`; duplicate reuses `Option --port was given more than once` (no new parser code; both exit 1 today).
  - Bind failed with `--port`: `Cannot bind 127.0.0.1:<n> (port in use or not permitted)` where `<n>` is the already validated number. Free-port mode keeps `Cannot bind a loopback port`.
- D4 Order: parser errors, unknown option, positional limit, root resolution and `assert_safe_state_dir` run first (they already touch the file system). Port validation then runs at the start of `monitor::run`, before the `store_state` telemetry check, token and bind. For a refused value nothing is written and no startup line is printed.

## Open questions

- OQ-1 (decided, D5, AI conductor 2026-11-28, following the existing convention: `--port` joins the merged flag table like `--depth`; the listed `status` cases are disclosed and get tests) `parse_args` merges every command's flags into one global table, so adding `--port` as a String option changes other commands: `status --port` / `--port=` / `--port --intent x` fail with `Option --port requires a non-empty value` instead of `Unknown option --port for 'status'. Allowed options: --intent, --project.`; `status --port 1 --wat` names `--wat`; `status --port 1 --port 2` says `was given more than once`. No golden covers these. Default: accept as the same effect every command-specific option already has (as `--depth`), add tests for the listed cases. Alternative: a design that keeps `--port` out of the global table. Monitor's own unknown-option text is `Allowed options: --port, --project.`
- OQ-2 Windows: Rust std sets neither SO_REUSEADDR nor SO_EXCLUSIVEADDRUSE, so binding a port someone already holds fails with WSAEADDRINUSE. The real gap is the reverse: while the monitor holds a known fixed port, another local process may share the address and serve a page that reads the token from the URL fragment. Per Windows enhanced socket security this needs the same OS user (Assumption from documentation, not tested), so it falls under T2-06. Design decides: set SO_EXCLUSIVEADDRUSE through std-only FFI (intent 2 already uses raw BCryptGenRandom), or record it as an accepted residual in the threat review next to T2-05/T2-06. Default: record as residual and say so in README.
- OQ-3 Bookmark behaviour: the saved URL without `#token=...` shows `Token missing`; an old URL with a previous run's token is rejected (as today after a restart). Fixed port does not make the bookmark self-sufficient. Owner may later want a persistent token; deliberately out of scope here (security decision from intent 2).
- OQ-4 Not verified here: macOS/Windows low-port behaviour on real machines; CI covers only ephemeral and test-chosen high ports.
