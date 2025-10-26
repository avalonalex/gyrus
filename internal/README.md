# Internal Documentation

This directory contains **internal design documentation** for FerrousCortex developers and contributors.

## Purpose

Documents here explain:
- **How things work** (implementation details)
- **Why we made certain decisions** (design rationale)
- **Future roadmap** (what comes next)

Unlike external docs (README.md, CLAUDE.md), these are for maintainers who need to understand the internals.

## Documents

### [`debug-symbols-design.md`](./debug-symbols-design.md)
**Status**: Phase 1 Complete ✅

Complete design document for the debug symbols and runtime diagnostics system.

**Contains**:
- Architecture overview with diagrams
- Step-by-step walkthrough of a real example
- Implementation details (data structures, threading model)
- Performance analysis
- Future work (Phase 2, Phase 3)

**Read this if you want to**:
- Understand how runtime warnings map to source locations
- Extend the debug system (loop stack, tracing)
- Debug issues with source location tracking
- Learn about the flat index DFS alignment technique

## Contributing

When adding new major features:

1. **Before implementing**: Check PRD/ directory for requirements
2. **During development**: Take notes on key decisions
3. **After completion**: Document the design in internal/
   - Explain the architecture
   - Show example walkthroughs
   - Note future extensions
   - Include performance considerations

## Document Template

See `debug-symbols-design.md` as a template for internal docs:

```markdown
# Feature Name: Design Document

**Status**: [In Progress / Complete]
**Authors**: [Team/Names]
**Last Updated**: YYYY-MM-DD

---

## Overview
[What problem does this solve?]

## Design Philosophy
[Core principles and key insights]

## Architecture
[Diagrams and structure]

## Step-by-Step Walkthrough
[Real example traced through the system]

## Implementation Details
[Data structures, APIs, threading model]

## Future Work
[What's next?]

## Performance Considerations
[Overhead analysis]
```

## Related Documentation

- **PRD/**: Product requirements (what to build)
- **CLAUDE.md**: Project overview for AI assistants
- **README.md**: User-facing documentation
- **internal/**: Design documentation (you are here)
