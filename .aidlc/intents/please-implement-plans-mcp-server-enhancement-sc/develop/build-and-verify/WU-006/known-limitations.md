# Known limitations: WU-006

- In values (not messages), an unregistered absolute path that contains a space is redacted only up to the space (`/Volumes/My Disk/secret` -> `<redacted-path> Disk/secret`). Messages fall back to the generic text, so nothing leaks there. Registered prefixes with spaces are matched whole.
- Paths after uncommon punctuation (`|`, `!`, `*`, `>`) or a non-ASCII letter are not token starts; URL schemes other than `file:` are treated as URLs.
- A registered prefix followed by sentence punctuation loses the punctuation context (`/srv/proj.` -> `.`); no leak.
- Not wired yet: WU-005 builds the Redactor at startup and applies it to every tool result and error.
- Observed flake (not this unit): `prerequisites::tests::check_timeout_is_reported` can fail under load; a follow-up should give it margin (for example `sleep 3` with a 1 s timeout) before CI relies on it.
- Rollback: revert this unit's commit.
