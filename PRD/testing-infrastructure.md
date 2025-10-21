# PRD: Comprehensive Testing Infrastructure

## Overview

Design and implement a production-grade testing infrastructure for FerrousCortex that ensures correctness, performance, and maintainability. Leverage the new I/O abstraction to create comprehensive test coverage across all components.

**Status**: Phase 2 (I/O Abstraction) ✅ Complete
**Next**: Implement comprehensive testing infrastructure

## Current State

### Existing Test Coverage

**Test Count**: 67 tests (as of I/O abstraction implementation)

**Breakdown:**
- Unit tests in modules: ~54 tests
  - `interpreter.rs`: ~20 tests
  - `parser.rs`: ~15 tests
  - `validator.rs`: ~8 tests
  - `config.rs`: ~5 tests
  - `types.rs`: ~5 tests
  - `io.rs`: ~10 tests
- Doc tests: 13 tests
- Integration tests: 0 ❌
- Property-based tests: 0 ❌
- Benchmarks: 0 ❌

### Test Quality Assessment

**Strengths:**
- ✅ Good coverage of core functionality
- ✅ Error cases tested (memory bounds, step limits, timeouts)
- ✅ Can now verify output with StringIo
- ✅ All tests pass

**Gaps:**
- ❌ No integration tests (end-to-end scenarios)
- ❌ No property-based testing (random input fuzzing)
- ❌ No performance regression tests
- ❌ Limited BrainFuck program corpus
- ❌ No test utilities for common patterns
- ❌ No benchmark infrastructure
- ❌ No optimization verification tests

## Goals

### Primary Goals
1. **Comprehensive Coverage**: 90%+ code coverage with meaningful tests
2. **Correctness Assurance**: Property-based testing to catch edge cases
3. **Performance Tracking**: Benchmark infrastructure to prevent regressions
4. **Developer Experience**: Easy-to-write tests with helper utilities

### Secondary Goals
4. **Test Organization**: Clear separation of unit/integration/bench tests
5. **BF Program Corpus**: Collection of standard BF programs for testing
6. **Optimization Validation**: Verify optimizations preserve behavior

### Non-Goals
- 100% code coverage (diminishing returns)
- GUI testing (future work)
- Stress testing (separate PRD)

## Success Metrics

- ✅ 90%+ code coverage
- ✅ 100+ total tests across all categories
- ✅ Property-based tests run on every `cargo test`
- ✅ Benchmarks available via `cargo bench`
- ✅ Zero false positives (flaky tests)
- ✅ Fast test execution (< 1 second for unit tests)

## Detailed Design

---

## Category 1: Test Utilities (Foundation)

### 1.1 Test Utilities Module

**New file**: `crates/ferrous-cortex/src/test_utils.rs`

**Purpose**: Common test helpers to reduce boilerplate

```rust
//! Test utilities for BrainFuck interpreter testing.
//!
//! This module provides helper functions and utilities for writing tests.
//! Only available in test builds.

use crate::io::StringIo;
use crate::config::{ExecutionConfig, ExecutionConfigBuilder};
use crate::parser::parse;
use crate::interpreter::interpret_with_io;
use crate::stats::ExecutionStats;
use crate::error::BfError;

/// Run a BrainFuck program with string input and capture output.
pub fn run_bf(source: &str, input: &str) -> Result<(String, ExecutionStats), BfError> {
    let instructions = parse(source)?;
    let config = ExecutionConfig::default();
    let mut input_io = StringIo::new(input);
    let mut output_io = StringIo::empty();

    let stats = interpret_with_io(&instructions, config, &mut input_io, &mut output_io)?;
    Ok((output_io.output_string(), stats))
}

/// Run a BrainFuck program with custom config.
pub fn run_bf_with_config(
    source: &str,
    input: &str,
    config: ExecutionConfig,
) -> Result<(String, ExecutionStats), BfError> {
    let instructions = parse(source)?;
    let mut input_io = StringIo::new(input);
    let mut output_io = StringIo::empty();

    let stats = interpret_with_io(&instructions, config, &mut input_io, &mut output_io)?;
    Ok((output_io.output_string(), stats))
}

/// Run and expect success.
pub fn run_bf_expect_ok(source: &str, input: &str) -> (String, ExecutionStats) {
    run_bf(source, input).expect("BF execution should succeed")
}

/// Run and expect failure.
pub fn run_bf_expect_err(source: &str, input: &str) -> BfError {
    run_bf(source, input).expect_err("BF execution should fail")
}

/// Assert that two BF programs produce the same output given the same input.
pub fn assert_bf_equivalent(source1: &str, source2: &str, input: &str) {
    let (output1, _) = run_bf_expect_ok(source1, input);
    let (output2, _) = run_bf_expect_ok(source2, input);
    assert_eq!(output1, output2, "Programs should produce identical output");
}

/// Common test configurations
pub mod configs {
    use super::*;
    use crate::config::MEMORY_SIZE;

    pub fn tiny_memory() -> ExecutionConfig {
        ExecutionConfigBuilder::new().with_memory_size(10).build()
    }

    pub fn small_memory() -> ExecutionConfig {
        ExecutionConfigBuilder::new().with_memory_size(100).build()
    }

    pub fn with_step_limit(limit: u64) -> ExecutionConfig {
        ExecutionConfigBuilder::new()
            .with_memory_size(MEMORY_SIZE)
            .with_max_steps(limit)
            .build()
    }

    pub fn with_timeout(ms: u64) -> ExecutionConfig {
        ExecutionConfigBuilder::new()
            .with_memory_size(MEMORY_SIZE)
            .with_timeout_ms(ms)
            .build()
    }
}
```

**Integration:**
```rust
// In lib.rs (only in test mode)
#[cfg(test)]
pub mod test_utils;
```

