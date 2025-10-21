# PRD: Enhanced Error Handling and Interpreter Reliability

## Overview

Improve the BrainFuck interpreter's error handling and reliability to provide production-grade diagnostics, graceful failure modes, and comprehensive validation.

## Current State (Updated: October 2025)

### ✅ COMPLETED - Existing Error Handling
- ✅ Rich error types with source context
- ✅ Full position tracking (line, column, offset)
- ✅ Error messages with source code snippets (2 lines before/after with caret)
- ✅ Multiple bracket error reporting (shows all bracket mismatches at once)
- ✅ Validation pass with warnings (empty loops, infinite loops, extreme nesting)
- ✅ Memory safety with multiple memory models (Fixed, Wrapping, Unbounded)
- ✅ Configurable EOF behavior (SetZero, SetNegOne, NoChange, Error)
- ✅ Resource limits (step count, timeout)
- ✅ Comprehensive statistics tracking
- ✅ CLI flags: --verbose, --validate, --strict, --max-steps, --timeout
- ✅ 67 comprehensive tests

### Implementation Status by Phase

**Phase 1: Enhanced Error Types and Context** ✅ COMPLETE
- 1.1 Source Location Tracking ✅
- 1.2 Improved Error Messages ✅

**Phase 2: Parser Enhancements** ✅ COMPLETE
- 2.1 Validation Pass ✅ (--validate, --strict flags implemented)
- 2.2 Better Bracket Matching ✅ (multiple errors reported at once)

**Phase 3: Runtime Reliability** ✅ COMPLETE
- 3.1 Execution Limits ✅ (--max-steps, --timeout implemented)
- 3.2 Memory Safety ✅ (Fixed, Wrapping, Unbounded models)
- 3.3 I/O Error Handling ✅ (--eof-behavior with 4 modes)

**Phase 4: Developer Tools** ✅ PARTIALLY COMPLETE
- 4.1 Verbose Mode ✅ (--verbose flag shows stats)
- 4.2 Debug Symbols ⏳ (covered in separate PRD: debug-symbols-and-runtime-diagnostics.md)

**Phase 5: Testing and Documentation** ✅ PARTIALLY COMPLETE
- 5.1 Error Test Suite ✅ (67 tests covering most scenarios)
- 5.2 Error Handling Examples ✅ (examples/errors/ directory exists)
- 5.3 Documentation ⏳ (README updated, module docs pending)

## Goals

1. **Rich Error Messages**: Provide actionable error messages with source context
2. **Early Validation**: Catch issues during parsing when possible
3. **Graceful Degradation**: Handle edge cases without panicking
4. **Resource Safety**: Prevent resource exhaustion
5. **Developer Experience**: Clear diagnostics for debugging BF programs

## Detailed Implementation Steps

### Phase 1: Enhanced Error Types and Context

#### 1.1 Add Source Location Tracking
**File**: `src/bf.rs`

**Changes needed**:
```rust
// Add position tracking struct
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,  // Character offset in source
}

// Enhance Instruction to carry location info (optional, for debugging)
#[derive(Debug, Clone, PartialEq)]
pub struct InstructionWithLocation {
    pub instruction: Instruction,
    pub location: SourceLocation,
}
```

**Implementation steps**:
1. Create `SourceLocation` struct to track line, column, and offset
2. Modify parser to maintain current position as it scans
3. Track newlines to calculate line numbers
4. Optionally attach locations to instructions for runtime error reporting

**Testing**:
- Test line/column calculation with multiline programs
- Verify positions are accurate after loops and comments

---

#### 1.2 Improve Error Messages
**File**: `src/bf.rs`

**Changes needed**:
```rust
#[derive(Error, Debug)]
pub enum BfError {
    #[error("Unmatched '[' at {location}\n{context}")]
    UnmatchedOpenBracket {
        location: SourceLocation,
        context: String,  // Source snippet showing the error
    },

    #[error("Unmatched ']' at {location}\n{context}")]
    UnmatchedCloseBracket {
        location: SourceLocation,
        context: String,
    },

    #[error("Memory pointer out of bounds at {location}\nAttempted to access cell {attempted}, valid range: 0-{max}\n{context}")]
    MemoryOutOfBounds {
        location: Option<SourceLocation>,
        attempted: isize,  // Can be negative
        max: usize,
    },

    #[error("IO error at {location}: {message}\n{context}")]
    IoError {
        location: Option<SourceLocation>,
        message: String,
        context: String,
    },

    // New error types
    #[error("File read error: {0}")]
    FileError(String),

    #[error("Execution timeout: program exceeded {limit_ms}ms execution limit")]
    ExecutionTimeout { limit_ms: u64 },

    #[error("Warning: Potential infinite loop detected at {location}\n{context}")]
    PotentialInfiniteLoop {
        location: SourceLocation,
        context: String,
    },
}
```

