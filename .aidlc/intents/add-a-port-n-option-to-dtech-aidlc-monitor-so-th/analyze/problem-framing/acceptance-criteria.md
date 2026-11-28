# Acceptance criteria

Intent 3, `dtech-aidlc monitor --port <n>`. Messages are quoted without the `dtech-aidlc: ` prefix and a trailing newline; the stderr line is `dtech-aidlc: <message>\n`. See problem-statement.md D1-D4.

## AC-001 Valid port is bound on loopback
- Statement: The system shall, given `monitor --port <n>` with n from 1 to 65535, listen on 127.0.0.1:n only and print the existing startup line on stderr with n as the port.
- Rationale: A fixed address is the whole purpose of the request.
- Source: User-provided request (intent.md).
- Story: US-001

### Scenario: free port given
- Given port 8787 is free
- When the user runs `dtech-aidlc monitor --port 8787`
- Then stderr shows `monitor: http://127.0.0.1:8787/#token=<32 hex>  (Ctrl+C to stop)` (exact text of `rust-cli/monitor/mod.rs:29`, unchanged apart from the port) and nothing is written to stdout
- And a TCP connection to 127.0.0.1:8787 succeeds, and the startup line and the Host check (AC-005) show the monitor is bound to 127.0.0.1 with that port; no test connects to non-loopback or other 127.x addresses (not portable across macOS, Linux, Windows and CI)

### Scenario: equals form
- Given port 8787 is free
- When the user runs `dtech-aidlc monitor --port=8787`
- Then the result is the same as `--port 8787`

### Scenario: global option order
- Given port 8787 is free
- When the user runs `dtech-aidlc monitor --port 8787 --project <dir>`, or with `--project` first
- Then the monitor starts on 127.0.0.1:8787 for `<dir>` in both orders

## AC-002 No option keeps the free-port behaviour
- Statement: The system shall, given `monitor` without `--port`, bind a free loopback port chosen by the OS and print the same startup line as before.
- Rationale: Existing users and scripts must not change (US-002).
- Source: User-provided request.
- Story: US-002

### Scenario: default run
- Given no `--port`
- When the user runs `dtech-aidlc monitor` twice at the same time
- Then both start, on two different ports, each printing its own startup line and token, in the same format as the existing ts029 parsing in `tests/monitor_http.rs`

## AC-003 Invalid port values are refused
- Statement: The system shall refuse any `--port` value that is not 1 to 65535 in plain ASCII digits, exit 1, print one fixed line on stderr, print nothing on stdout, and write nothing to the project.
- Order (D4): checks run in this order and the first failure wins: (1) parser errors (missing, empty, duplicate), (2) unknown or disallowed option, (3) positional limit, (4) root resolution and `assert_safe_state_dir`, (5) in `monitor::run`: `--port` value validation, then the `store_state` telemetry check, token, bind. So `monitor --port abc extra` gives `Unexpected argument`, `monitor --port 0 --wat` gives `Unknown option`, and with an unsafe telemetry path plus `--port abc` the invalid-port message wins.
- Rationale: A clear, scriptable failure; no surprising fallback.
- Source: User-provided request; decision D1, D3, D4.
- Story: US-001

### Scenario: out of range and not a number
- Given any project
- When the user runs `monitor --port <v>` with v in `0`, `65536`, `99999999999999999999`, `abc`, `80a`, `8 0`, `0x50`, `1e3`, `+8080`, `-8080`, `8080.0`
- Then stderr is exactly `dtech-aidlc: Option --port requires a whole number from 1 to 65535\n`, stdout is empty, exit code is 1, no startup line is printed, and no file under the project is created or changed (test projects are temp dirs, not git repos)
- And the digits-only rule is checked explicitly, because Rust `u16::from_str` accepts a leading `+`

### Scenario: leading zeros accepted (D1)
- Given port 8787 is free
- When the user runs `monitor --port 08787`
- Then validation accepts it and the monitor listens on 127.0.0.1:8787 with the startup line showing 8787
- And `monitor --port 00` is refused with the range message above

### Scenario: boundaries
- Given ports 1 and 65535
- When the user runs `monitor --port 65535`
- Then the value is accepted by validation (the bind may still fail, see AC-004)
- And `--port 65536` is refused as above

### Scenario: empty, missing, option-like value
- Given any project
- When the user runs `monitor --port`, `monitor --port ""`, `monitor --port=`, or `monitor --port --project x`
- Then stderr is exactly `dtech-aidlc: Option --port requires a non-empty value\n`, stdout is empty, exit code is 1

