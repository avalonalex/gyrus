# PRD: Performance Optimizations

## Overview

Optimize the BrainFuck interpreter's runtime performance through instruction fusion, I/O buffering, memory management improvements, and execution strategies. Target 10-100x speedup for typical programs while maintaining correctness and compatibility.

## Motivation

### Current Performance Characteristics

**Strengths:**
- Correct implementation of BrainFuck semantics
- Good error handling and diagnostics
- Flexible memory models

**Performance Bottlenecks:**
1. **Naive instruction execution**: Each `+` is a separate operation (no batching)
2. **Unbuffered I/O**: Each `.` flushes immediately (syscall overhead)
3. **Interpreted loops**: No loop analysis or optimization
4. **Memory inefficiency**: Large memory allocations even for simple programs
5. **No instruction rewriting**: Missed optimization opportunities

### Performance Goals

| Benchmark | Current | Target | Speedup |
|-----------|---------|--------|---------|
| Mandelbrot set | ~5.0s | ~0.5s | 10x |
| Hello World | ~1ms | ~0.5ms | 2x |
| Fibonacci | ~100ms | ~10ms | 10x |
| Large I/O | ~50ms | ~5ms | 10x |

**Note**: Benchmarks TBD - these are illustrative targets

---

## Phase 1: Instruction Fusion and Optimization

### Problem

Current IR (Intermediate Representation) is too granular:

```brainfuck
++++++     * Increment 6 times
>>>>>>     * Move right 6 times
------     * Decrement 6 times
```

Parsed as:
```rust
vec![
    Increment, Increment, Increment, Increment, Increment, Increment,
    IncrementPointer, IncrementPointer, IncrementPointer, IncrementPointer, IncrementPointer, IncrementPointer,
    Decrement, Decrement, Decrement, Decrement, Decrement, Decrement,
]
```

This requires 18 instruction dispatches, bounds checks, and operations.

### Solution: Optimized IR

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum OptimizedInstruction {
    // Fused arithmetic
    Add(u8),           // Add n to current cell (wrapping)
    Sub(u8),           // Subtract n from current cell

    // Fused pointer movement
    MoveRight(usize),  // Move pointer right by n
    MoveLeft(usize),   // Move pointer left by n

    // Original operations
    Output,
    Input,

    // Optimized loops
    Loop(Vec<OptimizedInstruction>),

    // Special loop patterns
    SetZero,           // [-] or [+] - set current cell to 0
    ScanRight,         // [>] - scan right until zero cell
    ScanLeft,          // [<] - scan left until zero cell

    // Advanced optimizations
    AddMove { amount: i32, offset: isize },  // Add to cell at offset
    MultiplyMove { offset: isize },          // Multiply-accumulate pattern
}
```

**Example transformation**:
```
Input:  ++++++>>>>>>------
Naive:  18 instructions
Fused:  Add(6), MoveRight(6), Sub(6)
Result: 3 instructions (6x reduction)
```

### Implementation: Optimization Pass

```rust
pub fn optimize(instructions: &[Instruction]) -> Vec<OptimizedInstruction> {
    let mut optimized = Vec::new();
    let mut i = 0;

    while i < instructions.len() {
        match &instructions[i] {
            Instruction::Increment => {
                // Count consecutive increments
                let mut count = 0;
                while i < instructions.len() && instructions[i] == Instruction::Increment {
                    count += 1;
                    i += 1;
                }
                optimized.push(OptimizedInstruction::Add((count % 256) as u8));
            }

            Instruction::IncrementPointer => {
                // Count consecutive pointer movements
                let mut count = 0;
                while i < instructions.len() && instructions[i] == Instruction::IncrementPointer {
                    count += 1;
                    i += 1;
                }
                optimized.push(OptimizedInstruction::MoveRight(count));
            }

            Instruction::Loop(body) => {
                // Check for special loop patterns
                if is_clear_loop(body) {
                    optimized.push(OptimizedInstruction::SetZero);
                } else if is_scan_right(body) {
                    optimized.push(OptimizedInstruction::ScanRight);
                } else if is_scan_left(body) {
                    optimized.push(OptimizedInstruction::ScanLeft);
                } else {
                    // Recursively optimize loop body
                    optimized.push(OptimizedInstruction::Loop(optimize(body)));
                }
                i += 1;
            }

            // ... other instructions
        }
    }

    optimized
}

