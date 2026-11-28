# Known limitations: WU-016

- The acceptance is an AI review (owner decision Q15), not an owner approval.
- Contrast ratios are computed, not measured in a browser; the manual browser pass is WU-019's (TS-030 checklist at 320, 360 with an error state, and 1280 px).
- The latency population (n equals toolCalls) is an Assumption that WU-017 must confirm or correct in IF-017.
- Client-side id checks follow the per-field rules in the pack; WU-017 must keep the server rules identical.
