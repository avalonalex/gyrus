# FerrousCortex Library Examples

This directory contains Rust examples demonstrating how to use **FerrousCortex as a library** in your own applications.

## Running Examples

```bash
# Run any example with:
cargo run --example <example_name>

# For example:
cargo run --example basic_usage
cargo run --example custom_io
```

## Available Examples

### 1. `basic_usage.rs` - Getting Started

**Learn:** Core library usage, parsing, execution, error handling

```bash
cargo run --example basic_usage
```

**Demonstrates:**
- Parsing BrainFuck source code
- Executing programs with string I/O
- Capturing output
- Accessing execution statistics
- Error handling

**Use this when:** You're just getting started with FerrousCortex

---

### 2. `custom_io.rs` - Custom I/O Implementation

**Learn:** Implementing your own I/O for integration with files, networks, GUIs

```bash
cargo run --example custom_io
```

**Demonstrates:**
- Implementing `BfInput` trait
- Implementing `BfOutput` trait
- Uppercase output transformer
- ROT13 input transformer
- Logging output wrapper

**Use this when:** You need to integrate FerrousCortex with custom systems

**Key traits:**
```rust
pub trait BfInput {
    fn read_byte(&mut self) -> io::Result<Option<u8>>;
}

pub trait BfOutput {
    fn write_byte(&mut self, byte: u8) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
```

---

### 3. `memory_models.rs` - Memory Model Configuration

**Learn:** Three different memory models and when to use each

```bash
cargo run --example memory_models
```

**Demonstrates:**
- **Fixed memory**: Traditional bounds-checked array
- **Wrapping memory**: Circular buffer (wraps at boundaries)
- **Unbounded memory**: Dynamic growth with limits

**Use this when:** You need to customize memory behavior

**Configuration:**
```rust
// Fixed (default)
.with_memory_size(30000)

// Wrapping
.with_wrapping_memory(30000)

// Unbounded
.with_unbounded_memory(initial, max)?
```

---

### 4. `validation.rs` - Program Validation

**Learn:** Static analysis and warning detection

```bash
cargo run --example validation
```

**Demonstrates:**
- Validating programs before execution
- Detecting empty loops
- Detecting infinite increment loops
- Detecting extreme nesting
- Recommended validation workflows

**Use this when:** Building tools that need code quality checks

**API:**
```rust
let instructions = parse(source)?;
let warnings = validate(&instructions);
for warning in warnings {
    println!("{}", warning);
}
```

---

### 5. `minify.rs` - Code Minification

**Learn:** Removing comments and whitespace

```bash
cargo run --example minify
```

**Demonstrates:**
- Minifying BrainFuck code
- Preserving functionality
- Round-trip guarantees
- Size reduction metrics

**Use this when:** You need compact BrainFuck code

**API:**
```rust
let instructions = parse(source)?;
let minified = minify(&instructions);
// 95%+ size reduction typical
```

---

## Integration Patterns

### Pattern 1: Simple Execution

```rust
use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfig};

let instructions = parse(source)?;
let mut input = StringIo::new("input data");
let mut output = StringIo::empty();

interpret_with_io(&instructions, ExecutionConfig::default(), &mut input, &mut output)?;

println!("Output: {}", output.output_string());
```

### Pattern 2: With Configuration

```rust
use ferrous_cortex::{ExecutionConfigBuilder, EofBehavior};

let config = ExecutionConfigBuilder::new()
    .with_memory_size(30000)
    .with_max_steps(1_000_000)
    .with_timeout_ms(5000)
    .with_eof_behavior(EofBehavior::SetZero)
    .build();

interpret_with_io(&instructions, config, &mut input, &mut output)?;
```

### Pattern 3: With Validation

```rust
let instructions = parse(source)?;
let warnings = validate(&instructions);

if !warnings.is_empty() {
    for warning in warnings {
        eprintln!("Warning: {}", warning);
    }
}

interpret_with_io(&instructions, config, &mut input, &mut output)?;
```

### Pattern 4: Custom I/O

```rust
struct MyInput { /* ... */ }
impl BfInput for MyInput { /* ... */ }

struct MyOutput { /* ... */ }
impl BfOutput for MyOutput { /* ... */ }

let mut input = MyInput::new();
let mut output = MyOutput::new();

interpret_with_io(&instructions, config, &mut input, &mut output)?;
```

## Future Example Ideas

These are potential examples that would showcase advanced library usage. Contributions welcome!

### 1. Interactive REPL (`repl.rs`)
**Description**: Step-by-step BrainFuck execution with interactive debugging

