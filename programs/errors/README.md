# Error Handling Examples

This directory contains example BrainFuck programs that demonstrate gyrus's comprehensive error handling and diagnostic capabilities.

## Overview

gyrus reports errors with:
- **Rich error messages** with source context
- **Line and column tracking** for precise error locations
- **Visual error context** with caret (^) pointing to issues
- **Multiple error reporting** (shows all bracket errors at once)
- **Runtime safety** with configurable limits
- **Static validation** to catch issues before execution

## Examples

### 1. `unmatched_bracket.bf` - Parse Errors

Demonstrates bracket matching errors with detailed source context.

**Run:**
```bash
cargo run -- examples/errors/unmatched_bracket.bf
```

**What it shows:**
- Unmatched opening bracket `[`
- Exact line and column number
- Source code context (2 lines before/after)
- Caret (^) pointing to the exact error location

**Example output:**
```
Error: Unmatched '[' at line 10, column 1
    8 | >+          * Move and increment
    9 |
   10 | [>+++       * Opening bracket without matching close
      | ^
   11 |             * This will cause a parse error
   12 |
```

**Related examples:**
- `../unclosed_brackets.bf` - Multiple unmatched opening brackets
- `../multiple_bracket_errors.bf` - Mix of opening and closing errors

---

### 2. `memory_overflow.bf` - Memory Bounds Errors

Demonstrates memory access violations and how different memory models handle them.

**Run:**
```bash
# With small memory to see error quickly
cargo run -- examples/errors/memory_overflow.bf --memory-size 100

# Try with different memory models
cargo run -- examples/errors/memory_overflow.bf --memory-model unbounded
```

**What it shows:**
- Memory pointer out of bounds error
- Attempted cell access vs valid range
- Instruction number where error occurred
- Different memory model behaviors

**Example output:**
```
Error: Memory pointer out of bounds at instruction 101
Attempted to access cell 100, valid range: 0-99
```

**Memory model comparison:**
- `--memory-model fixed` (default): Errors on out-of-bounds access
- `--memory-model unbounded`: Grows dynamically (no error, up to max)

---

### 3. `infinite_loop.bf` - Execution Limits

Demonstrates infinite loop protection using step limits and timeouts.

**Run:**
```bash
# Step limit (fast)
cargo run -- examples/errors/infinite_loop.bf --max-steps 10000

# Timeout (wall-clock time)
cargo run -- examples/errors/infinite_loop.bf --timeout 1000

# Get warning before execution
cargo run -- examples/errors/infinite_loop.bf --validate
```

**What it shows:**
- Step limit exceeded error
- Execution timeout error
- Validation warnings for suspicious patterns

**Example outputs:**

With `--max-steps 10000`:
```
Error: Step limit exceeded: program executed 10000 steps
```

With `--timeout 1000`:
```
Error: Execution timeout: program exceeded 1000ms execution limit
```

With `--validate`:
```
Validation found 1 warning(s):

Warning: Potential infinite loop detected
Cell is only incremented in loop body, will never reach zero
   14 | [
   15 |   +   * Increment (makes it worse!)
      |   ^
   16 |   +   * This loop will never exit naturally
   17 | ]
```

---

### 4. `validation_warnings.bf` - Static Analysis

Demonstrates the validation pass that detects common issues before execution.

**Run:**
```bash
# See warnings
cargo run -- examples/errors/validation_warnings.bf --validate

# Treat warnings as errors (useful for CI/CD)
cargo run -- examples/errors/validation_warnings.bf --strict
```

**What it shows:**
- Empty loops (`[]`)
- Infinite loop patterns (`+[+]`, `+[++]`)
- Extreme nesting (>10 levels)
- Clear descriptions and suggestions

**Example output:**
```
Validation found 4 warning(s):

Warning: Empty loop detected
This loop does nothing and can be removed
    8 | []
      | ^

Warning: Potential infinite loop detected
Cell is only incremented in loop body, will never reach zero
   11 | +[++]
      |   ^

Warning: Extreme nesting detected
Loop nesting depth is 12 levels (performance warning)
   18 | [[[[[[[[[[[
      | ^

Use --strict to treat these warnings as errors
```

---

## Error Categories