// Pattern recognition helpers
fn is_clear_loop(body: &[Instruction]) -> bool {
    // Matches [-] or [+]
    body.len() == 1 &&
        (body[0] == Instruction::Decrement || body[0] == Instruction::Increment)
}

fn is_scan_right(body: &[Instruction]) -> bool {
    // Matches [>]
    body.len() == 1 && body[0] == Instruction::IncrementPointer
}

fn is_scan_left(body: &[Instruction]) -> bool {
    // Matches [<]
    body.len() == 1 && body[0] == Instruction::DecrementPointer
}
```

### Advanced Pattern Recognition

**Multiply-move pattern**: `[->++<]`
- Decrements current cell
- Adds 2× to cell at offset +1
- Effectively: `cell[1] += cell[0] * 2; cell[0] = 0;`

```rust
fn detect_multiply_move(body: &[Instruction]) -> Option<OptimizedInstruction> {
    // Pattern: [-XXX<] or [+XXX<] where XXX are pointer/increment operations
    // Common pattern: [->+++<] moves cell value * 3 to cell+1

    // Implementation TBD - complex pattern matching
}
```

### Optimization Levels

```rust
pub enum OptimizationLevel {
    None,      // No optimization (for debugging)
    Basic,     // Instruction fusion only
    Standard,  // Basic + clear loops + scans
    Aggressive, // Standard + multiply-move patterns
}
```

### CLI Integration

```bash
# Optimization flags
cargo run -- program.bf --opt none          # No optimization
cargo run -- program.bf --opt basic         # Default
cargo run -- program.bf --opt standard      # Recommended
cargo run -- program.bf --opt aggressive    # Experimental
```

---

## Phase 2: I/O Buffering

### Problem

Current I/O behavior:
```rust
Instruction::Output => {
    print!("{}", memory[pointer] as char);
    // Implicit flush on each character!
}
```

Every `.` command causes:
1. System call to write 1 byte
2. Kernel context switch
3. Terminal/file update

For programs that output 1000 characters: 1000 syscalls!

### Solution: Buffered Output

```rust
pub struct IoBuffer {
    output_buffer: Vec<u8>,
    buffer_size: usize,
    auto_flush: bool,
}

impl IoBuffer {
    pub fn new(buffer_size: usize, auto_flush: bool) -> Self {
        Self {
            output_buffer: Vec::with_capacity(buffer_size),
            buffer_size,
            auto_flush,
        }
    }

    pub fn write(&mut self, byte: u8) -> std::io::Result<()> {
        self.output_buffer.push(byte);

        if self.output_buffer.len() >= self.buffer_size {
            self.flush()?;
        }

        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.output_buffer.is_empty() {
            std::io::stdout().write_all(&self.output_buffer)?;
            std::io::stdout().flush()?;
            self.output_buffer.clear();
        }
        Ok(())
    }
}
```

### Buffering Strategies

**1. Line Buffered (Default)**
- Buffer until newline (`\n`) encountered
- Good for text output
- Feels "real-time" to users

**2. Block Buffered**
- Buffer until N bytes accumulated
- Best performance
- May delay output visibility

**3. Unbuffered (Current behavior)**
- Flush after every character
- Worst performance, best interactivity
- Good for debugging/interactive programs

**4. Smart Buffering**
- Auto-detect: unbuffered if TTY, buffered if file/pipe
- Best of both worlds

### Configuration

```rust
pub enum BufferingMode {
    None,           // Unbuffered (flush every char)
    Line,           // Flush on newline (default)
    Block(usize),   // Flush every N bytes
    Auto,           // Smart detection based on output target
}

pub struct ExecutionConfig {
    // ... existing fields ...
    pub output_buffering: BufferingMode,
    pub input_buffering: bool,  // Read input in chunks
}
```

### CLI Integration

```bash
# Buffering flags
cargo run -- program.bf --buffer none      # Unbuffered (current)
cargo run -- program.bf --buffer line      # Line buffered (default)
cargo run -- program.bf --buffer block     # Block buffered (fastest)
cargo run -- program.bf --buffer auto      # Smart detection
```

### Input Buffering

Similarly, buffer input reads:

```rust
pub struct IoBuffer {
    // ... output fields ...
    input_buffer: VecDeque<u8>,
}