**Features**:
- Execute instructions one at a time
- Inspect memory state after each step
- Set breakpoints at specific positions
- Pause/resume execution
- View loop stack and current position

**Use cases**: Education, debugging, understanding program flow

**Key APIs**: Parse, execute in steps, track state

---

### 2. Web API Server (`web_server.rs`)
**Description**: HTTP server that executes BrainFuck code via REST API

**Features**:
- POST endpoint accepting BF source code
- Configurable execution limits (timeout, steps)
- JSON response with output and statistics
- Error handling with proper HTTP status codes
- Rate limiting and sandboxing

**Use cases**: Online BF playground, code sharing platforms, teaching

**Tech stack**: `axum` or `actix-web`, custom I/O for HTTP

---

### 3. Testing Framework (`test_framework.rs`)
**Description**: Run BrainFuck test suites with assertions

**Features**:
- Load .bf files with expected outputs
- Compare actual vs expected results
- Report test successes/failures
- Support for input fixtures
- Test coverage metrics

**Use cases**: BF program development, CI/CD pipelines

**Example**:
```rust
let test = BfTest::new("program.bf")
    .with_input("test input")
    .expect_output("expected output")
    .expect_steps_less_than(1000);

assert!(test.run().is_pass());
```

---

### 4. Code Formatter (`formatter.rs`)
**Description**: Pretty-print BrainFuck code with configurable style

**Features**:
- Configurable indentation (spaces/tabs)
- Line wrapping for long sequences
- Comment alignment
- Syntax highlighting (terminal colors)
- Diff mode (show before/after)

**Use cases**: Code reviews, standardizing codebases

**Example output**:
```brainfuck
+++++ +++++    * Cell 0 = 10
[
  > +++++ +++  * Cell 1 += 8
  < -          * Cell 0 -= 1
]
```

---

### 5. Performance Profiler (`profiler.rs`)
**Description**: Analyze execution performance and hot paths

**Features**:
- Track time spent in each loop
- Count loop iterations per loop body
- Identify hot paths (most executed instructions)
- Memory access patterns
- Generate flamegraph-style output

**Use cases**: Optimization, understanding performance

**Output**:
```
Hot Paths:
  1. Loop at offset 45: 89.2% of execution time (2.3M iterations)
  2. Loop at offset 120: 8.1% of execution time (450K iterations)

Memory Access:
  - Cells 0-10: 95% of accesses
  - Cells 11-50: 4% of accesses
  - Cells 51+: 1% of accesses
```

---

### 6. AST Visualizer (`visualize_ast.rs`)
**Description**: Generate visual representations of BF program structure

**Features**:
- Output AST as DOT (Graphviz) format
- Generate Mermaid diagrams
- Show nested loop structure
- Highlight instruction types
- Export to SVG/PNG

**Use cases**: Understanding complex programs, documentation

**Example**:
```rust
let instructions = parse(source)?;
let dot = visualize_as_dot(&instructions);
println!("{}", dot);  // Paste into Graphviz
```

---

### 7. Transpiler (`transpile.rs`)
**Description**: Convert BrainFuck to other languages

**Features**:
- BF → Python
- BF → JavaScript
- BF → C
- BF → Rust
- Preserve comments as source comments
- Optimization hints

**Use cases**: Understanding BF semantics, cross-platform execution

**Example**:
```rust
let instructions = parse(bf_source)?;
let python_code = transpile_to_python(&instructions)?;
println!("{}", python_code);
```

---

### 8. Fuzzer (`fuzzer.rs`)
**Description**: Generate random valid BrainFuck programs for testing

**Features**:
- Generate random valid programs (balanced brackets)
- Configurable complexity (depth, length)
- Property-based testing integration
- Mutation-based fuzzing
- Crash detection

**Use cases**: Testing interpreter robustness, finding edge cases

**Example**:
```rust
let fuzzer = BfFuzzer::new()
    .max_depth(5)
    .max_length(100)
    .with_seed(12345);

for program in fuzzer.take(1000) {
    let _ = parse(&program); // Should never panic
}
```

---

### 9. Debugger Protocol (`debugger.rs`)
**Description**: Implement DAP (Debug Adapter Protocol) for BF

**Features**:
- Breakpoints at source locations
- Step over/into/out
- Variable inspection (memory cells)
- Call stack (loop stack)
- Watch expressions

**Use cases**: IDE integration (VS Code, IntelliJ)

**Tech stack**: `dap` crate, JSON-RPC

---

### 10. Code Coverage (`coverage.rs`)
**Description**: Track which instructions were executed

