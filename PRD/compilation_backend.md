# PRD: BrainFuck Compilation Backend with Cranelift

## Executive Summary

This document explores compilation options for FerrousCortex using **Cranelift** as the code generation backend. The goal is to provide both **debug builds** (with source location tracking) and **optimized builds** (maximum performance) while avoiding the complexity of LLVM.

**Recommendation:** Hybrid approach with both JIT and AOT capabilities, starting with JIT for development velocity.

## Background

### Current State (v0.2.x)

FerrousCortex has three execution modes:
1. **Optimized interpreter** (default): 13× faster than baseline
2. **Debug interpreter** (`--debug`): Source tracking for errors
3. **Trace interpreter** (`--trace`): Profiling with heatmap

**Performance:**
- hanoi.bf: 4.62s (optimized interpreter)
- Compression: 50,565 → 7,797 instructions (6.49×)

### Goals

1. **Debug build:** Preserve source ranges, enable debugging, good error messages
2. **Opt build:** Maximum performance, strip debug info, as fast as Cranelift allows
3. **Avoid LLVM:** Too complex, heavyweight, long compile times
4. **Production-ready:** Reliable, maintainable, well-tested

### Expected Performance

| Mode | Speed | Notes |
|------|-------|-------|
| Optimized Interpreter | 13× (baseline) | Current state |
| **JIT/AOT Compiled** | **100-1000×** | Target with Cranelift |

## Option 1: JIT (Just-In-Time) Compilation

### Architecture

```
Source Code (BF)
    ↓ parse_with_debug()
Standard IR (Instruction[])
    ↓ optimize()
Optimized IR (OptimizedInstruction[])
    ↓ jit_compile()
Cranelift IR (cranelift_codegen::ir::Function)
    ↓ compile()
Machine Code (in-memory)
    ↓ execute()
Results
```

### How It Works

1. **Parse**: Convert BF source to `OptimizedInstruction` IR
2. **Translate**: Convert `OptimizedInstruction` to Cranelift IR
3. **Compile**: Cranelift generates machine code in memory
4. **Execute**: Jump to generated code, run natively

### Example Translation

**BrainFuck:**
```bf
+++[->++<]
```

**OptimizedInstruction IR:**
```rust
Add(3, SourceRange(0, 3))
Loop([
    Right(1, SourceRange(5, 6))
    Add(2, SourceRange(6, 8))
    Left(1, SourceRange(8, 9))
], SourceRange(3, 10))
```

**Cranelift IR (pseudocode):**
```
function u0:0() -> i32 {
block0:
    v0 = iconst.i64 0          ; pointer
    v1 = global_value.i64 gv0  ; memory base

    ; Add(3)
    v2 = iadd_imm v0, 0        ; cell address
    v3 = load.i8 v1+v2         ; load cell
    v4 = iadd_imm v3, 3        ; add 3
    store.i8 v4, v1+v2         ; store back

    ; Loop [Right(1), Add(2), Left(1)]
loop_start:
    v5 = load.i8 v1+v0         ; load current cell
    brz v5, loop_end           ; if zero, exit loop

    ; Right(1)
    v6 = iadd_imm v0, 1        ; pointer++

    ; Add(2)
    v7 = load.i8 v1+v6         ; load cell[1]
    v8 = iadd_imm v7, 2        ; add 2
    store.i8 v8, v1+v6         ; store back

    ; Left(1)
    v9 = iadd_imm v6, -1       ; pointer--

    jump loop_start

loop_end:
    v10 = iconst.i32 0
    return v10
}
```

### Pros

✅ **Fast development cycle**: Compile and run immediately
✅ **Interactive**: REPL, debugger integration
✅ **No executable management**: Everything in-memory
✅ **Cross-platform**: Cranelift handles target differences
✅ **Debug info**: Can emit DWARF debug info for GDB/LLDB
✅ **Smaller codebase**: No linking, no executable writing

### Cons

⚠️ **Startup overhead**: JIT compilation adds latency (~1-10ms for small programs)
⚠️ **Memory usage**: Generated code stays in memory
⚠️ **Not standalone**: Requires the JIT runtime

### Use Cases

- **Development**: Fast iteration, debugging
- **REPL**: Interactive BrainFuck shell
- **Testing**: Quick program execution
- **Short-lived programs**: Scripts, one-off executions

### Implementation Complexity

**Estimated effort:** 2-3 weeks

1. Week 1: Basic JIT pipeline
   - Cranelift IR generation for simple instructions
   - Memory model integration
   - Basic loop compilation
2. Week 2: Advanced features
   - Optimized pattern compilation (MultiplyAdd, Zero, etc.)
   - I/O integration
   - Error handling
3. Week 3: Debug support
   - Source range preservation
   - Debug info emission
   - Testing and benchmarking

