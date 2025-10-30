# Archived PRDs

This directory contains Product Requirements Documents that have been completed and archived for historical reference.

## Archived Documents

### Cell Model (October 2024)

**CELL_MODEL.md** and **CELL_MODEL_FIX_SUMMARY.md**
- **Status**: ✅ COMPLETED (October 2024)
- **Summary**: Fixed validator logic that incorrectly claimed `[+]` creates infinite loops
- **Key Changes**:
  - Corrected validator warnings to say "inefficient pattern" instead of "infinite loop"
  - Added GCD analysis for patterns like `[++]`, `[+++]`
  - Documented hardcoded u8 wrapping arithmetic behavior
  - Updated all documentation to reflect correct behavior

**Why Archived**:
- Cell model is now working correctly with u8 wrapping arithmetic
- Validator gives accurate warnings
- Documentation has been corrected
- Future configurable cell models (U8Checked, U8Saturating) are supported via CellModel enum

### Syntax Highlighter (October 2025)

**syntax-highlighter-design.md**
- **Status**: ✅ COMPLETED (October 2025)
- **Implementation**: `crates/ferrous-cortex/src/syntax.rs`
- **Summary**: ANSI syntax highlighting for BrainFuck code
- **Key Features**:
  - Color-coded commands by type (pointer ops, cell ops, loops, comments)
  - Loop nesting depth visualization
  - Line numbers
  - Multiple output formats (ANSI, plain text)
  - Used in error messages, warnings, and `ferrous-cortex-tool view` command

**Why Archived**:
- Full implementation complete in syntax.rs (16,987 lines)
- Integrated into error/warning formatting
- View command available in ferrous-cortex-tool
- No remaining features from PRD

### Ferrous-Cortex-Tool Design (October 2025)

**ferrous-cortex-tool-design.md**
- **Status**: ✅ COMPLETED (October 2025)
- **Implementation**: `crates/ferrous-cortex-tool/`
- **Summary**: Separated utility CLI from execution CLI
- **Commands Implemented**:
  - `minify` - Remove comments and whitespace
  - `validate` - Show validation warnings
  - `debug-info` - Inspect debug symbol tables
  - `view` - Syntax-highlighted program viewer

**Why Archived**:
- Workspace structure implemented (v0.2.0)
- All specified commands working
- Clean separation between execution (ferrous-cortex) and tools (ferrous-cortex-tool)
- No remaining features from PRD

### Plugin/Hook Architecture (October 2025)

**plugin-hook-architecture.md**
- **Status**: ✅ COMPLETED (October 2025)
- **Implementation**: `crates/ferrous-cortex/src/hooks/`
- **Summary**: Extensible hook system for debugging, profiling, and tracing
- **Key Features**:
  - `ExecutionHook` trait with 5 hook points
  - `HookManager` for efficient dispatch
  - `HookContext` for immutable state snapshots
  - Built-in hooks: StatsTracker, WarningCollector, LimitEnforcer, DebugTracking
  - Zero-cost abstraction when disabled

**Why Archived**:
- Hook infrastructure complete (38,940 lines in mod.rs)
- 4 built-in hooks implemented (26,874 lines in builtin.rs)
- Examples in `crates/ferrous-cortex/examples/hooks_*.rs`
- Foundation ready for debugger and profiler
- No remaining features from Phase 1 PRD

### Architectural Improvements (October 2025)

**architectural-improvements.md**
- **Status**: ✅ COMPLETED (October 2025)
- **Summary**: Fixed critical architectural limitations
- **Key Improvements**:
  - ✅ I/O abstraction (`BfInput`, `BfOutput` traits)
  - ✅ Workspace migration (library + CLI separation)
  - ✅ StringIo for testing
  - ✅ ExecutionConfig builder pattern
  - ✅ Cell model and memory model orthogonality

**Why Archived**:
- All critical limitations resolved
- Workspace structure in place
- I/O abstraction complete
- Library ready for external use
- Clean module boundaries

### Hook Refactoring (October 2025)

**hook_refactoring_proposal.md**
- **Status**: ✅ COMPLETED (October 2025)
- **Implementation**: All phases (1-7) completed
- **Summary**: Refactored interpreter to use hooks instead of built-in stats/limits
- **Key Changes**:
  - ✅ Removed `stats` and `start_time` from VmState
  - ✅ `StatsTrackerHook` - statistics tracking via hooks
  - ✅ `LimitEnforcerHook` - step/timeout limits via hooks
  - ✅ `WarningCollectorHook` - runtime warnings via hooks
  - ✅ `DebugTrackingHook` - source location tracking
  - ✅ Auto-registration in `interpret_with_io()`
  - ✅ Backward compatible API

**Why Archived**:
- All 7 phases implemented (192-207 in interpreter.rs)
- VmState simplified to core execution state only
- Zero-cost abstraction when hooks disabled (claimed 40-60% speedup)
- Full backward compatibility maintained
- Optional future work: benchmarking, optional hook disabling

### Testing Strategy for Debug Symbols (October 2025)

**TESTING_STRATEGY.md**
- **Status**: ✅ COMPLETED (October 2025)
- **Implementation**: `crates/ferrous-cortex/tests/property_debug_symbols.rs`
- **Summary**: Research document exploring industry testing approaches for debug features
- **Key Strategies Implemented**:
  - ✅ Property-based testing with proptest
  - ✅ Invariant testing (completeness, monotonicity, containment)
  - ✅ Source location bounds checking
  - ✅ Error location validation
  - ✅ 6 property tests in test suite

**Why Archived**:
- Property tests fully implemented
- Testing strategy successfully applied
- Invariants enforced in test suite
- Document served its design/research purpose
- Ongoing testing tracked in `PRD/TESTING.md`

## Active PRDs

See parent directory for currently active PRDs:
- `debug-symbols-and-runtime-diagnostics.md` - Debug symbols complete, advanced diagnostics pending
- `performance-optimizations.md` - JIT/AOT compiler and instruction fusion
- `optimization-and-advanced-features.md` - Advanced compiler features
- `macro-preprocessor-design.md` - Macro system for BrainFuck
- `TESTING.md` - Testing infrastructure documentation
