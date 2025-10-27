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
- **`rot13.bf`** - ROT13 Caesar cipher (interactive/infinite loop)
- **`fibonacci.bf`** - Fibonacci sequence generator with sophisticated multi-digit arithmetic (see fibonacci_README.md)
- **`deep_nesting.bf`** - Deeply nested loops for testing parser and validator

**Run examples:**
```bash
cargo run -- programs/advanced/quine.bf
cargo run -- programs/advanced/factor.bf
echo "Hello World" | cargo run -- programs/advanced/rot13.bf  # Ctrl-C to stop
cargo run -- programs/advanced/fibonacci.bf  # Ctrl-C to stop
```

### `utilities/` - Practical Utilities

Small, useful programs from D.B. Cristofani's collection demonstrating that BrainFuck can be practical.

- **`cat.bf`** - Copy input to output (`,[ .[-],]`)
- **`reverse.bf`** - Reverse input text
- **`strip_tabs_lf.bf`** - Remove tabs and linefeeds
- **`ascii_unary.bf`** - Show ASCII values in unary (as '!' characters)
- **`clearscreen.bf`** - Output 100 newlines to clear screen
- **`beep.bf`** - Output bell character (ASCII 7)
- **`true.bf`** - Do nothing, exit successfully (shortest quine!)
- **`brainfuck_print.bf`** - Print "brainfuck\n"
- **`text_to_bf.bf`** - Convert text to BrainFuck code

**Run examples:**
```bash
echo "hello" | cargo run -- programs/utilities/cat.bf
echo "hello" | cargo run -- programs/utilities/reverse.bf  # outputs: olleh
cargo run -- programs/utilities/beep.bf  # ring terminal bell
cargo run -- programs/utilities/brainfuck_print.bf
```

**Source**: http://www.hevanet.com/cristofd/brainfuck/

### `warnings/` - Runtime Warning Demonstrations

Programs that demonstrate the runtime warning system for memory expansion in unbounded mode.

All warnings include **syntax-highlighted source code** with line numbers, color-coded commands, and a red caret pointing at the exact instruction that triggered the warning!

- **`memory_expansion.bf`** - Demonstrates memory expansion in unbounded mode
- **`cell_overflow.bf`** - Tests cell wrapping behavior (no warnings - wrapping is standard BF)
- **`cell_underflow.bf`** - Tests cell wrapping behavior (no warnings - wrapping is standard BF)
- **`mixed_warnings.bf`** - Historical test file (retained for testing purposes)

See `warnings/README.md` for detailed explanations and usage with `--verbose` flag.

**Run examples:**
```bash
cargo run -- programs/warnings/cell_overflow.bf
cargo run -- programs/warnings/memory_expansion.bf --memory-model unbounded --unbounded-initial 5 --unbounded-max 20
cargo run -- programs/warnings/mixed_warnings.bf --quiet  # Suppress warnings
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

## Statistics

- **Total programs**: 33
- **Directories**: 5 (basic, advanced, utilities, tests, errors)
- **Coverage**: Basic syntax, advanced algorithms, practical utilities, error cases, edge cases
- **Sources**:
  - Original implementations
  - D.B. Cristofani's collection
  - Community contributions

## Contributing Programs

When adding new BrainFuck programs:

1. **Basic programs**: Simple, educational examples
2. **Advanced programs**: Complex algorithms and interesting patterns
3. **Utilities**: Small, practical tools
4. **Test programs**: Programs that test specific features
5. **Error programs**: Programs that demonstrate error handling

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