## Option 2: AOT (Ahead-of-Time) Compilation

### Architecture

```
Source Code (BF)
    ↓ parse_with_debug()
Standard IR (Instruction[])
    ↓ optimize()
Optimized IR (OptimizedInstruction[])
    ↓ aot_compile()
Cranelift IR (cranelift_codegen::ir::Function)
    ↓ compile()
Object File (.o)
    ↓ link (system linker)
Standalone Executable
    ↓ execute
Results
```

### How It Works

1. **Parse & Optimize**: Same as JIT
2. **Translate**: Convert to Cranelift IR
3. **Compile**: Generate native object file (.o)
4. **Link**: Use system linker (ld, lld) to create executable
5. **Execute**: Run as standalone binary

### Pros

✅ **Zero startup overhead**: Instant execution
✅ **Standalone**: No runtime dependencies
✅ **Distributable**: Can share compiled binaries
✅ **Maximum performance**: Full optimization, no JIT overhead
✅ **Production deployment**: Perfect for long-running programs

### Cons

⚠️ **Slower development**: Compile → link → run cycle
⚠️ **Platform-specific**: Need different builds for each OS/arch
⚠️ **Larger codebase**: Need to handle linking, executable formats
⚠️ **More complexity**: Object file formats, linker integration
⚠️ **Executable management**: Need to handle file I/O, permissions

### Use Cases

- **Production**: Deploy compiled BrainFuck programs
- **Benchmarking**: Maximum performance measurements
- **Distribution**: Share compiled programs without source
- **Long-running programs**: Servers, daemons (if anyone does this with BF!)

### Implementation Complexity

**Estimated effort:** 3-4 weeks

1. Week 1-2: Object file generation
   - Cranelift object module setup
   - Basic compilation pipeline
   - Memory model runtime library
2. Week 3: Linking
   - System linker integration
   - Runtime library linking
   - Cross-platform support (macOS, Linux, Windows)
3. Week 4: Debug and optimization
   - Debug info in object files
   - Symbol tables
   - Testing and benchmarking

## Option 3: Hybrid (JIT + AOT)

### Architecture

**Shared pipeline:**
```
Source Code (BF)
    ↓ parse_with_debug()
Standard IR (Instruction[])
    ↓ optimize()
Optimized IR (OptimizedInstruction[])
    ↓
    ├─ jit_compile() → Memory → Execute
    └─ aot_compile() → Object → Link → Executable
```

### How It Works

Both JIT and AOT share the same IR translation layer:
- **JIT mode**: `ferrous-cortex-jit program.bf` (instant execution)
- **AOT mode**: `ferrous-cortex-compile program.bf -o program` (create binary)

### Pros

✅ **Best of both worlds**: Fast development + production deployment
✅ **Code reuse**: 90% shared between JIT and AOT
✅ **Flexibility**: Choose execution mode based on use case
✅ **Complete toolchain**: Development, testing, production all supported

### Cons

⚠️ **More code to maintain**: Both execution paths
⚠️ **Testing overhead**: Need to test both modes
⚠️ **Larger binary**: Both JIT and AOT compiled in

### Mitigation

- Use feature flags: `jit` and `aot` features in Cargo.toml
- Users can build JIT-only or AOT-only if needed
- Shared core translation layer minimizes duplication

## Cranelift Overview

### What is Cranelift?

Cranelift is a **fast, secure code generator** designed for:
- WebAssembly (used in Wasmtime)
- JIT compilation (used in Mozilla SpiderMonkey)
- AOT compilation
- Embedded systems

**Key features:**
- Much simpler than LLVM (~50K lines vs ~3M lines)
- Fast compilation (~1-10ms for small functions)
- Secure by design (no undefined behavior)
- Good optimization (90% of LLVM performance with 10% of complexity)
- Excellent Rust integration

### Cranelift IR

Cranelift uses a typed, SSA-based IR similar to LLVM IR:

```rust
// Example: Add 5 to a cell
let mut builder = FunctionBuilder::new(&mut func, &mut func_ctx);
let block0 = builder.create_block();
builder.append_block_params_for_function_params(block0);
builder.switch_to_block(block0);

// Load memory pointer
let mem_ptr = builder.ins().global_value(types::I64, mem_global);

// Load cell value
let offset = builder.ins().iconst(types::I64, 0);
let addr = builder.ins().iadd(mem_ptr, offset);
let value = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

// Add 5
let five = builder.ins().iconst(types::I8, 5);
let new_value = builder.ins().iadd(value, five);

// Store back
builder.ins().store(MemFlags::trusted(), new_value, addr, 0);

// Return
let zero = builder.ins().iconst(types::I32, 0);
builder.ins().return_(&[zero]);
```

### Performance Expectations