### Parse Errors (Caught Before Execution)
- **Bracket mismatches**: Unmatched `[` or `]`
- **Multiple errors**: All bracket errors reported at once
- Source context with line/column numbers

**Prevention:** Check your brackets before running, use an editor with bracket matching

### Runtime Errors (Caught During Execution)
- **Memory bounds**: Accessing cells outside valid range
- **Step limits**: Exceeding maximum instruction count
- **Timeouts**: Program running too long
- **I/O errors**: Input/output failures, EOF handling

**Prevention:** Use `--max-steps` and `--timeout` for untrusted code, configure appropriate memory model

### Validation Warnings (Static Analysis)
- **Empty loops**: Loops that do nothing
- **Infinite loops**: Patterns that never exit
- **Extreme nesting**: Deep loop nesting (performance impact)

**Note:**
- `--validate` mode does NOT execute your program. It only analyzes the code for issues.
- `--strict` mode validates first, then executes ONLY if no warnings are found.

**Prevention:** Run `--validate` during development, use `--strict` in CI/CD pipelines

---

## Best Practices

### During Development
```bash
# Validate before running (does not execute)
cargo run -- your_program.bf --validate

# If validation passes, run the program
cargo run -- your_program.bf

# Or use strict mode: validate and run if clean
cargo run -- your_program.bf --strict

# Run with verbose diagnostics
cargo run -- your_program.bf --verbose
```

### For Production/Untrusted Code
```bash
# Set safety limits
cargo run -- untrusted.bf \
  --max-steps 1000000 \
  --timeout 5000 \
  --memory-size 10000

# Validate first, then execute with limits
cargo run -- untrusted.bf --validate
cargo run -- untrusted.bf --max-steps 1000000 --timeout 5000
```

### In CI/CD Pipelines
```bash
# Fail on any warnings, execute only if clean
cargo run -- program.bf --strict

# With verbose output for CI logs
cargo run -- program.bf --strict --verbose

# Minify and validate (does not execute)
cargo run -- program.bf --minify --validate -o program.min.bf
```

---

## Understanding Error Messages

gyrus error messages follow this format:

```
Error: [Error Type] at [Location]
[Detailed description]
[Source context]
   line_num | source code
            | ^
[Additional information]
```

**Components:**
1. **Error Type**: What went wrong (bracket mismatch, memory bounds, etc.)
2. **Location**: Line and column number (or instruction number for runtime errors)
3. **Description**: Clear explanation of the problem
4. **Source Context**: 2 lines before/after with caret (^) pointing to issue
5. **Additional Info**: Suggestions, valid ranges, etc.

---

## Troubleshooting Guide

### "Unmatched bracket" errors
- **Problem**: Brackets don't pair correctly
- **Solution**: Count your `[` and `]`, ensure each opening has a closing
- **Tip**: Use an editor with bracket matching/highlighting

### "Memory pointer out of bounds" errors
- **Problem**: Trying to access invalid memory cells
- **Solution**:
  - Check your `<` and `>` movements
  - Use `--memory-model unbounded` for dynamic growth
  - Increase `--memory-size` if needed

### "Step limit exceeded" errors
- **Problem**: Program taking too many steps (possible infinite loop)
- **Solution**:
  - Check for infinite loop patterns with `--validate`
  - Increase `--max-steps` if program is legitimately long
  - Fix the infinite loop in your code

### "Execution timeout" errors
- **Problem**: Program running too long (wall-clock time)
- **Solution**:
  - Similar to step limit - check for infinite loops
  - Increase `--timeout` if program is slow but correct
  - Optimize your BrainFuck code

### Validation warnings
- **Problem**: Suspicious patterns detected
- **Solution**:
  - Review the warnings - they often indicate bugs
  - Fix the patterns or confirm they're intentional
  - Use `--strict` to enforce no warnings

---

## Additional Resources

- **Main README**: `../../README.md` - Full documentation
- **Working Examples**: `../` - Valid programs (hello_world.bf, etc.)
- **PRD**: `../../PRD/error-handling-and-reliability.md` - Design details

---

## Contributing

Found a new error case that should be documented? Create an example file following this pattern:

1. Clear comments explaining what the example demonstrates
2. Command to run the example
3. Expected error output
4. Tips for fixing or understanding the error

Submit examples that help users understand gyrus's error handling!
