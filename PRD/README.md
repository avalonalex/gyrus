# gyrus PRD Directory

This directory contains Product Requirements Documents (PRDs) for gyrus features and improvements.

## Structure

- **Active PRDs**: Documents for features currently in development or planned
- **Archived PRDs** (`archived/`): Completed features preserved for historical reference

## Active PRDs

### Project & Release

**public-release-and-rename.md**
- **Status**: ❌ Not Started
- **Summary**: Rename FerrousCortex -> `gyrus` and close the legal/metadata/doc
  gaps blocking a first public push (no LICENSE files, GPL third-party program,
  placeholder repo URLs, committed build artifact, doc drift, no CI)
- **Priority**: High (blocks going public)
- **Dependencies**: None technical; Phase 0 requires human decisions

### Debug & Diagnostics

**debug-symbols-and-runtime-diagnostics.md**
- **Status**: ⏳ Partially Complete
- **Completed**: Debug symbol tracking, source location in errors/warnings
- **Remaining**: Advanced diagnostics, call stack visualization
- **Priority**: Medium (foundation complete)

### Performance & Optimization

**optimization-and-advanced-features.md**
- **Status**: ❌ Not Started (Design Complete ✅)
- **Summary**: Comprehensive optimization roadmap with hook integration
- **Key Features**:
  - Three-tier optimization strategy (parse-time, adaptive, AOT/JIT)
  - RLE, clear loops, scan loops, copy/multiply optimizations
  - I/O buffering and memory optimizations
  - Hook compatibility and debug symbol preservation
  - Language extensions (# debug, @ breakpoint)
  - Developer tools (REPL, debugger, profiler)
- **Priority**: High (next major milestone)
- **Dependencies**: Hook system ✅ complete
- **Note**: Merged from performance-optimizations.md (October 2025)

### Language Extensions

**macro-preprocessor-design.md**
- **Status**: ❌ Not Started
- **Summary**: Macro system for BrainFuck (named macros, parameters, standard library)
- **Priority**: Low
- **Dependencies**: Parser infrastructure ✅ complete

### Documentation

**TESTING.md**
- **Status**: 📚 Living Document
- **Summary**: Testing strategy, coverage goals, property-based testing
- **Note**: Continuously updated as testing evolves

## Archived PRDs

See `archived/README.md` for completed features:

### Recently Completed (November 2025)
- ✅ **Interpreter Refactoring** - Split interpreter.rs into focused modules (state, dispatch, execution)
- ✅ **String-to-BF Compiler** - Dynamic programming codegen achieving ~11 BF ops/char (72% size reduction)
- ✅ **Random Program Generator** - Configurable BF program generator for testing (exposed via CLI)

### Previously Completed (October 2025)
- ✅ **Syntax Highlighter** - ANSI color highlighting for BF code
- ✅ **gyrus-tool** - Separated utility CLI from execution CLI
- ✅ **Plugin/Hook Architecture** - Extensible execution monitoring system
- ✅ **Architectural Improvements** - Workspace migration, I/O abstraction
- ✅ **Hook Refactoring** - Moved stats/limits to hooks, simplified VmState
- ✅ **Testing Strategy** - Property-based testing for debug symbols implemented
- ✅ **PRD Consolidation** - Merged performance-optimizations.md into optimization-and-advanced-features.md

### Previously Completed (October 2024)
- ✅ **Cell Model** - Fixed validator and documented wrapping behavior

## How to Use This Directory

### When Planning New Features
1. Review active PRDs to avoid duplication
2. Check archived PRDs for related historical context
3. Create new PRD in this directory
4. Update this README with status

### When Completing Features
1. Update PRD with implementation details
2. Move to `archived/` directory
3. Update `archived/README.md` with completion summary
4. Update this README to remove from active list

### PRD Template

```markdown
# PRD: Feature Name

**Status**: Not Started | In Progress | Partially Complete | Complete
**Last Updated**: YYYY-MM-DD
**Priority**: High | Medium | Low

## Summary
Brief 2-3 sentence overview

## Motivation
Why this feature is needed

## Requirements
- Functional requirements
- Non-functional requirements

## Design
High-level architecture and approach

## Implementation Plan
Phased rollout or implementation steps

## Success Criteria
How to measure completion

## Dependencies
Prerequisites and related features
```

## Priority Guidelines

- **High**: Critical features, blocking other work, or major user value
- **Medium**: Important but not blocking, quality of life improvements
- **Low**: Nice-to-have, research, experimental features

## Notes

- PRDs are living documents - update status as work progresses
- Keep archived PRDs for historical reference and lessons learned
- Link between related PRDs to show dependencies
- Update CLAUDE.md when major features are completed
