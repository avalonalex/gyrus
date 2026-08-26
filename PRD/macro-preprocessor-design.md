# PRD: BrainFuck Macro Preprocessor

**Status**: First slice in progress. `gyrus-macro` exists with `@define`,
repeat counts and the source map; see "What is left" below
**Last Updated**: 2026-08-26
**Priority**: Medium — the next thing to build, and started

## Decisions taken on review, 2026-08-25

The design below is kept, with four changes. Where it and this section
disagree, this section is what was decided.

**Source mapping moves to the first phase.** The plan has it in phase 4, weeks
7-8, after variables, macros and parameters -- while phase 1's own success
criteria claim "good error messages". Those cannot both be true: a macro that
expands to four thousand characters of BrainFuck and then reports a cell
overflow at column 3,847 of the *expanded* output is precisely the experience
gyrus exists to replace. Located errors are the project's identity, not a
finishing touch, and retrofitting a mapping through an expander that was not
built to carry one is the kind of work that does not get done. It is the
hardest part of the feature and it goes first.

**The language stops at phase 1.** Constants, `+{N}` repetition, named
variables with pointer tracking, and `@macro` with parameters. Conditionals,
includes, a standard library, and the assembly-like BFASM syntax of language
phase 3 are out of scope until the first slice exists and has been used to
write something real. The pointer tracking is the part that earns the feature:
manual pointer arithmetic is what makes hand-written BrainFuck unmaintainable
past a few dozen cells, and it is the one abstraction the expander can provide
that a comment cannot.

**Why it is worth building, stated honestly.** Not "more test programs" -- the
engines are already covered by the manifest corpus and the three-engine
differential, and a preprocessor emits ordinary BrainFuck that tests them no
differently. The real argument is that it is an *oracle generator*. The most
valuable test added to this repository recently is the one that compiles a
known string, runs it, and asserts the program prints that string: it proves
correctness rather than agreement between engines, which can be wrong together.
A macro system generalises that to arbitrary programs whose answer is known by
construction. Worth naming the other side too: the expander is a new surface
that needs its own tests, so on the first day it is a thing to test rather than
a way to test.

**A fifth crate, `gyrus-macro`, is right.** It is a front end and shares
nothing with execution; the workspace boundary is what keeps it that way. It
depends on `gyrus` for `SourceLocation` and the error formatting, not the other
way round.

## Decisions taken while building, 2026-08-26

**It needs no change to `gyrus`.** This was the open question and the answer is
yes. `Expansion::remap` rewrites the `DebugInfo` that `parse_with_debug`
produced for the expansion so it names the `.bfm` instead, and everything that
requires — `DebugInfo::with_source`, `record`, `lookup`, `len` — is already
public. Loop call stacks come along for free, which was not obvious: they look
like they would need the loop metadata a foreign crate cannot rebuild
(`LoopMetadata` is unnameable outside `gyrus`), but `DebugTrackingHook` builds
them from a plain `debug_info.lookup(index)`. The parallel with the debugger
holds — a second front end written entirely against the public surface.

**The CLI defaults to a located engine for `.bfm`, and gains no flag.**
`gyrus prog.bfm` runs the tree-walker with debug symbols. `--jit` and `--trace`
also carry symbols, so both keep macro locations, and `--jit` is the answer for
anyone who wants speed. There is deliberately no `--optimized`:

> The optimized interpreter is the only engine that cannot name a source
> position. A `.bfm` exists to name source positions, so it is the one engine a
> `.bfm` never runs on.

That rule keeps the flag table exactly as it is today; only the "default:
optimized" sentence in the docs becomes conditional on the extension. The
option was considered and rejected: the optimized interpreter beats the JIT
only on programs that finish in a few milliseconds, and on those the
tree-walker is fast enough that the difference buys nothing — its advantage
over the tree-walker appears only where the JIT already wins. Anyone who
genuinely wants to measure it runs `gyrus-tool expand` and then the `.bf`,
which is the honest thing to measure anyway.

The cost, stated rather than discovered later: `--no-default-features` builds
without `--jit`, so in that binary a long-running `.bfm` has only the
tree-walker and no way to trade locations for speed.