### Scenario: duplicate option
- Given any project
- When the user runs `monitor --port 8787 --port 8788`
- Then stderr is exactly `dtech-aidlc: Option --port was given more than once\n`, stdout is empty, exit code is 1, and nothing is bound

### Scenario: extra positional unchanged
- Given any project
- When the user runs `monitor extra0 --port 8787` (same token as the golden at `tests/golden/routing_golden/validation.json:851`)
- Then stderr is exactly `dtech-aidlc: Unexpected argument 'extra0' for 'monitor'.\n` as before (existing text, unchanged) and exit code is 1

## AC-004 Port in use or not permitted is refused without fallback
- Statement: The system shall, when 127.0.0.1:n cannot be bound, exit 1 with one fixed stderr line, print nothing on stdout, and never bind a different port.
- Rationale: A bookmark that silently moves is worse than a clear failure.
- Source: User-provided request; decision D2, D3.
- Story: US-001

### Scenario: port in use
- Given another process listens on 127.0.0.1:8787
- When the user runs `dtech-aidlc monitor --port 8787`
- Then stderr is exactly `dtech-aidlc: Cannot bind 127.0.0.1:8787 (port in use or not permitted)\n`, stdout is empty, exit code is 1, and no startup line and no token are printed (the only observable proof that no other port was opened; the code never retries)

### Scenario: privileged port
- Given an unprivileged user on a system that forbids binding port 80
- When the user runs `monitor --port 80`
- Then validation accepts the value (D2) and the result is the same bind-failed line and exit 1; where the OS allows the bind, the monitor starts on 127.0.0.1:80

### Scenario: free-port failure text unchanged
- Given the OS cannot give any loopback port
- When the user runs `monitor` without `--port`
- Then stderr is `dtech-aidlc: Cannot bind a loopback port\n` as today

## AC-005 Host check uses the chosen port
- Statement: The system shall accept requests whose Host is exactly `127.0.0.1:<chosen port>` and answer 421 `host_rejected` for any other Host, including `localhost:<chosen port>` and a different port.
- Rationale: DNS-rebinding defence from intent 2 must not weaken or break with a fixed port.
- Source: Intent 2 knowledge record (Host exactly 127.0.0.1:<port>).
- Story: US-001

### Scenario: matching host
- Given the monitor runs with `--port 8787`
- When a client requests `/` with `Host: 127.0.0.1:8787`
- Then the response is 200

### Scenario: other host
- Given the monitor runs with `--port 8787`
- When a client sends `Host: 127.0.0.1:8788` or `Host: localhost:8787` or `Host: 127.0.0.1`
- Then the response is 421 with code `host_rejected`

## AC-006 Token stays per run
- Statement: The system shall make a new random token on every run, with or without `--port`, print it once on stderr in the URL fragment, and reject API requests that lack the current token.
- Rationale: Same security boundary as intent 2; a fixed port must not become a fixed credential.
- Source: Intent 2 knowledge record; OQ-3.
- Story: US-001

### Scenario: two runs, same port
- Given the monitor ran on `--port 8787` and was stopped
- When it is started again on `--port 8787`
- Then the second startup line shows a different token and the first token is rejected with 401 `unauthorized` (`rust-cli/monitor/http.rs:364`)
- And the test avoids a Windows restart flake (no SO_REUSEADDR in Rust std there) by either using a different free port for the restart check of the token only, or retrying the bind for a few seconds; the code itself does not retry (AC-004)

### Scenario: bookmark without token
- Given a bookmark of `http://127.0.0.1:8787/` (no fragment)
- When the user opens it while the monitor runs
- Then the page shows `Token missing`; the user must reopen the full URL from stderr (documented in README)

## AC-007 Help, completion, flag table and goldens agree
- Statement: The system shall list `--port` for `monitor` only, in the help text, the zsh completion and the command flag table, and the only golden changes shall be the monitor/`--port` text inside the goldens named below.
- Rationale: Existing help/completion/flag-table agreement test; byte-compat for the rest.
- Source: Owner decision on scoped re-bless (intent.md).
- Story: US-002

### Scenario: help and completion
- Given the new build
- When the user runs `dtech-aidlc help` and `dtech-aidlc completion zsh`
- Then the `monitor` line shows `[--port N]` with a description, the completion offers `--port` for `monitor`, and `tests/repo_content.rs` agreement checks pass

