# FerrousCortex Architecture & Project Structure

## Current Implementation (v0.2.0)

### ✅ Implemented - Clean Modular Architecture

The project now follows idiomatic Rust structure with **workspace organization** and **clean module separation**:

**Workspace Structure:**
- ✅ `crates/ferrous-cortex/` - Core library (1,502 lines across 10 modules)
- ✅ `crates/ferrous-cortex-cli/` - CLI binary
- 🔮 Future: `ferrous-cortex-debugger/`, `ferrous-cortex-jit/`, `ferrous-cortex-repl/`

**Core Library Modules:**

| Module | Lines | Purpose | Public API | Tests |
|--------|-------|---------|------------|-------|
| `lib.rs` | 21 | Module interface | Re-exports all public APIs | 0 |
| `parser.rs` | 431 | Source → AST | `parse()` | 22 |
| `interpreter.rs` | 484 | AST → Execution | `interpret()`, `interpret_with_config()` | 20 |
| `validator.rs` | 145 | AST validation | `validate()` | 5 |
| `minify.rs` | 75 | AST → Source | `minify()` | 5 |
| `error.rs` | 127 | Error types | `BfError`, `BfWarning` | - |
| `config.rs` | 137 | Configuration | `ExecutionConfig`, `MemoryModel` | - |
| `instruction.rs` | 11 | AST nodes | `Instruction` | - |
| `location.rs` | 35 | Source tracking | `SourceLocation` | - |
| `stats.rs` | 36 | Execution stats | `ExecutionStats` | - |

**Key Achievements:**
- ✅ **Idiomatic Rust**: lib.rs is just 21 lines - pure module interface
- ✅ **Separation of concerns**: Each module has single responsibility
- ✅ **Test co-location**: 52 tests distributed across relevant modules
- ✅ **Workspace ready**: Easy to add new crates (debugger, REPL, JIT)
- ✅ **Clean dependencies**: No circular dependencies, clear module boundaries

---

## Proposed Architecture

### High-Level Vision

```
┌─────────────────────────────────────────────────────────┐
│                    FerrousCortex                         │
│                   (Workspace Root)                       │
└─────────────────────────────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┬──────────────┐
        │                 │                 │              │
┌───────▼──────┐  ┌──────▼──────┐  ┌──────▼─────┐  ┌─────▼──────┐
│ Core Library │  │  CLI Tool   │  │  Debugger  │  │    REPL    │
│  (Library)   │  │  (Binary)   │  │  (Binary)  │  │  (Binary)  │
└──────────────┘  └─────────────┘  └────────────┘  └────────────┘
```

### Why Workspace Structure?

**Benefits:**
- **Separation of concerns**: Library vs applications
- **Independent versioning**: CLI can be v1.0, debugger v0.5
- **Faster compilation**: Only rebuild what changed
- **Easier testing**: Test library separately from binaries
- **Third-party integration**: Others can use the library

**Trade-offs:**
- Slightly more complex project structure
- Need to manage dependencies between crates
- More Cargo.toml files to maintain

**Recommendation**: ✅ **Use workspace** - benefits far outweigh costs

---

## Detailed Structure Proposal

### Directory Layout

