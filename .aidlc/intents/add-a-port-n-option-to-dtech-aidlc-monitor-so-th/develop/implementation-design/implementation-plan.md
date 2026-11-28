# Implementation plan: `dtech-aidlc monitor --port <n>`

Intent 3, depth fast, mode ai. Stage develop/implementation-design. Fast depth: this plan carries the functional design, interface, test strategy and the single work unit. No `work-units.json` or `ids.json`; criteria are cited by AC- id only. Inputs: `analyze/problem-framing/{problem-statement,acceptance-criteria,gate-recommendation}.md` (decisions D1..D5, OQ-1..OQ-4).

## 1. Functional design

Today `monitor::run(root)` (`rust-cli/monitor/mod.rs`) checks the telemetry store, makes the token, binds `127.0.0.1:0`, prints the startup line and serves. The change adds one optional value.

1. `args.rs`: a new `PORT_FLAGS` table `[OptionSpec{name:"port", kind:String}]`; `command_flags("monitor")` returns it (removed from the empty-table arm). `parse_args` already merges every command table, so `--port` and `--port=<n>` parse everywhere with the existing messages (empty, missing, option-like value, duplicate). No new parser code. (AC-003, AC-010, D5)
2. `main.rs` monitor arm: read `flags.get("port")` as `Option<&str>` (only `FlagValue::String` is possible) and call `monitor::run(&root, port)`. The generic unknown-option code at `main.rs:55-75` already builds the allowed-options text from `command_flags`, so `monitor --no-such-option` becomes `... Allowed options: --port, --project.` with no edit there. (AC-007)
3. `monitor/mod.rs`: new `parse_port`; `run` validates the value first, then the unchanged telemetry check and token, then binds `("127.0.0.1", port)` where `port` is 0 when no option was given. Bind error text depends on mode. Startup line, `http::serve` and the Host check are untouched: the Host check already derives from the bound listener's `local_addr()`, so it uses the chosen port for free. (AC-001, AC-002, AC-004, AC-005, AC-006)
4. Help, zsh completion, README, CHANGELOG, goldens. (AC-007, AC-008)

Validation order (D4), first failure wins: parser errors (missing, empty, duplicate) -> unknown or disallowed option -> positional limit -> root resolution and `assert_safe_state_dir` -> in `monitor::run`: `--port` value validation -> `store_state` telemetry check -> token -> bind. Steps 1-4 already exist in this order in `main.rs`; the only new rule is that `parse_port` runs as the first statement of `run`. (AC-003)

## 2. Interface (exact)

CLI syntax: `dtech-aidlc monitor [--port N] [--project DIR]`, also `--port=N`, either option order. Without `--port`: free OS-chosen port as today. (AC-001, AC-002)

```rust
// rust-cli/monitor/mod.rs
/// Digits-only 1..=65535. Leading zeros allowed ("08787" -> 8787). "0", "00", "+80", " 80", "0x50",
/// "1e3", "-5", "8080.0", empty and anything over 65535 are refused.
pub fn parse_port(value: &str) -> Result<u16, String>;
pub fn run(root: &Path, port: Option<&str>) -> Result<u8, String>;
```

`parse_port`: if `value` is empty or has any non-ASCII-digit byte -> `Err(PORT_RANGE_MSG)`; strip leading `0`s; if the remainder is longer than 5 chars or `parse::<u16>()` fails or the result is 0 -> same Err. The explicit digit check is required because `u16::from_str` accepts `+`. `main.rs` call: `dtech_aidlc::monitor::run(&root, flags.get("port").and_then(|v| match v { FlagValue::String(s) => Some(s.as_str()), _ => None }))`. Only `monitor` reads `port`; every other command ignores it.

Exact stderr (each is `dtech-aidlc: <message>\n`, exit 1, stdout empty, nothing written to the project, no startup line):

