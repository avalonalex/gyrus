# Optimizer Improvements - Missed Optimization Opportunities

## Overview

This document tracks optimization patterns that are currently missed by the FerrousCortex optimizer. Each pattern includes examples, current behavior, desired behavior, and implementation notes.

## Current Optimizer Capabilities

**Instruction Fusion:**
- ✅ `+++` → `Add(3)` - Combine repeated increments
- ✅ `---` → `Sub(3)` - Combine repeated decrements
- ✅ `>>>` → `Right(3)` - Combine pointer movements
- ✅ `<<<` → `Left(3)` - Combine pointer movements

**Loop Pattern Recognition:**
- ✅ `[-]` or `[+]` → `Zero` - Clear current cell
- ✅ `[>]` → `SeekRight` - Find next zero cell
- ✅ `[<]` → `SeekLeft` - Find previous zero cell
- ✅ `[->+<]` → `MultiplyAdd([(1, 1)])` - Move value to next cell
- ✅ `[->++<]` → `MultiplyAdd([(1, 2)])` - Multiply by 2 and move
- ✅ `[->+++>+<<]` → `MultiplyAdd([(1, 3), (2, 1)])` - Multi-target multiply

## Missed Optimization Opportunities

### 1. Loop Rotation/Normalization for MultiplyAdd

**Priority:** High
**Complexity:** Medium
**Impact:** Common pattern in real BrainFuck programs

**Problem:**
The MultiplyAdd pattern matcher requires the loop to start with `-` or `+`, but BrainFuck loops are circular - the starting position doesn't matter semantically.

**Examples:**

```brainfuck
[>+++++<-]    ❌ NOT recognized (decrement at end)
[->+++++<]    ✅ Recognized as MultiplyAdd([(1, 5)])
```

Both patterns are **mathematically equivalent**:
- Decrement cell[0] by 1
- Add 5 to cell[1]
- Loop until cell[0] = 0
- Result: cell[1] += cell[0] * 5; cell[0] = 0

**Current Behavior:**
```
[>+++++<-] → Loop([Right(1), Add(5), Left(1), Sub(1)])
9 instructions → 5 optimized (1.80× compression)
```

**Desired Behavior:**
```
[>+++++<-] → MultiplyAdd([(1, 5)])
9 instructions → 1 optimized (9.00× compression)
```

**Implementation Notes:**
- Option A: **Loop rotation** - Try all rotations of loop body to find one that starts with +/-
- Option B: **Flexible pattern matching** - Modify `recognize_multiply_loop()` to find the +/- anywhere in the loop
- Option C: **Canonical form** - Normalize all loops to start with +/- before pattern matching

**Recommendation:** Option A (loop rotation) is cleanest - try each rotation and pick the one that matches a known pattern.

**Code Location:** `crates/ferrous-cortex/src/optimizer.rs:343` (`recognize_loop_pattern`)

---

### 2. Scan by N Cells

**Priority:** Medium
**Complexity:** Low
**Impact:** Useful for data structures like arrays

**Problem:**
Currently we only recognize `[>]` and `[<]` for scanning by 1 cell. Many programs scan by N cells (e.g., for packed data structures).

**Examples:**

```brainfuck
[>>]      → SeekRight(2)  - Find next zero cell, stepping by 2
[>>>]     → SeekRight(3)  - Find next zero cell, stepping by 3
[<<]      → SeekLeft(2)   - Find previous zero cell, stepping by 2
[<<<<]    → SeekLeft(4)   - Find previous zero cell, stepping by 4
```

**Current Behavior:**
```
[>>] → Loop([Right(2)])
```

**Desired Behavior:**
```
[>>] → SeekRight(2)
```

**Implementation Notes:**
- Extend `SeekRight` and `SeekLeft` to include step size parameter
- Modify pattern matcher to recognize `[>+]`, `[>>+]`, etc.
- Update optimized interpreter to handle variable step sizes

**Code Location:** `crates/ferrous-cortex/src/optimizer.rs:366-378` (SeekRight/SeekLeft pattern recognition)

---

### 3. Set Value Pattern

**Priority:** Medium
**Complexity:** Medium
**Impact:** Common in initialization code

**Problem:**
Setting a cell to a specific value currently requires separate Zero + Add instructions. We could fuse these into a single Set instruction.

**Examples:**

```brainfuck
[-]++++    → Set(4)    - Clear cell then set to 4
[+]---     → Set(-3)   - Clear cell then set to -3 (wrapping: 253)
[-]+++++++ → Set(8)    - Clear cell then set to 8
```

**Current Behavior:**
```
[-]++++    → Zero, Add(4)
6 instructions → 2 optimized
```

**Desired Behavior:**
```
[-]++++    → Set(4)
6 instructions → 1 optimized
```

**Implementation Notes:**
- Add `Set(value)` optimized instruction
- Requires lookahead in optimizer: when we see Zero, check if next instruction is Add/Sub
- Must handle both `[-]` and `[+]` zero patterns
- Alternative: Post-optimization pass to fuse Zero + Add → Set

**Code Location:** `crates/ferrous-cortex/src/optimizer.rs:211-340` (optimize_block function)

**Consideration:** This might be premature - modern JIT compilers can easily fuse these at a lower level. Benefit may be minimal.

---

### 4. Addition/Subtraction Cancellation

**Priority:** Low
**Complexity:** Low
**Impact:** Mostly benefits obfuscated code

**Problem:**
Consecutive Add/Sub operations can partially or fully cancel out.

**Examples:**

```brainfuck
+++-      → Add(2)          - 3 increments + 1 decrement = net +2
-----+++  → Sub(2)          - 5 decrements + 3 increments = net -2
+++---    → (nothing)       - Complete cancellation
>><<      → (nothing)       - Pointer movement cancellation
```

