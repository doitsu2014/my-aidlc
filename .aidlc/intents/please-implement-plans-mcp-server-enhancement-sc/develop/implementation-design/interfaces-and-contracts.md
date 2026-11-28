# Interfaces and contracts

Signatures are Rust-like prose. Registered identifiers: IF-001 to IF-011, FN-001 to FN-022, CFG-001 to CFG-006. ADR and TH labels are Ideate labels.

## IF-001 application facade (rust-cli/application.rs, new)
Traces: FN-001, FN-002, FN-003, FN-004, FN-005, FN-006. Serves AC-001, AC-002, AC-006 to AC-010 (ADR-006).

```
pub enum Caller { Cli, Mcp }                       // Copy, Eq
pub struct Project { root: PathBuf, graph: scope::Graph, engine: orchestrate::Engine, caller: Caller }
pub const APPROVER_REQUIRED: &str = "approved_by is required for a gate decision over MCP (no git or OS fallback)";

pub fn open(project: &Path, caller: Caller) -> Result<Project, String>;
pub fn external_engine() -> Option<PathBuf>;
pub fn status(p: &Project, intent: Option<&str>) -> Result<Value, String>;
pub fn queue(p: &Project) -> Result<Value, String>;
pub fn knowledge_preflight(p: &Project) -> Result<(), String>;
pub fn next(p: &Project, o: &NextOptions) -> Result<NextOutcome, String>;
pub fn report(p: &Project, o: &ReportOptions) -> Result<Value, String>;
```
Rules: no stdin, no stdout, no exit codes, no knowledge of a protocol era. `open` order is fixed (resolve root, assert safe state dir, engine, graph). `next` for `Mcp` runs the preflight, resets intent, scope, depth, mode, approved_by, detail to `None`, `force_resume=false`, `skip_prerequisite_checks=true`. `report` for `Mcp` returns `APPROVER_REQUIRED` first when an approval is present and the label is missing or blank after trim (before any state read and before identity lookup), and adds `pendingPrerequisites`. The final text of `APPROVER_REQUIRED` is fixed here and frozen by tests.

New field: `NextOptions.skip_prerequisite_checks: bool` (default false). Existing struct literals get `..NextOptions::default()`.

## IF-002 exact-match error constants
Traces: FN-006, FN-008.
`pub const STATE_CHANGED: &str = "Workflow state changed while this command was running; no changes were made. Run the command again."` in `orchestrate.rs`; used by `ensure_unchanged` and `intent_commands.rs`. The MCP adapter classifies by equality with `STATE_CHANGED` and `application::APPROVER_REQUIRED`, never by substring. Bytes equal the current CLI text, so CLI output is unchanged.

## IF-003 prerequisites::check_prerequisites
Traces: FN-007.
`pub fn check_prerequisites(root: &str, config: &Value, names: &[String], skip: bool) -> Vec<Value>`
- `skip=false`: identical to `run_prerequisites`.
- `skip=true`: spawns nothing; one row per distinct name. Declared: `{name, ok:false, hint, exitCode:null, executed:false}`. Undeclared: `{name, ok:true, hint:null, exitCode:null, undeclared:true, executed:false}`. The `check` command line is never included.
- Skip-only wording in the Blocked payload: "prerequisite checks are not run over MCP (not executed). Ask the owner to run `dtech-aidlc orchestrate next` in a terminal". CLI wording is byte-identical to today.

## IF-004 orchestrate::pending_prerequisites
Traces: FN-020.
`pub fn pending_prerequisites(root: &Path, graph: &Value, config: &Value) -> OrchestrateResult<Vec<Value>>`: read-only; returns skip-form rows (IF-003); empty when complete or nothing needed; a preview that can differ from the next real selection (claim, pinned, parallel).

## IF-005 protocol boundary (rust-cli/mcp/protocol.rs)
Traces: FN-011, FN-012.
```
pub struct ToolHost {
  pub list: fn() -> Value,
  pub call: fn(&ToolContext, &str /*tool*/, &Value /*arguments*/) -> Option<Result<Value, Value>>,
}
pub fn serve<R: BufRead, W: Write>(input: R, output: W, host: &ToolHost) -> i32;   // 0 on EOF
```
`None` from `call` means unknown tool (`-32602`). `Ok`/`Err` carry structuredContent objects. The era is never visible to the host. The internal dispatch boundary is `(method, params) -> Result<Value, Value>`, so an SDK swap stays contained (ADR-001). Generic reader and writer make the protocol testable in process on every OS.

## IF-006 MCP wire contract
Traces: FN-011, FN-013, FN-014. Serves AC-003, AC-005, AC-006 to AC-011 (ADR-004, ADR-005, ADR-008, ADR-009).

