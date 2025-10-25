# BrainFuck Programs

This directory contains BrainFuck programs for testing and demonstrating FerrousCortex functionality.

## Directory Structure

### `basic/` - Introductory Programs

Simple programs demonstrating core BrainFuck features and FerrousCortex capabilities.

- **`hello_world.bf`** - Classic "Hello World!" program
- **`simple.bf`** - Minimal program that prints 'H'
- **`line_comments.bf`** - Demonstrates line comment syntax using `*`
- **`comments_demo.bf`** - Comment usage examples
- **`comments_test.bf`** - Testing comment handling

**Run examples:**
```bash
cargo run -- programs/basic/hello_world.bf
cargo run -- programs/basic/simple.bf
```

### `advanced/` - Complex Programs

More sophisticated BrainFuck programs showcasing advanced capabilities.

- **`quine.bf`** - Self-replicating program (prints its own source code)
- **`factor.bf`** - Integer factorization program
- **`deep_nesting.bf`** - Deeply nested loops for testing parser and validator

**Run examples:**
```bash
cargo run -- programs/advanced/quine.bf
cargo run -- programs/advanced/factor.bf
```

### `tests/` - Feature Testing Programs

Programs designed to test specific FerrousCortex features.

- **`test_eof.bf`** - Tests EOF behavior (default: set to zero)
- **`test_eof_nochange.bf`** - Tests EOF with no-change behavior
- **`warnings_test.bf`** - Triggers validation warnings
- **`warnings_only.bf`** - Contains only warning-triggering patterns
- **`infinite_loop.bf`** - Infinite loop for testing step limits
- **`infinite_loop2.bf`** - Alternative infinite loop pattern

**Run examples:**
```bash
cargo run -- programs/tests/test_eof.bf --eof-behavior zero
cargo run -- programs/tests/warnings_test.bf --validate
cargo run -- programs/tests/infinite_loop.bf --max-steps 1000
```

### `errors/` - Error Demonstration Programs

Programs that intentionally trigger errors to demonstrate error handling.

- **`README.md`** - Detailed error handling documentation
- **`unmatched_bracket.bf`** - Parse error: unmatched `[`
- **`memory_overflow.bf`** - Runtime error: memory out of bounds
- **`infinite_loop.bf`** - Step limit exceeded error
- **`validation_warnings.bf`** - Programs with validation warnings
- **`error_test.bf`** - General error testing
- **`unclosed_brackets.bf`** - Multiple bracket errors
- **`multiple_bracket_errors.bf`** - Shows multiple error reporting

**Run examples:**
```bash
# Parse errors with rich context
cargo run -- programs/errors/unmatched_bracket.bf

# Runtime errors
cargo run -- programs/errors/memory_overflow.bf --memory-size 100

# Validation warnings
cargo run -- programs/errors/validation_warnings.bf --validate
cargo run -- programs/errors/validation_warnings.bf --strict  # Exit on warnings
```

## Running Programs

### Basic Execution
```bash
cargo run -- programs/basic/hello_world.bf
```

### With Options
```bash
# Verbose mode with statistics
cargo run -- programs/basic/hello_world.bf --verbose

# Limit execution
cargo run -- programs/tests/infinite_loop.bf --max-steps 10000

# Different memory models
cargo run -- programs/advanced/factor.bf --memory-model unbounded

# Validate before running
cargo run -- programs/tests/warnings_test.bf --validate
```

## Contributing Programs

When adding new BrainFuck programs:

1. **Basic programs**: Simple, educational examples
2. **Advanced programs**: Complex algorithms and interesting patterns
3. **Test programs**: Programs that test specific features
4. **Error programs**: Programs that demonstrate error handling

Include comments using `*` for better documentation:
```brainfuck
* This is a line comment
+++    * Increment cell 0 by 3
[      * Start loop
  >.   * Output cell 1
]
```

## See Also

- [Error Handling Documentation](errors/README.md)
- [Main README](../README.md)
- [Library Examples](../examples/) - Rust code showing how to use FerrousCortex as a library