**A directive must start its line; `@` elsewhere is prose.** Reserving `@`
everywhere was the first attempt, and it hard-errors on converting an existing
program: `calc.bf`, `pi.bf` and `char.bf` all contain one in their comments,
and BrainFuck prose is free-form by convention. Line-start-only follows `cpp`,
costs nothing, and keeps directives unambiguous. `{` and `}` stay reserved
everywhere — no bundled program has either, and reserving them buys the error
for the likeliest typo there is, a space between an instruction and its count.

The other half of the rule is that a directive *owns* the rest of its line. It
used to skip to the newline silently, which discarded any instruction written
after a `@define` — and a discarded `]` went on to report an unmatched `[` that
the source plainly matched. Refusing is the only acceptable behaviour: quietly
dropping code somebody wrote is the one thing a preprocessor must not do.

**The pointer-tracking hazard is resolved, and the first answer here was
wrong.** This document said: require every loop body to have net-zero
movement. That bans `[>]`, the ordinary scan idiom, which is not a trade a
BrainFuck preprocessor can make. What shipped distinguishes a body that
returns the cursor, a body that does not and so loses the position, and a
`@to` inside the latter. The rules live in one place, `expand.rs`'s module
documentation, and the reasoning is in the commit that changed them; repeating
either here would be a third copy to keep in sync.

**`NegativePointer` is dropped.** The design lists it; it predates the tape
contract, under which movement off the tape is legal and only *access* is
checked. An expander stricter than the interpreter would be its own trap.

**Brackets are balanced in the expander, not left to the parser.** The parser
would catch an unbalanced `[`, but it renders the source context into the error
at parse time, so the position the user sees would be a column in generated
code. Checking during expansion costs a stack and gives back a position they
can act on.

**Dependencies: two, not five.** `gyrus` and `thiserror`. `regex` and
`lazy_static` are unnecessary — the scanner is a char loop, like `gyrus`'s
parser and `gyrus-corpus`'s TOML reader — and `serde`/`serde_json` only served
the JSON source map, which has no reader. When one appears, `serde_json` is
already a workspace dependency.

**Test hookup: golden expansions, not a manifest extension.** `gyrus-corpus`
learns nothing about `.bfm`. A macro program is committed alongside its
expansion, and the expansion is an ordinary `[[test]]` entry picked up by all
three engines. Teaching the manifest to expand would put `gyrus-macro` into
`gyrus`'s dev-dependency graph and make `cargo test -p gyrus` build it for
nothing. The repository already does generate-commit-diff twice —
`benchmarks/expected/` and `capture-debugger-svg.py --check`.

For `hello_world` the golden file is better still: the expansion is
byte-identical to `programs/basic/hello_world.bf`, which is already a manifest
case, so the round-trip test inherits an output that three engines already
check rather than committing a duplicate.

## What is left

The expander exists. What it does is described where every other crate's job
is, in [`docs/architecture.md`](../docs/architecture.md), and in the code; this
document keeps only what is still to be decided or built. A running inventory
of shipped behaviour is exactly the thing this directory deleted twelve
thousand lines of.

**Not built**: `gyrus-tool expand`, and `gyrus` accepting `.bfm`. The scoped
phase-1 language is complete, and so is the oracle generator that was the
stated reason for building it -- see `docs/testing.md`. What remains is
plumbing: nothing can run a `.bfm` from a terminal yet.

**A decision `@include` would force.** The origin map holds one position per
emitted byte against one source, which `@macro` fits by policy -- a byte names
the invocation, not the definition -- but a second *file* it cannot express at
all. `Expansion::origin`'s return type should be treated as not yet final.

**A gap to close in `gyrus` before the debugger can take a `.bfm`.** A remapped
`DebugInfo` carries locations but not loop metadata, because `LoopMetadata`
lives in a private module and is not re-exported, so `record_loop_metadata`
takes a type no foreign crate can build. Nothing in the error path reads it —
which is why located errors and loop call stacks are unaffected — but
`gyrus-debug` does, for loop navigation. Closing it means either exporting
`LoopMetadata` or, better, a `DebugInfo::remap` inside `gyrus` that keeps the
whole table consistent. It is not the only obstacle: the debugger's position
map holds one instruction per position, and remapping is many-to-one, since
every instruction of `+{65}` shares a column.

