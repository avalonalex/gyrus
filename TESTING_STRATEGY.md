# Testing Strategy for Debug Features

**Question**: How are debug symbol features tested in industry?

**Short answer**: Unit tests are necessary but insufficient. Production compilers and interpreters use a multi-layered testing approach combining unit tests, integration tests, property-based testing, differential testing, fuzzing, and end-to-end validation.

---

## Current Testing (Insufficient)

### What We Have

```
✅ 179 unit tests
✅ Property tests (proptest) for basic invariants
✅ Integration tests for CLI
```

### What We're Missing

❌ **End-to-end validation**: Do source locations actually match the source?
❌ **Property-based testing for debug features**: Invariants about source tracking
❌ **Differential testing**: Compare against reference implementations
❌ **Fuzzing**: Random programs to find edge cases
❌ **Golden file testing**: Snapshot comparisons
❌ **Manual validation**: Interactive debugging workflow tests

---

## Industry Approach: Multi-Layer Testing

### 1. Property-Based Testing (Invariants)

**Concept**: Specify invariants that must hold for ALL programs.

#### Example from Rust Compiler

```rust
// Property: Every error must have a valid source location
#[proptest]
fn all_errors_have_source_locations(program: BrainfuckProgram) {
    let (ast, debug_info) = parse_with_debug(&program.source).unwrap();

    let result = interpret_with_config(&ast, config, Some(&debug_info));

    if let Err(error) = result {
        prop_assert!(
            error.source_location().is_some(),
            "Error without source location: {:?}",
            error
        );

        let loc = error.source_location().unwrap();
        prop_assert!(
            loc.offset < program.source.len(),
            "Source location offset out of bounds"
        );
    }
}
```

#### Key Invariants for Debug Symbols

1. **Completeness**: Every instruction must map to a source location
2. **Monotonicity**: Instruction indices are monotonically increasing in execution order
3. **Containment**: All source locations must be within source bounds
4. **Loop consistency**: Loop call stack depth matches actual nesting
5. **Reachability**: Every source location in errors must be reachable in source

### 2. Differential Testing (Oracle Testing)

**Concept**: Compare our interpreter against a **reference implementation** or **ground truth**.

#### Example from LLVM

LLVM compares generated DWARF debug info against expected output:

```rust
// Compare our source location tracking against "ground truth"
#[test]
fn test_differential_source_locations() {
    let programs = load_test_corpus();

    for (source, expected_traces) in programs {
        let mut actual_traces = Vec::new();

        // Run with tracing hook
        let tracer = ExecutionTracerHook::new();
        let config = ExecutionConfigBuilder::new()
            .with_hook(Box::new(tracer.clone()))
            .build();

        interpret_with_config(&ast, config, Some(&debug_info)).unwrap();

        // Compare actual vs expected traces
        assert_eq!(
            tracer.get_traces(),
            expected_traces,
            "Source location trace mismatch for program: {}",
            source
        );
    }
}
```

#### Golden File Testing

Store expected execution traces in files:

```
tests/golden/
├── hello_world.bf
├── hello_world.trace     # Expected execution trace
├── nested_loops.bf
├── nested_loops.trace
└── ...
```

Each `.trace` file contains:
```
Step 1: line 1, col 1, instruction: +, cell[0]=1
Step 2: line 1, col 2, instruction: +, cell[0]=2
Step 3: line 1, col 3, instruction: >, cell[1]=0
...
```

Run tests by comparing actual output against golden files.

### 3. Fuzzing (Randomized Testing)

**Concept**: Generate random programs and look for crashes, panics, or invariant violations.

#### Example: cargo-fuzz

```rust
// fuzz/fuzz_targets/debug_symbols.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        // Try to parse
        if let Ok((ast, debug_info)) = parse_with_debug(source) {
            // Try to execute
            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_max_steps(10000)
                .build();

            let _ = interpret_with_config(&ast, config, Some(&debug_info));

            // Invariant: No panics allowed!
            // Invariant: All errors must have source locations
        }
    }
});
```

Run fuzzer:
```bash
cargo fuzz run debug_symbols -- -max_total_time=3600
```

**Fuzzing has found 1000s of bugs in production compilers** (GCC, Clang, rustc).

### 4. Integration Testing (End-to-End)

**Concept**: Test complete workflows with real programs.

#### Example: GDB Test Suite

GDB has 10,000+ integration tests that:
1. Compile a program with debug symbols
2. Run under GDB
3. Set breakpoints, step through code
4. Verify stack traces, variable inspection, etc.

For FerrousCortex:

```rust
#[test]
fn test_e2e_error_message_quality() {
    let source = r#"
+++[>++<-]  # Simple loop
>>>>>>      # Move right
[invalid    # Unclosed bracket ERROR
"#;

    let result = parse(source);

    assert!(result.is_err());
    let error = result.unwrap_err();

    // Verify error message quality
    let formatted = error.format_with_source(source);

    // Should show line 3
    assert!(formatted.contains("line 3"));

    // Should show column 1 (where '[' is)
    assert!(formatted.contains("col 1"));

    // Should highlight the problematic line
    assert!(formatted.contains("[invalid"));

    // Should point to exact character with caret
    assert!(formatted.contains("^"));

    // Human validation: Does this actually look correct?
    println!("{}", formatted);
}
```

### 5. Snapshot Testing (Regression Prevention)

**Concept**: Capture current behavior as "snapshots", then detect any changes.

#### Example: Insta (Snapshot Testing for Rust)

```rust
use insta::assert_snapshot;

#[test]
fn test_error_formatting_snapshots() {
    let programs = vec![
        ("unclosed_bracket.bf", "[++"),
        ("memory_overflow.bf", ">".repeat(100)),
        ("nested_error.bf", "++[>++[>>]]"),
    ];

    for (name, source) in programs {
        let result = run_and_format_error(source);

        // Compare against saved snapshot
        assert_snapshot!(name, result);
    }
}
```

First run creates snapshots in `snapshots/`:
```
snapshots/
├── error_formatting__unclosed_bracket.bf.snap
├── error_formatting__memory_overflow.bf.snap
└── error_formatting__nested_error.bf.snap
```

Future runs compare against snapshots. If behavior changes:
- Review the diff
- Accept if intentional (`cargo insta review`)
- Reject if regression

### 6. Manual Validation (The Most Important!)

**Concept**: Actually USE the features interactively.

#### Checklist for Debug Features

Manual testing checklist:
```
□ Run a program with an error
□ Look at the error message
□ Verify the source location is CORRECT
□ Verify the caret points to the RIGHT character
□ Verify syntax highlighting makes sense
□ Verify loop call stack shows right iterations

□ Try with multiline programs
□ Try with comments
□ Try with deeply nested loops
□ Try with edge cases (empty program, only comments)

□ Have someone ELSE try it (fresh eyes catch issues)
```