impl IoBuffer {
    pub fn read(&mut self) -> std::io::Result<u8> {
        if self.input_buffer.is_empty() {
            self.refill_input_buffer()?;
        }

        self.input_buffer.pop_front()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "End of input"
            ))
    }

    fn refill_input_buffer(&mut self) -> std::io::Result<()> {
        let mut buf = [0u8; 4096];
        let n = std::io::stdin().read(&mut buf)?;
        self.input_buffer.extend(&buf[..n]);
        Ok(())
    }
}
```

---

## Phase 3: Memory Optimizations

### 3.1 Lazy Memory Allocation

**Problem**: Allocating 30,000 bytes for simple programs

**Current**:
```rust
let memory = vec![0u8; 30000];  // Always allocate full size
```

**Optimized**:
```rust
pub struct LazyMemory {
    chunks: HashMap<usize, Box<[u8; CHUNK_SIZE]>>,
    default_chunk: Box<[u8; CHUNK_SIZE]>,
}

const CHUNK_SIZE: usize = 1024;

impl LazyMemory {
    pub fn get(&self, index: usize) -> u8 {
        let chunk_idx = index / CHUNK_SIZE;
        let offset = index % CHUNK_SIZE;

        self.chunks
            .get(&chunk_idx)
            .map(|chunk| chunk[offset])
            .unwrap_or(0)
    }

    pub fn set(&mut self, index: usize, value: u8) {
        let chunk_idx = index / CHUNK_SIZE;
        let offset = index % CHUNK_SIZE;

        self.chunks
            .entry(chunk_idx)
            .or_insert_with(|| Box::new([0u8; CHUNK_SIZE]))
            [offset] = value;
    }
}
```

**Benefits**:
- Only allocate chunks that are actually used
- Better cache locality
- Reduced memory footprint for simple programs

### 3.2 Zero Cell Optimization

Track which cells are known to be zero:

```rust
pub struct OptimizedMemory {
    memory: Vec<u8>,
    zero_cells: BitVec,  // Track known-zero cells
}

impl OptimizedMemory {
    pub fn set(&mut self, index: usize, value: u8) {
        self.memory[index] = value;
        self.zero_cells.set(index, value == 0);
    }

    pub fn is_zero(&self, index: usize) -> bool {
        self.zero_cells[index]
    }
}
```

**Usage**: Skip loop iterations if cell is known zero:
```rust
Instruction::Loop(body) => {
    if memory.is_zero(pointer) {
        // Skip loop entirely
        continue;
    }
    // ... execute loop
}
```

### 3.3 Memory Access Patterns

Track hot/cold memory regions:

```rust
pub struct MemoryStats {
    access_count: Vec<u64>,  // Access frequency per cell
    hot_threshold: u64,
}

impl MemoryStats {
    pub fn record_access(&mut self, index: usize) {
        self.access_count[index] += 1;
    }

    pub fn get_hot_regions(&self) -> Vec<(usize, usize)> {
        // Return ranges of frequently-accessed memory
        // Useful for profiling and optimization
    }
}
```

---

## Phase 4: Loop Optimizations

### 4.1 Loop Unrolling

**Pattern**: Loops with constant iteration count

```brainfuck
+++[>++<-]  * Runs exactly 3 times
```

**Optimization**: Unroll to straight-line code:
```rust
// Before: Loop(3 iterations)
// After:
>++<  // Iteration 1
>++<  // Iteration 2
>++<  // Iteration 3
```

**Detection**:
```rust
fn can_unroll(loop_start_value: Option<u8>) -> bool {
    // Only unroll if:
    // 1. We know the starting cell value
    // 2. Loop body doesn't modify the loop counter unpredictably
    // 3. Iteration count is small (< 10)
}
```

### 4.2 Loop Invariant Code Motion

Move calculations outside loops:

```brainfuck
+++[>++++<-]  * Adds 12 to cell[1], clears cell[0]
```

Can be optimized to:
```rust
cell[1] += cell[0] * 4;
cell[0] = 0;
```

### 4.3 Dead Loop Elimination

Remove loops that have no effect:

```brainfuck
+++
[-]    * Clear cell (useful)
[+]    * Infinite loop on non-zero (validator already catches this)
[]     * Empty loop (validator already catches this)
```

---

## Phase 5: Execution Strategies

### 5.1 Interpretation Modes

**Naive Interpreter** (current):
```rust
fn execute_naive(instr: &Instruction) {
    match instr {
        Instruction::Increment => memory[ptr] = memory[ptr].wrapping_add(1),
        // ... direct interpretation
    }
}
```

**Optimized Interpreter**:
```rust
fn execute_optimized(instr: &OptimizedInstruction) {
    match instr {
        OptimizedInstruction::Add(n) => memory[ptr] = memory[ptr].wrapping_add(*n),
        OptimizedInstruction::SetZero => memory[ptr] = 0,
        OptimizedInstruction::ScanRight => {
            while memory[ptr] != 0 { ptr += 1; }
        }
        // ... optimized patterns
    }
}
```

**Bytecode Interpreter** (future):
```rust
// Compile to bytecode, then interpret bytecode
// Even faster dispatch, better for hot loops
enum Bytecode {
    AddImm8(u8),
    MoveImm16(u16),
    // ... compact representation
}
```

### 5.2 Configuration

```rust
pub enum ExecutionMode {
    Naive,      // Simple, easy to debug
    Optimized,  // With instruction fusion
    Bytecode,   // Compiled to bytecode (future)
    JIT,        // Just-in-time compilation (future)
}

