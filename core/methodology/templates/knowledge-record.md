Use this for the Archived output knowledge-record. Write it at the path given in `context.knowledge.record.path` of the `orchestrate next` directive, which is `.aidlc/knowledge/<intent-id>.md`. Copy the frontmatter values from `context.knowledge.record.frontmatter` and add a title.

```markdown
---
id: <intent-id>
title: <short title>
origin: intent
scope: solo
closed: <ISO date>
phases-run: analyze, ideate, develop, archived
supersedes:
tags:
---

# <title>

## Outcome

## Decisions

## Limitations

## Follow-ups

## Links
```

## Rules

- Keep every heading and write "None." under one that has nothing.
- Cite paths relative to the project root and never write an absolute path.
- Keep each frontmatter value on one line.
- Do not edit another record; list it under supersedes.
- A record a person writes by hand uses `origin: human`, needs only id, title, origin and closed, may use any body headings, and is checked with `dtech-aidlc knowledge check`.
