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
node scripts/package.mjs          # regenerate dist/
node scripts/package.mjs --check  # confirm dist/ matches core/ + harness/
node scripts/lint.mjs             # methodology integrity
node tests/run-tests.mjs          # tests
```

All four must pass. `--check` fails if `dist/` is stale, so commit regenerated
`dist/` (or leave it out of the repository if you choose to build on install —
this repository commits it for convenience).

## What to change, where

| Change | Where |
| --- | --- |
| A phase, stage, or scope | `core/phases/`, `core/scopes/` |
| An agent persona | `core/agents/` |
| A protocol | `core/protocols/` |
| Engine behaviour | `core/tools/` |
| A harness projection | `harness/<name>/` |
| Packaging or install | `scripts/` |
| Docs | `docs/` |

Never edit `dist/` by hand.

## Pull request checklist

- [ ] `node scripts/package.mjs` run and `dist/` updated
- [ ] `node scripts/package.mjs --check` passes
- [ ] `node scripts/lint.mjs` passes
- [ ] `node tests/run-tests.mjs` passes
- [ ] New stages have a `condition` when conditional and reference known agents
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