**Implementation steps**:
1. Add helper function `extract_source_context(source: &str, location: SourceLocation) -> String`
   - Show 2 lines before and after error
   - Add caret (^) pointing to exact position
   - Include line numbers
2. Update all error construction sites to include context
3. Store original source in parser for context extraction

**Example output**:
```
Error: Unmatched '[' at line 5, column 12
    3 | +++[->+<]
    4 | >>++
    5 | [>++>+++[
              ^
    6 | +++
    7 | ]
```

**Testing**:
- Test context extraction at start/end of file
- Test with various error positions
- Test with very long lines

---

### Phase 2: Parser Enhancements

#### 2.1 Add Validation Pass
**File**: `src/bf.rs`

**New function**:
```rust
/// Validate parsed instructions for common issues
pub fn validate(instructions: &[Instruction], source: &str) -> Vec<BfWarning> {
    let mut warnings = Vec::new();
    // Check for potential issues
    warnings
}

#[derive(Debug)]
pub enum BfWarning {
    DeadCode {
        location: SourceLocation,
        reason: String,
    },
    SuspiciousPattern {
        location: SourceLocation,
        pattern: String,
        suggestion: String,
    },
}
```

**Validations to implement**:
1. **Dead loops**: `[-]` at position 0 (cell already 0)
2. **Unclosed I/O**: Input without subsequent output might be intentional but could be a bug
3. **Extreme nesting**: Loops nested > 10 deep (performance warning)
4. **Empty loops**: `[]` does nothing
5. **Suspicious patterns**: `[>]` without bounds check could run off memory

**Implementation steps**:
1. Create warning enum and collection mechanism
2. Add optional validation phase after parsing
3. Implement warning checks as separate functions
4. Add CLI flag `--strict` to treat warnings as errors
5. Add CLI flag `--no-warn` to suppress warnings

**Testing**:
- Test each warning type individually
- Test warning suppression
- Test strict mode

---

#### 2.2 Better Bracket Matching
**File**: `src/bf.rs`

**Current issue**: Only reports first unmatched bracket

**Enhancement**:
```rust
fn validate_brackets(source: &str) -> Result<(), Vec<BfError>> {
    let mut stack = Vec::new();
    let mut errors = Vec::new();

    // Track ALL bracket mismatches, not just first
    // Return multiple errors at once for better UX
}
```

**Implementation steps**:
1. Separate bracket validation into pre-parse phase
2. Collect all bracket errors before returning
3. Report all errors at once with context
4. Suggest likely matches for unmatched brackets

**Testing**:
- Test multiple unmatched brackets in one program
- Test nested bracket errors
- Test bracket matching suggestions

---

### Phase 3: Runtime Reliability

#### 3.1 Add Execution Limits
**File**: `src/bf.rs`

**Changes needed**:
```rust
pub struct ExecutionConfig {
    pub memory_size: usize,
    pub max_steps: Option<u64>,      // Prevent infinite loops
    pub timeout_ms: Option<u64>,     // Wall-clock timeout
    pub allow_negative_pointer: bool, // Some BF variants allow this
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            memory_size: 30000,
            max_steps: None,  // Unlimited by default
            timeout_ms: None,
            allow_negative_pointer: false,
        }
    }
}

pub fn interpret_with_config(
    instructions: &[Instruction],
    config: ExecutionConfig,
) -> Result<(), BfError>
```

**Implementation steps**:
1. Add `ExecutionConfig` struct with safety limits
2. Add step counter to interpreter loop
3. Add timeout mechanism using `std::time::Instant`
4. Check limits on each instruction
5. Make memory size configurable
6. Add option for unbounded memory (vec growing mode)

**Testing**:
- Test step limit enforcement
- Test timeout enforcement
- Test configurable memory sizes
- Test infinite loop detection

---

#### 3.2 Improve Memory Safety
**File**: `src/bf.rs`

**Current issue**: Hard bounds at 0 and 30000

**Enhancements**:
```rust
pub enum MemoryModel {
    Fixed(usize),           // Current behavior: fixed array
    Bounded(usize),         // Grow up to limit, error beyond
    Unbounded,              // Grow as needed (with max system limit)
    Wrapping(usize),        // Wrap around (e.g., 30000 -> 0)
}
```

**Implementation steps**:
1. Create `MemoryModel` enum for different behaviors
2. Implement wrapping mode (some BF variants use this)
3. Implement growing memory (allocate on demand)
4. Add memory usage tracking and reporting
5. Add memory access logging for debugging

**Testing**:
- Test each memory model
- Test memory growth behavior
- Test memory wrapping
- Test memory limits

---

#### 3.3 Better I/O Error Handling
**File**: `src/bf.rs`

**Current issue**: EOF handling not specified, errors are opaque

**Enhancements**:
```rust
pub enum EofBehavior {
    SetZero,      // Set cell to 0 (common)
    SetNegOne,    // Set cell to 255 (some interpreters)
    NoChange,     // Leave cell unchanged
    Error,        // Return error on EOF
}

// Add to ExecutionConfig
pub struct ExecutionConfig {
    // ... existing fields ...
    pub eof_behavior: EofBehavior,
    pub output_buffering: bool,  // true = flush each char, false = buffer
}
```

