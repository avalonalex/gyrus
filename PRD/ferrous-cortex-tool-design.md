# ferrous-cortex-tool: Utility CLI Design

## Overview

Separate utility/development tools from the main execution CLI (`ferrous-cortex`) into a dedicated tool binary (`ferrous-cortex-tool`). This keeps the main CLI focused on execution while providing rich tooling for development and analysis.

## Motivation

**Current Problem**:
- Main CLI (`ferrous-cortex`) mixes execution and utility functions
- Flags like `--minify`, `--validate`, `--inspect-debug` prevent execution
- Growing list of tool-oriented features clutters the execution CLI
- Users running programs don't need debugging/analysis tools in their mental model

**Proposed Solution**:
- Create `ferrous-cortex-tool` as a separate binary crate
- Use subcommands for different tool functions (more idiomatic than flags)
- Keep `ferrous-cortex` focused on program execution
- Similar to how `cargo` and `rustc` are separate tools with different purposes

## CLI Design

### ferrous-cortex (execution-focused)

```bash
ferrous-cortex [OPTIONS] <FILE>

OPTIONS:
    --max-steps <N>           Maximum execution steps
    --timeout <MS>            Execution timeout
    --memory-size <SIZE>      Memory size
    --memory-model <MODEL>    fixed or unbounded
    --cell-model <MODEL>      wrapping or checked
    --eof-behavior <BEHAVIOR> zero, neg-one, no-change, error
    -v, --verbose             Show execution statistics
    -q, --quiet               Suppress warnings
```

**Keeps only execution-related options.**

### ferrous-cortex-tool (development/analysis tools)

```bash
ferrous-cortex-tool <COMMAND> [OPTIONS]

COMMANDS:
    minify        Minify BF program (strip comments and whitespace)
    validate      Validate program and show warnings
    debug-info    Inspect debug symbols and source locations
    format        Pretty-print BF program with indentation (FUTURE)
    stats         Show program statistics without execution (FUTURE)

Run 'ferrous-cortex-tool <COMMAND> --help' for more information.
```

## Command Details

### 1. `minify` Command

**Purpose**: Strip comments and whitespace from BF programs

**Usage**:
```bash
ferrous-cortex-tool minify <FILE> [OPTIONS]

OPTIONS:
    -o, --output <FILE>    Output file (stdout if not specified)
    -v, --verbose          Show compression statistics
```

**Example**:
```bash
# Minify to stdout
ferrous-cortex-tool minify hello.bf

# Minify to file
ferrous-cortex-tool minify hello.bf -o hello.min.bf

# With statistics
ferrous-cortex-tool minify hello.bf -v
# Output:
# Minified 1,234 bytes to 89 bytes (92.8% reduction)
# ++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.
```

**Implementation**: Already exists in main CLI, just needs to be moved.

---

### 2. `validate` Command

**Purpose**: Validate BF programs and show warnings about suspicious patterns

**Usage**:
```bash
ferrous-cortex-tool validate <FILE> [OPTIONS]

OPTIONS:
    --cell-model <MODEL>    Cell model to assume (wrapping, checked) [default: wrapping]
    --strict                Exit with error if warnings found
    -v, --verbose           Show additional validation context
```

**Example**:
```bash
# Basic validation
ferrous-cortex-tool validate program.bf

# Strict mode (for CI/CD)
ferrous-cortex-tool validate program.bf --strict
# Exit code 1 if warnings found

# With verbose output
ferrous-cortex-tool validate program.bf -v
```

**Output Format**:
```
Validation found 2 warning(s):

Warning: Inefficient pattern at line 5, column 10
Pattern [+] loops ~256 times to reach zero. Consider using [-] to clear the cell.
    5 | +++[+]<<<
      |     ^^^

Warning: Empty loop at line 8, column 3
Loop body is empty - this has no effect and can be removed.
    8 | []
      | ^^

Validation complete: 2 warnings
```

**Implementation**: Already exists in main CLI, just needs to be moved.

---

### 3. `debug-info` Command

**Purpose**: Inspect debug symbols and source location mappings

**Usage**:
```bash
ferrous-cortex-tool debug-info <FILE> [OPTIONS]

OPTIONS:
    --format <FORMAT>      Output format: table, json, csv [default: table]
    --show-source          Include source code context
```

**Example**:
```bash
# Table format (default)
ferrous-cortex-tool debug-info hello.bf

# Output:
# Debug Symbols: 89 instruction locations
#
# Step  Instruction  Line  Column  Source Context
# ----  -----------  ----  ------  --------------
# 0     '+'          1     1       ++++++++[>++++...
# 1     '+'          1     2       ++++++++[>++++...
# 2     '+'          1     3       ++++++++[>++++...
# ...

# JSON format (for tooling)
ferrous-cortex-tool debug-info hello.bf --format json
# {"symbols": [{"step": 0, "instruction": "+", "line": 1, "column": 1}, ...]}

# With source context
ferrous-cortex-tool debug-info hello.bf --show-source
```

**Implementation**: Already exists in main CLI as `--inspect-debug`, needs adaptation for subcommands.

---

### 4. `format` Command (FUTURE)

**Purpose**: Pretty-print BF programs with indentation

**Usage**:
```bash
ferrous-cortex-tool format <FILE> [OPTIONS]

OPTIONS:
    -o, --output <FILE>    Output file (stdout if not specified)
    --indent <N>           Indentation level [default: 2]
    --comments             Preserve comments [default: true]
```

**Example**:
```bash
# Before (minified):
# ++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>

# After formatting:
ferrous-cortex-tool format program.bf

++++++++[
  >++++[
    >++
    >+++
    >+++
    >+
    <<<<-
  ]
  >+
  >+
  >-
  >>+[
    <
  ]
  <-
]
>>
```

