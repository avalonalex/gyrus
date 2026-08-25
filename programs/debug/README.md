# Debug Programs

Programs in this directory demonstrate debug symbol functionality and runtime diagnostics.

## Symbol Inspection

### `symbol_demo.bf`

**Purpose**: Demonstrates debug symbol mapping for different code patterns.

**Usage**:
```bash
# View debug symbol table
cargo run -- programs/debug/symbol_demo.bf --inspect-debug

# Expected output shows:
# - Simple sequence: steps 0-2 (three + instructions)
# - Loop with body: steps 3-7 ([ followed by >+<-)
# - Nested loops: steps 8-20 (outer loop with inner loop inside)
```

**Key observations**:
- Step indices follow depth-first traversal (DFS) order
- Loop bodies appear immediately after the `[` instruction
- Nested loops show complete inner loop before continuing outer loop
- Comments don't appear in symbol table (only BF instructions)

**Compression ratio**: ~10% (heavily commented for education)

## How Debug Symbols Work

Debug symbols map runtime execution back to source code locations:

1. **Parsing**: Parser assigns sequential step indices during DFS traversal
2. **Execution**: Interpreter's `StepCount` increments in same order
3. **Lookup**: O(1) HashMap lookup from step index → source location
4. **Display**: Runtime warnings show source context with caret pointers

**Example**: Cell overflow at step 42 → lookup shows line 5, column 8 → display 2 lines before/after with caret

See `internal/debug-symbols-design.md` for complete design documentation.

## Using the Inspection Tool

The `--inspect-debug` flag shows the complete symbol table:

```bash
cargo run -- <program.bf> --inspect-debug
```

**Output includes**:
- Full source code (with escape sequences)
- Symbol table: step index → character → line/column/offset
- Summary: total instructions, source bytes, compression ratio

**Use cases**:
1. **Understanding execution order**: See how parser traverses your code
2. **Debugging location issues**: Verify step indices match expectations
3. **Performance analysis**: Check compression ratio (code vs comments)

## Related Documentation

- `internal/debug-symbols-design.md` - Complete design document
- `PRD/debug-symbols-and-runtime-diagnostics.md` - Requirements and roadmap
- `crates/ferrous-cortex/src/debug.rs` - DebugInfo implementation
- `crates/ferrous-cortex-cli/src/main.rs` - Inspection tool implementation
