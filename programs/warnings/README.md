# Runtime Warnings Examples

This directory contains BrainFuck programs that demonstrate the runtime warning system in FerrousCortex.

## What are Runtime Warnings?

Runtime warnings alert you to potentially problematic behavior during program execution:

- **Cell Overflow**: When a cell value wraps from 255 to 0 (in wrapping mode)
- **Cell Underflow**: When a cell value wraps from 0 to 255 (in wrapping mode)
- **Memory Expansion**: When unbounded memory grows to accommodate pointer movement

These warnings help debug programs before compilation by showing exactly when and where wrapping or expansion occurs.

**NEW in v0.2.1**: All runtime warnings now include **syntax-highlighted source code** with:
- Exact line and column numbers
- Color-coded BrainFuck commands
- Red caret (^) pointing at the instruction that triggered the warning
- Context lines for better readability

## Example Programs

### 1. Cell Overflow (`cell_overflow.bf`)

Demonstrates cell overflow warnings by incrementing cells past 255.

```bash
ferrous-cortex programs/warnings/cell_overflow.bf
```

**Expected output:**
```
***
```

**Warnings:**
```
=== Runtime Warnings ===
Detected 6 runtime event(s):

Runtime warning: Cell underflow (wrapped 0→255)

At line 3, column 1:
   1 │ * Cell Overflow Demonstration
   2 │ * This program shows multiple cell overflow and underflow events
   3 │ -
       ^

Runtime warning: Cell overflow (wrapped 255→0)

At line 4, column 1:
   2 │ * This program shows multiple cell overflow and underflow events
   3 │ -
   4 │ +
       ^

... (4 more warnings with source locations)
```

*(Actual output includes syntax highlighting: commands are color-coded, line numbers in gray, caret in red)*

### 2. Cell Underflow (`cell_underflow.bf`)

Demonstrates cell underflow warnings by decrementing cells from 0.

```bash
ferrous-cortex programs/warnings/cell_underflow.bf
```

**Expected output:** Three bytes with values 255, 254, 253

**Warnings:**
```
=== Runtime Warnings ===
Detected 3 runtime event(s):

Runtime warning: Cell underflow (wrapped 0→255)

At line 3, column 1:
   1 │ * Cell Underflow Demonstration
   2 │ * Decrementing from 0 to trigger underflow
   3 │ -
       ^

... (2 more warnings with source locations)
```

*(With syntax highlighting: `-` in green, comments in gray, caret in red)*

### 3. Memory Expansion (`memory_expansion.bf`)

Demonstrates memory expansion warnings in unbounded mode.

```bash
ferrous-cortex programs/warnings/memory_expansion.bf \
  --memory-model unbounded \
  --unbounded-initial 5 \
  --unbounded-max 20
```

**Expected output:** Three bytes with values 1, 2, 3

**Warnings:**
```
=== Runtime Warnings ===
Detected 10 runtime event(s):

Runtime warning: Memory expanded from 5 to 6 bytes

At line 4, column 5:
   2 │ * Demonstrates memory expansion in unbounded mode
   3 │ * Start with small memory, trigger growth
   4 │ >>>>>>>>>>
               ^

Runtime warning: Memory expanded from 6 to 7 bytes

At line 4, column 6:
   2 │ * Demonstrates memory expansion in unbounded mode
   3 │ * Start with small memory, trigger growth
   4 │ >>>>>>>>>>
                ^

... (8 more expansion warnings as memory grows)
```

*(With syntax highlighting: `>` in cyan, comments in gray, caret in red)*

### 4. Mixed Warnings (`mixed_warnings.bf`)

Combines multiple warning types in one program.

**With default (fixed memory):**
```bash
ferrous-cortex programs/warnings/mixed_warnings.bf
```

Shows only cell overflow/underflow warnings.

**With unbounded memory:**
```bash
ferrous-cortex programs/warnings/mixed_warnings.bf \
  --memory-model unbounded \
  --unbounded-initial 3 \
  --unbounded-max 20
```

Shows cell warnings AND memory expansion warnings.

## Suppressing Warnings

Use the `--quiet` flag to suppress warnings and run silently:

```bash
ferrous-cortex programs/warnings/cell_overflow.bf --quiet
```

This is useful when you want only the program output without diagnostic information.

**Note**: `--quiet` is designed for **permissive modes** (wrapping cells, unbounded memory) where warnings are informational. Using `--quiet` with `--cell-model checked` doesn't make much sense, since checked mode produces errors (not warnings) that stop execution.

## Understanding the Warnings

Each warning includes:
- **Source location**: Exact line and column number
- **Syntax-highlighted code**: The instruction and surrounding context
- **Visual caret**: Points directly at the instruction that triggered the warning

This rich formatting helps you quickly identify and fix issues in your BrainFuck code.

### Why These Warnings Matter

1. **Before JIT/AOT Compilation**: These warnings help you understand your program's behavior before compiling it, where debugging is harder.

2. **Catching Bugs**: Unexpected overflow/underflow often indicates logic errors in your BrainFuck code.

3. **Memory Usage**: Memory expansion warnings show when your program's memory needs grow, helping optimize memory configurations.

4. **Performance**: Excessive wrapping or memory expansion can impact performance in the compiled version.

## Cell Models and Warnings

Warnings only appear in **wrapping mode** (default). In **checked mode**, overflow/underflow raises errors instead:

```bash
# Wrapping mode: shows warnings
ferrous-cortex programs/warnings/cell_overflow.bf

# Checked mode: raises errors instead of warnings
ferrous-cortex programs/warnings/cell_overflow.bf --cell-model checked
```

This makes checked mode useful for catching arithmetic bugs during development.
