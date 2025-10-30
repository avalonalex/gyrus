# Internal Documentation

This directory contains **internal design documentation** for FerrousCortex developers and contributors.

## Purpose

Documents here explain:
- **How things work** (implementation details)
- **Why we made certain decisions** (design rationale)
- **Future roadmap** (what comes next)

Unlike external docs (README.md, CLAUDE.md), these are for maintainers who need to understand the internals.

## Documents

### Design Documents

#### [`debug-symbols-design.md`](./debug-symbols-design.md)
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

#### [`phase2-loop-tracking-design.md`](./phase2-loop-tracking-design.md)
**Status**: Phase 2 Complete ✅

Design document for Phase 2 loop tracking with accurate source location in nested loops.

**Contains**:
- Problem statement (nested loop error attribution)
- Architecture for loop call stack
- Flat index to source location mapping
- Implementation details

#### [`hook-system-complete.md`](./hook-system-complete.md)
**Status**: Complete ✅ - 2025-10-29

Milestone document for the complete plugin/hook architecture implementation.

**Contains**:
- Complete hook system overview (ExecutionHook, HookContext, HookManager)
- Integration points and API design
- What the hook system enables (debugger, profiler, tracer, time-travel debugging)
- Architecture highlights (zero-cost abstraction, type safety)
- Test coverage summary
- Future enhancements

### Implementation Status & Test Results

#### [`architecture-status.md`](./architecture-status.md)
**Status**: Current (v0.2.0)

Current implementation state, workspace structure, and module organization.

**Contains**:
- Workspace structure
- Module breakdown with line counts
- Key achievements
- Current feature status

#### [`phase2-debug-test-results.md`](./phase2-debug-test-results.md)
**Status**: Complete ✅ - 2025-10-29

Test results for Phase 2 loop tracking in deeply nested loops.

**Contains**:
- 5 comprehensive test cases (4 passing, 1 ignored)
- Deep nesting verification (up to 4 levels)
- Memory boundary testing (100-cell memory)
- Key findings and test patterns

### Tool Documentation

#### [`debug-tools-usage.md`](./debug-tools-usage.md)

Internal guide for debug symbol inspection tools.

**Contains**:
- Debug symbol inspector usage (`--inspect-debug`)
- Output format explanation
- Example programs and use cases

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
