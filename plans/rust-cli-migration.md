# Rust CLI migration

## Goal and release boundary

Replace the Node.js CLI in one production cutover with a native Rust executable, while retaining compatibility with existing commands, project state, artifacts, harness projections, and installations. The Node CLI remains authoritative and the production entry point until parity and installer upgrade tests pass. The current Rust binary supports version, help, completion and read-only scope inspection; it rejects stateful project commands without changing state. Do not ship it as the replacement.

## Compatibility requirements

- Preserve command names, arguments, validation, JSON shapes, stdout/stderr and exit statuses, including prerequisite-blocked exit status 3.
- Read and write existing `.aidlc` state, audit logs, work-unit state, gates, prerequisites and identifier registries; preserve legacy state migration and recovery semantics. Never silently reset an existing project.
- Preserve graph, methodology, harness and projection behavior, including protection of user-modified files and safe paths/symlinks.
- Preserve dependency and knowledge manifests, Git safety checks, onboarding, and shell completion.
- Provide runtime access to graph, methodology and harness assets; a standalone executable alone is not sufficient unless those assets are embedded or installed alongside it.
- Support upgrades from the current Windows and POSIX installers without deleting or overwriting unrelated files. Document build, install, uninstall and distribution requirements.

## Implementation sequence

1. Establish binary-level parity tests using isolated project fixtures and the existing Node CLI as oracle. Compare structured JSON semantically, normalize only nondeterministic fields, and check side effects, failure behavior and exit codes. Preserve existing Node tests until cutover.
2. Port argument parsing, command dispatch, graph loading, safe filesystem operations and project/config state. Exercise legacy state migration and error cases before enabling mutation.
3. Port workflow directives, work-unit sequencing, reporting, gates, prerequisites, IDs and intent lifecycle; compare entire intent runs and resume/rework behavior against the oracle.
4. Port onboarding, projections, knowledge, dependency/repository operations, completion and diagnostics, including path and security behavior across supported platforms.
5. Implement asset packaging and Windows/POSIX install, upgrade and uninstall. Test a real upgrade from a Node installation and verify that existing project data and user-modified projection files survive.
6. Run the full parity matrix and Rust tests on Windows and POSIX. Only then switch the shipped entry point and documentation to Rust, and remove Node runtime dependencies in the same cutover.

## Current implementation status

The Rust binary implements version, help, zsh completion and read-only `scope`; other project commands fail without changing state. Scope has Node comparison coverage for scope/depth combinations and selected configurations, but its graph/config validation and asset discovery still need full parity. Draft library modules compile and have unit tests, but their stateful operations are not connected to commands and are not parity-validated. An independent review identified workflow blockers including repeat/unit tracking, gate validation and intent-path checks: the draft has since been revised, and malformed v2 pointers are now rejected without rewriting them. Standalone AI-gate, artifact-validation and legacy-state migration modules are now present but remain disconnected and have not achieved full parity. Gate whitespace bypasses have a regression test; Windows legacy migration deliberately fails before writes until atomic replacement is implemented. Audit logging, complete filesystem safeguards and workflow integration remain incomplete. Onboarding/projection still lacks full existing-stamp/history semantics and source registration; its hash now uses normalized SHA-256, and a repeated-refresh test protects user-modified obsolete files. Dependency pulls have a preliminary implementation, but Git timeouts, lock validation and error handling need parity; knowledge and complete command routing are not finished. Prototype installers are experimental and are not the shipped installers. Never use draft stateful modules to operate on real project state or switch installation until the requirements below pass.

## Verification and release criteria

- Existing representative projects continue from their current state without manual edits, including legacy state fixtures and blocked/resumed workflows.
- Every supported command has success, validation and failure parity coverage; workflow and gate decisions match; errors never fabricate completion.
- Rust tests, formatting and lint pass; cross-platform installer and safety tests pass. Record any intentional behavior differences for owner approval before release.
- The production installer invokes Rust and works without Node after cutover. No production command silently falls back to Node.