`crates/gyrus-macro/tests/source_locations.rs` asserts the loss, so closing it
shows up as a failing test rather than as nobody noticing.

## Overview

Add a macro preprocessor system that allows writing BrainFuck programs with high-level abstractions including named constants, variables, parameterized macros, and eventually an assembly-like syntax. This enables maintainable BF development while preserving the ability to compile down to pure BrainFuck.

**Inspiration**: Frans Faase's bfmacro system (Tufts University,
<https://www.cs.tufts.edu/~couch/bfmacro/bfmacro/>) and the classic Mandelbrot
BF program (created using C preprocessor as macro compiler).

## Goals

### Primary Goals
1. **Named memory locations** - Use symbolic names instead of manual pointer arithmetic
2. **Constants and symbols** - Define values once, reuse everywhere
3. **Parameterized macros** - Code generation with arguments
4. **Maintainability** - Large BF programs become readable and modifiable
5. **Debuggability** - Map macro source → expanded BF → runtime errors
6. **Compatibility** - Compile to standard BrainFuck (works with any BF interpreter)

### Non-Goals
- Not a new language (compiles to BF, not a replacement)
- Not adding new runtime features (no extensions to BF semantics)
- Not abandoning pure BF (macro system is optional)

## Architecture

### Pipeline Overview

```
Macro Source (.bfm) → Macro Preprocessor → Pure BF → Parser → AST → Interpreter
                      │                     │
                      │                     └─ Can save to .bf file
                      │
                      └─ Source maps for debugging
```

### Crate Structure

**New Crate**: `gyrus-macro` (`crates/gyrus-macro/`)

**Purpose**:
- Library crate for macro preprocessing
- Independent of core interpreter
- Can be used standalone (e.g., as build tool)

**Modules**:
```
src/
├── lib.rs              - Module interface
├── lexer.rs            - Tokenize macro source
├── parser.rs           - Parse macro definitions and usage
├── expander.rs         - Macro expansion engine
├── symbol_table.rs     - Track symbols, constants, variables
├── source_map.rs       - Map macro source → BF output
├── codegen.rs          - Generate BF code
└── error.rs            - Macro-specific errors
```

### Integration Points

**CLI Integration** (`gyrus-cli`):
```bash
# Execute macro source directly
gyrus program.bfm

# Expand macros and save to .bf
gyrus program.bfm --expand -o program.bf

# Show expansion only (no execution)
gyrus program.bfm --expand
```

**Tool Integration** (`gyrus-tool`):
```bash
# Expand macros
gyrus-tool expand program.bfm

# Validate macro syntax
gyrus-tool check program.bfm

# Debug macro expansion
gyrus-tool macro-debug program.bfm --step

# Generate source map
gyrus-tool source-map program.bfm --output map.json
```

## Macro Language Design

### Phase 1: Basic Macros

#### Constants
```brainfuck
@define CELL_A 0
@define CELL_B 1
@define CELL_C 2
@define NEWLINE 10
@define CHAR_A 65

* Use in code
+{CHAR_A}   * Add 65 times → +++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++
```

**Expansion Rules**:
- `{NAME}` → Expand to numeric value
- `+{N}` → Repeat `+` N times
- `-{N}` → Repeat `-` N times
- `<{N}` → Repeat `<` N times
- `>{N}` → Repeat `>` N times

#### Named Variables (Memory Locations)
```brainfuck
@var counter at 0
@var temp at 1
@var result at 2

* Variables track memory locations
* Compiler generates pointer movement code
```

#### Simple Macros
```brainfuck
@macro clear {
    [-]
}

@macro set(value) {
    [-]        * Clear cell
    +{value}   * Set to value
}

* Usage
>clear        * Move right and clear
set(65)       * Set current cell to 65
```

### Phase 2: Advanced Macros

#### Pointer Tracking
```brainfuck
@var x at 0
@var y at 1
@var z at 2

* Automatic pointer movement
@to x         * Generate code to move pointer to x
set(10)       * Now at x, set to 10

@to z         * Generate >> to reach z
+             * Increment z
```

