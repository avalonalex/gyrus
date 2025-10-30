# Debug Symbol Tools

## Quick Reference

FerrousCortex includes tools for inspecting and debugging BrainFuck programs at the source level.

## Debug Symbol Inspector

**Command**: `cargo run -- <program.bf> --inspect-debug`

**Purpose**: Visualize the debug symbol table showing how runtime execution maps to source code.

### Example Usage

```bash
# Inspect a simple program
cargo run -- programs/basic/simple.bf --inspect-debug

# Inspect nested loops
cargo run -- programs/debug/symbol_demo.bf --inspect-debug

# Create and inspect your own
echo "+++[>++<-]>." > test.bf
cargo run -- test.bf --inspect-debug
```

### Output Explained

```
=== Debug Symbol Table ===

Source code (17 bytes):
"+++[>++[<.>-]<-]\n"

Symbol table (14 entries):
Step Index   Character       Line     Column   Offset
=================================================================
0            '+'             1        1        0
1            '+'             1        2        1
2            '+'             1        3        2
3            '['             1        4        3         <- Outer loop start
4            '>'             1        5        4
5            '+'             1        6        5
6            '+'             1        7        6
7            '['             1        8        7         <- Inner loop start
8            '<'             1        9        8         <- Inner loop body
9            '.'             1        10       9
10           '>'             1        11       10
11           '-'             1        12       11
12           '<'             1        14       13        <- Outer loop body
13           '-'             1        15       14

=== Summary ===
Total instructions: 14
Source bytes: 17
Compression ratio: 82.4%
```

**Key points**:
- **Step Index**: Sequential execution order (matches interpreter's StepCount)
- **Character**: The BF instruction at this location
- **Line/Column**: Position in source file (1-indexed)
- **Offset**: Byte offset in source (0-indexed)
- **Compression ratio**: Instructions/source (high = minimal comments, low = heavily documented)

### Understanding DFS Traversal

The step indices follow depth-first search (DFS) order:

```brainfuck
+++[>++[<.>-]<-]
```

**Execution order**:
1. Steps 0-2: `+++` (before outer loop)
2. Step 3: `[` (outer loop entry)
3. Steps 4-6: `>++` (before inner loop)
4. Step 7: `[` (inner loop entry)
5. Steps 8-11: `<.>-` (inner loop body)
6. Step 12-13: `<-` (outer loop body)

When the inner loop repeats, it jumps back to step 8. When the outer loop repeats, it jumps back to step 4.

## Runtime Warnings with Source Context

Runtime warnings automatically show source context when using debug symbols and the `--verbose` flag:

```bash
# This will show memory expansion warnings with source locations (unbounded mode)
cargo run -- programs/warnings/memory_expansion.bf \
  --memory-model unbounded \
  --unbounded-initial 5 \
  --unbounded-max 20 \
  --verbose
```

**Example output**:
```
Runtime warning: Memory expanded from 5 to 6 bytes

At line 4, column 5:
   2 | * Demonstrates memory expansion in unbounded mode
   3 | * Start with small memory, trigger growth
   4 | >>>>>>>>>>
             ^
```

**Note**: Cell wrapping (255+1=0, 0-1=255) does not generate warnings as it's standard BrainFuck behavior.

## Use Cases

### 1. Understanding Execution Order

Use `--inspect-debug` to see exactly how your program executes:
- Verify loop bodies are in expected order
- Confirm step indices are sequential
- Check DFS traversal matches your mental model

### 2. Debugging Runtime Issues

When a runtime warning shows up:
1. Note the step index from the warning
2. Use `--inspect-debug` to find that step
3. Verify the source location is correct
4. Check surrounding steps for context

### 3. Code Analysis

- **High compression ratio** (>70%): Production code, minimal comments
- **Low compression ratio** (<30%): Educational code, heavily documented
- **Medium ratio** (40-60%): Balanced documentation

### 4. Verifying Parser Behavior

If you suspect parser issues:
- Check if all instructions appear in symbol table
- Verify line/column numbers are correct
- Ensure offset values are sequential

## Advanced Usage

### Combining Flags

```bash
# Inspect symbols for a program that would generate warnings
cargo run -- overflow.bf --inspect-debug

# Validate AND inspect (two separate runs needed)
cargo run -- program.bf --validate
cargo run -- program.bf --inspect-debug

# Minify AND inspect original
cargo run -- program.bf --minify -o min.bf
cargo run -- program.bf --inspect-debug
```

### Integration with Other Tools

```bash
# Pipe to grep to find specific instructions
cargo run -- program.bf --inspect-debug | grep "'>'"

# Count loops
cargo run -- program.bf --inspect-debug | grep -c "'['"

# Save for later analysis
cargo run -- program.bf --inspect-debug > symbols.txt
```

## Implementation Details

**Files**:
- `crates/ferrous-cortex/src/debug.rs` - DebugInfo data structure
- `crates/ferrous-cortex/src/parser.rs` - DFS traversal and index assignment
- `crates/ferrous-cortex-cli/src/main.rs` - Display formatting (display_debug_symbols)

**Design**:
- O(1) lookup using HashMap<step_index, SourceLocation>
- Memory overhead: ~24 bytes per instruction (SourceLocation struct)
- No runtime overhead when not inspecting (tool exits before execution)

## Related Documentation

- `internal/debug-symbols-design.md` - Complete design document with walkthrough
- `programs/debug/README.md` - Debug example programs
- `PRD/debug-symbols-and-runtime-diagnostics.md` - Requirements and roadmap

## Future Enhancements

Coming in future phases:
- **Loop call stack**: Show nested loop context in warnings
- **Execution tracing**: `--trace` flag to show each instruction as it runs
- **Breakpoints**: Interactive debugger with step-through
