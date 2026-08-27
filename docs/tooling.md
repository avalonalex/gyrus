# Development Tools (`gyrus-tool`)

`gyrus` runs programs. `gyrus-tool` is everything else: macro expansion,
validation, minification, syntax highlighting, and inspection of what the
parser and optimizer did.

The two full-screen tools have their own pages: [the debugger](debugger.md) for
stepping through a program, and [the tutorial](tutorial.md) for learning the
language.

Back to the [README](../README.md).

## Macro Expansion

```bash
gyrus-tool expand prog.bfm                 # to stdout
gyrus-tool expand prog.bfm -o prog.bf      # to a file
gyrus-tool expand prog.bfm --verbose       # and what it cost
```

Turns `.bfm` macro source into BrainFuck. `gyrus prog.bfm` expands and runs in
one step; this is the step on its own — for reading what a macro produced, for
handing the result to something that only understands BrainFuck, and for
checking that a `.bfm` expands at all, since anything wrong with it is reported
here rather than at run time.

Errors are rendered against the macro source with a caret, the way a parse
error is:

```
Error: Undefined symbol 'nowhere' at line 2, column 5

   1 │ @var a
   2 │ @to nowhere
           ^
```

`--verbose` reports instructions written against instructions emitted, which is
what the macros were worth. Not source bytes against instructions: a `.bfm` is
mostly prose and declarations, so that would say more about how well it is
commented than about what expanding it did.

```
  Instructions written: 23
  Instructions emitted: 737
  Expansion:            32.0x
```

Expanding over the macro source is refused rather than done — `-o prog.bf` is
one character away from `-o prog.bfm`, and the second would replace a
hand-written program with generated BrainFuck.

## Program Validation

gyrus can validate your BrainFuck programs and warn about potential issues:

```bash
# Validate only (does not execute)
gyrus-tool validate program.bf
```

### What Validation Does

**`gyrus-tool validate` (Lint Mode)**
- Parses and analyzes the code for issues
- Shows all warnings, each with its line, column, and a caret into the source
  (or "No warnings found")
- Never executes the program
- Useful for checking code quality without running

**Validation target: pick the cell model you will run under**
- `--cell-model wrapping` (the default) or `--cell-model checked`
- The model changes what a pattern *means*, not just how fast it is. `[+]`
  under wrapping is a slow way to clear a cell -- it counts up, wraps through
  255 to zero, and stops after about 256 iterations. Under checked cells the
  same loop never wraps: it reports an overflow at 255. One is an
  inefficiency, the other is a program that stops working, and the warning
  says which.
- Independent of the model used to *execute*; this is the model to assume
  while reading.

### Warning Types

The validator checks for:

- **Empty loops**: `[]` - Does nothing and can be removed
- **Inefficient increment loops**: `[+]` or `[++]` - Loop many times (~256, ~128 iterations) to reach zero via wrapping
- **Extreme nesting**: Loops nested more than 10 levels deep (performance impact)
- **Inefficient patterns**: Multiple operations that could be optimized (e.g., `[--]` instead of `[-]`)

### Example Workflows

```bash
# Development: Check for issues without running
gyrus-tool validate program.bf

# CI/CD: Validate, then run if clean
gyrus-tool validate program.bf && gyrus program.bf

# CI/CD with verbose output
gyrus-tool validate program.bf && gyrus program.bf --verbose
```

## Code Minification

Strip all comments and whitespace to create compact BrainFuck programs:

```bash
# Output to stdout
gyrus-tool minify program.bf

# Save to file
gyrus-tool minify program.bf -o program.min.bf

# With verbose stats
gyrus-tool minify program.bf -o program.min.bf --verbose
```

