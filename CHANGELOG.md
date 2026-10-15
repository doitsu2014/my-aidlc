# Changelog

All notable changes to my-aidlc are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **The methodology is now native AWS AI-DLC.** The five hand-authored phases
  (Analyze, Ideate, Develop, Launch, Curate), the 21 stages, and the 15 agents
  are replaced by the upstream methodology from
  [`awslabs/aidlc-workflows`](https://github.com/awslabs/aidlc-workflows):
  **5 phases** (Initialization, Ideation, Inception, Construction, Operation),
  **33 stages**, and **14 agents** (`aidlc-*` slugs).
- The methodology is fetched, not authored. `aidlc-version.json` pins one
  release (version, tag, commit, and archive SHA-256);
  `node scripts/fetch-native.mjs` — also `my-aidlc update` — downloads it,
  verifies the checksum, and normalises it into `core/native/`. Stage bodies are
  copied byte-for-byte and keep the upstream `{{INVOKE}}` and `{{HARNESS_DIR}}`
  placeholders. `--check` verifies the snapshot against the pin, and `doctor`
  reports and checks it. `core/native/ADAPTATION.md` records how native engine
  calls map onto my-aidlc's binary and which native helpers are reference-only.
- **Workflow profiles now live in the space layer.**
  `aidlc/spaces/<space>/scopes/<name>.md` is where a team's workflow is
  authored; `my-aidlc config`/`init` seed them from
  `core/templates/scopes/` and never overwrite an existing file. A space scope
  states its complete stage list in `include` (plus the remainder in `skip`),
  and that list is authoritative — it overrides a stage's own native `scopes:`
  declaration, so a team re-maps the grid without touching a stage file.
- The seven original profiles were re-expressed over the native 33-stage
  inventory, preserving the intent they were written with: `classic` 33 stages
  (full lifecycle), `feature` 32, `infra` 27, `mvp` 16, `bugfix` 10,
  `express` 10, `poc` 7. The 11 native baseline scopes stay available
  underneath; `my-aidlc scope` marks each profile `space` or `native`.
- Harness manifests project `native/` and `templates/scopes/` instead of the
  removed `phases/`, `agents/`, and `scopes/` trees. Run-stage directives now
  resolve `stage_file` and agent files under `native/`.
- `scripts/lint.mjs` additionally verifies the native snapshot against the pin,
  requires each space-scope seed to partition the native stage inventory
  exactly, and allows the reserved `orchestrator` lead the Initialization
  stages declare.

### Added

- `my-aidlc update [--check]` — fetch the pinned AI-DLC release and rewrite the
  methodology, or verify the committed snapshot without network access.
- `my-aidlc scope` now lists every profile with its stage count and origin
  (`space` or `native`); `my-aidlc scope <name>` shows the resolved workflow,
  what it skips, and the file that defines it.
- `my-aidlc graph` records a `native` block naming the pinned release.
- Active-space resolution: `aidlc/active-space` selects the space whose scopes
  the engine layers over the baseline (defaults to `default`).
- Shell completion for `bash`, `zsh`, `fish`, and `powershell`, generated from
  the live methodology. New `my-aidlc completion [<shell> | install | uninstall]`
  command prints scripts or wires them into your shell rc file (idempotently).
- The installers now install completion for the detected login shell; opt out
  with `--no-completion` (`-NoCompletion` on Windows).
- Per-stage and per-phase review checkpoints. A stage with `review: required`
  always presents its gate; a phase with `review: required`, or the project
  toggle `my-aidlc config --review-required true`, pauses the workflow at the
  phase boundary. Approving continues; requesting changes re-opens the phase
  and re-gates. Recorded as `PHASE_AWAITING_APPROVAL`, `PHASE_APPROVED`, and
  `PHASE_REJECTED`.
- DeepSeek Harness as a fourth harness: `my-aidlc config --harness deepseek`
  projects the core into `.dsh/`, puts the orchestrator skill in the
  `project-dsh` root at `.dsh/skills/aidlc/SKILL.md`, and merges the onboarding
  block into `AGENTS.md`. DSH exposes a user-invocable skill as `/<name>`, so
  the workflow is invoked as `/aidlc`. DSH keeps its plugin config home-scoped
  (`$DSH_HOME/profiles/<name>/cordis.patch.yml`), so the harness ships no
  project-level native config file.

### Removed

- `core/phases/`, `core/agents/`, and `core/scopes/`. The first two are
  superseded by `core/native/{phases,stages,agents}`; the scope profiles moved
  to `core/templates/scopes/` and the workspace space layer.

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
  [AIDLC cheatsheet](https://aidlc.io/cheatsheet/). Replaced in Unreleased by
  the native AWS AI-DLC methodology.
- Four stack-neutral database integration skills: `db-postgres`, `db-mysql`,
  `db-mssql`, and `db-mongodb`. Each covers configuration, drivers/ORMs,
  schema design, migrations, indexing and query plans, transactions, pooling,
  security, testing, and how it plugs into the stages.
- Fifteen agents: composer, product, research, business analyst, architect,
  design, security, delivery, developer, code reviewer, QA, DevOps, SRE,
  performance, and technical writer. Replaced in Unreleased by the 14 native
  AI-DLC agents.
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