Based on Cranelift benchmarks and WebAssembly experience:

| Optimization Level | Expected Speedup | Compile Time |
|-------------------|------------------|--------------|
| No optimization | 50-100× | <1ms |
| Speed optimization | 100-500× | 1-5ms |
| Full optimization | 500-1000× | 5-10ms |

**Compared to optimized interpreter (13×):**
- JIT compiled: **7-77× additional speedup**
- Total: **100-1000× faster than baseline interpreter**

## Recommended Approach

### Phase 1: JIT Foundation (Recommended Start)

**Goal:** Get basic JIT working for development velocity

**Deliverables:**
1. `cranelift-codegen` integration
2. IR translation layer: `OptimizedInstruction → Cranelift IR`
3. Memory model runtime
4. Basic loop compilation
5. I/O function calls
6. Simple benchmarks

**Timeline:** 2-3 weeks

**Benefits:**
- Faster to implement
- Immediate performance wins
- Foundation for AOT
- Better for debugging/REPL

### Phase 2: Optimization & Debug Info

**Goal:** Maximize performance, enable debugging

**Deliverables:**
1. Pattern optimization (MultiplyAdd, Zero, etc.)
2. Source range preservation
3. Debug info emission (DWARF)
4. Comprehensive benchmarks
5. Performance tuning

**Timeline:** 2 weeks

### Phase 3: AOT Extension (Optional)

**Goal:** Standalone executables

**Deliverables:**
1. Object file generation (`cranelift-object`)
2. Linker integration
3. Executable output
4. Cross-platform support

**Timeline:** 3-4 weeks

## Implementation Details

### Crate Structure

```
crates/
├── ferrous-cortex/              # Core library (existing)
├── ferrous-cortex-cli/          # Interpreter CLI (existing)
├── ferrous-cortex-tool/         # Dev tools (existing)
├── ferrous-cortex-codegen/      # NEW: IR → Cranelift translation
│   ├── src/
│   │   ├── lib.rs
│   │   ├── translator.rs        # OptimizedInstruction → Cranelift IR
│   │   ├── runtime.rs           # Memory model runtime
│   │   ├── debug.rs             # Debug info emission
│   │   └── patterns.rs          # Optimized pattern compilation
│   └── Cargo.toml
└── ferrous-cortex-jit/          # NEW: JIT runtime
    ├── src/
    │   ├── lib.rs
    │   ├── compiler.rs          # JIT compilation pipeline
    │   ├── executor.rs          # Execute compiled code
    │   └── main.rs              # CLI for JIT execution
    └── Cargo.toml
```

### Dependencies

```toml
# ferrous-cortex-codegen/Cargo.toml
[dependencies]
cranelift-codegen = "0.109"
cranelift-frontend = "0.109"
cranelift-module = "0.109"
ferrous-cortex = { path = "../ferrous-cortex" }

# ferrous-cortex-jit/Cargo.toml
[dependencies]
cranelift-codegen = "0.109"
cranelift-jit = "0.109"
cranelift-module = "0.109"
ferrous-cortex = { path = "../ferrous-cortex" }
ferrous-cortex-codegen = { path = "../ferrous-cortex-codegen" }
```

### API Design

#### ferrous-cortex-codegen

```rust
pub struct Translator {
    config: CompilationConfig,
}

pub struct CompilationConfig {
    pub debug_info: bool,
    pub optimization_level: OptLevel,
    pub memory_model: MemoryModel,
    pub cell_model: CellModel,
}

pub enum OptLevel {
    None,      // Fast compile, slower runtime
    Speed,     // Balanced
    SpeedAndSize, // Maximum optimization
}

impl Translator {
    pub fn new(config: CompilationConfig) -> Self;

    /// Translate OptimizedInstruction IR to Cranelift IR
    pub fn translate(
        &mut self,
        instructions: &[OptimizedInstruction],
        debug_info: Option<&DebugInfo>,
    ) -> Result<cranelift_codegen::ir::Function>;
}
```

#### ferrous-cortex-jit

```rust
pub struct JitCompiler {
    translator: Translator,
    jit_module: JITModule,
}

impl JitCompiler {
    pub fn new(config: CompilationConfig) -> Result<Self>;

    /// Compile BrainFuck program to machine code
    pub fn compile(
        &mut self,
        instructions: &[OptimizedInstruction],
        debug_info: Option<&DebugInfo>,
    ) -> Result<CompiledProgram>;
}

pub struct CompiledProgram {
    entry_point: *const u8,
    debug_info: Option<DebugInfo>,
}

impl CompiledProgram {
    /// Execute compiled code
    pub fn execute(&self) -> Result<ExecutionStats>;
}
```

### CLI Integration

