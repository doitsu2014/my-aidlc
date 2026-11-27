# Known limitations: WU-010

- The `#[cfg(windows)]` branch (`C:/escape` must be an error) was not executed here; it runs only on the Windows CI leg (WU-008, after an owner-authorized push).
- The two tests were fixed in place under their existing names, not renamed with a `ts019_` prefix, because WU-010 specifies fixing the existing tests. Their TS-019 ownership is recorded in `work-units.json`.
- Rollback: revert this unit's commit; it touches only test code.
