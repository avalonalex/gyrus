# PRD: Debug Symbols and Runtime Diagnostics

## Overview

Add debug symbol support to map runtime execution back to source code locations, enabling precise error reporting, execution tracing, and future debugger integration.

## Context: What are "Debug Symbols" in BrainFuck?

In traditional compiled languages (C, Rust, etc.), debug symbols are metadata that map machine code back to source code (line numbers, variable names, function calls). This allows debuggers to show you WHERE in your source code the program is executing or crashed.

For BrainFuck, we face a similar challenge:
- **Source code**: Human-readable BF with comments, whitespace, and formatting
- **Parsed instructions**: Stripped-down instruction stream (no comments/whitespace)
- **Runtime execution**: Just instruction indices (0, 1, 2, ...) and memory state

**The problem**: When a runtime error occurs, we only know the instruction index (e.g., "error at instruction 5042"), but we need to show the user WHERE in their source file this corresponds to.

**Example scenario**:
```brainfuck
* File: complex_program.bf (1000 lines with comments)

... (many lines of code)

+++[>++<-]    * Line 456: This is where the error happens
>>.

... (more lines)
```

**Current error (not helpful)**:
```
Error: Memory pointer out of bounds at instruction 5042
```

**Desired error (with debug symbols)**:
```
Error: Memory pointer out of bounds
  at line 456, column 3
  in loop body starting at line 456, column 4

  454 | * Process data
  455 |
  456 | +++[>++<-]    * Line 456: This is where the error happens
      |    ^
  457 | >>.

Call stack (nested loops):
  #0: Loop at line 456, column 4
  #1: Loop at line 320, column 12
  #2: Loop at line 45, column 5
```

## Goals

1. **Precise Runtime Errors**: Show exact source location for runtime errors
2. **Loop Call Stack**: Track nested loop execution (like function call stacks)
3. **Execution Tracing**: Optional mode to show what source line is executing
4. **Debugger Foundation**: Enable future step-by-step debugging
5. **Minimal Overhead**: Debug info should be optional and lightweight

## Current State

### What We Have
- ✅ Parse-time error locations (bracket mismatches)
- ✅ Source location tracking during parsing (`SourceLocation` struct)
- ✅ Error context generation with source snippets
- ✅ Instruction parsing to IR

### What We're Missing
- ❌ Mapping from runtime instruction index → source location
- ❌ Loop call stack tracking
- ❌ Runtime error messages with source context
- ❌ Execution tracing capability
- ❌ Debug info attached to instructions

## Detailed Design

### Phase 1: Instruction-to-Source Mapping

#### 1.1 Attach Source Locations to Instructions

**Current**:
```rust
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
```

**Proposed**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct InstructionWithDebug {
    pub instruction: Instruction,
    pub location: SourceLocation,  // Where in source this came from
}

// Keep the simple Instruction enum for the core logic
// Wrap it with debug info when needed
```

**Alternative approach** (less memory overhead):
```rust
// Keep instructions as-is, but maintain a parallel mapping
pub struct DebugInfo {
    pub source: String,
    pub instruction_map: Vec<SourceLocation>,  // instruction_index → location
}

// During parsing, build the map:
// instruction_map[0] = SourceLocation { line: 1, col: 1, offset: 0 }
// instruction_map[1] = SourceLocation { line: 1, col: 2, offset: 1 }
// ...
```

**Recommendation**: Use the alternative approach (parallel mapping) because:
- Smaller memory footprint (instructions are cloned frequently)
- Debug info can be optional
- Easier to disable in production

#### 1.2 Build Instruction Map During Parsing

Modify the parser to track instruction indices:

```rust
pub fn parse_with_debug(source: &str) -> Result<(Vec<Instruction>, DebugInfo), BfError> {
    let mut instructions = Vec::new();
    let mut instruction_map = Vec::new();
    let mut location = SourceLocation::new();

    for ch in source.chars() {
        match ch {
            '>' => {
                instructions.push(Instruction::IncrementPointer);
                instruction_map.push(location);
            }
            // ... for each instruction, record its source location
        }
        location.advance(ch);
    }

    let debug_info = DebugInfo {
        source: source.to_string(),
        instruction_map,
    };

    Ok((instructions, debug_info))
}
```

**Challenges**:
- Nested loops: Need to flatten instruction indices
- Line comments: Should map to the instruction AFTER the comment ends
- Minified code: May not have meaningful locations

**Solution for nested loops**:
```rust
// Use a depth-first traversal to assign flat indices
// Example: [>++[<->>]<]
// Flat indices: 0:>, 1:+, 2:+, 3:[, 4:<, 5:-, 6:>, 7:>, 8:], 9:<