```
FerrousCortex/
├── Cargo.toml                      # Workspace root
├── README.md
├── ARCHITECTURE.md                 # This file
├── CLAUDE.md                       # AI context
├── LICENSE
├── PRD/                            # Product requirement docs
│   ├── error-handling-and-reliability.md
│   ├── debug-symbols-and-runtime-diagnostics.md
│   └── performance-optimizations.md
│
├── crates/
│   ├── ferrous-cortex/             # 📦 Core library crate
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src/
│   │       ├── lib.rs              # Public API
│   │       ├── ast/                # Abstract Syntax Tree
│   │       │   ├── mod.rs
│   │       │   ├── instruction.rs
│   │       │   └── location.rs
│   │       ├── parser/             # Source → AST
│   │       │   ├── mod.rs
│   │       │   ├── lexer.rs
│   │       │   ├── validator.rs
│   │       │   └── minifier.rs
│   │       ├── ir/                 # Intermediate Representation
│   │       │   ├── mod.rs
│   │       │   ├── basic.rs        # Basic IR (current)
│   │       │   ├── optimized.rs    # Optimized IR (fused)
│   │       │   └── bytecode.rs     # Bytecode (future)
│   │       ├── optimizer/          # IR → Optimized IR
│   │       │   ├── mod.rs
│   │       │   ├── fusion.rs       # Instruction fusion
│   │       │   ├── patterns.rs     # Pattern matching
│   │       │   ├── loops.rs        # Loop optimization
│   │       │   └── levels.rs       # Optimization levels
│   │       ├── runtime/            # Execution engines
│   │       │   ├── mod.rs
│   │       │   ├── interpreter.rs  # Basic interpreter
│   │       │   ├── optimized.rs    # Optimized interpreter
│   │       │   ├── memory/         # Memory management
│   │       │   │   ├── mod.rs
│   │       │   │   ├── fixed.rs
│   │       │   │   ├── wrapping.rs
│   │       │   │   ├── unbounded.rs
│   │       │   │   └── lazy.rs     # Lazy allocation (future)
│   │       │   └── io/             # I/O management
│   │       │       ├── mod.rs
│   │       │       ├── buffer.rs   # Buffering strategies
│   │       │       └── eof.rs      # EOF handling
│   │       ├── debug/              # Debug symbols & diagnostics
│   │       │   ├── mod.rs
│   │       │   ├── symbols.rs      # Debug info
│   │       │   ├── trace.rs        # Execution tracing
│   │       │   └── stack.rs        # Loop call stack
│   │       ├── error/              # Error types
│   │       │   ├── mod.rs
│   │       │   ├── types.rs        # BfError enum
│   │       │   └── context.rs      # Error context generation
│   │       ├── config/             # Configuration
│   │       │   ├── mod.rs
│   │       │   ├── execution.rs    # ExecutionConfig
│   │       │   └── optimization.rs # OptimizationConfig
│   │       └── stats/              # Statistics
│   │           ├── mod.rs
│   │           └── execution.rs    # ExecutionStats
│   │
│   ├── ferrous-cortex-cli/         # 🔧 CLI binary crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── args.rs             # CLI argument parsing
│   │       └── output.rs           # Output formatting
│   │
│   ├── ferrous-cortex-debugger/    # 🐛 Visual debugger (future)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── ui/                 # TUI components
│   │       │   ├── mod.rs
│   │       │   ├── memory_view.rs
│   │       │   ├── source_view.rs
│   │       │   └── controls.rs
│   │       └── session.rs          # Debug session state
│   │
│   ├── ferrous-cortex-repl/        # 💬 REPL (future)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       └── repl.rs
│   │
│   └── ferrous-cortex-jit/         # ⚡ JIT compiler (future, maybe)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── codegen/            # Machine code generation
│           └── backend/            # LLVM or cranelift integration
│
├── examples/                        # Shared examples
│   ├── hello_world.bf
│   ├── errors/
│   └── benchmarks/
│
├── benches/                         # Criterion benchmarks
│   └── performance.rs
│
└── docs/                            # Additional documentation
    ├── API.md
    ├── PERFORMANCE.md
    └── CONTRIBUTING.md
```

---

## Core Library Design (`ferrous-cortex` crate)

### Module Organization

#### 1. AST Module (`ast/`)
**Purpose**: Define the abstract syntax tree representation

```rust
// ast/instruction.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    IncrementPointer,
    DecrementPointer,
    Increment,
    Decrement,
    Output,
    Input,
    Loop(Vec<Instruction>),
}

// ast/location.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}
```

**Why separate**: AST is the "source of truth" for program structure

#### 2. Parser Module (`parser/`)
**Purpose**: Transform source code into AST

```rust
// parser/mod.rs
pub fn parse(source: &str) -> Result<Vec<Instruction>, BfError>;
pub fn parse_with_locations(source: &str) -> Result<(Vec<Instruction>, DebugInfo), BfError>;

// parser/validator.rs
pub fn validate(instructions: &[Instruction]) -> Vec<BfWarning>;

// parser/minifier.rs
pub fn minify(instructions: &[Instruction]) -> String;
```

**Why separate**: Clear separation of parsing concerns

#### 3. IR Module (`ir/`)
**Purpose**: Multiple intermediate representations for different optimization levels

```rust
// ir/basic.rs
pub type BasicIR = Vec<Instruction>;  // Just alias the AST

// ir/optimized.rs
#[derive(Debug, Clone)]
pub enum OptimizedInstruction {
    Add(u8),
    Sub(u8),
    MoveRight(usize),
    MoveLeft(usize),
    SetZero,
    ScanRight,
    ScanLeft,
    Output,
    Input,
    Loop(Vec<OptimizedInstruction>),
}

// ir/bytecode.rs (future)
#[derive(Debug, Clone)]
pub enum Bytecode {
    AddImm8(u8),
    MoveImm16(u16),
    // ... compact representation
}
```

