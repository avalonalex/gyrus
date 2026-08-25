# PRDs

Design documents for things that **do not exist yet**. Once something ships, its
PRD is deleted rather than archived — the code and `docs/` describe what is
built, and git history keeps the reasoning for anyone who wants it.

That rule cost this directory about 6,000 lines of completed-milestone records
in August 2026, and it is the rule that keeps it readable.

## Active

| Document | Status | Priority |
|---|---|---|
| [public-release-and-rename.md](public-release-and-rename.md) | In progress — phases 1-4 done, release remaining | High |
| [optimizer-hook-integration.md](optimizer-hook-integration.md) | Design complete, unimplemented | High — blocks aggressive optimization |
| [optimizer_improvements.md](optimizer_improvements.md) | Catalogue of missed optimizations, and of the ones tried and dropped | Medium |
| [tui_debugger_and_tutorial.md](tui_debugger_and_tutorial.md) | Design complete, unimplemented | Medium |
| [macro-preprocessor-design.md](macro-preprocessor-design.md) | Design complete, unimplemented | Low |

## Writing one

```markdown
# PRD: Feature Name

**Status**: Not Started | In Progress | Complete
**Last Updated**: YYYY-MM-DD
**Priority**: High | Medium | Low

## Summary        Two or three sentences.
## Motivation     Why this is worth building.
## Requirements   Functional and non-functional.
## Design         Architecture and approach.
## Implementation Plan
## Success Criteria   How you know it is done.
## Dependencies
```

Prefer one focused document over a section inside a large one. The umbrella PRD
this directory used to have grew to 2,500 lines by restating four other
documents, and the only part unique to it was the hook-integration design now
in `optimizer-hook-integration.md`.
