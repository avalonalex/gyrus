# Memory, Cells, and EOF

Three orthogonal knobs control execution semantics: what happens when a cell
outside the tape is accessed, how cell arithmetic behaves at its boundaries, and
what a read at end-of-input
does. Any combination is valid.

Back to the [README](../README.md).

## The tape contract

Both memory models share one rule about what the tape bound *means*:

> **Reading or writing a cell outside the tape is an error. Moving the cursor
> outside the tape is not** — a cursor that points nowhere valid, and is never
> used, has no effect.

So this runs cleanly, because nothing outside the five cells is ever touched:

```bash
$ gyrus --memory-size 5 program.bf     # program is: >>>>>>>>>><<<<<<<<<<+.
```

while `<+` errors, because the `+` writes a cell that does not exist. The rule
is uniform: only access counts, and where the cursor rests is never itself an
error.

### What the contract changes about diagnostics

Two consequences worth knowing when reading an error:

- **The reported instruction is the access, not the move.** `>>>>` past the end
  of the tape is fine; the `+` that follows is what fails, and that is what the
  source location points at.
- **A loop's condition can be the failing instruction**, since `[` and `]` read
  the current cell to decide. When that happens the error's loop call stack is
  one frame shallower than you might expect: a loop's condition runs before that
  loop's own frame is pushed.

Statistics follow the same rule. `Peak memory used` counts the highest cell a
program *used*, so travelling to cell 100,000 and back without touching anything
reports one cell, not 100,000 — and under `--memory-model unbounded` the tape
grows to cover cells that are used, not cells the cursor passed over.

## The JIT

`--jit` compiles the optimized program to native code with Cranelift and runs
that. It enforces the same contract as the interpreters -- every read or write
is checked against the tape, every failure is reported as the same error with
the same message -- and is held to them by test: the benchmark corpus runs
under all three engines and must agree byte for byte, under both cell models.

Where it differs, on purpose:

- **Errors carry source locations.** The JIT knows which instruction each
  failure belongs to, so `--jit` errors show the line, column and caret that
  the optimized interpreter can only show under `--debug`. The
  `instruction_index` in the error is the instruction's index in the program
  rather than the interpreter's step count.
- **Steps are loop iterations.** `--max-steps` bounds the number of loop
  iterations, and `Total steps executed` reports that number. The
  interpreter's count is in optimized-instruction units; both are
  approximations of the unfused program, and the JIT counts nothing finer.
  A loop that ends exactly at its budget completes, as in the interpreter.
- **Compile time is part of the run.** Tens of milliseconds for the largest
  programs in the corpus; a program too short to loop is faster interpreted.
- **Statistics are counted only when asked for.** Keeping the counters costs
  up to a fifth of the run, so the JIT keeps them only under `--verbose`,
  which is the only thing that prints them. With `--verbose` every statistic
  both engines define alike is equal.

Everything else -- memory models, growth on access under `--memory-model
unbounded`, cell models, EOF behaviour, the timeout, and every statistic both
engines define alike -- is the same.

## Memory Models

gyrus supports two different memory models to handle different BrainFuck variants and use cases:

### Fixed Memory (Default)

Traditional BrainFuck behavior with a fixed-size memory array.

```bash
gyrus program.bf --memory-model fixed --memory-size 30000
```

**Characteristics:**
- Memory size is fixed at startup
- Reading or writing a cell outside `0..size` returns an error; moving the
  cursor there does not (see the tape contract above)
- Most compatible with standard BrainFuck programs
- Best for production use and debugging

### Unbounded Memory

Memory grows dynamically as needed, up to a maximum limit.

```bash
gyrus program.bf --memory-model unbounded \
  --unbounded-initial 1000 \
  --unbounded-max 1000000
```

**Characteristics:**
- Starts with small initial allocation (default: 1000 bytes)
- Grows to cover cells that are *used* beyond the current size — travelling
  past the end without touching anything allocates nothing
- Maximum size limit prevents runaway memory usage
- Efficient for programs with unpredictable memory needs