**Current Behavior:**
```
+++- → Add(3), Sub(1)
```

**Desired Behavior:**
```
+++- → Add(2)
```

**Implementation Notes:**
- Post-optimization pass to merge adjacent Add/Sub operations
- Also applies to Right/Left pointer movements
- Edge case: `Add(3), Sub(5)` → `Sub(2)`

**Consideration:** This is rarely seen in hand-written BrainFuck. Only benefits generated or obfuscated code.

---

### 5. Dead Code Elimination

**Priority:** Medium
**Complexity:** Medium
**Impact:** Helps with generated code and initialization

**Problem:**
Operations at the start of a program that are overwritten before being read are unnecessary.

**Examples:**

```brainfuck
+++[-]+++.    → [-]+++.     - Initial +++ is overwritten by [-]
>><<+.        → +.          - Pointer movement that returns to start
```

**Current Behavior:**
```
+++[-]+++. → Add(3), Zero, Add(3), Output
```

**Desired Behavior:**
```
+++[-]+++. → Zero, Add(3), Output
```

**Implementation Notes:**
- Requires dataflow analysis to track which values are live
- `Zero` instruction makes all previous operations on that cell dead
- Pointer movements that cancel out can be eliminated
- This is complex - may want to defer to JIT/AOT compiler stage

**Consideration:** Medium complexity, medium benefit. Good candidate for JIT optimization pass.

---

### 6. Copy Pattern Recognition

**Priority:** Low
**Complexity:** Medium
**Impact:** Common pattern, but already optimized via MultiplyAdd

**Problem:**
Cell copy with preservation of source is currently a general loop.

**Examples:**

```brainfuck
[->+>+<<]     → Copy cell[0] to cell[1] and cell[2], then zero cell[0]
              → Already recognized as MultiplyAdd([(1, 1), (2, 1)])
              → Could add explicit Copy instruction for clarity

[->>+>+<<<]   → Copy to non-adjacent cells
              → Already recognized as MultiplyAdd([(2, 1), (3, 1)])
```

**Current Behavior:**
```
[->+>+<<] → MultiplyAdd([(1, 1), (2, 1)])
```

**Alternative Representation:**
```
[->+>+<<] → Copy([1, 2])  - Copy cell[0] to offsets 1 and 2, then zero
```

**Implementation Notes:**
- This is purely a semantic distinction - MultiplyAdd with all multipliers = 1
- Could add Copy as an alias/wrapper around MultiplyAdd for debugging clarity
- Minimal performance benefit (already optimized)

**Consideration:** Low priority - MultiplyAdd already handles this efficiently.

---

## Future Advanced Optimizations

These optimizations are complex and likely better suited for a JIT/AOT compiler stage:

### 7. Constant Folding

Track cell values at compile time when deterministic:

```brainfuck
+++>>---<<    → Cell[0]=3, Cell[2]=-3 (known at compile time)
```

### 8. Loop Unrolling

For loops with known iteration counts:

```brainfuck
+++[>+<-]     → Iterations known (3), could unroll to: >+<>+<>+<
```

### 9. Common Subexpression Elimination

Detect repeated computation patterns:

```brainfuck
>+++<>+++<    → Both add 3 to cell[1], could cache
```

### 10. Inlining

Inline simple loop bodies when beneficial.

---

## Implementation Priority

**Phase 1 - High Value, Low Complexity:**
1. Loop rotation for MultiplyAdd recognition ⭐
   - High impact on compression ratio
   - Medium complexity
   - Fixes common real-world pattern

**Phase 2 - Medium Value:**
2. Scan by N cells (SeekRight/Left with step size)
   - Medium impact
   - Low complexity
   - Useful for data structure manipulation

3. Set value pattern (Zero + Add fusion)
   - Medium impact
   - Medium complexity
   - Consider if benefit justifies complexity

**Phase 3 - Nice to Have:**
4. Addition/subtraction cancellation
   - Low impact (rare in practice)
   - Low complexity
   - Easy win for generated code

5. Dead code elimination
   - Medium impact
   - Medium-high complexity
   - Better suited for JIT/AOT stage

**Defer to JIT/AOT:**
- Constant folding
- Loop unrolling
- Common subexpression elimination
- Inlining

---

## Testing Strategy

For each new optimization:

1. **Unit tests** - Add to `crates/ferrous-cortex/src/optimizer.rs` tests
2. **Integration tests** - Test with real BrainFuck programs
3. **Regression tests** - Ensure existing optimizations still work
4. **Compression ratio benchmarks** - Measure improvement on standard programs:
   - `programs/advanced/hanoi.bf`
   - `programs/advanced/pi.bf`
   - `programs/basic/hello_world.bf`

---

## Related Files

- **Optimizer implementation:** `crates/ferrous-cortex/src/optimizer.rs`
- **Optimized interpreter:** `crates/ferrous-cortex/src/optimized_interpreter.rs`
- **Optimizer tests:** `crates/ferrous-cortex/src/optimizer.rs:484-653`
- **Optimizer example:** `crates/ferrous-cortex/examples/optimizer.rs`
- **CLI integration:** `crates/ferrous-cortex-cli/src/main.rs`
- **Tool visualization:** `crates/ferrous-cortex-tool/src/main.rs` (optimize subcommand)

---

## Notes

- All optimizations must preserve source range tracking for debugging
- Performance improvements should be measured, not assumed
- Complex optimizations may be better suited for JIT/AOT compilation stage
- Focus on patterns that appear in real-world BrainFuck programs, not theoretical cases

---

## Revision History

- 2025-11-02: Initial document with 6 optimization opportunities identified
