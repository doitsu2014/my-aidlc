# Known limitations: monitor --port (intent 3)

- D5 (AI conductor decision): `--port` joins the merged flag table, so invalid invocations such as `status --port`, `status --port=`, `status --intent --port` and `status --port 1 --port 2` now print a different stderr line (exit code unchanged, pinned by `status_port_effects_are_the_disclosed_ones`).
- No test proves the listener is loopback-only; the 127.0.0.1 bind is guarded by review (pre-existing).
- Windows: same-user address sharing on a fixed port is a residual next to T2-06; SO_EXCLUSIVEADDRUSE not set. Windows excluded port ranges and low-port privileges are documented, not tested.
- A well-known fixed port can be pre-bound by another local user; the monitor then refuses to start (no token printed).
- A fixed port does not make a bookmark self-sufficient: the token is still new on every run.
- The free-port bind-failure text is not tested.
