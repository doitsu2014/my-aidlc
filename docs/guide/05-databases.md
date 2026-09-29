# Database skills

my-aidlc ships four stack-neutral reference skills for database integration.
They are native harness skills: advertised by name and description, loaded on
demand when a stage touches a database.

| Skill | Engine | Aliases |
| --- | --- | --- |
| `db-postgres` | PostgreSQL | postgres, pg, psql |
| `db-mysql` | MySQL / MariaDB | mariadb, innodb |
| `db-mssql` | SQL Server / Azure SQL | sqlserver, t-sql, azure-sql |
| `db-mongodb` | MongoDB | mongo, document-db, atlas |

## What each skill covers

Every skill is organised the same way so it is predictable to use:

- **When this skill applies** — which AIDLC stages should load it
- **Configuration and secrets** — connection strings, TLS, least privilege
- **Drivers and ORMs** — a per-ecosystem table so you can choose and record one
- **Schema and data modelling** — engine-specific rules and conventions
- **Migrations** — the common tools plus the safety rules (forward-only,
  expand → migrate → contract)
- **Query patterns and performance** — indexes, plans, N+1, pagination
- **Transactions and concurrency** — isolation defaults that differ per engine
- **Connection pooling** — sizing and pooler caveats
- **Security** — parameterisation, encryption, auditing
- **Testing** — Testcontainers and ephemeral databases
- **Common pitfalls** — the mistakes that hurt in production
- **How this skill plugs into my-aidlc** — which stage records what

The skills are deliberately stack-neutral. They name the common driver and ORM
per ecosystem (Node, Python, Java, Go, .NET) but never force one. The
`technical-spec` stage should record the choice so later stages are consistent.

## How they load

| Harness | Location | Invocation |
| --- | --- | --- |
| PI Agent | `.pi/skills/db-*` | automatic, or `/skill:db-postgres` |
| Claude Code | `.claude/skills/db-*` | automatic, or the skill command |
| Codex CLI | `.agents/skills/db-*` | automatic when the task matches |

The orchestrator also names them, so the conductor loads the right one at the
right stage.

## Where the choice is recorded

During **Ideate → Technical Specification**, record:

- engine and version (e.g. PostgreSQL 16.3, MySQL 8.4, SQL Server 2022,
  MongoDB 7.0)
- driver and ORM/ODM, with version pins
- migration tool
- pooling strategy and limits
- read/write concerns (MongoDB), isolation level (relational)
- deployment topology (standalone, replica set, sharded, managed)

During **Develop → Code Generation**, every schema change is a reviewed
migration. Note the exact commands in `code-generation-notes.md`.

During **Launch → Test Generation**, integration tests use an ephemeral
database (Testcontainers or equivalent), and CI runs migrations before tests.

During **Curate → Performance Optimization**, evidence comes from query plans
(`EXPLAIN`, Query Store, `explain("executionStats")`) and engine statistics —
never from guesses.

## Adding another engine

Add `core/skills/db-<engine>/SKILL.md` with the same section structure, then run
`node scripts/package.mjs` and `node tests/run-tests.mjs`. The manifests project
the whole `core/skills` tree, so no manifest change is needed. Examples worth
adding later: `db-oracle`, `db-sqlite`, `db-redis` (cache), `db-cassandra`.