```bash
# Interpreter (current, default)
ferrous-cortex program.bf                  # Optimized interpreter (13×)
ferrous-cortex program.bf --debug          # Debug interpreter
ferrous-cortex program.bf --trace          # Trace interpreter

# JIT compiled (NEW)
ferrous-cortex program.bf --jit            # JIT compile and run (100-1000×)
ferrous-cortex program.bf --jit --debug    # JIT with debug info
ferrous-cortex program.bf --jit --opt-level speed  # Optimization level

# AOT compiled (FUTURE)
ferrous-cortex-compile program.bf -o program       # Create standalone binary
ferrous-cortex-compile program.bf -o program --debug  # Debug build
```

## Debug Builds vs Opt Builds

### Debug Build

**Configuration:**
```rust
CompilationConfig {
    debug_info: true,              // Emit DWARF debug info
    optimization_level: OptLevel::None,  // Fast compile
    memory_model: /* from args */,
    cell_model: CellModel::U8Checked,    // Strict checking
}
```

**Features:**
- ✅ Source location tracking
- ✅ Debug symbols for GDB/LLDB
- ✅ Runtime bounds checking
- ✅ Cell overflow detection
- ✅ Better error messages

**Performance:**
- Compile time: <1ms
- Runtime: 50-100× faster than interpreter (still great!)

**Use cases:**
- Development
- Debugging
- Testing
- Learning BrainFuck

### Opt Build

**Configuration:**
```rust
CompilationConfig {
    debug_info: false,             // Strip debug info
    optimization_level: OptLevel::SpeedAndSize, // Maximum performance
    memory_model: /* from args */,
    cell_model: CellModel::U8Wrapping,  // Fast wrapping
}
```

**Features:**
- ✅ Maximum performance
- ✅ Aggressive optimizations
- ✅ Minimal overhead
- ✅ Standalone (AOT mode)

**Performance:**
- Compile time: 5-10ms
- Runtime: **500-1000× faster than interpreter**

**Use cases:**
- Production
- Benchmarking
- Distribution
- Performance-critical applications

### Comparison

| Feature | Debug Build | Opt Build |
|---------|-------------|-----------|
| Debug symbols | ✅ DWARF | ❌ Stripped |
| Source tracking | ✅ Full | ❌ None |
| Cell checking | ✅ Checked | ❌ Wrapping (fast) |
| Optimization | ❌ None/Basic | ✅ Maximum |
| Compile time | <1ms | 5-10ms |
| Runtime speed | 50-100× | 500-1000× |
| Binary size | Larger | Smaller |
| Use case | Development | Production |

## Debug Info Embedding

### Source Range Preservation

**Strategy:** Use Cranelift's source location API

```rust
use cranelift_codegen::ir::{SourceLoc, Function};

// When translating each OptimizedInstruction
fn translate_instruction(
    &mut self,
    instruction: &OptimizedInstruction,
    builder: &mut FunctionBuilder,
) {
    // Get source range
    let source_range = instruction.source_range();

    // Create Cranelift source location
    let srcloc = SourceLoc::new(source_range.start as u32);

    // Set location for all instructions in this block
    builder.set_srcloc(srcloc);

    // Emit instructions...
}
```

### DWARF Debug Info

For AOT builds, emit DWARF debug info:

```rust
use cranelift_object::{ObjectModule, ObjectBuilder};
use gimli::write::Dwarf;

// Create debug info
let mut dwarf = Dwarf::default();

// Add compilation unit
let unit = dwarf.units.add(/* ... */);

// Map source locations to line numbers
for (instruction_offset, source_location) in debug_info {
    dwarf.line_program.add_row(
        instruction_offset,
        source_location.line,
        source_location.column,
    );
}

// Emit to object file
object_module.emit_debug_info(&dwarf);
```

**Result:** GDB/LLDB can show original BF source when debugging!

```bash
$ gdb ./program
(gdb) break main
(gdb) run
(gdb) list
1: ++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.
   ^
   Current instruction
```

## Debug Build Strategies

When compiling BrainFuck to native code, we lose the interpreter's ability to hook into every instruction. There are multiple strategies for preserving debuggability, each with different tradeoffs.

### Strategy 1: DWARF Debug Symbols (Recommended for AOT/JIT)

**Approach:** Use Cranelift's built-in DWARF support to emit industry-standard debug information.

**How it works:**
1. Map each BF instruction to source locations during compilation
2. Emit DWARF sections in the compiled binary (AOT) or in-memory (JIT)
3. Standard debuggers (GDB, LLDB) can step through BF source code

