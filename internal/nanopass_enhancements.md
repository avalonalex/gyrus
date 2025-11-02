# Nanopass-Inspired Optimizer Enhancements

## Summary

Enhanced the optimizer with sophisticated pattern recognition inspired by the nanopass compiler design, specifically implementing **generalized multiplication loop recognition**.

## Changes Made

### 1. Replaced Limited Patterns with MultiplyAdd

**Before:**
```rust
MoveRight(usize, SourceRange),  // Only [->+<] with multiplier=1
MoveLeft(usize, SourceRange),   // Only [-<+>] with multiplier=1
CopyRight(usize, SourceRange),  // Not implemented
CopyLeft(usize, SourceRange),   // Not implemented
```

**After:**
```rust
MultiplyAdd(Vec<(isize, i32)>, SourceRange),  // ANY multiplication pattern!
```

**Benefits:**
- Single variant handles all multiplication/move/copy patterns
- Extensible to complex multi-target patterns
- Cleaner API, fewer special cases

### 2. Implemented Generalized Pattern Recognition

**Algorithm** (from `recognize_multiply_loop()`):

1. **Check initial decrement**: Must start with `-` or `+` (typically `-`)
2. **Track position**: Accumulate pointer movements as `isize`
3. **Collect targets**: For each Add/Sub at non-zero position, record `(offset, multiplier)`
4. **Verify return**: Loop must return to position 0
5. **Create pattern**: If valid, return `MultiplyAdd(adds, range)`

**Handles:**
```rust
// Simple move
[->+<]          → MultiplyAdd([(1, 1)])

// Multiply by N
[->++<]         → MultiplyAdd([(1, 2)])
[->+++<]        → MultiplyAdd([(1, 3)])

// Multi-target
[->+++>+<<]     → MultiplyAdd([(1, 3), (2, 1)])
[->+>++>+++<<<] → MultiplyAdd([(1, 1), (2, 2), (3, 3)])

// Negative multipliers
[->-<]          → MultiplyAdd([(1, -1)])  // Subtract source from target
```

### 3. Enhanced Compression Ratios

| Pattern | Before | After | Improvement |
|---------|--------|-------|-------------|
| `[->+<]` | 5× (5→1) | 5× (5→1) | Same |
| `[->++<]` | ❌ Not optimized | **6× (6→1)** | **NEW** |
| `[->+++>+<<]` | ❌ Not optimized | **10× (10→1)** | **NEW** |

## Test Coverage

**New tests:**
1. `test_recognize_multiply_add_simple` - `[->+<]` → `MultiplyAdd([(1, 1)])`
2. `test_recognize_multiply_add_with_multiplier` - `[->++<]` → `MultiplyAdd([(1, 2)])`
3. `test_recognize_multiply_add_multi_target` - `[->+++>+<<]` → `MultiplyAdd([(1, 3), (2, 1)])`

**Total optimizer tests:** 9 (all passing ✅)

## Example Output

```bash
$ cargo run --example optimizer

=== Example 3b: Multiply Pattern ===
Source: [->++<] (multiply by 2 and move)
Original instructions: 6
Optimized instructions: 1
Compression ratio: 6.00×
Optimized IR:
  [0] MultiplyAdd([(1, 2)], SourceRange { start: 0, end: 6 })

=== Example 3c: Multi-Target Multiply ===
Source: [->+++>+<<] (multiply by 3 to offset 1, by 1 to offset 2)
Original instructions: 10
Optimized instructions: 1
Compression ratio: 10.00×
Optimized IR:
  [0] MultiplyAdd([(1, 3), (2, 1)], SourceRange { start: 0, end: 10 })
```

## Design Comparison: Our Implementation vs Nanopass

### Similarities
- **Generalized algorithm**: Track position, collect (offset, multiplier) pairs
- **Flexible pattern matching**: Works for any valid multiplication loop
- **Multi-target support**: Single instruction can update multiple cells

### Differences
- **Source tracking**: We add `SourceRange` to every variant (nanopass doesn't have this)
- **IR stages**: We use single-stage optimization (nanopass uses L0→L1→L2→L3→L4)
- **Signed arithmetic**: We keep separate Add/Sub, Right/Left (nanopass uses Add(i32), Move(i32))

### Why Not Multi-Stage IR?

**Pros of nanopass approach:**
- Clear separation of concerns
- Each pass does one thing
- Easy to debug intermediate stages

**Pros of our single-stage approach:**
- Simpler implementation
- Fewer allocations
- Source tracking is easier (one-to-one mapping)
- Sufficient for current optimization goals

**Decision:** Keep single-stage for now, consider multi-stage when implementing JIT compiler.

## Future Enhancements (From Nanopass)

### 1. Signed Arithmetic (Medium Priority)
Replace:
```rust
Add(u8, SourceRange) + Sub(u8, SourceRange)
```
With:
```rust
Add(i32, SourceRange)  // Positive = increment, negative = decrement
```

**Benefits:**
- Algebraic simplification: `Add(5) + Add(-3)` → `Add(2)`
- Simpler IR
- Closer to JIT representation

**Costs:**
- Less type-safe (can represent `Add(0)` which is no-op)
- Requires validation pass to remove no-ops

### 2. Copy Patterns (Low Priority)
Currently `MultiplyAdd` zeros the source cell. Add variant for copying:
```rust
Copy(Vec<isize>, SourceRange),  // Copy source to offsets, keep source
```

Example: `[->+>+<<+]` → `Copy([1, 2])`

### 3. Dead Code Elimination (Low Priority)
Remove unreachable code after infinite loops.

### 4. Constant Propagation (Future JIT)
Track known cell values through execution to eliminate redundant operations.

## Impact on Real Programs

**Expected improvements:**
- **Arithmetic-heavy programs**: 5-10× speedup (heavy use of multiplication patterns)
- **hanoi.bf**: May use multiplication patterns for disk counting
- **mandelbrot.bf**: Likely benefits from multiply loops in fractal calculation

**Actual benchmarks:** TBD (run after implementing optimized interpreter)

## Files Modified

1. **`src/optimizer.rs`** (+120 lines, -30 lines):
   - Added `MultiplyAdd` variant
   - Removed `MoveRight`, `MoveLeft`, `CopyRight`, `CopyLeft`
   - Implemented `recognize_multiply_loop()` function
   - Added 3 new tests

2. **`examples/optimizer.rs`** (+65 lines):
   - Added examples 3b and 3c for new patterns
   - Demonstrates compression ratios

3. **`internal/nanopass_enhancements.md`** (this file):
   - Documentation of enhancements

## References

- Nanopass compiler design: Multi-stage IR transformation
- Original nanopass BF code: Shared in user message
- Our optimizer: `src/optimizer.rs`

## Next Steps

1. ✅ Implement `MultiplyAdd` pattern (DONE)
2. ✅ Add tests for new patterns (DONE)
3. ✅ Update examples (DONE)
4. ⏳ Implement optimized interpreter to execute `MultiplyAdd`
5. ⏳ Benchmark real programs (hanoi.bf, mandelbrot.bf)
6. ⏳ Consider signed arithmetic for future JIT