**Code Generation**:
- Track current pointer position during expansion
- Generate minimal `>` or `<` to reach target
- Validate pointer never goes negative
- Warn if pointer exceeds expected memory

#### Conditional Macros
```brainfuck
@ifdef DEBUG
    * Debug-only code
    .
@endif

@ifndef OPTIMIZE
    * Non-optimized path
@endif
```

#### Include Files
```brainfuck
@include "stdlib.bfm"
@include "math.bfm"
```

### Phase 3: Assembly-Like Language (BFASM)

**Syntax Extension** (optional, separate from macro system):
```asm
; Comments with semicolons
mov x, 65        ; Move 65 to variable x
mov y, x         ; Copy x to y
add y, 2         ; y = y + 2
sub x, 1         ; x = x - 1
out x            ; Print value at x
in y             ; Read input to y

; Loop syntax
loop x {         ; while (x != 0)
    dec x        ; x--
    inc y        ; y++
}

; Conditional (if x != 0)
if x {
    out x
}
```

**Compilation to Macros**:
- BFASM → Macro expansion → BF code
- Layered approach: each level compiles to previous

## Detailed Design

### Symbol Table

```rust
pub struct SymbolTable {
    constants: HashMap<String, i32>,
    variables: HashMap<String, MemoryLocation>,
    macros: HashMap<String, MacroDefinition>,
    current_scope: Scope,
}

pub struct MemoryLocation {
    name: String,
    offset: usize,
    size: usize,  // Future: multi-cell variables
}

pub struct MacroDefinition {
    name: String,
    params: Vec<String>,
    body: Vec<Token>,
}
```

### Macro Expansion Engine

```rust
pub struct MacroExpander {
    symbol_table: SymbolTable,
    current_pointer: usize,  // Track pointer during expansion
    source_map: SourceMapBuilder,
}

impl MacroExpander {
    pub fn expand(&mut self, tokens: Vec<Token>) -> Result<String, MacroError>;

    fn expand_constant(&mut self, name: &str) -> Result<i32, MacroError>;
    fn expand_macro_call(&mut self, name: &str, args: Vec<Arg>) -> Result<String, MacroError>;
    fn expand_variable_ref(&mut self, name: &str) -> Result<String, MacroError>;

    fn generate_pointer_movement(&mut self, from: usize, to: usize) -> String;
    fn track_pointer(&mut self, movement: &str);
}
```

### Source Mapping

**Purpose**: Map expanded BF code back to macro source for error reporting

```rust
pub struct SourceMap {
    mappings: Vec<Mapping>,
}

pub struct Mapping {
    bf_offset: usize,           // Position in expanded BF code
    bf_line: usize,
    bf_column: usize,
    macro_offset: usize,        // Position in macro source
    macro_line: usize,
    macro_column: usize,
    macro_file: String,         // Handle includes
}

impl SourceMap {
    pub fn lookup(&self, bf_offset: usize) -> Option<&Mapping>;
    pub fn save_to_file(&self, path: &Path) -> Result<(), io::Error>;
    pub fn load_from_file(path: &Path) -> Result<Self, io::Error>;
}
```

**Format**: JSON for tooling integration
```json
{
  "version": 1,
  "macro_file": "program.bfm",
  "bf_file": "program.bf",
  "mappings": [
    {
      "bf_offset": 0,
      "bf_line": 1,
      "bf_column": 1,
      "macro_offset": 45,
      "macro_line": 5,
      "macro_column": 1,
      "macro_file": "program.bfm"
    }
  ]
}
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum MacroError {
    #[error("Undefined symbol '{name}' at {location}")]
    UndefinedSymbol {
        name: String,
        location: SourceLocation,
    },

    #[error("Macro '{name}' expects {expected} arguments, got {actual}")]
    ArgumentCountMismatch {
        name: String,
        expected: usize,
        actual: usize,
        location: SourceLocation,
    },

    #[error("Pointer would go negative (currently at {current}, attempted -{movement})")]
    NegativePointer {
        current: usize,
        movement: usize,
        location: SourceLocation,
    },

    #[error("Circular macro expansion detected: {chain}")]
    CircularMacro {
        chain: String,
        location: SourceLocation,
    },

    #[error("Cannot redefine symbol '{name}'")]
    SymbolRedefinition {
        name: String,
        original: SourceLocation,
        redefinition: SourceLocation,
    },
}
```