**Implementation:**
```rust
// In translator.rs
fn translate_instruction(
    &mut self,
    inst: &OptimizedInstruction,
    builder: &mut FunctionBuilder,
    debug_info: Option<&DebugInfo>,
) {
    // Set source location for this instruction
    if let Some(debug_info) = debug_info {
        let source_range = inst.source_range();
        let srcloc = SourceLoc::new(source_range.start as u32);
        builder.set_srcloc(srcloc);  // Cranelift API
    }

    // Emit Cranelift IR (all instructions inherit this source location)
    match inst {
        OptimizedInstruction::Add(n, _) => {
            let ptr = builder.use_var(pointer_var);
            let addr = builder.ins().iadd(mem_base, ptr);
            let cell = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
            let new_cell = builder.ins().iadd_imm(cell, *n as i64);
            builder.ins().store(MemFlags::trusted(), new_cell, addr, 0);
        }
        // ... other instructions
    }
}
```

**For JIT builds:**
- Debug info stays in memory
- GDB/LLDB can attach to running process
- Source locations available for crash dumps

**For AOT builds:**
```rust
use cranelift_object::ObjectModule;
use gimli::write::Dwarf;

// After compiling function
let mut dwarf = Dwarf::default();
let unit = dwarf.units.add(/* compilation unit */);

// Add line number program
for (inst_offset, source_loc) in debug_info.iter() {
    dwarf.line_program.add_row(
        inst_offset,
        source_loc.line,
        source_loc.column,
    );
}

// Embed in object file
object_module.emit_debug_info(&dwarf);
```

**Debugging experience:**
```bash
$ gdb ./compiled_bf_program
(gdb) break 456  # Set breakpoint at BF source line 456
Breakpoint 1 at 0x1234: file program.bf, line 456.
(gdb) run
Breakpoint 1, line 456 in program.bf
456 | +++[>++<-]    * This is where the error happens
    |    ^
(gdb) backtrace
#0  line 456 in program.bf
#1  loop at line 320 in program.bf
#2  loop at line 45 in program.bf
(gdb) print *memory@30000  # Inspect memory
```

**Advantages:**
- ✅ Zero runtime overhead (debug info is separate)
- ✅ Works with standard debuggers (GDB, LLDB, lldb-vscode, VS Code)
- ✅ Industry-standard approach
- ✅ Cranelift has built-in support via `builder.set_srcloc()`
- ✅ Can strip debug info for release builds (no penalty)

**Disadvantages:**
- ⚠️ Complex to implement fully (DWARF format has many features)
- ⚠️ Limited control over debug behavior (standard debuggers only)
- ⚠️ Requires understanding debug sections and formats

**When to use:**
- Default for all JIT/AOT debug builds
- Production error reporting (stack traces)
- Integration with IDEs and external tools

---

### Strategy 2: Runtime Instrumentation (For Interactive Debugger)

**Approach:** Insert function calls before/after each instruction that callback into runtime debugger.

**How it works:**
1. Compile BF program with debug hooks at each instruction
2. Runtime function checks breakpoints, updates UI, handles step-through
3. Full control over debug behavior (time-travel, custom features)

**Implementation:**
```rust
// In translator.rs (debug mode)
fn translate_with_instrumentation(
    inst: &OptimizedInstruction,
    inst_index: usize,
    builder: &mut FunctionBuilder,
) {
    // Emit call to debug hook BEFORE each instruction
    let inst_idx = builder.ins().iconst(types::I32, inst_index as i64);
    let mem_ptr = builder.use_var(memory_var);
    let ptr_val = builder.use_var(pointer_var);

    // Call: void debug_hook(u32 inst_index, *mut u8 memory, u64 pointer)
    builder.ins().call(debug_hook_func, &[inst_idx, mem_ptr, ptr_val]);

    // Now emit the actual instruction
    match inst {
        OptimizedInstruction::Add(n, _) => {
            // ... normal translation
        }
        // ... other instructions
    }
}
```

```rust
// Runtime function (in ferrous-cortex-jit or linked into AOT binary)
extern "C" fn debug_hook(
    inst_index: u32,
    memory: *mut u8,
    pointer: u64,
) {
    // Check breakpoints
    if BREAKPOINT_MANAGER.has_breakpoint_at(inst_index) {
        let loc = DEBUG_INFO.lookup(inst_index);
        println!("Breakpoint hit at line {}, column {}", loc.line, loc.column);

        // Pause execution, show debugger UI
        DEBUGGER_UI.pause_and_show(memory, pointer, loc);
    }

    // Update trace/profiler
    if TRACE_ENABLED.load(Ordering::Relaxed) {
        let loc = DEBUG_INFO.lookup(inst_index);
        println!("[{:06}] Line {}, Col {}: executing",
            inst_index, loc.line, loc.column);
    }

    // Update instruction heat map
    PROFILER.record_execution(inst_index);
}
```

**Advantages:**
- ✅ Full control over debug behavior
- ✅ Can implement custom features (time-travel, memory watchpoints, heat maps)
- ✅ Easier to implement than DWARF
- ✅ Perfect for TUI debugger integration
- ✅ Can capture execution history for replay

