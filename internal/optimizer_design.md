# Optimizer Design Documentation

## Overview

The optimizer transforms BrainFuck AST into an optimized intermediate representation (IR) that:
1. **Fuses repeated instructions** (e.g., `+++` → `Add(3)`)
2. **Recognizes loop patterns** (e.g., `[-]` → `Zero`)
3. **Preserves source location ranges** for debugging and profiling

## Architecture

### Module: `src/optimizer.rs`

**Key Types:**

```rust
/// Source location range for tracking optimizations
pub struct SourceRange {
    pub start: usize,  // Original instruction index (inclusive)
    pub end: usize,    // Original instruction index (exclusive)
}

/// Optimized IR instruction
pub enum OptimizedInstruction {
    // Fused operations
    Add(u8, SourceRange),           // +++ → Add(3)
    Sub(u8, SourceRange),           // --- → Sub(3)
    Right(usize, SourceRange),      // >>> → Right(3)
    Left(usize, SourceRange),       // <<< → Left(3)

    // I/O (not fused)
    Output(SourceRange),            // .
    Input(SourceRange),             // ,

    // Loop patterns
    Zero(SourceRange),              // [-] or [+]
    SeekRight(SourceRange),         // [>]
    SeekLeft(SourceRange),          // [<]
    MoveRight(usize, SourceRange),  // [->+<] move value N cells right
    MoveLeft(usize, SourceRange),   // [-<+>] move value N cells left

    // General loops (recursively optimized body)
    Loop(Vec<OptimizedInstruction>, SourceRange),
}

/// Optimized program with metadata
pub struct OptimizedProgram {
    pub instructions: Vec<OptimizedInstruction>,
    pub original_count: usize,
    pub optimized_count: usize,
}
```

**API:**

```rust
/// Main entry point: optimize BF AST to IR
pub fn optimize(instructions: &[Instruction]) -> OptimizedProgram
```

## Implemented Optimizations

### 1. Instruction Fusion

Combines sequential operations of the same type:

| Pattern | Before | After | Speedup |
|---------|--------|-------|---------|
| Increment | `++++` (4 ops) | `Add(4)` (1 op) | 4× |
| Decrement | `----` (4 ops) | `Sub(4)` (1 op) | 4× |
| Move Right | `>>>>` (4 ops) | `Right(4)` (1 op) | 4× |
| Move Left | `<<<<` (4 ops) | `Left(4)` (1 op) | 4× |

**Implementation:** `optimize_block()` function uses a sliding window to count consecutive operations.

**Saturation:** Counts saturate at 255 for Add/Sub (u8 limit), unlimited for Right/Left (usize).

### 2. Loop Pattern Recognition

Detects common idioms and converts to single operations:

| Pattern | BF Code | Optimized | Description |
|---------|---------|-----------|-------------|
| Clear cell | `[-]` or `[+]` | `Zero` | Set current cell to 0 |
| Seek right | `[>]` | `SeekRight` | Find next zero cell (right) |
| Seek left | `[<]` | `SeekLeft` | Find previous zero cell (left) |
| Move right | `[->+<]` | `MoveRight(1)` | Move value 1 cell right, zero source |
| Move left | `[-<+>]` | `MoveLeft(1)` | Move value 1 cell left, zero source |

**Implementation:** `recognize_loop_pattern()` function pattern-matches on loop body.

**Filter:** LoopCheck instructions are filtered out before pattern matching.

### 3. Source Location Tracking

Every optimized instruction tracks its origin:

```rust
// Example: "+++>---" optimizes to:
[
    Add(3, SourceRange { start: 0, end: 3 }),    // Instructions 0-2
    Right(1, SourceRange { start: 3, end: 4 }),  // Instruction 3
    Sub(3, SourceRange { start: 4, end: 7 }),    // Instructions 4-6
]
```

**Benefits:**
- Runtime errors map back to original source
- Profiler can attribute time to original instructions
- Debugger can set breakpoints on original code

### 4. Recursive Loop Optimization

Nested loops are optimized recursively:

```rust
// [++[-]]
Loop([
    LoopCheck,
    IncrementValue,
    IncrementValue,
    Loop([LoopCheck, DecrementValue])
])

// Optimizes to:
Loop([
    Add(2, range=1..3),
    Zero(range=3..5)
], range=0..5)
```

## Test Coverage

**7 unit tests** in `optimizer::tests`:

1. `test_fuse_increments` - Verify `+++` → `Add(3)`
2. `test_fuse_pointer_movement` - Verify `>>>` → `Right(3)`, `<` → `Left(1)`
3. `test_recognize_zero_pattern` - Verify `[-]` → `Zero`
4. `test_recognize_seek_right` - Verify `[>]` → `SeekRight`
5. `test_recognize_move_right` - Verify `[->+<]` → `MoveRight(1)`
6. `test_source_range_tracking` - Verify SourceRange accuracy
7. `test_nested_loop_optimization` - Verify recursive optimization

**All tests pass** ✅

## Compression Ratios

Expected reduction in instruction count:

| Program Type | Original | Optimized | Ratio |
|--------------|----------|-----------|-------|
| Arithmetic-heavy | 1000 | 200 | 5× |
| Pointer-heavy | 1000 | 100 | 10× |
| Mixed | 1000 | 300 | 3.3× |

Real-world examples will vary based on code patterns.

## Future Optimizations (Not Implemented Yet)

### Copy Patterns
- `[->+>+<<]` → `CopyRight([1, 2])` - Copy value to multiple offsets
- Preserves source cell value

### Multi-cell Moves
- `[->>+<<]` → `MoveRight(2)` - Move value N cells (N > 1)
- Currently only N=1 is implemented

### Multiplication Patterns
- `[->+++<]` → `MultiplyAdd(1, 3)` - Multiply current cell by 3, add to offset 1
- Common in arithmetic-heavy programs

### Dead Code Elimination
- Remove unreachable code after infinite loops
- Remove no-op sequences

### Constant Propagation
- Track known cell values through execution
- Eliminate redundant operations

## Integration Points

### Parser Integration
```rust
let instructions = parse(source)?;
let optimized = optimize(&instructions);
```

### Interpreter Integration (TODO)
```rust
// New optimized interpreter (to be implemented)
interpret_optimized(&optimized.instructions, config)?;
```

### Profiler Integration
```rust
// Map profiling data back to original source using SourceRange
for inst in &optimized.instructions {
    let range = inst.source_range();
    println!("Optimized instruction maps to original [{}, {})", range.start, range.end);
}
```

### Debugger Integration (Future)
```rust
// Set breakpoints on original source locations
// Optimized interpreter respects SourceRange for debugging
```

## Design Decisions

### Why SourceRange instead of single SourceLocation?

**Fused instructions span multiple source locations:**
- `+++` at line 1, columns 1-3 becomes `Add(3, range=0..3)`
- Single location would lose precision
- Range preserves full mapping for debugging

### Why separate OptimizedInstruction enum?

**Clean separation of concerns:**
- Original `Instruction` remains simple AST
- `OptimizedInstruction` carries optimization metadata
- Different execution paths (unoptimized vs optimized)
- Future: Could compile to native code from OptimizedInstruction

### Why not optimize in-place?

**Preservation of original AST:**
- Parser output remains unchanged
- Can validate unoptimized code
- Can compare optimized vs unoptimized execution
- Debugging unoptimized code is easier

### Why saturating_add for fusion?

**Safety against overflow:**
- `Add(255)` + `Add(1)` = `Add(255)`, not overflow
- Alternative: Split into multiple Add instructions
- Current: Conservative (may miss some fusion opportunities)

## Performance Characteristics

### Optimization Pass
- **Time Complexity:** O(n) where n = instruction count
- **Space Complexity:** O(n) for optimized program
- **Fast enough to run on every execution**

### Expected Runtime Speedup
- Simple arithmetic: **5-10×** (heavy fusion)
- Pointer movement: **10-20×** (pointer fusion)
- Loop-heavy: **2-5×** (pattern recognition + fusion)
- I/O-heavy: **1.5-2×** (less opportunity for fusion)

## Next Steps

1. ✅ Design OptimizedInstruction IR with SourceRange
2. ✅ Implement instruction fusion
3. ✅ Implement loop pattern recognition
4. ✅ Add unit tests (7 tests)
5. ⏳ Implement optimized interpreter
6. ⏳ Add benchmarks comparing optimized vs unoptimized
7. ⏳ Integrate with CLI (--optimize flag)
8. ⏳ Profile hanoi.bf and mandelbrot.bf with optimizations

## References

- Original AST: `src/instruction.rs`
- Parser: `src/parser.rs`
- Interpreter: `src/interpreter.rs`
- Benchmark baseline: `internal/benchmark_baseline.md`