**Rich Error Messages** (with syntax highlighting):
```
Error: Undefined symbol 'unknown_var' at program.bfm:10:5

   8 │ @var counter at 0
   9 │ @var temp at 1
  10 │ @to unknown_var
        ^^^^^^^^^^^ undefined symbol
  11 │ set(10)

Available symbols: counter, temp
```

## Standard Library

**File**: `stdlib.bfm` (included with distribution)

```brainfuck
* Standard Library for BrainFuck Macros
* Version 0.1.0

* ===== Cell Operations =====

@macro clear {
    [-]
}

@macro set(value) {
    [-]+{value}
}

@macro inc {
    +
}

@macro dec {
    -
}

* ===== I/O Operations =====

@macro print {
    .
}

@macro read {
    ,
}

@macro print_char(ascii) {
    [-]+{ascii}.
}

@macro print_newline {
    [-]+{10}.
}

* ===== Memory Operations =====

@macro move_right(n) {
    >{n}
}

@macro move_left(n) {
    <{n}
}

@macro copy_to_next {
    * Copy current cell to next cell (destroys temp at +2)
    [->+>+<<]     * Move to next and temp
    >>[<<+>>-]    * Move temp back
    <
}

@macro move_to_next {
    * Move current cell to next (destructive)
    [->+<]
}

@macro add_to_next {
    * Add current cell to next (destructive of current)
    [->+<]
}

@macro sub_from_next {
    * Subtract current cell from next (destructive of current)
    [->-<]
}

* ===== Control Flow =====

@macro if_not_zero {
    [
}

@macro end_if {
    [-]]
}

@macro while_not_zero {
    [
}

@macro end_while {
    ]
}

* ===== Seeking =====

@macro seek_right_zero {
    * Move right until finding zero cell
    [>]
}

@macro seek_left_zero {
    * Move left until finding zero cell
    [<]
}
```

## Implementation Plan

### Phase 1: Foundation (Week 1-2)

**Deliverables**:
- ✅ Create `gyrus-macro` crate
- ✅ Implement lexer for macro syntax
- ✅ Basic symbol table
- ✅ `@define` directive
- ✅ Constant expansion (`+{N}`)
- ✅ Unit tests

**Success Criteria**:
```brainfuck
@define CHAR_A 65
+{CHAR_A}
```
Expands to: 65 `+` characters

### Phase 2: Variables and Macros (Week 3-4)

**Deliverables**:
- ✅ `@var` directive
- ✅ Pointer tracking during expansion
- ✅ `@to` directive (pointer movement)
- ✅ `@macro` directive (parameterless)
- ✅ Basic macro expansion
- ✅ Integration tests

**Success Criteria**:
```brainfuck
@var x at 0
@var y at 1

@macro clear { [-] }

@to x
clear
@to y
+
```
Expands to correct pointer movement + operations

### Phase 3: Parameterized Macros (Week 5-6)

**Deliverables**:
- ✅ Macro parameters
- ✅ Argument substitution
- ✅ Argument validation
- ✅ Error handling
- ✅ Standard library v0.1

**Success Criteria**:
```brainfuck
@include "stdlib.bfm"

@macro set(value) {
    [-]+{value}
}

set(65)
print
```
Expands and executes correctly

### Phase 4: Source Mapping (Week 7-8)

> **Superseded.** This moves to the first phase; see Decisions at the top of
> this document. It is kept here because the deliverables are still the right
> ones, only in the wrong order.


**Deliverables**:
- ✅ Source map generation
- ✅ Runtime error mapping
- ✅ Integration with error formatter
- ✅ JSON source map export

**Success Criteria**:
- Runtime errors show macro source location, not BF location
- Source maps are accurate across includes
- Debugger can step through macro source

### Phase 5: Advanced Features (Week 9-10)

**Deliverables**:
- ✅ `@ifdef`/`@ifndef` conditionals
- ✅ `@include` directive
- ✅ Multi-file macro projects
- ✅ Circular dependency detection
- ✅ Macro expansion debugger