**Example:**
```bash
# Start with 100 bytes, allow growth up to 10MB
gyrus program.bf --memory-model unbounded \
  --unbounded-initial 100 \
  --unbounded-max 10000000
```

### Choosing a Memory Model

- **Fixed**: Use for standard BrainFuck programs, strict bounds checking, and production/JIT targets (default)
- **Unbounded**: Use for programs with unknown memory requirements or when prototyping

## Cell Models and Arithmetic Behavior

gyrus provides **configurable cell arithmetic** to support different use cases. Cell arithmetic is completely independent from memory models - you can mix any cell model with any memory model.

### Understanding Memory vs Cell Models

gyrus distinguishes between two orthogonal (independent) configuration axes:

| Aspect | Controlled By | What It Affects |
|--------|--------------|-----------------|
| **Tape access** (a cell outside the tape) | `--memory-model` | Whether it errors or the tape grows. Movement itself always succeeds — see the tape contract above |
| **Cell arithmetic** (`+`, `-`) | `--cell-model` | How cell values increment/decrement |

These are **completely independent** - you can combine any memory model with any cell model.

### Available Cell Models

Configure cell arithmetic with the `--cell-model` flag:

#### U8 Wrapping (Default - Production)

Standard BrainFuck behavior with wrapping arithmetic. This is the **default** and aligns with traditional BrainFuck semantics and future JIT/AOT compilation.

```bash
gyrus program.bf --cell-model wrapping
```

**Characteristics:**
- Cell type: `u8` (unsigned 8-bit integer, range 0-255)
- Increment overflow: `255 + 1 = 0` (wraps to zero)
- Decrement underflow: `0 - 1 = 255` (wraps to 255)
- Use case: **Production use, standard BrainFuck programs**

**Example:**
```brainfuck
+++      * Increment 3 times
[-]      * Decrement until zero (standard clear pattern)
```

**Validation behavior:**
- `[+]` → Warning: "Inefficient pattern: loops ~256 times" (NOT infinite, just slow!)
- `[-]` → No warning (idiomatic pattern)

#### U8 Checked (Debugging)

Strict overflow detection mode that raises errors on overflow/underflow. Use this to catch bugs where your program unexpectedly reaches cell boundaries.

```bash
gyrus program.bf --cell-model checked
```

**Characteristics:**
- Cell type: `u8` (unsigned 8-bit integer, range 0-255)
- Increment overflow: `255 + 1` → **ERROR** (execution stops)
- Decrement underflow: `0 - 1` → **ERROR** (execution stops)
- Use case: **Debugging, finding arithmetic bugs**

**Example error:**
```
Error: Cell overflow at instruction 42: attempted to increment cell with value 255
```

**Validation behavior:**
- `[+]` → Warning: "Will error on overflow with checked arithmetic"
- `[-]` → No warning (will terminate at zero before underflow)

### Combining Models

Since CellModel and MemoryModel are orthogonal, all combinations are valid:

```bash
# Fixed memory + Wrapping cells (traditional BrainFuck, default)
gyrus program.bf --memory-model fixed --cell-model wrapping

# Fixed memory + Checked cells (strict debugging)
gyrus program.bf --memory-model fixed --cell-model checked

# Unbounded memory + Wrapping cells (dynamic memory, standard arithmetic)
gyrus program.bf --memory-model unbounded --cell-model wrapping
```

**Example combinations:**

| Memory Model | Cell Model | Pointer at boundary | Cell at 255, execute `+` |
|--------------|-----------|---------------------|--------------------------|
| Fixed | Wrapping | Error (out of bounds) | Wraps to 0 |
| Fixed | Checked | Error (out of bounds) | Error (overflow) |
| Unbounded | Wrapping | Grows memory | Wraps to 0 |
| Unbounded | Checked | Grows memory | Error (overflow) |

### When to Use Each Cell Model

**Use Wrapping (default) when:**
- Running standard BrainFuck programs
- In production environments
- When you want traditional BrainFuck semantics
- When preparing for JIT/AOT compilation (uses u8 wrapping)

