# Changelog

All notable changes to my-aidlc are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Initial release

### Added

- The five AIDLC phases — Analyze, Ideate, Develop, Launch, Curate — and 21
  stages with approval gates, derived from the
  [AIDLC cheatsheet](https://aidlc.io/cheatsheet/).
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