**Disadvantages:**
- ❌ Significant runtime overhead (50-90% slower due to function call per instruction)
- ❌ Not compatible with standard debuggers (GDB/LLDB)
- ❌ Debug build is MUCH slower than release build
- ❌ Larger compiled code size (extra calls everywhere)

**When to use:**
- Interactive TUI debugger (`ferrous-cortex --debug --interactive`)
- Execution tracing and profiling
- Teaching/learning mode with step-through
- When you need custom debug features not supported by DWARF

---

### Strategy 3: Safepoint Polling (Hybrid Approach)

**Approach:** Insert lightweight checks only at strategic points (loop headers, not every instruction).

**How it works:**
1. Emit debug checks only at loop boundaries
2. Check a global flag: if debugging is active, call debug handler
3. Hot path (normal execution) has minimal overhead (single load + branch)

**Implementation:**
```rust
fn translate_loop_with_safepoint(
    loop_body: &[OptimizedInstruction],
    loop_start_location: SourceLocation,
    builder: &mut FunctionBuilder,
) {
    let header_block = builder.create_block();
    let loop_block = builder.create_block();
    let debug_check_block = builder.create_block();
    let after_block = builder.create_block();

    builder.ins().jump(header_block, &[]);
    builder.switch_to_block(header_block);

    // SAFEPOINT: Check debug flag once per loop iteration
    let debug_flag_addr = builder.ins().global_value(types::I64, debug_flag_global);
    let debug_flag = builder.ins().load(types::I8, MemFlags::trusted(), debug_flag_addr, 0);
    let is_debugging = builder.ins().icmp_imm(IntCC::NotEqual, debug_flag, 0);

    // Branch to debug check if flag is set (cold path)
    builder.ins().brnz(is_debugging, debug_check_block, &[]);

    // Check loop condition (hot path)
    let ptr = builder.use_var(pointer_var);
    let addr = builder.ins().iadd(mem_base, ptr);
    let cell = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
    builder.ins().brz(cell, after_block, &[]);
    builder.ins().jump(loop_block, &[]);

    // Debug check block (only executed when debugging is active)
    builder.switch_to_block(debug_check_block);
    let loop_loc = builder.ins().iconst(types::I32, loop_start_location.offset as i64);
    builder.ins().call(debug_safepoint_func, &[loop_loc]);
    // Re-check loop condition after debug
    let ptr = builder.use_var(pointer_var);
    let addr = builder.ins().iadd(mem_base, ptr);
    let cell = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
    builder.ins().brz(cell, after_block, &[]);
    builder.ins().jump(loop_block, &[]);

    // Loop body (no instrumentation in body for performance)
    builder.switch_to_block(loop_block);
    for inst in loop_body {
        translate_instruction(inst, builder); // No hooks
    }
    builder.ins().jump(header_block, &[]);

    builder.seal_block(header_block);
    builder.seal_block(loop_block);
    builder.seal_block(debug_check_block);
    builder.switch_to_block(after_block);
    builder.seal_block(after_block);
}
```

**Advantages:**
- ✅ Minimal overhead when debugging is disabled (~1-2% from branch predictor)
- ✅ Can pause execution at loop boundaries (sufficient for most debugging)
- ✅ Good compromise between speed and debuggability
- ✅ Flag can be toggled at runtime (enable/disable debugging dynamically)

**Disadvantages:**
- ⚠️ Can't break/step in middle of long non-looping sequences
- ⚠️ Requires maintaining global debug state
- ⚠️ Less precise than full instrumentation

**When to use:**
- Balance between performance and debuggability
- Programs with heavy computation inside loops
- Production builds that might need occasional debugging

---

### Strategy 4: Tiered Compilation (Ultimate Flexibility)

**Approach:** Switch between interpreter (debuggable) and compiled code (fast) based on use case.

**Implementation:**
```rust
pub enum ExecutionMode {
    Interpreter,       // Full debug support via hooks (existing)
    JitInstrumented,   // Compiled with debug hooks (Strategy 2)
    JitDebug,          // Compiled with DWARF (Strategy 1)
    JitRelease,        // Compiled, no debug info (maximum speed)
}

pub fn execute_program(
    instructions: &[OptimizedInstruction],
    debug_info: &DebugInfo,
    mode: ExecutionMode,
) -> Result<ExecutionStats> {
    match mode {
        ExecutionMode::Interpreter => {
            // Use existing interpreter with hooks
            interpret_with_config(instructions, config, Some(debug_info))
        }
        ExecutionMode::JitInstrumented => {
            // Compile with debug hooks
            let compiler = JitCompiler::new_instrumented()?;
            let program = compiler.compile(instructions, Some(debug_info))?;
            program.execute()
        }
        ExecutionMode::JitDebug => {
            // Compile with DWARF
            let compiler = JitCompiler::new_debug()?;
            let program = compiler.compile(instructions, Some(debug_info))?;
            program.execute()
        }
        ExecutionMode::JitRelease => {
            // Compile for speed
            let compiler = JitCompiler::new_release()?;
            let program = compiler.compile(instructions, None)?;
            program.execute()
        }
    }
}
```