impl ExecutionConfig {
    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }
}
```

---

## Phase 6: Benchmarking and Profiling

### 6.1 Built-in Benchmarks

```rust
// Add benchmark suite
#[cfg(test)]
mod benchmarks {
    use criterion::{black_box, criterion_group, criterion_main, Criterion};

    fn bench_hello_world(c: &mut Criterion) {
        let source = include_str!("../examples/hello_world.bf");
        let instructions = parse(source).unwrap();

        c.bench_function("hello_world", |b| {
            b.iter(|| {
                interpret(black_box(&instructions)).unwrap();
            });
        });
    }

    criterion_group!(benches, bench_hello_world);
    criterion_main!(benches);
}
```

### 6.2 Profiling Mode

```bash
# Profile execution
cargo run -- program.bf --profile

# Output:
Execution Profile:
  Total time: 1.23s
  Instructions executed: 10,450,234
  Hot loops (>50% time):
    - Loop at line 45: 650ms (52.8%)
    - Loop at line 120: 180ms (14.6%)
  Memory access:
    - Hot region: cells 0-10 (98.2% of accesses)
    - Peak usage: 45 cells
  I/O:
    - Output: 1,250 chars (12ms)
    - Input: 0 chars
```

### 6.3 Optimization Comparison

```bash
# Compare optimization levels
cargo run -- program.bf --opt-compare

# Output:
Optimization Comparison for program.bf:
  None:       5.23s  (baseline)
  Basic:      0.82s  (6.4x speedup)
  Standard:   0.51s  (10.3x speedup) ← recommended
  Aggressive: 0.48s  (10.9x speedup)