### Tools and annotations
| Tool | readOnlyHint | destructiveHint | openWorldHint |
| --- | --- | --- | --- |
| `status` | true | (absent) | false |
| `queue` | true | (absent) | false |
| `next` | false | false | false |
| `report` | false | true | false |

No `outputSchema`. The tool list has exactly these four tools; no doctor, config, onboarding or intent-creation tool exists (TS-018).

### Input schemas
All are `{"type":"object","additionalProperties":false}`. Size limits are CFG-006. An unknown or excluded field (root, intent text, scope, depth, mode, force_resume, artifact or evidence fields, `approved_by` on `next`) gives `invalid_arguments`.

`status`:
```json
{"type":"object","additionalProperties":false,
 "properties":{"intent_id":{"type":"string","pattern":"^[a-z0-9-]{1,64}$","maxLength":64}}}
```
`queue`: `{"type":"object","additionalProperties":false,"properties":{}}`

`next`:
```json
{"type":"object","additionalProperties":false,
 "properties":{"claim":{"type":"boolean"},
               "unit":{"type":"string","maxLength":64,"pattern":"^[A-Za-z0-9-]{1,64}$"}}}
```
`claim` together with `unit` is refused with the CLI's wording.

`report`:
```json
{"type":"object","additionalProperties":false,
 "properties":{
  "stage":{"type":"string","maxLength":64,"pattern":"^[a-z0-9-]{1,64}$"},
  "unit":{"type":"string","maxLength":64,"pattern":"^[A-Za-z0-9-]{1,64}$"},
  "result":{"type":"string","enum":["complete","blocked","skipped"]},
  "approval":{"type":"string","enum":["approved","rejected"]},
  "approved_by":{"type":"string","minLength":1,"maxLength":200},
  "approval_note":{"type":"string","maxLength":4096},
  "detail":{"type":"string","maxLength":16384},
  "needs":{"type":"array","maxItems":16,"items":{"type":"string","pattern":"^[a-z0-9-]{1,32}$"}}}}
```
Enum and pattern values are Estimates: WU-005 reads the `result` set from the CLI parser and the stage and unit patterns from the phase graph before freezing the schema. `approved_by` is validated at the length level only (CLI parity).

### Result shape (both eras)
Every `tools/call` result: `{"content":[{"type":"text","text":"<compact JSON of structuredContent>"}],"structuredContent":<object>,"isError":<bool>}`.
- `status`, `queue`: structuredContent is the facade value.
- `next`: `{"outcome":"directive"|"complete"|"blocked"|"busy","result":<payload>}`; all with `isError:false`. `blocked` covers both a prerequisite hold and a stalled queue; the CLI exit code 3 has no MCP counterpart (ADR-007).
- `report`: the CLI result plus `pendingPrerequisites` (always an array of IF-003 rows).
- Failure: `{"error":{"code":<code>,"message":<redacted, <=1000 chars>,"retryable":<bool>}}` with `isError:true`.

### Per-era differences
- Legacy (initialize handshake, `2025-11-25` or `2025-06-18`): `initialize` result is exactly `{protocolVersion, capabilities:{tools:{}}, serverInfo:{name:"dtech-aidlc-mcp", version:<crate version>}}`; results carry nothing extra.
- Modern (`2026-07-28`, `_meta` protocol-version key per request): served methods `server/discover`, `tools/list`, `tools/call`; results add `resultType:"complete"` and `_meta` serverInfo; discover and list add `ttlMs:0` and `cacheScope:"private"`; errors are never stamped. Exact field names are checked once against the saved 2026-07-28 and 2025-11-25 schema files in WU-004.

### Protocol error codes
| Code | When |
| --- | --- |
| -32700 | invalid JSON or invalid UTF-8 line (id null); session continues |
| -32600 | not an object, batch array, null id, wrong or missing `jsonrpc`, oversized line (id null) |
| -32601 | unknown method; `ping` in the modern era |
| -32602 | malformed params or `_meta`; unknown tool; missing handshake before a legacy request; missing or non-string `protocolVersion` in `initialize` |
| -32022 | unsupported modern version; `data:{supported, requested}` |
| -32603 | handler panic; message "Internal error", no detail |

### Tool error codes (inside structuredContent)
| code | retryable | Source |
| --- | --- | --- |
| `invalid_arguments` | false | validator (FN-013) |
| `approver_required` | false | `APPROVER_REQUIRED` equality |
| `state_conflict` | true | `STATE_CHANGED` equality; the host decides whether to retry, the server never does |
| `operation_failed` | false | any other library error, including no active intent and, on Windows, a rename blocked by another process |