| Case | Message |
|---|---|
| Invalid value (`0`, `00`, `65536`, `99999999999999999999`, `abc`, `80a`, `8 0`, `0x50`, `1e3`, `+8080`, `-8080`, `8080.0`) | `Option --port requires a whole number from 1 to 65535` |
| Missing, empty, `--port=`, or followed by a known option such as `--project` (parser, existing) | `Option --port requires a non-empty value` |
| `--port` followed by an unknown option-like token, e.g. `monitor --port --wat` (the token is taken as the value) | `Option --port requires a whole number from 1 to 65535` |
| Duplicate (parser, existing) | `Option --port was given more than once` |
| Bind failed with `--port` (in use or not permitted) | `Cannot bind 127.0.0.1:<n> (port in use or not permitted)` with `<n>` the validated number, no leading zeros |
| Free-port bind failed, no `--port` (unchanged) | `Cannot bind a loopback port` |
| Unknown option on monitor | `Unknown option --no-such-option for 'monitor'. Allowed options: --port, --project.` |
| Extra positional (unchanged) | `Unexpected argument 'extra0' for 'monitor'.` |

Startup line unchanged: `monitor: http://127.0.0.1:<port>/#token=<32 hex>  (Ctrl+C to stop)`. In fixed-port mode the code binds once and never retries or falls back. The bind-failed branch returns before any token is printed (the token is only printed after a successful bind, as now). Messages are consts so tests and code share one spelling.

Help line (replaces `rust-cli/help.txt:63`): `  monitor [--port N]      Serve the read-only telemetry dashboard on 127.0.0.1 (--port N: fixed port 1-65535, default a free port; address and token on stderr; Ctrl+C stops it)`. Keeps the 24-column description alignment of neighbouring lines. The existing agreement test requires `--port` in help because the flag table now lists it.

Completion: keep the `'monitor:Serve the read-only telemetry dashboard on 127.0.0.1'` command entry unchanged and add an arm in the `args)` case, next to `status)`: `monitor) _arguments '--port[Fixed loopback port 1-65535]:port:' ;;` written in the same style as the `status)` arm (use the same one- or two-line layout as `status)`; either passes the name-extraction filter, this is style only). Must pass `zsh -n` (already checked by `help_completion_and_the_command_table_stay_in_step`).

## 3. D5 effect on other commands (disclosed)

Because the flag table is merged, these cases change on `status` (no golden covers them; exit code stays 1):

| Command | Before | After |
|---|---|---|
| `status --port` | `Unknown option --port for 'status'. Allowed options: --intent, --project.` | `Option --port requires a non-empty value` |
| `status --port=` | same as above | `Option --port requires a non-empty value` |
| `status --port --intent x` | same as above | `Option --port requires a non-empty value` |
| `status --port 1 --wat` | names `--port` | `Unknown option --wat for 'status'. Allowed options: --intent, --project.` |
| `status --port 1 --port 2` | `Unknown option --port ...` | `Option --port was given more than once` |
| `status --intent --port` | `--port` taken as the intent id (then `Unknown option --port`) | `Option --intent requires a non-empty value` |

