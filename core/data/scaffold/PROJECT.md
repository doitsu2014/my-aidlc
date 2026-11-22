# Project

Standing knowledge for this repository. Humans edit this file. The tool never overwrites it.

## Purpose and users

Who the software is for, and what problem it solves.

## Domain glossary

Terms the team must use the same way.

## Architecture overview

The current shape of the system, in enough detail for Analyze to start.

## Conventions

Code, review, and collaboration conventions.

## Constraints and non-negotiables

Legal, operational, and product rules that an intent may not break.

## Testing and release

How to run tests, what must pass, and how a change is released.

## Commands

The exact commands an agent runs, from the repository root: build, test, lint, format, and a baseline command that runs the whole existing suite.

## Environment prerequisites

What must be installed or running before code can be built or tested (for example Docker Desktop, a database, a language toolchain), and how to check it. Register a check with `dtech-aidlc config prerequisite set NAME --check "COMMAND"`.

## Known baseline failures

Checks that already fail before any change (for example existing type errors or lint findings), with the count and the reason, so a new failure can be told apart from an old one.

## Deeper docs

Pointers to architecture notes, ADRs, API specs, and runbooks. Register those paths in `aidlc.knowledge.json` (kinds: project, docs, requirements, architecture, adr, api-spec, runbook, conventions, readme).
