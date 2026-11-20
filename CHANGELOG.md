# Changelog

Each release lists the behaviour changes a user or a script can notice.

## 0.3.0

- Existing documentation is registered by reference in `aidlc.knowledge.json`; BRD files are no longer copied into `.aidlc/shared/brd`, and the `brd` key is removed from `onboard` output and the project profile. Existing copies are left in place and may be deleted.
- `onboard` adds `--docs`, `--no-detect` and `--rescan`; it creates `aidlc.knowledge.json` only when it registers at least one document.
- `init` is an alias of `onboard`; it scans the project, writes `.aidlc/shared/`, and creates a root `aidlc.knowledge.json` when documentation is detected or supplied with `--docs`.
- The project profile is schema version 2, marked generated, and supports the `unknown` project kind. Folders such as `vendor`, `target`, `out`, `obj` and `venv` are excluded from source evidence, and `.kt` files are reported as Kotlin.
- `orchestrate next` directives add `context.profile.kind`, `context.sourceChanges` and `context.codebaseMap`, with warnings for missing or changed registered sources. `doctor` adds a `Knowledge sources:` check.
- The methodology adds the one-time codebase map at `.aidlc/knowledge/_codebase-map.md` and brownfield guidance for Analyze, Ideate and Develop.
- `build-and-verify` requires a fourth output, `regression-report`; intents that have not completed that stage must write it.
- A new project must choose `--harness` on its first `init` or `onboard`; existing projects can add a harness with `harness add`, and `config scope` shows or changes the default scope for new intents.
- Decisions are written once in the decision record; the architecture design only lists them.
- State migrates by itself; run `dtech-aidlc harness refresh` for the skill files and `dtech-aidlc onboard` for PROJECT.md, the manifest and the profile.

## 0.1.3

- Unknown options, options without a value, and unexpected extra arguments now make a command fail before anything is written; the error lists the allowed options.
- `dtech-aidlc --version` prints only the version line.