`status --port 1` alone still reports `Unknown option --port for 'status'. Allowed options: --intent, --project.` (disallowed option, exit 1), so the common misuse keeps its old text. General rule: `is_known_option` (args.rs:478-482) uses the merged flag table, so after the change `--port` and `--port=...` can no longer be the value of any String or List option on any command (any other command's String/List option behaves like the `--intent` row). Tests pin all seven rows (AC-010).

## 4. Windows decision (OQ-2)

Decision: do not add `SO_EXCLUSIVEADDRUSE`; record same-user address sharing as an accepted residual next to T2-06 (README and the intent's knowledge write-back). Justification: `std::net::TcpListener::bind` creates, binds and listens in one call, so setting the option before `bind` needs the whole socket built by hand: `WSAStartup` or reliance on std's lazy init, `socket`, `setsockopt`, `bind` with a hand-made `SOCKADDR_IN`, `listen`, `FromRawSocket`, plus Windows-only `unsafe` FFI that cannot be built or tested on the macOS dev machine (CI only). That is not trivial, and the exposure needs the same OS user, which T2-06 already accepts (a same-user process can read the terminal and `.aidlc/telemetry/` anyway). Documented, not equalised (AC-009). Revisit only if the token model changes.

## 5. Test strategy

Framework: Rust `cargo test`, existing suites. Baseline 657 passed, 0 failed, 1 ignored; the final count must be 657 plus the new tests, 0 failed. No new crate (AC-010). All new tests run on macOS, Linux and Windows; none is `cfg(unix)`-only, none uses ports below 1024, none connects to non-loopback addresses.

Helpers in `tests/monitor_http.rs`:
- `free_port()`: bind `127.0.0.1:0`, read the port, drop the listener (small race accepted on the success path).
- `spawn_args(project, extra: &[&str]) -> Monitor`: the existing `spawn` generalised to put `--project X` before `monitor` and `extra` after it (`spawn(project)` calls it with `&[]`; no behaviour change for existing tests). Startup-line parsing is reused.
- `spawn_argv(argv: &[&str]) -> Monitor`: takes the whole argv with no `--project` prefix, for the `monitor --port P --project d` order (passing `--project` inside `extra` would give `Option --project was given more than once`). `spawn_args` builds its argv and calls this.
- `run_refused(project, args) -> (code, stdout, stderr)`: `Command::output()` for the refusal tests; a refused run exits, so nothing hangs.
- `spawn_retry(project, extra)`: for the same-port restart check, retries for up to 5 s (Windows restart flake); the product never retries. The current `spawn` panics when there is no startup line (tests/monitor_http.rs:57-63), so `spawn_retry` needs its own non-panicking spawn that returns failure (no startup line) so a failed bind can be retried; the panicking `spawn_argv` stays for the other tests.

Test-first list (written and observed failing before the code, they are a security/public-contract slice): the `port_` refusal and bind tests listed below as marked (R), plus `parse_port` unit tests (R). Their red run fails for behaviour (the flag is currently unknown: `Unknown option --port`), not for a compile error; the `parse_port` unit tests are red only after a stub `parse_port` that returns `Ok(0)` is added so the test compiles. Documentation, help and completion checks are written alongside the change.

Unit tests (inline `#[cfg(test)]` in `rust-cli/monitor/mod.rs`, pure function):
- `parse_port_accepts_the_range_and_leading_zeros` (R): `1`, `80`, `8787`, `08787`, `65535`, `0065535` accepted with the right value. (AC-003, AC-001)
- `parse_port_refuses_everything_else` (R): `0`, `00`, `65536`, `99999999999999999999`, `abc`, `80a`, `8 0`, `0x50`, `1e3`, `+8080`, `-8080`, `8080.0`, empty, a non-ASCII digit such as the Arabic-Indic `٨٠`, each returns exactly the range message. (AC-003)

Integration tests in `tests/monitor_http.rs` (prefix `port_`):
- `port_flag_binds_the_chosen_loopback_port_in_both_forms_and_orders` (R): `--port P`, `--port=P`, `--port P --project d` (via `spawn_argv`, port option before the project option, both after `monitor`) and `--project d --port P` (project first, via `spawn_args`) after the old one is stopped (use four distinct free ports); startup line port equals P; stdout empty; a TCP connect to `127.0.0.1:P` succeeds; `GET /` with `Host: 127.0.0.1:P` is 200. (AC-001, AC-005)
- `port_flag_accepts_leading_zeros` (R): `--port 0<P>` for any free port P (e.g. `049152` gives 49152; `parse_port` strips leading zeros and checks at most 5 digits remain, so `0<P>` is valid for every P) shows P in the startup line; `--port 00` is refused. (AC-003)
- `port_flag_refuses_invalid_values_with_the_fixed_line_and_writes_nothing` (R): for every value in the AC-003 list, stderr is exactly `dtech-aidlc: Option --port requires a whole number from 1 to 65535\n`, stdout empty, exit 1, and the temp project (not a git repo) has no entries after the run. (AC-003)
- `port_flag_boundary_65535_passes_validation` (R): `--port 65535` either starts (startup line shows 65535, then kill) or fails with exactly the bind-failed line for 65535; it never prints the range message; `--port 65536` is refused with the range message. (AC-003, AC-004)
- `port_flag_missing_empty_and_option_like_values_are_refused` (R): `--port`, `--port ""`, `--port=`, `--port --project x` all give `dtech-aidlc: Option --port requires a non-empty value\n`, exit 1, stdout empty. (AC-003)
- `port_flag_duplicate_is_refused_and_nothing_is_bound` (R): `--port P --port Q` gives the duplicate message; exit 1, the duplicate message, stdout empty and no startup line are the evidence (no connect check: P was released by `free_port`, so another process may hold it and a refused loopback connect is slow on Windows). (AC-003)
- `port_flag_validation_order_follows_the_decision` (R): `monitor --port abc extra` gives `Unexpected argument 'extra0'`-style text (`Unexpected argument 'extra' for 'monitor'.`); `monitor --port 0 --wat` gives `Unknown option --wat for 'monitor'. Allowed options: --port, --project.`; `monitor extra0 --port 8787` gives `Unexpected argument 'extra0' for 'monitor'.`; with `.aidlc/telemetry` a regular file plus `--port abc` the range message wins, and with `--port <free>` the existing `Refusing unsafe .aidlc/telemetry path` still wins over a successful bind. (AC-003, D4)
- `port_in_use_is_refused_without_fallback` (R): the test holds a `TcpListener` on `127.0.0.1:P` for the whole run; the monitor run gives exactly `dtech-aidlc: Cannot bind 127.0.0.1:P (port in use or not permitted)\n`, exit 1, stdout empty, no `monitor:` or token on stderr; the listener is dropped only after the assertions. (AC-004)
- `no_port_flag_keeps_two_free_ports_and_the_old_bind_text`: existing `ts029_two_monitors_get_distinct_ports_and_tokens` stays unchanged and green (AC-002); a new assertion is not needed. The free-port failure text is not reproducible without OS support and is covered by code review only (documented gap).
- `port_flag_host_check_uses_the_chosen_port` (R): on `--port P`: `Host: 127.0.0.1:P` is 200, `127.0.0.1:Q` (Q = P±1), `localhost:P` and `127.0.0.1` are 421 `host_rejected`. (AC-005)
- `port_flag_token_is_new_on_every_run` (R): start on P, record the token, kill and wait, restart on P with `spawn_retry`; the tokens differ; the old token gets 401 `unauthorized` from the new instance; `GET /` without a token still serves the page shell. (AC-006)
- `status_port_effects_are_the_disclosed_ones` (D5, in `tests/rust_cli.rs` or the same file with `CARGO_BIN_EXE`): the seven rows of section 3 (including `status --intent --port`) exact stderr, exit 1, stdout empty. Also `intent list --port 1` (same disallowed-option text with its own allowed list) to show another command is unchanged apart from D5. (AC-010)

Documentation and agreement tests (edit existing tests, not new files):
- `tests/repo_content.rs` `help_completion_and_the_command_table_stay_in_step`: needs no edit; it now exercises `--port` through `command_flags("monitor")` and `help.txt`. Add one assertion in it that the completion text contains `--port` inside the monitor arm. (AC-007)
- `tests/repo_content.rs` `ts036_readme_telemetry_and_monitor_sections`: add needles `dtech-aidlc monitor --port 8787`, `1 to 65535`, `Token missing`, `no other port`, `excludedportrange`, `another local user` to the monitor needle list; and a CHANGELOG assertion that the 1.3.0 section mentions `--port`. (AC-008)
- `rust-cli/args.rs` inline table test (`("monitor", 0, &[])` near line 687): change to `("monitor", 0, &["port"])`. (AC-007)
- Version unchanged: a diff check that `Cargo.toml` and `Cargo.lock` are unchanged (`git diff --stat` empty for both). (AC-010)

Existing tests likely affected: the args table test above, `option_and_argument_validation_matches_the_golden_for_every_command`, `help_and_completion` goldens (utility_golden, routing_golden help and completion steps, `unsafe.json` completion entry), `help_completion_and_the_command_table_stay_in_step`, `ts036_readme_...`. Everything else must stay byte-identical.

Not tested, documented (AC-009): low-port privileges per OS, Windows excluded port ranges (WSAEACCES), macOS/BSD wildcard-versus-loopback sharing, Windows same-user address sharing (section 4), the free-port bind failure message.

## 6. File-level change list (single unit)

| File | Change |
|---|---|
| `rust-cli/args.rs` | add `PORT_FLAGS`; `command_flags`: `"monitor" => PORT_FLAGS` (drop it from the empty arm); update the inline table test `("monitor", 0, &["port"])`. |
| `rust-cli/main.rs` | monitor arm (~line 131) passes the `port` flag value to `monitor::run`. No change to lines 55-75. |
| `rust-cli/monitor/mod.rs` | `parse_port`, message consts, `run(root, port)` with validation first and the two bind-failed texts; inline unit tests. Update the module doc comment (one line: optional fixed port). |
| `rust-cli/help.txt` | the monitor line as in section 2. |
| `rust-cli/completion.zsh` | `monitor)` arm in `args)`. |
| `tests/monitor_http.rs` | helpers and `port_` tests above. |
| `tests/rust_cli.rs` | `status_port_effects_are_the_disclosed_ones` (or in `monitor_http.rs` if the CLI helper there is simpler). |
| `tests/repo_content.rs` | needles and completion assertion above. |
| `tests/golden/routing_golden/validation.json`, `tests/golden/utility_golden/help-and-completion.json`, `tests/golden/routing_golden/unsafe.json` | re-blessed, section 7. No other golden file. |
| `README.md` "## Monitor" | section 8. |
| `CHANGELOG.md` 1.3.0 | one new line, section 8. |
| `Cargo.toml`, `Cargo.lock` | unchanged. |

One work unit: the code, tests, goldens and docs are one small vertical slice; splitting would create a test-only or docs-only unit, which the methodology discourages (well under the split threshold of 6).

## 7. Golden re-bless procedure

1. Baseline environment first: run the full suite once before any change and record the baseline (657 passed, 0 failed, 1 ignored).
2. Implement code and tests; run `cargo test` and confirm the only failures are the golden mismatches for the three files below (and nothing else).
3. Re-bless in the same environment the goldens were blessed in (the baseline environment: same checkout line endings, LF): `DTECH_AIDLC_BLESS=1 cargo test --test routing_golden --test utility_golden`. Other suites are not run with the bless variable.
4. Expected changed files, and only these: `tests/golden/routing_golden/validation.json` (monitor unknown-option text near line 841, plus the help and completion entries), `tests/golden/utility_golden/help-and-completion.json`, `tests/golden/routing_golden/unsafe.json` (its `completion zsh` entry). Check `git status --short tests/golden` lists exactly three files.
5. Verify word-wise because help and completion are single JSON string values: `git diff --word-diff=porcelain tests/golden`. Every added or removed word concerns `monitor`, `--port` or the new description text; the monitor unknown-option line changes only by `--port, ` inserted; any other changed word means revert that file with `git checkout -- <file>` and investigate. Optionally decode the strings and diff them.
6. Re-run the whole suite without the bless variable; it must be green.

## 8. README and CHANGELOG

README "## Monitor": change the first bullet's "on a free port" and add `--port`. New text (keep existing bullets; edit wording to fit):

```sh
dtech-aidlc monitor            # a free port is chosen
dtech-aidlc monitor --port 8787
```

- `--port N` (1 to 65535, `--port=N` also works) listens on `127.0.0.1:N` only. If the port is in use or not permitted, the monitor exits with `Cannot bind 127.0.0.1:N (port in use or not permitted)` and starts on no other port. Low ports may need privileges on macOS and Linux. On Windows a port inside an excluded or reserved range (Hyper-V, WinNAT; see `netsh int ipv4 show excludedportrange protocol=tcp`) fails even when it is high. On macOS and BSD a listener on `0.0.0.0` with the same port may not block the bind, while Linux refuses it.
- The token is still new on every run, so a fixed port gives a stable address, not a working bookmark: open the full URL from stderr, otherwise the page says `Token missing`.
- On a shared machine another local user can bind a well-known port first; the monitor then refuses to start and prints no token. On Windows another process of the same OS user may share the address while the monitor holds the port; this falls under the same-OS-user boundary below.

CHANGELOG 1.3.0, one new line after the `monitor` line: `- Added \`dtech-aidlc monitor --port <n>\` to listen on a fixed port on 127.0.0.1 (1 to 65535) instead of a free one; a port that is in use or not permitted is refused with exit 1 and never replaced, the token is still new on every run, and \`--port\` also appears in help, completion and validation output.` No version bump. The existing last 1.3.0 line ("Golden snapshots changed only for ...") is left as is.

## 9. Rollback

No data, schema or on-disk format changes. Revert the single commit (`git revert <sha>`): `--port` returns to an unknown option for `monitor`, `status --port` messages return to the old text, goldens return with the revert. No migration, no installer change, nothing released (1.3.0 unreleased; no tag, push or merge in this intent).

## 10. Risks and open items for build

- D5 stderr changes for `status --port ...` (exit codes unchanged); tested.
- Test race: `free_port()` can be taken between release and use; acceptable, tests fail loudly, rerun.
- Windows restart check relies on `spawn_retry`; the product never retries.
- Knowledge write-back (later stage): add the Windows same-user residual next to T2-06, the pre-bind note, and refresh the monitor/telemetry codebase map entry.
- No other open decisions; nothing here changes an approved decision (D1..D5 are implemented as written).