**Critical**: The developer who wrote the feature is the WORST person to test it (they know what it's "supposed" to do). Get fresh eyes!

---

## Proposed Testing Strategy for FerrousCortex

### Phase 1: Enhanced Property Testing (Week 1)

⚠️ **Important Discovery**: Property testing interpreter *execution* fights the halting problem!

**What works:**
- ✅ Parser properties (fast, deterministic)
- ✅ Data structure properties (fast, deterministic)

**What doesn't work:**
- ❌ Execution properties (random programs with loops can take arbitrarily long)

Add property tests for **non-execution** invariants:

```rust
// tests/property_debug_symbols.rs

use proptest::prelude::*;
use ferrous_cortex::*;

// Generate random valid BF programs
fn arb_bf_program() -> impl Strategy<Value = String> {
    // ... implementation
}

#[proptest]
fn prop_source_locations_in_bounds(source: String) {
    // ✅ FAST: Only validates data structures, no execution
    if let Ok((ast, debug_info)) = parse_with_debug(&source) {
        for instruction_idx in 0..ast.len() {
            if let Some(loc) = debug_info.lookup(instruction_idx) {
                prop_assert!(loc.offset < source.len());
                prop_assert!(loc.line >= 1);
                prop_assert!(loc.column >= 1);
            }
        }
    }
}

#[proptest]
fn prop_parse_never_panics(source: String) {
    // ✅ FAST: Parsing is deterministic and fast
    let _ = parse_with_debug(&source);
}

// ❌ DON'T DO THIS: Execution hits halting problem
// #[proptest]
// fn prop_all_errors_have_locations(source: String) {
//     if let Ok((ast, debug_info)) = parse_with_debug(&source) {
//         let result = interpret_with_config(&ast, config, Some(&debug_info));
//         // ^ Random programs can run for arbitrarily long!
//     }
// }

```

**For execution behavior**, use targeted unit tests instead of property tests.

### Phase 2: Golden File Testing (Week 2)

Create test corpus with expected behavior:

```bash
tests/corpus/
├── basic/
│   ├── hello_world.bf
│   ├── hello_world.expected.trace
│   ├── simple_loop.bf
│   └── simple_loop.expected.trace
├── errors/
│   ├── memory_overflow.bf
│   ├── memory_overflow.expected.error
│   ├── unclosed_bracket.bf
│   └── unclosed_bracket.expected.error
└── edge_cases/
    ├── empty_program.bf
    ├── only_comments.bf
    └── deeply_nested.bf
```

Test runner:

```rust
#[test]
fn test_golden_files() {
    for entry in glob("tests/corpus/**/*.bf").unwrap() {
        let bf_file = entry.unwrap();
        let expected_file = bf_file.with_extension("expected.trace");

        if expected_file.exists() {
            let actual = run_with_trace(&bf_file);
            let expected = fs::read_to_string(&expected_file).unwrap();

            assert_eq!(
                actual, expected,
                "Trace mismatch for {:?}",
                bf_file
            );
        }
    }
}
```

### Phase 3: Fuzzing (Week 3)

Set up cargo-fuzz:

```bash
cargo install cargo-fuzz
cargo fuzz init
cargo fuzz add debug_symbols
cargo fuzz run debug_symbols
```

Let it run overnight. **Fuzzing has found critical bugs in every major compiler.**

### Phase 4: Manual Validation (Ongoing)

Create a checklist and ACTUALLY run through it:

```markdown
# Debug Feature Validation Checklist

## Source Location Accuracy

- [ ] Error in simple program (1 line) - location correct?
- [ ] Error in multiline program - line number correct?
- [ ] Error inside loop - still accurate?
- [ ] Error inside deeply nested loop - still accurate?
- [ ] Error after many loop iterations - still accurate?

## Loop Call Stack

- [ ] Single loop error - shows 1 frame?
- [ ] Nested loops error - shows N frames?
- [ ] Iteration counts accurate?
- [ ] Source locations in stack frames correct?

## Error Message Quality

- [ ] Syntax highlighted?
- [ ] Caret points to right character?
- [ ] Shows surrounding context (2 lines before/after)?
- [ ] Loop call stack formatting readable?

## Edge Cases

- [ ] Empty program
- [ ] Only comments
- [ ] Very long programs (1MB+)
- [ ] Programs with unicode characters
- [ ] Programs with tabs vs spaces
```

**CRITICAL**: Have someone who DIDN'T write the code go through this checklist.

---

## Industry Examples

### GCC/Clang Debug Info Testing

1. **DejaGNU test suite**: 40,000+ tests
2. **DWARF validation tools**: Verify debug info format
3. **Comparison tests**: Compare against reference compiler
4. **Manual inspection**: Developers run GDB on test cases

Example GCC test:
```c
/* Test that line info is correct for nested loops */
/* { dg-options "-g -O0" } */

int main() {
    for (int i = 0; i < 10; i++) {      // Line 5
        for (int j = 0; j < 10; j++) {  // Line 6
            // Breakpoint here should show i and j
        }
    }
}

/* { dg-final { gdb-test 6 "i" "0" } } */
```

### Python (CPython) Testing

Python tests line number tracking with:

1. **`inspect` module tests**: Verify `inspect.getframeinfo()` returns correct line numbers
2. **Traceback tests**: Verify exception tracebacks show correct lines
3. **PDB tests**: Verify debugger steps to correct lines

Example:
```python
def test_lineno_in_nested_function():
    def outer():
        def inner():
            raise ValueError()  # Line 4
        inner()

    try:
        outer()
    except ValueError as e:
        tb = e.__traceback__
        # Verify traceback points to line 4
        assert tb.tb_lineno == 4
```

### Node.js (V8) Testing

V8 tests source maps and stack traces:

1. **Stack trace tests**: 200+ tests verifying Error.stack format
2. **Source map tests**: Verify transpiled code maps back correctly
3. **Debugger protocol tests**: Verify Chrome DevTools integration

---

## Recommended Next Steps for FerrousCortex

### Immediate (This Week)

1. **Add 5-10 property tests** for debug symbol invariants
2. **Manual validation checklist** - actually run through it
3. **Snapshot test for error formatting** using `insta` crate

### Short Term (This Month)

1. **Golden file test suite** with 50+ real BF programs
2. **Set up cargo-fuzz** and run overnight
3. **Get external validation** - have someone else try it

### Long Term (Future)

1. **Differential testing** against other BF interpreters
2. **Performance regression tests** for debug overhead
3. **Integration tests** for debugger (when implemented)

---

## Conclusion

**Unit tests are necessary but insufficient** for testing debug features.

**Industry approach**:
1. ✅ Unit tests (we have this)
2. ❌ Property tests for invariants (NEED)
3. ❌ Golden file testing (NEED)
4. ❌ Fuzzing (SHOULD HAVE)
5. ❌ Manual validation (CRITICAL)
6. ❌ Differential testing (NICE TO HAVE)

**The most important test**: Actually USE the feature and verify it works correctly with your own eyes. No amount of automated testing replaces human validation of error message quality, source location accuracy, and overall UX.

**Bottom line**: We need more testing, but we're on the right track. The current unit tests caught many bugs during development. The next step is adding property tests and manual validation.