fn flatten_instructions(
    instructions: &[Instruction],
    map: &mut Vec<SourceLocation>,
    current_index: &mut usize
) {
    for instr in instructions {
        match instr {
            Instruction::Loop(body) => {
                map.push(current_location); // For the '[' itself
                *current_index += 1;
                flatten_instructions(body, map, current_index); // Recurse
                map.push(current_location); // For the ']'
                *current_index += 1;
            }
            _ => {
                map.push(current_location);
                *current_index += 1;
            }
        }
    }
}
```

---

### Phase 2: Loop Call Stack Tracking

#### 2.1 Track Loop Entry/Exit

During execution, maintain a stack of loop locations:

```rust
pub struct LoopFrame {
    pub loop_start_location: SourceLocation,  // Where '[' is in source
    pub loop_body_start_instr: usize,         // Instruction index of first instr in body
    pub iteration_count: u64,                 // How many times we've looped
}

pub struct ExecutionContext {
    pub loop_stack: Vec<LoopFrame>,
    pub debug_info: Option<DebugInfo>,
}
```

**Example**:
```brainfuck
+++[        * Outer loop at line 1
  >++[      * Inner loop at line 2
    <-.     * Error happens here at line 3
  ]
  >+
]
```

When error occurs at instruction 8 (the `<` at line 3):
```rust
loop_stack = [
    LoopFrame {
        loop_start_location: SourceLocation { line: 1, col: 4 },
        iteration_count: 2
    },
    LoopFrame {
        loop_start_location: SourceLocation { line: 2, col: 6 },
        iteration_count: 5
    },
]
```

#### 2.2 Generate Call Stack on Error

```rust
fn format_runtime_error_with_stack(
    error_location: SourceLocation,
    loop_stack: &[LoopFrame],
    debug_info: &DebugInfo,
) -> String {
    let mut output = String::new();

    // Show immediate error location
    output.push_str(&format!("Error at line {}, column {}\n",
        error_location.line, error_location.column));
    output.push_str(&extract_error_context(&debug_info.source, error_location));

    // Show loop call stack
    if !loop_stack.is_empty() {
        output.push_str("\nLoop call stack:\n");
        for (i, frame) in loop_stack.iter().enumerate() {
            output.push_str(&format!(
                "  #{}: Loop at line {}, column {} (iteration {})\n",
                i,
                frame.loop_start_location.line,
                frame.loop_start_location.column,
                frame.iteration_count
            ));
        }
    }

    output
}
```

**Example output**:
```
Error: Memory pointer out of bounds
  at line 3, column 5

    1 | +++[        * Outer loop
    2 |   >++[      * Inner loop
    3 |     <-.     * Error happens here
      |     ^
    4 |   ]

Loop call stack:
  #0: Loop at line 2, column 6 (iteration 5)
  #1: Loop at line 1, column 4 (iteration 2)
```

---

### Phase 3: Execution Tracing

#### 3.1 Add Trace Mode

```rust
pub struct ExecutionConfig {
    // ... existing fields ...
    pub trace_execution: bool,  // Print each instruction as it executes
    pub trace_memory: bool,     // Show memory state with trace
}
```

#### 3.2 Implement Tracing

```rust
fn execute_with_trace(
    instruction: &Instruction,
    instruction_index: usize,
    debug_info: &DebugInfo,
    memory: &[u8],
    pointer: usize,
) {
    if config.trace_execution {
        let loc = &debug_info.instruction_map[instruction_index];
        eprintln!("[{:06}] Line {}, Col {}: {:?}",
            instruction_index, loc.line, loc.column, instruction);

        if config.trace_memory {
            eprintln!("  Memory[{}] = {}", pointer, memory[pointer]);
        }
    }

    // Execute the instruction...
}
```

**Example trace output**:
```
[000000] Line 1, Col 1: Increment
  Memory[0] = 1
[000001] Line 1, Col 2: Increment
  Memory[0] = 2
[000002] Line 1, Col 3: Increment
  Memory[0] = 3
[000003] Line 1, Col 4: Loop (enter)
  Memory[0] = 3
[000004] Line 2, Col 3: IncrementPointer
  Memory[1] = 0
[000005] Line 2, Col 4: Increment
  Memory[1] = 1
...
```

---

### Phase 4: CLI Integration

#### 4.1 Add Debug Flags

```rust
#[derive(Parser)]
struct Cli {
    // ... existing fields ...

    /// Enable debug symbols for runtime errors
    #[arg(long)]
    debug: bool,

    /// Trace execution (shows each instruction)
    #[arg(long)]
    trace: bool,

    /// Show memory state with trace
    #[arg(long, requires = "trace")]
    trace_memory: bool,
}
```

#### 4.2 Usage Examples

```bash
# Normal execution (no debug overhead)
cargo run -- program.bf

# With debug symbols (better runtime errors)
cargo run -- program.bf --debug

# Trace execution (see what's running)
cargo run -- program.bf --trace

# Trace with memory state
cargo run -- program.bf --trace --trace-memory

# Trace but limit output
cargo run -- program.bf --trace --max-steps 100
```

---

### Phase 5: Performance Considerations

#### 5.1 Memory Overhead

**Debug info size**:
- Source string: O(source_size)
- Instruction map: O(num_instructions) × sizeof(SourceLocation)
  - Each SourceLocation: ~24 bytes (3 × usize)
  - 10,000 instructions = 240 KB

**Mitigation**:
- Make debug info optional (disabled by default)
- Only build debug info when `--debug` flag is used
- Consider compressed source (store offsets only, reconstruct lines on demand)

#### 5.2 Runtime Overhead

**Loop stack tracking**:
- Push on loop entry: O(1)
- Pop on loop exit: O(1)
- Negligible performance impact

**Trace mode**:
- Significant overhead (I/O for each instruction)
- Should only be used for debugging, not production

#### 5.3 Optimization Strategy

```rust
// Fast path (production): No debug info
pub fn interpret(instructions: &[Instruction]) -> Result<(), BfError> {
    // No debug overhead
}

// Debug path: With debug info
pub fn interpret_with_debug(
    instructions: &[Instruction],
    debug_info: DebugInfo,
    config: ExecutionConfig,
) -> Result<(), BfError> {
    // Include loop stack tracking, better errors
}
```

---

## Implementation Plan

### Phase 1: Core Debug Info (Week 1)
1. Add `DebugInfo` struct with instruction map
2. Modify parser to build instruction map
3. Add `parse_with_debug()` function
4. Write tests for instruction mapping

### Phase 2: Runtime Error Enhancement (Week 1-2)
1. Add `--debug` flag to CLI
2. Pass debug info to interpreter
3. Map runtime errors to source locations
4. Update error messages to show source context
5. Test with various error scenarios

### Phase 3: Loop Call Stack (Week 2)
1. Add `LoopFrame` and execution context
2. Track loop entry/exit during execution
3. Generate call stack on errors
4. Add tests for nested loop errors

### Phase 4: Execution Tracing (Week 3)
1. Add `--trace` and `--trace-memory` flags
2. Implement trace output
3. Add trace formatting options
4. Performance testing with trace enabled

### Phase 5: Documentation and Examples (Week 3)
1. Update README with debug features
2. Create debug examples
3. Write troubleshooting guide
4. Document performance characteristics

---

## Success Metrics

1. **Error Precision**: Runtime errors show exact source line/column
2. **Call Stack Depth**: Nested loops show complete call stack
3. **Performance**: < 5% overhead with `--debug`, 0% without
4. **Usability**: Users can find bugs 5x faster (qualitative)
5. **Trace Quality**: Trace output is readable and useful

---

## Future Enhancements (Not in Scope)

### Interactive Debugger (Separate PRD)
- Breakpoints at source lines
- Step-by-step execution
- Memory inspection
- Watch expressions

### Profiling (Separate PRD)
- Hot spot detection (which loops run most)
- Instruction heat map
- Performance optimization suggestions

### Source Maps (Separate PRD)
- For minified/optimized code
- Map optimized instructions back to original source
- Similar to JavaScript source maps

---

## Open Questions

1. **Should we compress source in debug info?**
   - Pro: Saves memory for large programs
   - Con: Adds complexity
   - Decision: Start without compression, add if needed

2. **How to handle optimized/minified code?**
   - Minified code has no comments/formatting
   - Should we preserve original source separately?
   - Decision: Debug info stores original source before minification

3. **Should debug info be in a separate file?**
   - Like .pdb files for C++, .dSYM for macOS
   - Pro: Smaller runtime binary
   - Con: More complex to manage
   - Decision: Keep embedded for now (simpler)

4. **Should we support remote debugging?**
   - Debug server that listens on TCP port
   - IDE connects and controls execution
   - Decision: Out of scope, future enhancement

---

## References

- Traditional debug symbols: DWARF, PDB formats
- JavaScript source maps: Similar concept for web
- Python traceback: Loop stack similar to call stack
- GDB debugging: Inspiration for trace output format

---

## Non-Goals

- JIT compilation debug info (covered in compiler PRD)
- Visual debugger GUI (covered in debugger PRD)
- Remote debugging protocol
- Debug info encryption/obfuscation
- Backwards compatibility (this is a new feature)
