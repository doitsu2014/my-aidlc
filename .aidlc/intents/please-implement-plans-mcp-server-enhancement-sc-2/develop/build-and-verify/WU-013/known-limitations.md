# Known limitations: WU-013

- Windows: the BCryptGenRandom link and Windows store behaviour are Assumptions until windows-latest CI (raw-dylib fallback per ADR-020); symlink-refusal tests are Unix-only.
- Rotation cap is soft by up to one open part (about 55 MiB observed against 50 MiB), as ADR-020 records.
- Queue headroom: lower bound only (200k events/s, no drop).
- The EACCES test self-skips when mode bits are not enforced (root).
- Contract for WU-014 (Observed in parse_line): eventId, traceId and spanId must carry the line's sessionId prefix; durationMs is integer or null only on tool.completed and operation.completed; outcome is null exactly on started kinds; errorCode must be in the closed tool or protocol code sets. Lines that break these rules are counted as skipped by the reader.
- Test seams beyond IF-014: `FileStore::new`, `FileStore::write`, `prune_with`, pub consts `PART_MAX`, `TOTAL_MAX`, `MAX_AGE`, `QUEUE`; `disabled` is `Arc<AtomicBool>`.