**Success Criteria**:
- Can build multi-file BF projects
- Conditional compilation works
- Clear error messages for all edge cases

### Phase 6: CLI Integration (Week 11-12)

**Deliverables**:
- ✅ `gyrus` accepts `.bfm` files
- ✅ `--expand` flag
- ✅ `gyrus-tool expand` command
- ✅ `gyrus-tool check` command
- ✅ `gyrus-tool macro-debug` command
- ✅ Documentation and examples

**Success Criteria**:
- All CLI tools work with macros
- Seamless experience (macros feel native)
- Good error messages

### Phase 7: BFASM (Future)

**Deliverables**:
- Assembly-like syntax parser
- BFASM → Macro compiler
- Extended standard library
- Optimization pass

**Success Criteria**:
```asm
mov x, 65
out x
```
Compiles to efficient BF code

## Benefits

### For BrainFuck Developers
- **Readability**: Named variables instead of pointer arithmetic
- **Maintainability**: Change memory layout without rewriting code
- **Reusability**: Standard library of common patterns
- **Debugging**: Errors reference meaningful source, not BF gibberish
- **Productivity**: Write complex programs faster

### For gyrus Project
- **Differentiation**: First BF toolkit with modern macro system
- **Ecosystem**: Enable library development (math, string, algorithms)
- **Education**: Lower barrier to learning BF
- **Benchmarking**: Write complex benchmarks more easily
- **Testing**: Generate test cases programmatically

### For the Community
- **Preservation**: Classic BF programs can be documented with macros
- **Innovation**: Enable new algorithms previously too complex
- **Accessibility**: Make BF development practical
- **Tooling**: Foundation for IDE support, formatters, linters

## Example Use Cases

### Use Case 1: Hello World (Readable)

**Macro Source** (`hello.bfm`):
```brainfuck
@include "stdlib.bfm"

@var h at 0
@var e at 1
@var l at 2
@var o at 3

* Print "Hello"
@to h
set(72)
print

@to e
set(101)
print

@to l
set(108)
print
print

@to o
set(111)
print
```

**Benefits**:
- Clear intent (what each cell represents)
- Easy to modify (change messages)
- Self-documenting (variable names)

### Use Case 2: Fibonacci (Maintainable)

**Macro Source** (`fibonacci.bfm`):
```brainfuck
@include "stdlib.bfm"

@var a at 0
@var b at 1
@var temp at 2
@var counter at 3

* Initialize
@to a
set(0)

@to b
set(1)

@to counter
set(10)    * Print 10 numbers

* Loop
@to counter
while_not_zero {
    @to a
    print_char(a)

    * temp = a + b
    @to a
    copy_to(temp)
    @to b
    add_to(temp)

    * a = b
    @to b
    copy_to(a)

    * b = temp
    @to temp
    move_to(b)

    @to counter
    dec
}
end_while
```

**Benefits**:
- Algorithm is clear
- Variables have meaning
- Easy to adjust (number count, starting values)
- Portable (change memory layout easily)

### Use Case 3: Library Development

**Math Library** (`math.bfm`):
```brainfuck
* Math operations library

@macro multiply(x, y, result) {
    * Multiply x by y, store in result
    * Destroys x and y
    @to result
    clear

    @to y
    while_not_zero {
        @to x
        add_to(result)
        @to y
        dec
    }
    end_while
}

@macro divide(dividend, divisor, quotient, remainder) {
    * Integer division
    * quotient = dividend / divisor
    * remainder = dividend % divisor
    @to quotient
    clear

    @to dividend
    while_not_zero {
        @to divisor
        if_not_zero {
            @to dividend
            sub_from(divisor)
            @to quotient
            inc
        }
        end_if
    }
    end_while
}
```

**Benefits**:
- Reusable across projects
- Documented behavior
- Tested once, used everywhere
- Foundation for complex programs

## Testing Strategy

### Unit Tests (Macro System)