**Features**:
- Line coverage for BF programs
- Branch coverage for loops
- Instruction coverage percentage
- Highlight uncovered code
- Coverage reports (HTML, JSON)

**Use cases**: Testing quality, identifying dead code

**Output**:
```
Coverage: 85% (17/20 instructions)
Uncovered:
  - Line 5: Loop at [+++]
  - Line 8: Output instruction
```

---

### 11. Static Analyzer (`analyzer.rs`)
**Description**: Deep static analysis beyond basic validation

**Features**:
- Complexity metrics (cyclomatic complexity)
- Data flow analysis
- Dead code detection
- Unreachable loop detection
- Suggest optimizations

**Use cases**: Code quality tools, optimization suggestions

**Example**:
```rust
let analysis = analyze(&instructions)?;
println!("Cyclomatic complexity: {}", analysis.complexity());
println!("Suggestions: {:?}", analysis.optimization_hints());
```

---

### 12. Sandbox / Playground (`sandbox.rs`)
**Description**: Safe execution environment with resource limits

**Features**:
- Pre-configured safety limits
- Execution in separate thread
- Timeout enforcement
- Memory quotas
- Result caching

**Use cases**: Public playgrounds, untrusted code execution

**Example**:
```rust
let sandbox = Sandbox::new()
    .max_execution_time(Duration::from_secs(5))
    .max_memory(1_000_000)
    .max_output_size(10_000);

let result = sandbox.run(untrusted_code)?;
```

---

### 13. Language Server (`lsp_server.rs`)
**Description**: LSP implementation for BF editors

**Features**:
- Diagnostics (errors, warnings)
- Hover information
- Go to definition (loop pairs)
- Code completion (common patterns)
- Rename refactoring

**Use cases**: IDE support, developer productivity

**Tech stack**: `tower-lsp` crate

---

### 14. Benchmark Harness (`benchmark_harness.rs`)
**Description**: Automated performance testing framework

**Features**:
- Load benchmark programs
- Run with different configurations
- Compare memory models
- Regression detection
- Historical tracking

**Use cases**: Performance testing, optimization validation

**Example**:
```rust
let harness = BenchmarkHarness::new()
    .add_benchmark("mandelbrot.bf")
    .add_benchmark("fibonacci.bf")
    .compare_configs(vec![
        config_fixed(),
        config_wrapping(),
        config_unbounded(),
    ]);

harness.run_and_report()?;
```

---

### 15. Migration Tool (`migrate.rs`)
**Description**: Convert between BrainFuck dialects/extensions

**Features**:
- Support different BF variants (EOF behavior, tape size)
- Convert extended BF (with extra commands) to standard
- Normalize code style
- Detect dialect from code

**Use cases**: Cross-compatibility, standardization

---

### 16. Teaching Tool (`tutorial.rs`)
**Description**: Interactive BF learning with step-by-step visualization

**Features**:
- Visualize memory as ASCII art
- Highlight current instruction
- Show loop iterations
- Explain each step
- Quiz mode

**Use cases**: Education, onboarding

**Example output**:
```
Memory: [0, 3, 7, 0, 0, ...]
         ^
         pointer

Step 5: Increment cell at position 1
  Before: [0, 3, 7, 0, 0]
  After:  [0, 4, 7, 0, 0]
```

---

## See Also

- [BrainFuck Programs](../programs/) - Sample .bf programs to run
- [Main Documentation](../README.md) - Full CLI documentation
- [API Documentation](https://docs.rs/ferrous-cortex) - Complete API reference (when published)

## Contributing

### Adding New Examples

When contributing examples:
1. **Focus**: Keep each example focused on one concept
2. **Documentation**: Include comprehensive comments explaining the pattern
3. **Error handling**: Show both success and error cases
4. **Output**: Provide clear, informative output
5. **Testing**: Ensure the example compiles and runs successfully
6. **README**: Update this README with usage instructions

### Implementing Future Ideas

If you implement any of the future ideas above:
1. Create the example file in this directory
2. Add it to the "Available Examples" section
3. Remove it from "Future Example Ideas"
4. Add usage instructions and key learnings
5. Consider adding tests if the example is complex

### Example Template

```rust
//! Brief description of what this example demonstrates
//!
//! Detailed explanation of the concept, use cases, and any
//! important notes.
//!
//! Run with: cargo run --example example_name

use ferrous_cortex::{/* imports */};

fn main() -> Result<(), BfError> {
    println!("=== Example Name ===\n");

    // Example 1: Basic usage
    println!("Example 1: Description");
    println!("----------------------");

    // Implementation with comments

    println!();

    // More examples...

    Ok(())
}
```
