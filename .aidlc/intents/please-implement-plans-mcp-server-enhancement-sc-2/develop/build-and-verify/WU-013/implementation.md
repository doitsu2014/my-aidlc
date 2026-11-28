# Implementation: WU-013 (Telemetry core plus managed ignore line)

- Status: Complete (build-and-verify, 2026-11-28). Cites FN-023..FN-028, IF-012..IF-014, CFG-008, CFG-009.
- New `rust-cli/telemetry/mod.rs`: event schema (IF-012 field set, key order), `to_line` / `parse_line` (strict reader; null `durationMs` accepted on completed kinds per IF-012), `os_random` (Unix `/dev/urandom`; Windows `BCryptGenRandom` via `#[link(name = "bcrypt")]`, ADR-020), hex, session-prefixed ids, store file-name rules.
- New `rust-cli/telemetry/sink.rs`: `Sink` (bounded queue, background writer thread, never blocks the producer, panics contained and disable telemetry with one stderr line; `UnsafeLocation` marker separates `unsafe telemetry location` from `write failed`), `FileStore` (never creates `.aidlc`, refuses symlinked store, `.gitignore` in the store, 5 MiB parts), prune (7 days, then 50 MiB, never the current file or unrelated files).
- `rust-cli/lib.rs`: `pub mod telemetry;`. `rust-cli/knowledge.rs`: `IGNORE_BLOCK` gains `/telemetry/`.
- Goldens: `tests/golden/routing_golden/lifecycle.json` and `lifecycle-embedded.json`, one line each (the `.aidlc/.gitignore` value gains `/telemetry/`), re-blessed with `DTECH_AIDLC_BLESS=1 cargo test --locked --test routing_golden` in the baseline environment (scoped re-bless reason: managed .gitignore line, owner Q2/Q4).
- New `tests/telemetry_guards.rs`: the three Q1 guards.
- Not wired into MCP yet (WU-014).