**Why separate**: Different IRs for different backends (interpreter, JIT, etc.)

#### 4. Optimizer Module (`optimizer/`)
**Purpose**: Transform IR to more efficient representations

```rust
// optimizer/mod.rs
pub fn optimize(
    instructions: &[Instruction],
    level: OptimizationLevel
) -> Vec<OptimizedInstruction>;

// optimizer/fusion.rs
pub fn fuse_instructions(instructions: &[Instruction]) -> Vec<OptimizedInstruction>;

// optimizer/patterns.rs
pub fn detect_patterns(instructions: &[Instruction]) -> Vec<Pattern>;

// optimizer/loops.rs
pub fn optimize_loops(instructions: &[OptimizedInstruction]) -> Vec<OptimizedInstruction>;
```

**Why separate**: Complex optimization logic needs its own space

#### 5. Runtime Module (`runtime/`)
**Purpose**: Execute IR in various ways

```rust
// runtime/mod.rs
pub trait ExecutionBackend {
    fn execute(&mut self, ir: &IR, config: &ExecutionConfig) -> Result<ExecutionStats>;
}

// runtime/interpreter.rs
pub struct BasicInterpreter;
impl ExecutionBackend for BasicInterpreter { ... }

// runtime/optimized.rs
pub struct OptimizedInterpreter;
impl ExecutionBackend for OptimizedInterpreter { ... }

// runtime/memory/mod.rs
pub trait Memory {
    fn get(&self, index: usize) -> u8;
    fn set(&mut self, index: usize, value: u8);
    fn size(&self) -> usize;
}

// runtime/io/buffer.rs
pub struct IoBuffer {
    // Handles buffering for input/output
}
```

**Why separate**: Different execution strategies, pluggable backends

#### 6. Debug Module (`debug/`)
**Purpose**: Debug symbols and runtime diagnostics

```rust
// debug/symbols.rs
pub struct DebugInfo {
    pub source: String,
    pub instruction_map: Vec<SourceLocation>,
}

// debug/trace.rs
pub struct ExecutionTracer {
    pub fn trace_step(&mut self, instr: &Instruction, state: &State);
}

// debug/stack.rs
pub struct LoopStack {
    frames: Vec<LoopFrame>,
}
```

**Why separate**: Debug features are optional, shouldn't clutter runtime

#### 7. Error Module (`error/`)
**Purpose**: Centralized error handling

```rust
// error/types.rs
#[derive(Error, Debug)]
pub enum BfError {
    UnmatchedOpenBracket { location: SourceLocation, context: String },
    UnmatchedCloseBracket { location: SourceLocation, context: String },
    MemoryOutOfBounds { /* ... */ },
    // ...
}

// error/context.rs
pub fn extract_error_context(source: &str, location: SourceLocation) -> String;
```

**Why separate**: Error types used everywhere, need central definition

#### 8. Config Module (`config/`)
**Purpose**: Configuration structs

```rust
// config/execution.rs
pub struct ExecutionConfig {
    pub memory_model: MemoryModel,
    pub max_steps: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub eof_behavior: EofBehavior,
    pub output_buffering: BufferingMode,
}

// config/optimization.rs
pub enum OptimizationLevel {
    None,
    Basic,
    Standard,
    Aggressive,
}
```

**Why separate**: Keeps configuration options organized

---

## Public API Design

### Simple API (for basic users)

```rust
// lib.rs
pub fn interpret(source: &str) -> Result<(), BfError> {
    let instructions = parser::parse(source)?;
    runtime::execute(&instructions, ExecutionConfig::default())
}

pub fn interpret_with_config(
    source: &str,
    config: ExecutionConfig
) -> Result<ExecutionStats, BfError> {
    let instructions = parser::parse(source)?;
    runtime::execute(&instructions, config)
}
```

### Advanced API (for tooling/debugger)

```rust
// lib.rs
pub fn parse(source: &str) -> Result<Program, BfError> {
    parser::parse(source)
}

pub fn optimize(program: &Program, level: OptimizationLevel) -> Program {
    optimizer::optimize(program, level)
}

pub fn validate(program: &Program) -> Vec<BfWarning> {
    parser::validate(program)
}

pub struct Executor {
    program: Program,
    config: ExecutionConfig,
}

impl Executor {
    pub fn new(program: Program, config: ExecutionConfig) -> Self;
    pub fn run(&mut self) -> Result<ExecutionStats>;
    pub fn step(&mut self) -> Result<StepResult>;  // For debugger
    pub fn get_state(&self) -> &ExecutionState;     // For debugger
}
```

