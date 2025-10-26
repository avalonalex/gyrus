# Runtime Warnings Examples

This directory contains BrainFuck programs that demonstrate the runtime warning system in FerrousCortex.

## What are Runtime Warnings?

Runtime warnings alert you to potentially problematic behavior during program execution:

- **Cell Overflow**: When a cell value wraps from 255 to 0 (in wrapping mode)
- **Cell Underflow**: When a cell value wraps from 0 to 255 (in wrapping mode)
- **Memory Expansion**: When unbounded memory grows to accommodate pointer movement

These warnings help debug programs before compilation by showing exactly when and where wrapping or expansion occurs.

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

Runtime warning at instruction 1: Cell underflow (wrapped 0→255)
Runtime warning at instruction 2: Cell overflow (wrapped 255→0)
Runtime warning at instruction 47: Cell underflow (wrapped 0→255)
Runtime warning at instruction 48: Cell overflow (wrapped 255→0)
Runtime warning at instruction 93: Cell underflow (wrapped 0→255)
Runtime warning at instruction 94: Cell overflow (wrapped 255→0)
```

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

Runtime warning at instruction 1: Cell underflow (wrapped 0→255)
Runtime warning at instruction 4: Cell underflow (wrapped 0→255)
Runtime warning at instruction 8: Cell underflow (wrapped 0→255)
```

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

Runtime warning at instruction 5: Memory expanded from 5 to 6 bytes
Runtime warning at instruction 6: Memory expanded from 6 to 7 bytes
Runtime warning at instruction 10: Memory expanded from 7 to 11 bytes
Runtime warning at instruction 14: Memory expanded from 11 to 15 bytes
...
```

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

Each warning includes the **instruction index** where the event occurred. This helps you locate the exact instruction in your code that triggered the warning.

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