**Implementation**: Not yet implemented, requires new formatter module.

---

### 5. `stats` Command (FUTURE)

**Purpose**: Show program statistics without execution

**Usage**:
```bash
ferrous-cortex-tool stats <FILE> [OPTIONS]

OPTIONS:
    --format <FORMAT>    Output format: human, json [default: human]
```

**Example**:
```bash
ferrous-cortex-tool stats hello.bf

# Program Statistics:
#
# Size:
#   Total bytes:        1,234
#   Instructions:       89
#   Comments:           145 bytes (11.7%)
#
# Instruction Mix:
#   +/-:               45 (50.6%)
#   </>:               23 (25.8%)
#   ,.                 8 (9.0%)
#   []:                13 loops (14.6%)
#
# Structure:
#   Max nesting depth:  5
#   Loop count:         13
#   Avg loop size:      6.8 instructions
```

**Implementation**: Not yet implemented, requires static analysis module.

---

## Implementation Plan

### Phase 1: Create Crate Structure ✅

1. Create `crates/ferrous-cortex-tool/` directory
2. Create `Cargo.toml` with dependencies
3. Set up basic CLI structure with `clap` subcommands
4. Add to workspace

### Phase 2: Move Existing Features ✅

1. **Move `minify` command**:
   - Copy minify logic from main CLI
   - Implement subcommand structure
   - Add output file handling
   - Add verbose statistics

2. **Move `validate` command**:
   - Copy validation logic from main CLI
   - Add cell-model selection
   - Add strict mode
   - Improve warning formatting

3. **Move `debug-info` command**:
   - Copy debug symbol inspection from main CLI
   - Add format options (table, JSON, CSV)
   - Add source context display

### Phase 3: Update Main CLI ✅

1. Remove `--minify`, `--validate`, `--inspect-debug` flags
2. Simplify CLI to execution-only options
3. Update documentation
4. Add help text pointing to `ferrous-cortex-tool` for utilities

### Phase 4: Documentation ✅

1. Update README with both CLIs
2. Add examples for common workflows
3. Update architectural-improvements.md
4. Add man pages / help documentation

### Phase 5: Future Commands (Optional)

1. Implement `format` command
2. Implement `stats` command
3. Add more utility commands as needed

## Benefits

### User Experience
- ✅ Clear separation of concerns (execution vs. tooling)
- ✅ Subcommands are more discoverable than flags
- ✅ Help text is focused and relevant
- ✅ Main CLI is simpler for users just running programs

### Developer Experience
- ✅ Tool features can grow without cluttering main CLI
- ✅ Each tool can have rich options without confusion
- ✅ Easier to test and maintain separate concerns
- ✅ Follows Rust ecosystem patterns (cargo, rustc, etc.)

### Architecture
- ✅ Workspace structure supports multiple binaries naturally
- ✅ Both CLIs share the same library crate
- ✅ Clear dependency graph
- ✅ Can add more specialized tools in the future

## Migration Strategy

### Backward Compatibility

**Option 1: Deprecation Warning (Recommended)**
```bash
# Old usage (still works, shows warning)
ferrous-cortex --minify program.bf
# Warning: The --minify flag is deprecated. Use 'ferrous-cortex-tool minify' instead.
# Will be removed in version 1.0.0
```

**Option 2: Hard Break (Simpler)**
- Remove flags immediately
- Update documentation
- Release as minor version bump (0.x.y)
- Since we're pre-1.0, breaking changes are acceptable

**Recommendation**: Use hard break with clear documentation since we're pre-1.0.

### Documentation Updates

Update all docs to show:
```bash
# Running programs
ferrous-cortex program.bf

# Development/analysis tools
ferrous-cortex-tool validate program.bf
ferrous-cortex-tool minify program.bf
ferrous-cortex-tool debug-info program.bf
```

## Testing

### Unit Tests
- Test each subcommand independently
- Test output formats (JSON, CSV, table)
- Test error handling

### Integration Tests
- Test common workflows
- Test file I/O
- Test stdout/stderr behavior

### Example Tests
```rust
#[test]
fn test_minify_command() {
    let output = Command::new("ferrous-cortex-tool")
        .arg("minify")
        .arg("test.bf")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
}

#[test]
fn test_validate_strict_mode() {
    let output = Command::new("ferrous-cortex-tool")
        .arg("validate")
        .arg("bad_program.bf")
        .arg("--strict")
        .output()
        .unwrap();

    assert!(!output.status.success()); // Should fail with warnings
}
```

## Success Criteria

- ✅ `ferrous-cortex` CLI has only execution-related flags
- ✅ `ferrous-cortex-tool` has all utility commands working
- ✅ Both CLIs share the same library crate
- ✅ Documentation is updated and clear
- ✅ All existing tests pass
- ✅ New tool has comprehensive help text
- ✅ Can run: `cargo install ferrous-cortex` and `cargo install ferrous-cortex-tool`

## Future Extensions

Possible future commands:
- `ferrous-cortex-tool decompile` - Convert to higher-level pseudocode
- `ferrous-cortex-tool optimize` - Apply optimization passes
- `ferrous-cortex-tool benchmark` - Benchmark program performance
- `ferrous-cortex-tool convert` - Convert between BF dialects
- `ferrous-cortex-tool check` - Linter with configurable rules

## References

- Clap subcommands: https://docs.rs/clap/latest/clap/
- Cargo workspace: https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html
- Similar patterns: `cargo`, `rustc`, `git`, `docker` CLI organization
