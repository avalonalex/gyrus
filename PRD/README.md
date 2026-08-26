# PRDs

Design documents for things that **do not exist yet**. Once something ships, its
PRD is deleted rather than archived — the code and `docs/` describe what is
built, and git history keeps the reasoning for anyone who wants it.

That rule cost this directory about 6,000 lines of completed-milestone records
in August 2026, another 1,300 when the TUI debugger and tutorial shipped, and
200 more when breakpoint markers did. It is the rule that keeps this directory
readable.

It has a second effect worth naming: a shipped PRD is often *wrong* by the time
the code exists — the breakpoint-marker design reached the opposite conclusion
from the one that shipped — and deleting it is how that stops mattering. Why a
design changed belongs in the commit that changed it.

## Active

| Document | Status | Priority |
|---|---|---|
| [macro-preprocessor-design.md](macro-preprocessor-design.md) | First slice in progress: `gyrus-macro` expands `@define` and repeat counts, and runtime errors name the `.bfm` | **Medium — being built** |

## Future

Recorded rather than scheduled. These are not next, and saying so here is the
point: a directory where everything looks imminent is one nobody trusts.

| Document | Status | Why it is not next |
|---|---|---|
| [formal-verification.md](formal-verification.md) | Not started | Narrow but real: the two correctness bugs this project has had are both the kind a model checker settles and a test cannot. Waiting on confirmation that the toolchain friction is tolerable |

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
documents, and the only part unique to it was a design for keeping hooks
meaningful under optimization -- which was extracted into its own file, and
then deleted a year later, because it planned work that had since shipped
differently and rested on an assumption the code never adopted. Both halves of
that are the lesson: extract the part that exists nowhere else, and be willing
to delete it when the code has moved past it.