## IF-007 Redactor (rust-cli/mcp/redact.rs)
Traces: FN-015.
```
pub struct Redactor { /* built once: root lexical+canonical, DTECH_AIDLC_HOME raw+resolved, HOME or USERPROFILE */ }
impl Redactor {
  pub fn new(root: &Path) -> Redactor;
  pub fn redact_value(&self, v: &mut Value);                                   // every string value and every object key, any depth
  pub fn redact_message(&self, text: &str, fallback_code_text: &str) -> String; // before truncation
}
fn rewrite(text: &str, prefixes: &[(String, String)], case_insensitive: bool) -> String;   // pure, tested with Windows-form strings on any OS
```
Replacement order: project-relative path (forward slashes, `.` for the root), `$DTECH_AIDLC_HOME`, `~`, then `<redacted-path>` for any remaining absolute-looking string. Prefix matches respect path boundaries (`/a/b` does not match `/a/bc`). Windows forms handled: `C:\...`, `c:/...`, `\\?\C:\...`, UNC `\\host\share`. A helper strips the `\\?\` prefix from `canonicalize` output (no new crate).

## IF-008 tool adapter surface (rust-cli/mcp/tools.rs)
Traces: FN-013, FN-014.
```
pub struct ToolContext { pub root: PathBuf /*fixed at launch*/, pub redactor: Redactor }
pub fn tool_list() -> Value;
pub fn call_tool(ctx: &ToolContext, name: &str, args: &Value) -> Option<Result<Value, Value>>;
```
Limits are `pub const` values (CFG-006). Description text for `report` must contain "self-asserted" and "not authentication"; the `next` description must say prerequisite checks are not executed.

## IF-009 binary contracts
Traces: FN-009, FN-016, FN-017.
- `dtech-aidlc`: argv, stdout, exit codes (including 3 for a blocked `next`) and audit rows unchanged. No new CLI flag or command, so help.txt, completion.zsh, COMMAND_KEYS and the args.rs table are untouched.
- `dtech-aidlc-mcp [--project <dir>]` (CFG-005): stdout carries protocol frames only; diagnostics, startup warning and panic hook use stderr; exit 0 on stdin EOF; startup failure prints `dtech-aidlc-mcp: <msg>` to stderr, exits 1, writes nothing, reads no stdin. No network listener; no child process on any tool path; no `--version` or `--help`.
- CLI parity guarantees: for equivalent projects, `status`, `queue`, `next` and `report` return semantically equal values after path redaction; state.json and audit rows are equal after timestamp normalisation; deliberate differences are only (a) prerequisite checks are not executed over MCP, (b) a gate decision needs an explicit label, (c) paths are redacted, (d) no intent creation over MCP.

## IF-010 CI workflow contract (.github/workflows/ci.yml, new)
Traces: FN-018. Serves the post-gate amendment, AC-001, AC-003, AC-013.
- Triggers: `push`, `pull_request`.
- Job `build-test`: `strategy.fail-fast: false`; `os` in `windows-latest`, `macos-latest`; optional `ubuntu-latest` with `continue-on-error: true` (best-effort, not a release claim).
- Steps in order: checkout; install Rust 1.100 with `rustup`; `cargo build --locked`; `cargo test --locked --no-fail-fast`; a separate step named `MCP transcript tests` running `cargo test --locked --test mcp_stdio --test mcp_parity --test mcp_redaction`.
- macOS environment: `TMPDIR=/private/tmp/aidlc-test` (created first) and `target/debug` appended to `GITHUB_PATH`. Windows: `target\debug` appended to `GITHUB_PATH`; PowerShell for environment steps, bash for cargo lines.
- Never runs `scripts/rust-install/*`; no secrets; no deployment; no push or run happens without separate owner authorization.
- Known gap: macOS carries two pre-existing lib-test failures (onboarding path and symlink tests). The owner chooses between a fix intent, a named expected-failure list, or a documented skip; the MCP step is never skipped.

## IF-011 State write lock contract (rust-cli/orchestrate.rs)
Traces: FN-022. Serves AC-001, AC-011, AC-012 and the owner decision of 2026-11-28 (ADR-015 superseded).
- Signature: `pub(crate) fn with_state_lock<T>(root: &Path, f: impl FnOnce() -> OrchestrateResult<T>) -> OrchestrateResult<T>`, plus a test seam `with_state_lock_for(root, wait: Duration, f)`; the public wrapper passes `Duration::from_millis(STATE_LOCK_WAIT_MS)` (5000). Poll `STATE_LOCK_POLL_MS` (10). Constants, not configuration (no CFG key). `intent_commands.rs` imports it with `STATE_CHANGED`.
- Contract: at most one dtech-aidlc process (or thread) runs `f` for a given project at a time. `f` is the compare-then-write only: `ensure_unchanged` plus `save_intent` and `save_pointer`. It never contains an audit append and never calls `with_state_lock` (no re-entrancy).
- Errors: wait exceeded gives exactly `STATE_CHANGED` (maps to `state_conflict`, `retryable: true`, ADR-008); an open error of kind `PermissionDenied` is retried inside the same wait budget and ends with `STATE_CHANGED` on timeout; a lock file that is still marked after 3 reopens gives `operation_failed` with the text `.aidlc/state.lock is stale; delete .aidlc/state.lock when no dtech-aidlc command is running` (not retryable, a private string in orchestrate.rs, not an exported constant); other io errors give `<path>: <error>` (maps to `operation_failed`). No new public error constant.
- Lock target decision (evidence): a dedicated `.aidlc/state.lock`, created on demand, removed before release, never read by anything else.
  - Chosen over a persistent lock file: every golden helper walks `.aidlc` recursively (tests/rust_intent_commands.rs `snapshot`, rust_config_commands, rust_harness_commands, rust_knowledge_commands, rust_repos_deps_ids, rust_doctor, routing_golden), so a lingering file would appear in the goldens, and listing it in the managed `.gitignore` block (knowledge.rs `IGNORE_BLOCK`) would change `routing_golden/lifecycle*.json` and need an upgrade path. The file only exists during a write, as `deps.lock` already does, so snapshots taken after a command are unchanged.
  - Chosen over locking `.aidlc/audit.jsonl`: it does not exist before the first audit (first-write cases, and `preflight_audit` plus tests assert it is absent after failures), creating it to lock it would change goldens; on Windows `LockFileEx` is mandatory, so an exclusive lock would make `doctor`, the `application.rs` audit read and `audit()`'s own append (a second handle in the same process) fail with a lock violation; and the check and write do not need the audit file at all.
  - Chosen over locking the intent directory: `File::lock` does not work on directories on Windows, and the pointer-only operations (pause, resume, new-intent) have no intent directory.
  - Chosen over locking `intents/<id>/state.json`, `state.json`, `config.json` or `intent.md`: the state files are replaced by rename (the locked inode changes, so two writers can lock different inodes), config and intent files are read by other commands and a mandatory Windows lock would make those reads fail.
  - Rejected: a lock file in the OS temp dir (TMPDIR can differ between the CLI and the MCP host, so the two would not exclude each other); a pid-file with `create_new` like `deps.lock` (needs stale-pid heuristics; the owner asked for `File::lock`).
- Delete-on-release safety: the holder deletes the path and only if `remove_file` succeeds writes a retired marker (one byte) through its still-open handle, then releases; if the delete fails it writes no marker and leaves a plain reusable file. A waiter that acquires the lock reads the marker through its own handle, drops that handle and reopens, so it never works on an unlinked inode while another process creates and locks a new file. Reading through the locking handle is permitted on Windows.
- Windows: `try_lock` is `LockFileEx` with `LOCKFILE_FAIL_IMMEDIATELY`; deleting an open file works because std opens with delete sharing. If the delete fails the guard ignores the error, writes no marker and leaves a reusable file (never a permanent block). Unverified: on file systems where `remove_file` falls back to delete-pending semantics (FAT, some network or ReFS volumes) a waiter opening the file during the pending delete gets `ERROR_ACCESS_DENIED` (`PermissionDenied`); that is retried within the wait budget, ending in `STATE_CHANGED`. First CI run is the only Windows evidence.
- Blocking `lock()` was rejected: an MCP call must not hang if a writer is wedged; `try_lock` with a 5 s cap bounds it.

## Configuration keys
- CFG-001 `[[bin]] name="dtech-aidlc-mcp" path="rust-cli/mcp_main.rs"`; no features, no dependencies; Cargo.lock byte-identical (ADR-002).
- CFG-002 `rust-version = "1.100"` (ADR-003); version and CHANGELOG untouched (ADR-018).
- CFG-003 `.github/workflows/ci.yml` (IF-010).
- CFG-004 test and CI environment: same pass/fail set as the recorded baseline (0 lib failures after WU-010; the two known Unix test bugs are fixed there) with `TMPDIR` under `/private/tmp` (macOS only) and `target/debug` on PATH; no golden re-bless; new MCP tests create projects under a canonical symlink-free base with the Windows `\\?\` prefix stripped.
- CFG-005 `--project <dir>` flag and the `DTECH_AIDLC_HOME` fallback warning (IF-009).
- CFG-006 limits: line 1 MiB; `intent_id` 64; `needs` 16 items of 32 bytes; `detail` 16 KiB; `approval_note` 4 KiB; `approved_by` 1 to 200 after trim; error message 1,000 characters. MCP only; CLI behaviour is unchanged. Values are Estimates to confirm in WU-004 and WU-005.