**Implementation steps**:
1. Add EOF behavior configuration
2. Handle stdin EOF gracefully per config
3. Add better error messages for I/O failures
4. Optionally buffer output for performance
5. Add I/O statistics (bytes read/written)

**Testing**:
- Test each EOF behavior
- Test I/O errors (closed streams, etc.)
- Test output buffering modes
- Test with redirected I/O

---

### Phase 4: Developer Tools

#### 4.1 Add Verbose Mode
**File**: `src/main.rs`

**Changes needed**:
```rust
#[derive(Parser)]
struct Cli {
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Show detailed execution information
    #[arg(short, long)]
    verbose: bool,

    /// Validate program and show warnings
    #[arg(long)]
    validate: bool,

    /// Treat warnings as errors
    #[arg(long)]
    strict: bool,

    /// Maximum execution steps (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_steps: u64,

    /// Execution timeout in milliseconds (0 = unlimited)
    #[arg(long, default_value = "0")]
    timeout: u64,
}
```

**Implementation steps**:
1. Add CLI flags for validation and verbosity
2. Print warnings when `--validate` is used
3. Show execution stats in verbose mode (steps, memory used, time)
4. Add `--dry-run` to parse and validate without executing

**Testing**:
- Test each CLI flag
- Test flag combinations
- Test verbose output format

---

#### 4.2 Add Debug Symbols
**File**: `src/bf.rs`

**Changes needed**:
```rust
// Track original source positions for runtime errors
pub struct DebugInfo {
    source: String,
    instruction_locations: Vec<SourceLocation>,
}

// Attach to interpreter state
pub struct Interpreter {
    memory: Vec<u8>,
    pointer: usize,
    debug_info: Option<DebugInfo>,
    step_count: u64,
}
```

**Implementation steps**:
1. Store source and instruction mapping
2. Map runtime instruction index to source location
3. Include source context in runtime errors
4. Add stack trace for nested loops

**Testing**:
- Test error locations in nested loops
- Test with and without debug info
- Test stack traces

---

### Phase 5: Testing and Documentation

#### 5.1 Add Error Test Suite
**File**: `src/bf.rs` (tests module)

**New tests needed**:
```rust
#[test]
fn test_memory_underflow() { }

#[test]
fn test_memory_overflow() { }

#[test]
fn test_eof_handling() { }

#[test]
fn test_execution_timeout() { }

#[test]
fn test_step_limit() { }

#[test]
fn test_io_error_propagation() { }

#[test]
fn test_error_context_formatting() { }

#[test]
fn test_multiple_bracket_errors() { }
```

**Implementation steps**:
1. Create test programs that trigger each error
2. Verify error messages contain expected context
3. Test error recovery where applicable
4. Add integration tests for CLI error handling

---

#### 5.2 Create Error Handling Examples
**Directory**: `examples/errors/`

**Example files to create**:
- `unmatched_bracket.bf` - Demonstrates bracket errors
- `memory_overflow.bf` - Memory bounds errors
- `infinite_loop.bf` - Timeout demonstration
- `eof_test.bf` - EOF handling demonstration

**Implementation steps**:
1. Create examples directory structure
2. Write documented error examples
3. Add README explaining each error case
4. Reference in main documentation

---

#### 5.3 Update Documentation
**Files**: `README.md`, `CLAUDE.md`

**Updates needed**:
1. Document all CLI flags
2. Explain error messages and how to fix them
3. Document execution limits and configurations
4. Add troubleshooting section
5. Update CLAUDE.md with new architecture details

---

## Success Metrics

1. **Error Coverage**: All failure modes produce clear, actionable errors
2. **User Experience**: Users can diagnose issues without reading source code
3. **Reliability**: No panics or crashes on invalid input
4. **Performance**: Error checking adds < 5% overhead to execution
5. **Compatibility**: Maintains compatibility with standard BrainFuck programs

## Implementation Order

1. **Phase 1** (Error types & context) - Foundation for everything else
2. **Phase 3.1** (Execution limits) - Critical for reliability
3. **Phase 2** (Parser enhancements) - Better diagnostics
4. **Phase 4** (Developer tools) - Improved usability
5. **Phase 3.2-3.3** (Memory & I/O) - Advanced features
6. **Phase 5** (Testing & docs) - Continuous throughout

## Future Enhancements

- Syntax highlighting in error messages (with color support)
- Error recovery: attempt to continue after non-fatal errors
- Profiling mode: identify hot spots in BF programs
- Linter: suggest optimizations and best practices
- Error codes: unique identifiers for each error type

## Non-Goals

- Implementing non-standard BF extensions (separate PRD)
- JIT compilation error handling (covered in compiler PRD)
- Debugger integration (covered in debugger PRD)