**CLI integration:**
```bash
# Interpreter - maximum debug features (existing)
ferrous-cortex program.bf --debug --trace

# JIT with DWARF - fast + debugger support
ferrous-cortex program.bf --jit --debug

# JIT instrumented - full hooks for TUI debugger
ferrous-cortex program.bf --jit --debug --instrumented

# JIT release - maximum speed
ferrous-cortex program.bf --jit
```

**Advantages:**
- ✅ User chooses appropriate tradeoff for their use case
- ✅ Maintains existing interpreter path (no regression)
- ✅ Clear separation of concerns
- ✅ Easy to add new execution modes

**Disadvantages:**
- ⚠️ More code to maintain (multiple execution paths)
- ⚠️ Need to test all modes
- ⚠️ Users need to understand which mode to use

---

## Recommended Implementation Plan

### Phase 1: JIT with DWARF (2-3 weeks)

**Goal:** Basic compiled execution with debugger support

1. Implement Strategy 1 (DWARF) in JIT mode
2. Call `builder.set_srcloc()` for each instruction
3. Test with GDB/LLDB on simple programs
4. CLI: `--jit` and `--jit --debug` flags

**Expected results:**
- 100-500× speedup over interpreter
- GDB can show BF source lines
- Breakpoints work at source level

### Phase 2: Instrumented Mode (1-2 weeks)

**Goal:** Full debug support for TUI debugger

1. Implement Strategy 2 (instrumentation)
2. Add `--instrumented` flag
3. Integrate with hook system
4. Test with complex programs

**Expected results:**
- ~50% slower than JIT debug, but much faster than interpreter
- Full control over execution (breakpoints, step, watchpoints)
- Foundation for TUI debugger

### Phase 3: Optimization (1 week)

**Goal:** Minimize debug overhead

1. Implement Strategy 3 (safepoints) as option
2. Benchmark all modes
3. Profile and optimize hot paths
4. Document performance characteristics

**Expected results:**
- Safepoint mode: <5% overhead vs release
- Clear guidance on which mode to use when

---

## Comparison Matrix

| Strategy | Runtime Overhead | Debugger | Custom Features | Complexity | Best For |
|----------|-----------------|----------|-----------------|------------|----------|
| **DWARF** | 0% | ✅ GDB/LLDB | ❌ | Medium | AOT/JIT default |
| **Instrumented** | 50-90% | ❌ | ✅ Full control | Low | TUI debugger |
| **Safepoints** | 1-5% | ⚠️ Limited | ✅ Partial | Medium | Production + debug |
| **Interpreter** | N/A | ✅ Full hooks | ✅ Full control | N/A | Already have! |

---

## Integration with Existing Architecture

FerrousCortex already has excellent foundations for debug support:

✅ **`SourceRange` in every `OptimizedInstruction`**
- Already tracks source location spans
- Maps directly to Cranelift's `SourceLoc`

✅ **`DebugInfo` with instruction_map**
- O(1) lookup from instruction index to source location
- Can be passed to compiler unchanged

✅ **`parse_with_debug()`**
- Collects debug symbols during parsing
- Ready to use in compilation pipeline

✅ **Hook system (`ExecutionHook` trait)**
- Can integrate with instrumented builds
- Callbacks for breakpoints, tracing, profiling

**Minimal integration required:**
```rust
// In translator.rs - just add this line before each instruction translation:
let srcloc = SourceLoc::new(inst.source_range().start as u32);
builder.set_srcloc(srcloc);

// That's it! Cranelift handles the rest.
```

---

## Future Enhancements

**Hot-path optimization:**
- Profile-guided optimization: instrument first, compile optimized later
- Deoptimization: fall back to interpreter when debugging is needed

**Advanced debugging:**
- Reverse execution (requires execution history)
- Conditional breakpoints based on memory state
- Watchpoints on specific cells

**IDE integration:**
- Debug Adapter Protocol (DAP) server
- VS Code extension with visual debugging
- Source-level profiling with heat maps

## Performance Benchmarks (Projected)

### hanoi.bf (Towers of Hanoi)

| Mode | Time | Speedup |
|------|------|---------|
| Baseline Interpreter | 60.22s | 1× |
| Optimized Interpreter | 4.62s | 13× |
| **JIT Debug** | **~600ms** | **~100×** |
| **JIT Opt** | **~60ms** | **~1000×** |
| **AOT Opt** | **~60ms** | **~1000×** |

