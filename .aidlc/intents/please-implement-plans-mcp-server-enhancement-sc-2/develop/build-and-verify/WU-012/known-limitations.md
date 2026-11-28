# Known limitations: WU-012

- Windows untested locally (windows-latest CI).
- `with_state_lock` is a no-op when `.aidlc` does not exist (existing intent-1 behaviour); `deps add` already requires an initialized project.
- Red observed once locally; the design spike's 6-of-6 is the repetition.