```rust
#[test]
fn test_constant_expansion() {
    let source = "@define X 5\n+{X}";
    let expanded = expand(source).unwrap();
    assert_eq!(expanded, "+++++");
}

#[test]
fn test_variable_declaration() {
    let source = "@var x at 0\n@var y at 1";
    let symbols = parse_symbols(source).unwrap();
    assert_eq!(symbols.get_var("x").unwrap().offset, 0);
    assert_eq!(symbols.get_var("y").unwrap().offset, 1);
}

#[test]
fn test_pointer_movement_generation() {
    let source = "@var x at 0\n@var y at 5\n@to y";
    let expanded = expand(source).unwrap();
    assert_eq!(expanded, ">>>>>");  // 5 right movements
}

#[test]
fn test_macro_expansion() {
    let source = "@macro clear { [-] }\nclear";
    let expanded = expand(source).unwrap();
    assert_eq!(expanded, "[-]");
}

#[test]
fn test_parameterized_macro() {
    let source = "@macro set(v) { [-]+{v} }\nset(10)";
    let expanded = expand(source).unwrap();
    assert_eq!(expanded, "[-]++++++++++");
}

#[test]
fn test_undefined_symbol_error() {
    let source = "@to unknown";
    let result = expand(source);
    assert!(matches!(result, Err(MacroError::UndefinedSymbol { .. })));
}

#[test]
fn test_circular_macro_detection() {
    let source = "@macro a { b }\n@macro b { a }";
    let result = expand(source);
    assert!(matches!(result, Err(MacroError::CircularMacro { .. })));
}
```

### Integration Tests (End-to-End)

```bash
# Test expansion
gyrus-tool expand tests/macros/hello.bfm > /tmp/hello.bf
diff /tmp/hello.bf tests/macros/hello.expected.bf

# Test execution
gyrus tests/macros/fibonacci.bfm > /tmp/output.txt
diff /tmp/output.txt tests/macros/fibonacci.expected.txt

# Test error handling
gyrus tests/macros/error_undefined.bfm 2>&1 | grep "Undefined symbol"
```

### Test Programs

Create comprehensive test suite:
- `tests/macros/basic/` - Simple expansions
- `tests/macros/variables/` - Variable tracking
- `tests/macros/stdlib/` - Standard library usage
- `tests/macros/errors/` - Error conditions
- `tests/macros/complex/` - Real-world programs

## Dependencies

**New Dependencies** (`gyrus-macro/Cargo.toml`):
```toml
[dependencies]
# Core dependencies
thiserror = "2.0"          # Error handling
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"         # Source map serialization

# Parsing
regex = "1.11"             # Macro syntax patterns
lazy_static = "1.5"        # Compiled regex

# Optional
clap = { version = "4.5", features = ["derive"] }  # If adding CLI tools
```

## Performance Considerations

### Expansion Performance
- **Target**: < 10ms for 10KB macro file
- **Strategy**: Single-pass expansion where possible
- **Optimization**: Cache expanded macro bodies

### Memory Usage
- **Symbol table**: O(n) where n = number of symbols
- **Expansion**: O(m) where m = expanded code size
- **Source map**: O(p) where p = number of mappings

### Compilation Time
- **Macro expansion**: Fast (< 100ms for typical programs)
- **BF parsing**: Existing (already fast)
- **Total overhead**: < 200ms acceptable

## Documentation Plan

### User Documentation
1. **Getting Started Guide**
   - Basic macro syntax
   - Simple examples
   - Common patterns

2. **Macro Reference**
   - All directives documented
   - Parameter types
   - Edge cases

3. **Standard Library**
   - API documentation
   - Usage examples
   - Performance notes

4. **Advanced Topics**
   - Multi-file projects
   - Source mapping
   - Debugging macros

### Developer Documentation
1. **Architecture Overview**
   - Component diagram
   - Data flow
   - Extension points

2. **API Documentation**
   - Public API surface
   - Examples
   - Best practices

3. **Contributing Guide**
   - Adding new directives
   - Extending stdlib
   - Testing guidelines

## Success Criteria

Revised on review. The originals are below for the record; they mixed things
that can be checked with things that cannot, and a coverage percentage is a
number this repository deliberately does not keep.

**The first slice is done when:**

- `@define`, `+{N}`, `@var`/`@to` with pointer tracking, and `@macro` with
  parameters all expand correctly, with unit tests on the expander.
- A runtime error in an expanded program reports the line and column in the
  `.bfm` source, not in the expansion. This is the criterion that matters: it
  is what distinguishes this from a shell script that repeats characters.