**Example:**
```bash
$ cat programs/basic/line_comments.bf
* Line Comment Demo
* Everything after * is completely ignored!

++++++++++  * Cell 0 = 10
[           * Loop 10 times
  >+++++++  * Cell 1 += 7
  <-        * Cell 0 -= 1
]           * Result: Cell 1 = 70
>++.        * Add 2, print 'H'

$ gyrus-tool minify programs/basic/line_comments.bf
++++++++++[>+++++++<-]>++.

$ gyrus-tool minify programs/basic/line_comments.bf --verbose -o min.bf
Minified 514 bytes to 26 bytes (94.9% reduction)
```

Minification removes:
- All line comments (after `*`)
- All implicit comments (non-BF characters)
- All whitespace and formatting

How much that saves depends entirely on how much of the file is prose:
94.9% for a documented example like `line_comments.bf`, but 49.6% for a dense
program like `third-party/advanced/life.bf`, which is nearly all instructions
already. The minified code is functionally identical to the original.

## Debug Symbol Tools

### Quick Reference

gyrus includes tools for inspecting and debugging BrainFuck programs at the source level.

### Debug Symbol Inspector

**Command**: `gyrus-tool debug-info <program.bf>`

**Purpose**: Visualize the debug symbol table showing how runtime execution maps to source code.

### Example Usage

```bash
# Inspect a simple program
gyrus-tool debug-info programs/basic/simple.bf

# Inspect nested loops
gyrus-tool debug-info programs/debug/symbol_demo.bf

# Create and inspect your own
echo "+++[>++<-]>." > test.bf
gyrus-tool debug-info test.bf
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

### Runtime Warnings with Source Context

Runtime warnings automatically show source context when using debug symbols and the `--verbose` flag:

```bash
# This will show memory expansion warnings with source locations (unbounded mode)
cargo run -- programs/warnings/memory_expansion.bf \
  --debug \
  --memory-model unbounded \
  --unbounded-initial 5 \
  --unbounded-max 20 \
  --verbose
```

`--debug` is required: runtime warnings come from execution hooks, and the
optimized interpreter does not run hooks.

**Example output**:
```
Runtime warning: Memory expanded from 5 to 7 bytes

At line 16, column 1:
  15 | * Write a marker at cell 6
  16 | +
       ^
```

The warning points at the `+`, not at the `>`s before it, and the tape jumps by
more than one cell. Both follow from the tape contract: growth covers the cell
that is *used*, so travelling past the end expands nothing and the eventual
access grows straight to where it landed.

**Note**: Cell wrapping (255+1=0, 0-1=255) does not generate warnings as it's standard BrainFuck behavior.

### Use Cases

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

### Advanced Usage

### Combining Flags

```bash
# Inspect symbols for a program that would generate warnings
gyrus-tool debug-info overflow.bf

# Validate, then inspect
gyrus-tool validate program.bf
gyrus-tool debug-info program.bf

# Minify, and inspect the original
gyrus-tool minify program.bf -o min.bf
gyrus-tool debug-info program.bf
```

### Integration with Other Tools

```bash
# Pipe to grep to find specific instructions
gyrus-tool debug-info program.bf | grep "'>'"

# Count loops
gyrus-tool debug-info program.bf | grep -c "'['"

# Save for later analysis
gyrus-tool debug-info program.bf > symbols.txt
```

### Implementation Details

**Files**:
- `crates/gyrus/src/debug.rs` - DebugInfo data structure
- `crates/gyrus/src/parser.rs` - DFS traversal and index assignment
- `crates/gyrus-cli/src/main.rs` - Display formatting (display_debug_symbols)

**Design**:
- O(1) lookup using HashMap<step_index, SourceLocation>
- Memory overhead: ~24 bytes per instruction (SourceLocation struct)
- No runtime overhead when not inspecting (tool exits before execution)

### Related Documentation

- `programs/debug/README.md` - Debug example programs
- `PRD/debug-symbols-and-runtime-diagnostics.md` - Requirements and roadmap

### Future Enhancements

Coming in future phases:
- **Loop call stack**: Show nested loop context in warnings
- **Execution tracing**: `--trace` flag to show each instruction as it runs
- **Breakpoints**: Interactive debugger with step-through