### Scenario: validation text
- Given the new build
- When the user runs `monitor --no-such-option`
- Then stderr is exactly `dtech-aidlc: Unknown option --no-such-option for 'monitor'. Allowed options: --port, --project.\n` (the command's flags first, `--project` appended last, per `main.rs:66-74`; today `... Allowed options: --project.`), and the same unknown option on every other command lists exactly the same options as before

### Scenario: scoped re-bless
- Given goldens re-blessed with `DTECH_AIDLC_BLESS=1`
- When the diff is read with `git diff --word-diff=porcelain tests/golden` (help and completion goldens are single JSON string values, so a plain line diff shows the whole blob), or after decoding the JSON strings and diffing the decoded text
- Then the only changed files are `tests/golden/routing_golden/validation.json` (the monitor unknown-option text at about line 841, and the help and completion entries) `tests/golden/utility_golden/help-and-completion.json` and `tests/golden/routing_golden/unsafe.json` (its `completion zsh` entry embeds the monitor completion text); checked with `git diff --word-diff=porcelain tests/golden`; every changed word concerns `monitor` or `--port`, and no other golden entry changed

## AC-008 Documentation and changelog
- Statement: The system shall document `--port` in the README "## Monitor" section and in exactly one new CHANGELOG 1.3.0 line, with no version bump.
- Rationale: Folding into the unreleased 1.3.0 is an owner decision.
- Source: Owner decision (intent.md).
- Story: US-001

### Scenario: README
- Given the README
- When a reader opens "## Monitor"
- Then it shows `dtech-aidlc monitor --port 8787`, states the range 1-65535, 127.0.0.1 only, refusal without fallback when the port is in use or not permitted (low ports may need privileges on macOS/Linux; on Windows, ports in an excluded or reserved range such as Hyper-V/WinNAT, see `netsh int ipv4 show excludedportrange`, fail even when high; on macOS/BSD a listener on 0.0.0.0 with the same port may not block the bind while Linux refuses it), that the token is still new each run so a bookmark needs the full URL from stderr (otherwise `Token missing`), and that without `--port` a free port is used

### Scenario: changelog and version
- Given CHANGELOG.md and Cargo.toml
- When the diff is read
- Then CHANGELOG 1.3.0 gains one line about `--port`, and the version stays 1.3.0 everywhere

## AC-009 Platforms
- Statement: The system shall pass the CI-tested scenarios of AC-001 to AC-008 on macOS, Linux and Windows; the listed platform limits are documented, not equalized.
- Rationale: CI runs all three (intent 2).
- Source: Intent 2 knowledge record.
- Story: US-001

### Scenario: CI
- Given the branch is pushed
- When CI runs on macos-latest, ubuntu-latest and windows-latest
- Then the tests for AC-001 to AC-008 pass on all three; tests choose a free high port by binding 127.0.0.1:0 and releasing it (small race, acceptable on the success path), and do not use ports below 1024; the port-in-use test keeps its own listener open for the whole run instead of releasing it
- And known limits are stated, not tested: low-port privileges differ per OS; Windows excluded port ranges (WSAEACCES); Unix wildcard-versus-loopback sharing differs between macOS/BSD and Linux; on Windows another local process may share the address while the monitor holds a fixed port (OQ-2)

## AC-010 No new crates, other commands byte-compatible
- Statement: The system shall add no dependency and shall leave the stdout, stderr and exit code of every command other than `monitor` unchanged, except for the merged-flag-table effects that every command-specific option already has (as `--depth` has on `status`): `parse_args` (`args.rs:384-395`) merges all flag tables, so `--port` becomes a known String option everywhere. Owner decision needed (OQ-1): accept these effects, or require a design that keeps `--port` out of the global table.
- Rationale: Intent 2 Q3; US-002.
- Source: Intent 2 knowledge record; intent.md.
- Story: US-002

### Scenario: dependencies
- Given the change
- When `Cargo.toml` and `Cargo.lock` are diffed
- Then they are unchanged

### Scenario: other commands
- Given the full test sweep
- When it runs
- Then 657 tests pass or more (new tests added), 0 fail, and no golden outside the monitor/`--port` text changed
- And the known changed cases are listed and tested, since no golden covers them: `status --port` and `status --port=` and `status --port --intent x` now give `Option --port requires a non-empty value` (was `Unknown option --port for 'status'. Allowed options: --intent, --project.`); `status --port 1 --wat` now names `--wat`; `status --port 1 --port 2` now gives `was given more than once`