- A macro error -- undefined name, wrong arity, a `@to` that would move the
  pointer negative -- is reported with the same quality of context as a parse
  error: source line, caret, and a hint.
- Something real is written in it. A program from `programs/` rewritten as
  `.bfm` and expanded, whose output matches the original byte for byte, is
  both the proof and the first regression test.

**Deliberately not criteria:** a coverage percentage, a standard library,
multi-file projects, IDE integration, or adoption by anyone. Those are either
unmeasurable here or a different project.

<details>
<summary>Original success criteria, superseded</summary>

### Phase 1 (MVP)
- Constants expand correctly
- Variables track memory locations
- Basic macros work
- Good error messages
- 90% test coverage

### Phase 2 (Production Ready)
- Source mapping accurate
- All edge cases handled
- Standard library comprehensive
- Documentation complete
- Performance targets met

### Phase 3 (Advanced)
- Multi-file projects work seamlessly
- BFASM syntax available
- IDE integration possible
- Community adoption

</details>

## Future Extensions

### IDE Support
- **Language Server Protocol (LSP)** implementation
- Syntax highlighting for `.bfm` files
- Auto-completion for macros and variables
- Jump to definition
- Inline macro expansion preview

### Optimization Pass
- Dead code elimination
- Instruction fusion
- Constant folding (already done during expansion)
- Loop optimization

### Macro Debugger
- Step through macro expansion
- Show symbol table state
- Visualize pointer movement
- Highlight source correspondence

### Package Manager
- Publish/install macro libraries
- Dependency resolution
- Version management
- Central registry (like crates.io)

### Web Integration
- Online macro playground
- Share macro libraries
- Collaborative editing
- Live execution

## Related Work

### Similar Systems
- **bfmacro** (Frans Faase, Tufts) - Original inspiration.
  <https://www.cs.tufts.edu/~couch/bfmacro/bfmacro/>. Worth reading before
  designing anything further here: it reached the same static-pointer-tracking
  rules independently, which is reassurance rather than coincidence. Two
  things it had that this did not are now built -- cells bound automatically,
  and constants written as characters and hex. What it does not have, and this
  does, is any mapping from a runtime error back to the macro source.
- **cpp** (C preprocessor) - Used for Mandelbrot BF
- **m4** - Macro processor (could be adapted)
- **BFASM** - Assembly-like BF syntax

### Lessons Learned
- Keep macro expansion deterministic
- Rich error messages crucial
- Source mapping enables debugging
- Standard library drives adoption

### Differentiation
- **Modern Rust implementation** (safe, fast)
- **Integrated tooling** (not standalone)
- **Source maps** (debugging support)
- **IDE-ready** (LSP foundation)

## Risks and Mitigations

### Risk: Complexity Creep
- **Mitigation**: Phased approach, MVP first
- **Mitigation**: Clear scope boundaries
- **Mitigation**: Regular review against goals

### Risk: Performance Impact
- **Mitigation**: Benchmarking from day 1
- **Mitigation**: Optimization pass if needed
- **Mitigation**: Make expansion optional (`.bf` still works)

### Risk: Adoption Friction
- **Mitigation**: Excellent documentation
- **Mitigation**: Rich examples and tutorials
- **Mitigation**: Backward compatible (pure BF still works)
- **Mitigation**: Clear migration path

### Risk: Maintenance Burden
- **Mitigation**: Comprehensive test suite
- **Mitigation**: Clear architecture
- **Mitigation**: Good documentation for contributors

## Conclusion

A macro preprocessor transforms gyrus from a BrainFuck interpreter into a **complete development toolkit**. By adding high-level abstractions while preserving compatibility with pure BrainFuck, we enable:

1. **Readable code** - Named variables and meaningful structure
2. **Maintainable programs** - Easy to modify and extend
3. **Reusable libraries** - Standard library and community packages
4. **Better debugging** - Source maps connect macros to runtime
5. **Ecosystem growth** - Foundation for IDE support and tooling

This positions gyrus as the premier environment for serious BrainFuck development, educational use, and algorithmic exploration.

**Start with Phase 1 (basic macros and constants), validate with users, then expand based on feedback.**