**Use Checked when:**
- Debugging your BrainFuck programs
- Finding arithmetic overflow bugs
- Verifying your program doesn't unexpectedly hit cell boundaries
- Learning BrainFuck and want strict error checking

**Checked cells run slower, and not only because of the extra check.** The
optimizer folds a multiply loop like `[->+++<]` into a single operation that
computes `target += source * 3` in one step. Under checked cells that would
sail past an overflow the loop reports partway through, so the fold is not
applied: the loop stays a loop and executes an instruction at a time. Programs
built around multiply loops can be markedly slower under `--cell-model checked`
than under the default. That is the intended trade — checked cells exist to
report the overflow, and reporting it correctly means not folding it away.

You can see the difference for a given program:

```bash
gyrus-tool optimize --cell-model wrapping program.bf   # folds multiply loops
gyrus-tool optimize --cell-model checked  program.bf   # does not
```

### Cell-Model-Aware Validation

The validator provides different warnings based on your cell model:

```bash
# Validate with wrapping model
gyrus-tool validate program.bf --cell-model wrapping

# Validate with checked model
gyrus-tool validate program.bf --cell-model checked
```

**Example - `[+]` pattern:**

With `--cell-model wrapping`:
```
Warning: Inefficient pattern [+]
Inefficient pattern: loops ~256 times before reaching zero. Use [-] to clear a cell.
```

With `--cell-model checked`:
```
Warning: Suspicious pattern [+]
Suspicious pattern: will error on overflow with checked arithmetic.
Cell will reach 255 and then increment will panic.
```

### Practical Examples

**Production execution with wrapping:**
```bash
gyrus programs/basic/hello_world.bf --verbose
# Configuration:
#   Memory model: Fixed(30000 bytes)
#   Cell model: U8Wrapping
```

**Debug mode with overflow checking:**
```bash
gyrus my_program.bf --cell-model checked
# Will catch runtime overflow/underflow errors during execution
```

**Testing with different models:**
```bash
# Test with standard wrapping (production)
gyrus program.bf --cell-model wrapping

# Test with checked mode to find overflow bugs
gyrus program.bf --cell-model checked
```

## EOF Handling

gyrus provides configurable end-of-file (EOF) handling for the input command (`,`). Different BrainFuck implementations handle EOF differently, so you can choose the behavior that matches your needs.

### EOF Behaviors

Configure EOF handling with the `--eof-behavior` flag:

#### SetZero (Default)

Sets the current cell to 0 when EOF is reached.

```bash
gyrus program.bf --eof-behavior zero
```

This is the most common behavior and matches many BrainFuck implementations. It's useful for programs that need to detect end of input by checking for a zero value.

**Example:**
```brainfuck
,           * Read input (becomes 0 on EOF)
[           * Loop while not zero (skip if EOF)
  .         * Process the character
  ,         * Read next character
]
```

#### SetNegOne

Sets the current cell to 255 (-1 as unsigned byte) when EOF is reached.

```bash
gyrus program.bf --eof-behavior neg-one
# Alternatives: negone, -1, 255
```

Some BrainFuck programs use 255 (which represents -1 in two's complement) as an EOF marker.

#### NoChange

Leaves the cell value unchanged when EOF is reached.

```bash
gyrus program.bf --eof-behavior no-change
# Alternatives: nochange, unchanged
```

This behavior is useful when you want to preserve the previous cell value or have pre-initialized sentinel values.

#### Error

Returns an error and stops execution when EOF is reached.

```bash
gyrus program.bf --eof-behavior error
```

This is the strictest mode - use it when your program requires valid input and EOF should be treated as an exceptional condition.

**Example error:**
```
Error: End of input reached
```

### Choosing an EOF Behavior

- **SetZero**: Best for most programs, standard behavior
- **SetNegOne**: Use when porting code that expects -1 for EOF
- **NoChange**: Use when you want to preserve cell values across EOF
- **Error**: Use when EOF should terminate execution (strict mode)