### Debugger-Specific API

```rust
// debug/mod.rs
pub struct Debugger {
    executor: Executor,
    breakpoints: Vec<Breakpoint>,
    watchpoints: Vec<Watchpoint>,
}

impl Debugger {
    pub fn new(program: Program) -> Self;

    // Execution control
    pub fn step(&mut self) -> Result<StepResult>;
    pub fn step_over(&mut self) -> Result<()>;  // Skip loop internals
    pub fn continue_execution(&mut self) -> Result<()>;
    pub fn run_until(&mut self, location: SourceLocation) -> Result<()>;

    // Breakpoints
    pub fn add_breakpoint(&mut self, location: SourceLocation);
    pub fn remove_breakpoint(&mut self, location: SourceLocation);

    // Inspection
    pub fn inspect_memory(&self, range: Range<usize>) -> &[u8];
    pub fn get_pointer(&self) -> usize;
    pub fn get_call_stack(&self) -> &[LoopFrame];
    pub fn get_source_location(&self) -> SourceLocation;
}
```

---

## CLI Binary Design (`ferrous-cortex-cli`)

### Clean Separation

```rust
// crates/ferrous-cortex-cli/src/main.rs
use ferrous_cortex::{interpret_with_config, ExecutionConfig};

fn main() {
    let args = Args::parse();
    let config = build_config(&args);

    match interpret_with_config(&source, config) {
        Ok(stats) => {
            if args.verbose {
                print_stats(&stats);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
```

**Benefits:**
- CLI is just a thin wrapper
- Core logic tested in library
- Easy to create alternative CLIs

---

## Migration Strategy

### Phase 1: Create Workspace Structure (Week 1)

**Step 1**: Create workspace
```bash
# Create workspace Cargo.toml
# Move current code to crates/ferrous-cortex/
# Move main.rs to crates/ferrous-cortex-cli/
```

**Step 2**: Split `bf.rs` into modules (in-place)
```rust
// Keep everything in ferrous-cortex crate first
// Just split the file into modules:
src/
  ├── lib.rs
  ├── ast.rs
  ├── parser.rs
  ├── interpreter.rs
  ├── memory.rs
  ├── error.rs
  └── config.rs
```

**Step 3**: Update imports and make it work
- Update CLI to use `ferrous_cortex::*`
- Ensure all tests pass
- Update documentation

### Phase 2: Refine Module Structure (Week 2)

**Step 4**: Create proper module directories
```rust
// Expand flat modules into directories:
src/
  ├── ast/
  │   ├── mod.rs
  │   └── instruction.rs
  ├── parser/
  │   ├── mod.rs
  │   └── validator.rs
  // etc.
```

**Step 5**: Design public API
- Define what's `pub` vs `pub(crate)`
- Write `lib.rs` with clear exports
- Document public API

### Phase 3: Future Features (As needed)

**When adding optimizer**:
```bash
# Add optimizer module
crates/ferrous-cortex/src/optimizer/
# Implement IR optimization
# Add --opt flag to CLI
```

**When adding debugger**:
```bash
# Create new crate
crates/ferrous-cortex-debugger/
# Depends on ferrous-cortex library
# Implements TUI using ratatui or similar
```

**When adding JIT**:
```bash
# Create new crate (maybe)
crates/ferrous-cortex-jit/
# Heavy dependencies (LLVM, cranelift)
# Keep separate to not bloat main library
```

---

## Testing Strategy

### Unit Tests

```rust
// In each module
#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_simple() { ... }

    #[test]
    fn test_optimize_fusion() { ... }
}
```

### Integration Tests

```rust
// tests/integration_test.rs
#[test]
fn test_hello_world_end_to_end() {
    let source = include_str!("../examples/hello_world.bf");
    let output = ferrous_cortex::interpret(source).unwrap();
    assert_eq!(output, "Hello World!\n");
}
```

### Benchmarks

```rust
// benches/performance.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_interpreter(c: &mut Criterion) {
    c.bench_function("hello_world", |b| {
        b.iter(|| {
            ferrous_cortex::interpret(black_box(HELLO_WORLD))
        });
    });
}

criterion_group!(benches, bench_interpreter);
criterion_main!(benches);
```

---

## Workspace Cargo.toml

