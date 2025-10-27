# Runtime Warnings Examples

This directory contains BrainFuck programs that demonstrate the runtime warning system in FerrousCortex.

## What are Runtime Warnings?

Runtime warnings alert you to memory expansion during program execution:

- **Memory Expansion**: When unbounded memory grows to accommodate pointer movement

These warnings help debug programs by showing exactly when and where memory expansion occurs.

All runtime warnings include **syntax-highlighted source code** with:
- Exact line and column numbers
- Color-coded BrainFuck commands
- Red caret (^) pointing at the instruction that triggered the warning
- Context lines for better readability

## Example Program

### Memory Expansion (`memory_expansion.bf`)

Demonstrates memory expansion warnings in unbounded mode.

```bash
ferrous-cortex programs/warnings/memory_expansion.bf \
  --memory-model unbounded \
  --unbounded-initial 5 \
  --unbounded-max 20 \
  --verbose
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

## Viewing Warnings

Use the `--verbose` flag to see runtime warnings:

```bash
ferrous-cortex programs/warnings/memory_expansion.bf \
  --memory-model unbounded \
  --unbounded-initial 5 \
  --unbounded-max 20 \
  --verbose
```

Warnings are opt-in via `--verbose` since they're primarily useful during development and debugging.

## Understanding the Warnings

Each warning includes:
- **Source location**: Exact line and column number
- **Syntax-highlighted code**: The instruction and surrounding context
- **Visual caret**: Points directly at the instruction that triggered the warning

This rich formatting helps you quickly identify memory usage patterns in your BrainFuck code.

### Why Memory Expansion Warnings Matter

1. **Before JIT/AOT Compilation**: These warnings help you understand your program's memory needs before compiling it.

2. **Memory Usage**: Memory expansion warnings show when your program's memory needs grow, helping optimize memory configurations.

3. **Performance**: Excessive memory expansion can impact performance in the compiled version.

4. **Debugging**: Unexpected memory growth may indicate logic errors or inefficient algorithms.

## Cell Wrapping Behavior

**Note**: Cell wrapping (255+1=0, 0-1=255) is standard BrainFuck behavior and does NOT generate warnings. This is expected behavior in most BrainFuck programs.

For strict arithmetic checking during development, use `--cell-model checked` which will error (not warn) on overflow/underflow:

```bash
# Checked mode: errors on overflow/underflow instead of wrapping
ferrous-cortex your_program.bf --cell-model checked
```

## Historical Note

The `cell_overflow.bf`, `cell_underflow.bf`, and `mixed_warnings.bf` programs are retained for testing purposes but no longer generate warnings since cell wrapping is standard BrainFuck behavior.