### mandelbrot.bf (Complex Computation)

| Mode | Time | Speedup |
|------|------|---------|
| Baseline Interpreter | ~5 min | 1× |
| Optimized Interpreter | ~20s | 15× |
| **JIT Debug** | **~3s** | **~100×** |
| **JIT Opt** | **~300ms** | **~1000×** |
| **AOT Opt** | **~300ms** | **~1000×** |

**Note:** These are conservative estimates based on:
- Cranelift's known performance characteristics
- WebAssembly JIT benchmarks
- Similar interpreter-to-compiled speedups in other projects

## Risk Analysis

### Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Cranelift API changes | Medium | Medium | Pin to specific version, upgrade incrementally |
| Cross-platform issues | Low | High | Test on macOS, Linux, Windows early |
| Debug info complexity | Medium | Low | Start simple, iterate |
| Performance lower than expected | Low | Medium | Benchmark early, optimize hot paths |

### Project Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Scope creep | Medium | Medium | Start with JIT only, add AOT later |
| Maintenance burden | Low | Medium | Good tests, documentation |
| User adoption | Low | Low | Interpreter still available |

## Success Metrics

### Phase 1 (JIT Foundation)

✅ **Compilation works**: Can compile and execute basic BF programs
✅ **Correctness**: All existing test programs produce same output
✅ **Performance**: At least 50× faster than optimized interpreter
✅ **Reliability**: No crashes, proper error handling

### Phase 2 (Optimization)

✅ **Performance**: 100-500× speedup on hanoi.bf
✅ **Debug support**: Can preserve source locations
✅ **Benchmarks**: Comprehensive performance measurements

### Phase 3 (AOT - Optional)

✅ **Standalone**: Can generate working executables
✅ **Cross-platform**: Works on macOS, Linux, Windows
✅ **Distribution**: Can share compiled binaries

## Alternatives Considered

### LLVM

**Pros:**
- Industry standard
- Maximum optimization
- Excellent tooling

**Cons:**
- ❌ Huge complexity (3M+ lines)
- ❌ Slow compile times (100ms-1s)
- ❌ Large dependency (~100MB)
- ❌ Difficult to integrate

**Decision:** Rejected - too heavyweight for BrainFuck

### Inkwell (LLVM Rust Bindings)

**Pros:**
- Rust-friendly LLVM wrapper
- Type-safe API

**Cons:**
- ❌ Still inherits LLVM complexity
- ❌ Still slow to compile
- ❌ Limited by LLVM's API

**Decision:** Rejected - same issues as LLVM

### Direct Machine Code Generation

**Pros:**
- Maximum control
- Minimal dependencies
- Educational value

**Cons:**
- ❌ Huge implementation effort
- ❌ Platform-specific (need x86-64, ARM, etc.)
- ❌ No optimization framework
- ❌ Need to reinvent register allocation, instruction selection, etc.

**Decision:** Rejected - not worth the effort

### QBE (Lightweight Compiler Backend)

**Pros:**
- Very simple (10K lines)
- Easy to integrate
- Good documentation

**Cons:**
- ⚠️ Limited optimization
- ⚠️ Slower than Cranelift
- ⚠️ Less mature

**Decision:** Considered but Cranelift is better fit

## Conclusion

### Recommendation: Hybrid Approach (JIT Primary, AOT Secondary)

**Start with JIT** for fastest development and immediate results:
1. Phase 1: JIT foundation (2-3 weeks)
2. Phase 2: Optimization & debug (2 weeks)
3. Phase 3: AOT extension (3-4 weeks, optional)

**Key benefits:**
- ✅ **Fast iteration**: JIT enables rapid testing
- ✅ **Debug support**: Can preserve source locations and emit debug info
- ✅ **Opt support**: Can strip debug info and maximize performance
- ✅ **Extensible**: Can add AOT later without major refactoring
- ✅ **Cranelift**: Perfect fit - fast, simple, reliable

**Expected results:**
- Debug build: 50-100× faster than interpreter
- Opt build: 500-1000× faster than interpreter
- Compile time: <10ms for most programs
- Memory usage: Minimal (generated code is compact)

### Next Steps

1. **PRD Review**: Get feedback on this document
2. **Prototype**: Simple proof-of-concept (1-2 days)
   - Compile single instruction (`+++`)
   - Execute in memory
   - Verify output
3. **Implementation**: Follow phased approach
4. **Documentation**: Update user docs with compilation options
5. **Benchmarking**: Measure real-world performance

This approach provides a clear path to **100-1000× performance improvement** while maintaining FerrousCortex's focus on reliability, debuggability, and user experience.