```toml
[workspace]
members = [
    "crates/ferrous-cortex",
    "crates/ferrous-cortex-cli",
    # Future members:
    # "crates/ferrous-cortex-debugger",
    # "crates/ferrous-cortex-repl",
    # "crates/ferrous-cortex-jit",
]
resolver = "2"

[workspace.package]
version = "0.2.0"
authors = ["Your Name <your.email@example.com>"]
edition = "2024"
license = "MIT OR Apache-2.0"
repository = "https://github.com/yourusername/FerrousCortex"

[workspace.dependencies]
# Shared dependencies
thiserror = "2.0"
clap = { version = "4.5", features = ["derive"] }

# Internal dependencies
ferrous-cortex = { path = "crates/ferrous-cortex" }

[profile.release]
lto = true
codegen-units = 1
```

---

## Questions to Consider

### 1. Should JIT be a separate crate?

**Pros:**
- Heavy dependencies (LLVM, cranelift)
- Optional feature
- Faster compile times if not using JIT

**Cons:**
- More crates to manage
- Slightly more complex

**Recommendation**: ✅ Separate crate (optional dependency)

### 2. Should we use traits for backends?

```rust
pub trait ExecutionBackend {
    fn execute(&mut self, ir: &IR, config: &Config) -> Result<Stats>;
}
```

**Pros:**
- Pluggable backends
- Easy to add new execution strategies
- Testability

**Cons:**
- Slight performance overhead (dynamic dispatch)
- More complex API

**Recommendation**: ✅ Use traits (flexibility > tiny perf cost)

### 3. Should debugger be in workspace or separate repo?

**In workspace:**
- Easier to keep in sync
- Shared CI/CD
- Single clone for development

**Separate repo:**
- Independent release cycle
- Different team could maintain
- Smaller main repo

**Recommendation**: ✅ In workspace initially, can split later if needed

### 4. How much should we expose in public API?

**Options:**
1. Minimal: Just `interpret()` and config
2. Moderate: Parser, optimizer, executor separately
3. Maximal: Everything public

**Recommendation**: ✅ Moderate - expose building blocks but not internals

---

## Benefits of This Structure

### 1. Scalability
- Easy to add new features (JIT, debugger, REPL)
- Modules can grow independently
- Clear boundaries between components

### 2. Maintainability
- Each module has single responsibility
- Easy to find code
- Changes are localized

### 3. Testability
- Test modules in isolation
- Mock backends for testing
- Integration tests are clean

### 4. Reusability
- Library can be used by other Rust projects
- Debugger can reuse core library
- REPL can reuse parser and runtime

### 5. Performance
- Can swap backends without changing API
- Optimization is opt-in
- Zero-cost abstractions

### 6. Developer Experience
- Clear imports: `use ferrous_cortex::parser::parse;`
- Good IDE support
- Easy to onboard new contributors

---

## Timeline Estimate

### Immediate (Next 1-2 weeks)
- **Phase 1**: Workspace setup and basic module split
- Keep all functionality working
- Update documentation

### Short-term (1-2 months)
- **Phase 2**: Refined module structure
- Public API design
- Start optimizer implementation

### Medium-term (3-6 months)
- Performance optimizations (IR, fusion, buffering)
- Debug symbols implementation
- Start debugger planning

### Long-term (6-12 months)
- Visual debugger
- REPL
- JIT compiler exploration

---

## Next Steps

### Option A: Aggressive Refactor
1. Create workspace structure now
2. Split modules immediately
3. Get it working in new structure
4. Then add new features

**Pros**: Clean slate, better foundation
**Cons**: Disrupts current work, risky

### Option B: Gradual Migration
1. Continue in current structure for now
2. Finish error handling PRD completely
3. Create workspace when starting optimizer work
4. Migrate incrementally

**Pros**: Less disruptive, proven code before restructure
**Cons**: Delay benefits of better structure

### Option C: Hybrid Approach (Recommended)
1. Create workspace structure now (low risk)
2. Move current code as-is to `crates/` (preserve structure)
3. Split modules gradually as we work on new features
4. Each new feature gets proper module structure

**Pros**: Best of both worlds
**Cons**: Temporary inconsistency

---

## Recommendation

**Start with Option C (Hybrid)**:

1. **This week**: Create workspace, move code to crates (no refactoring)
2. **Next 2 weeks**: Split `bf.rs` into basic modules as we finish error examples
3. **Following month**: Implement optimizer with proper module structure
4. **After that**: Each new PRD gets implemented with clean architecture

This gives us the benefits of workspace structure immediately while allowing gradual, safe refactoring.

What do you think? Should we start the workspace migration now, or finish the current PRD first?