Recommendation: Use --opt standard
```

---

## Implementation Plan

### Phase 1: Instruction Fusion (2 weeks)
1. **Week 1**: Design `OptimizedInstruction` enum
   - Implement fusion for `+`, `-`, `>`, `<`
   - Add basic pattern detection (clear loops, scans)
   - Write optimization pass
   - Unit tests for fusion

2. **Week 2**: Optimized interpreter
   - Implement execution for fused instructions
   - Add `--opt` flag
   - Benchmark and measure speedup
   - Integration tests

### Phase 2: I/O Buffering (1 week)
1. Design `IoBuffer` with multiple strategies
2. Implement line/block/auto buffering
3. Add `--buffer` flag
4. Benchmark I/O-heavy programs
5. Documentation

### Phase 3: Memory Optimizations (1-2 weeks)
1. Implement lazy memory allocation
2. Add zero-cell tracking
3. Benchmark memory usage
4. Optional: Memory access profiling

### Phase 4: Loop Optimizations (2 weeks)
1. Advanced pattern detection (multiply-move)
2. Loop unrolling for constant iterations
3. Dead loop elimination
4. Benchmarking

### Phase 5: Profiling and Benchmarks (1 week)
1. Add criterion benchmarks
2. Implement `--profile` mode
3. Create benchmark suite
4. Document performance characteristics

---

## Success Metrics

### Performance Targets

| Metric | Baseline | Target | Measurement |
|--------|----------|--------|-------------|
| Hello World | 1.0ms | 0.5ms | Criterion |
| Mandelbrot | 5.0s | 0.5s | Wall-clock |
| Memory usage | 30KB | 5KB | Process RSS |
| I/O throughput | 50 chars/ms | 500 chars/ms | Synthetic benchmark |

### Code Quality

- Zero performance regressions
- All existing tests pass
- Correctness maintained (output identical)
- Optimization can be disabled
- Memory safety preserved

### Usability

- Clear CLI flags for optimization levels
- Profiling output is actionable
- Documentation explains tradeoffs
- Benchmarks run in CI

---

## Risks and Mitigations

### Risk 1: Optimization Bugs

**Risk**: Optimizations may produce incorrect results

**Mitigation**:
- Extensive test suite with known outputs
- Fuzzing with random programs
- Compare optimized vs naive output
- Add `--verify` flag to double-check

### Risk 2: Diminishing Returns

**Risk**: Complex optimizations may not provide significant speedup

**Mitigation**:
- Profile first, optimize second
- Benchmark each optimization separately
- Document actual speedup achieved
- Only implement if >2x improvement

### Risk 3: Code Complexity

**Risk**: Optimization code is harder to maintain

**Mitigation**:
- Keep optimization passes separate
- Good documentation and tests
- Optimization is optional (can disable)
- Clear separation of concerns

---

## Future Work (Out of Scope)

### JIT Compilation
- Compile hot loops to machine code
- Use LLVM or cranelift backend
- 100-1000x speedup potential
- Separate PRD needed

### Parallelization
- Multi-threaded execution (if semantics allow)
- SIMD instructions for cell operations
- GPU acceleration for specific patterns

### Ahead-of-Time Compilation
- Compile BF to native executable
- Full optimization pipeline
- Distribution as binary

### Profile-Guided Optimization
- Run with profiling, recompile with hot path info
- Adaptive optimization based on actual usage

---

## Testing Strategy

### Correctness Tests

```rust
#[test]
fn test_optimization_equivalence() {
    let programs = [
        "+++>>>---",
        "[-]",
        "[>]",
        "++[>++<-]",
        // ... many more
    ];

    for program in programs {
        let naive_output = execute_naive(program);
        let optimized_output = execute_optimized(program);
        assert_eq!(naive_output, optimized_output);
    }
}
```

### Performance Tests

```rust
#[test]
fn test_fusion_reduces_instructions() {
    let program = "++++++++";  // 8 increments
    let naive = parse(program).len();
    let optimized = optimize(&parse(program)).len();

    assert_eq!(naive, 8);
    assert_eq!(optimized, 1);  // Fused to Add(8)
}
```

### Regression Tests

```rust
// Ensure optimizations don't break existing programs
#[test]
fn test_hello_world_optimization() {
    let source = include_str!("../examples/hello_world.bf");
    let output = execute_optimized(source);
    assert_eq!(output, "Hello World!\n");
}
```

---

## Documentation Updates

1. **README.md**:
   - Add "Performance" section
   - Document optimization flags
   - Show benchmark results

2. **examples/benchmarks/**:
   - Add benchmark programs
   - Document expected performance

3. **CLAUDE.md**:
   - Explain optimization architecture
   - Document pattern detection
   - IR transformation details

4. **Performance Guide**:
   - When to use which optimization level
   - Profiling and tuning guide
   - Known performance characteristics

---

## References

- **Instruction Fusion**: Common compiler optimization
- **BF Optimization**: https://github.com/rdebath/Brainfuck/blob/master/doc/Optimise.md
- **I/O Buffering**: libc buffering strategies
- **Loop Optimization**: Compiler optimization techniques (LLVM, GCC)
- **Benchmarking**: Criterion.rs for Rust benchmarking

---

## Appendix: Common BF Patterns

### Pattern 1: Clear Cell
```brainfuck
[-]    or    [+]
```
Optimized to: `SetZero`

### Pattern 2: Scan for Zero
```brainfuck
[>]    or    [<]
```
Optimized to: `ScanRight` or `ScanLeft`

### Pattern 3: Multiply-Move
```brainfuck
[->+++<]     * cell[1] = cell[0] * 3; cell[0] = 0
[->+>++<<]   * cell[1] = cell[0]; cell[2] = cell[0] * 2; cell[0] = 0
```
Optimized to: `MultiplyMove { amounts: [(1, 3)] }`

### Pattern 4: Copy Cell
```brainfuck
[->+>+<<]    * Copy cell[0] to cell[1] and cell[2]
[>>+<<-]     * Move cell[0] to cell[2]
```
Optimized to: `Copy` or `Move` instructions

### Pattern 5: If-Then (Zero Check)
```brainfuck
>+<[>-<[-]]  * If cell[0] != 0, set cell[1] = 1
```
Optimized to: `ConditionalSet`

These patterns appear frequently in real BF programs and have well-known optimal implementations.
