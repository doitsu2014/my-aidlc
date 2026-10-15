# Contributing to my-aidlc

Thanks for helping improve my-aidlc. This guide covers the development loop and
the expectations for a change.

## Setup

Node.js 20 or newer. No dependencies to install.

```bash
git clone https://github.com/doitsu2014/my-aidlc.git
cd my-aidlc
node tests/run-tests.mjs
```

## The development loop

```bash
node scripts/fetch-native.mjs --check  # the native snapshot matches the pin
node scripts/package.mjs               # regenerate dist/
node scripts/package.mjs --check       # confirm dist/ matches core/ + harness/
node scripts/lint.mjs                  # methodology integrity
node tests/run-tests.mjs               # tests
```

All five must pass. `--check` fails if `dist/` is stale. `dist/` is generated and
gitignored, so build it locally. `core/native/` is generated but **is**
committed: it is what lets a clone build, lint, and test with no network.

## What to change, where

| Change | Where |
| --- | --- |
| A workflow profile the team owns | `core/templates/scopes/` (seeded into each space) |
| The pinned AI-DLC version | `aidlc-version.json` |
| Engine behaviour | `core/tools/` |
| How native methodology is normalised | `core/data/native-phases.mjs`, `scripts/fetch-native.mjs` |
| A protocol | `core/protocols/` |
| A harness projection | `harness/<name>/` |
| Packaging or install | `scripts/` |
| Docs | `docs/` |

Never edit `dist/` by hand, and never edit `core/native/` by hand.

## Phases, stages, and agents are upstream

The five phases, 33 stages, and 14 agents are native AWS AI-DLC. They are not
authored here: `scripts/fetch-native.mjs` downloads the release pinned in
`aidlc-version.json`, verifies it, and normalises it into `core/native/`. To
change which stages run, edit a space scope's `include` list. To move to a newer
upstream release, update the pin and re-run the fetch; never patch a stage file.

## Pull request checklist

- [ ] `node scripts/fetch-native.mjs --check` passes
- [ ] `node scripts/package.mjs` run
- [ ] `node scripts/package.mjs --check` passes
- [ ] `node scripts/lint.mjs` passes
- [ ] `node tests/run-tests.mjs` passes
- [ ] A new space scope names a complete, disjoint partition of the 33 stages
- [ ] New behavior is covered by a test
- [ ] Docs updated when commands or methodology change

## Commit style

- One logical change per commit.
- Explain why in the body, not just what.
- Reference the stage or agent slug you touched.

## Code style

- ESM `.mjs`, no dependencies.
- Two-space indentation, double quotes, semicolons.
- Small modules; `core/tools/lib/` holds one concern per file.
- No `console.log` in library modules; return data and let the CLI print.

## Reporting issues

Include the output of `my-aidlc doctor --json`, your Node version, and the
harness you configured.

## License

By contributing you agree your contributions are licensed under
[MIT No Attribution](LICENSE).
