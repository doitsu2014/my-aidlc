# Known limitations: WU-017

- The event cap is checked after a whole file is added, so a partial result can exceed 250,000 events by up to one rotated file (about 10k events); memory stays bounded by the 64 MiB byte cap.
- Under a cap hit, a completion in an unread older file is not visible, so `incomplete` can over-count in a partial result (flagged `(partial)` on the page).
- Contract note for WU-019 (stage filter): WU-014 sends tool and operation events without ids, so `stage=<id>` matches only workflow events; toolCalls, latency and incomplete are 0 under a stage filter. This follows IF-017 literally; the page must label the stage filter as applying to workflow events.
- IF-017 additions not in the written contract: `Caps`, `query_with`, `Cache::len` / `is_empty` / `approx_bytes`, `Arc<FileData>` cache value.