---

## Category 2: Integration Tests

### 2.1 Standard BrainFuck Programs

**New directory**: `tests/`

**Structure:**
```
tests/
├── integration/
│   ├── mod.rs
│   ├── hello_world.rs
│   ├── arithmetic.rs
│   ├── algorithms.rs
│   └── edge_cases.rs
├── programs/           # BF program corpus
│   ├── hello_world.bf
│   ├── fibonacci.bf
│   ├── rot13.bf
│   ├── reverse.bf
│   ├── 99_bottles.bf
│   ├── mandelbrot.bf
│   └── quine.bf
└── common/
    └── mod.rs          # Shared test utilities
```

### 2.2 Integration Test Examples

See PRD for detailed examples of:
- Hello World tests
- Algorithm tests (Fibonacci, ROT13, reverse)
- Edge case tests (empty programs, cell wrapping, EOF behaviors)
- Deep nesting tests

---

## Category 3: Property-Based Testing

### 3.1 Add Proptest Dependency

```toml
[dev-dependencies]
proptest = "1.5"
```

### 3.2 Property Tests

Key properties to test:
- Parsing never panics
- Valid programs always parse
- Balanced programs execute without errors
- Execution is deterministic
- Arithmetic operations wrap correctly
- Same input produces same output

---

## Category 4: Benchmark Infrastructure

### 4.1 Add Criterion Dependency

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "interpreter"
harness = false

[[bench]]
name = "parser"
harness = false
```

### 4.2 Benchmark Categories

- **Arithmetic operations**: Simple +/- benchmarks
- **Pointer movement**: >/< benchmarks
- **Loops**: Simple and nested loops
- **I/O operations**: Input/output performance
- **Real programs**: Hello World, Fibonacci
- **Parser benchmarks**: Various program sizes

---

## Category 5: BrainFuck Program Corpus

### Standard Test Programs

Create collection in `tests/programs/`:

1. **hello_world.bf** - Classic Hello World
2. **fibonacci.bf** - Fibonacci sequence generator
3. **rot13.bf** - ROT13 cipher
4. **reverse.bf** - String reversal
5. **99_bottles.bf** - 99 Bottles of Beer
6. **mandelbrot.bf** - Mandelbrot set (stress test)
7. **quine.bf** - Self-replicating program
8. **prime.bf** - Prime number generator
9. **factor.bf** - Integer factorization
10. **hanoi.bf** - Towers of Hanoi solver

Each includes:
- Source code
- Expected input (if any)
- Expected output
- Description

---

## Implementation Plan

### Phase 1: Foundation (Week 1)
**Priority**: CRITICAL

1. Add dev dependencies (proptest, criterion)
2. Create `test_utils.rs` module
3. Set up directory structure
4. Implement basic test utilities

### Phase 2: Integration Tests (Week 2)
**Priority**: HIGH

1. Create BF program corpus (10 programs)
2. Write integration tests for each program
3. Add edge case tests
4. Add EOF behavior tests

### Phase 3: Property-Based Testing (Week 2-3)
**Priority**: MEDIUM

1. Add proptest dependency
2. Implement property test strategies
3. Write core property tests
4. Run property tests in CI

### Phase 4: Benchmark Infrastructure (Week 3)
**Priority**: MEDIUM

1. Add criterion dependency
2. Create interpreter benchmarks
3. Create parser benchmarks
4. Generate baseline performance data

### Phase 5: Optimization Validation (Week 4)
**Priority**: LOWER (for future)

1. Create optimization verification tests
2. Test that optimizations preserve behavior
3. Benchmark optimized vs. naive execution

---

## Testing Best Practices

### 1. Test Naming Convention

```rust
// Pattern: test_<component>_<scenario>_<expected_outcome>
#[test]
fn test_parser_unmatched_bracket_returns_error() { ... }

#[test]
fn test_interpreter_step_limit_halts_execution() { ... }
```

### 2. Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod parsing { }
    mod execution { }
    mod edge_cases { }
}
```

### 3. Test Data Management

```rust
// Use include_str! for test data
const HELLO_WORLD: &str = include_str!("../../tests/programs/hello_world.bf");

// Use test_utils for common patterns
use crate::test_utils::run_bf_expect_ok;
```

---

## Success Criteria

### Overall Success
- ✅ 100+ total tests
- ✅ 90%+ code coverage
- ✅ All tests pass in < 2 seconds
- ✅ Zero flaky tests
- ✅ CI/CD integration ready
- ✅ Benchmark baseline established

---

## Dependencies

```toml
[dev-dependencies]
proptest = "1.5"
criterion = { version = "0.5", features = ["html_reports"] }
```

---

## Timeline

- **Week 1**: Foundation (test_utils, structure)
- **Week 2**: Integration tests + Property tests
- **Week 3**: Benchmarks + baseline data
- **Week 4**: Optimization validation (future)

**Total**: 3-4 weeks for comprehensive testing infrastructure

---

## Risks and Mitigations

### Risk 1: Slow Test Execution
**Mitigation**: Separate unit/integration/bench, use `--test` flag

### Risk 2: Flaky Tests
**Mitigation**: Property tests have fixed seeds, deterministic I/O

### Risk 3: Maintenance Burden
**Mitigation**: Test utilities reduce boilerplate, good organization

---

## Conclusion

This comprehensive testing infrastructure provides:

1. **Confidence**: Property-based testing catches edge cases
2. **Performance**: Benchmark infrastructure prevents regressions
3. **Documentation**: Integration tests serve as examples
4. **Maintainability**: Test utilities reduce boilerplate
5. **Quality**: High coverage ensures correctness

With this infrastructure, FerrousCortex will have production-grade testing that supports rapid development while maintaining reliability.
