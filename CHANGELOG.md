# Changelog

All notable changes to my-aidlc are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Shell completion for `bash`, `zsh`, `fish`, and `powershell`, generated from
  the live methodology. New `my-aidlc completion [<shell> | install | uninstall]`
  command prints scripts or wires them into your shell rc file (idempotently).
- The installers now install completion for the detected login shell; opt out
  with `--no-completion` (`-NoCompletion` on Windows).

## [0.1.0] - Initial release

### Added

- Execution modes: `normal` (human questions and approval gates) and `yolo`
  (skips questions by choosing the recommended answer and auto-approves gates,
  recording `STAGE_AUTO_APPROVED` in the audit log). Configurable with
  `my-aidlc config --mode normal|yolo` and overridable per scope.
- A configurable question budget per stage (project, scope, or stage), with
  `max: 0` disabling the question flow.
- The five AIDLC phases — Analyze, Ideate, Develop, Launch, Curate — and 21
  stages with approval gates, derived from the
  [AIDLC cheatsheet](https://aidlc.io/cheatsheet/).
- Four stack-neutral database integration skills: `db-postgres`, `db-mysql`,
  `db-mssql`, and `db-mongodb`. Each covers configuration, drivers/ORMs,
  schema design, migrations, indexing and query plans, transactions, pooling,
  security, testing, and how it plugs into the stages.
- Fifteen agents: composer, product, research, business analyst, architect,
  design, security, delivery, developer, code reviewer, QA, DevOps, SRE,
  performance, and technical writer.
- Seven workflow profiles: `classic`, `express`, `feature`, `bugfix`, `mvp`,
  `poc`, and `infra`, with keyword-based auto-detection.
- A deterministic Node.js engine (`core/tools/`) providing `config`, `init`,
  `doctor`, `status`, `list`, and `orchestrate next|report|park`.
- Persistent state, an append-only audit log, and layered org/team/project
  memory.
- Three harness harnesses: PI Agent (`.pi/`), Codex CLI (`.codex/` +
  `.agents/skills/`), and Claude Code (`.claude/`).
- Packaging (`scripts/package.mjs`), installers (`scripts/install.sh`,
  `scripts/install.ps1`), structural lint (`scripts/lint.mjs`), and a test
  suite (`tests/run-tests.mjs`).
- Documentation: getting started, workflow, agent, harness, and CLI references.
