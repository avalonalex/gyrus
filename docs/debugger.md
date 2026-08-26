# The Debugger (`gyrus-debug`)

`gyrus` runs a program. `gyrus-debug` runs it one instruction at a time, with
the source, the tape, and the output on screen at once.

Back to the [README](../README.md).

```bash
cargo build --release
./target/release/gyrus-debug programs/basic/hello_world.bf
```

```
 gyrus-debug │ hello_world.bf │ breakpoint
┌ hello_world.bf  ● 1 ───────────────────┐┌ Memory  hex · follow ──────────────┐
│▶●   1 │ ++++++++[>++++[>++>+++>+++>+<<<││ptr 1   cell 1  0x01  30000 cells   │
│                                        ││                                    │
│                                        ││  addr │  0  1  2  3  4  5  6  7    │
│                                        ││     0 │ 08 01 00 00 00 00 00 00  │ │
│                                        ││     8 │ 00 00 00 00 00 00 00 00  │ │
└────────────────────────────────────────┘└────────────────────────────────────┘
┌ Output  0 bytes ─────────────────────────────────────────────────────────────┐
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
 ran 11   at 1:12   depth 1   ptr 1   cell 1   changed 2   next #11 of 103
 space step  n over  o out  c continue  g to cursor  b break  r restart  q quit
```

## Keys

Press `?` inside the debugger for this list; it is generated from the same
table the key handler uses.

**Execution**

| Key | Does |
|---|---|
| `space` | execute one instruction |
| `n` | step over: run the whole loop, if the next instruction is a `[` |
| `o` | step out: run to the end of the enclosing loop |
| `c` / `enter` | continue to the next breakpoint |
| `g` | run to the cursor |
| `p` / `esc` | pause a running program |
| `r` | restart from the beginning |
| `q` / `ctrl-c` | quit |

**Breakpoints**

| Key | Does |
|---|---|
| `b` | toggle one at the cursor |
| `B` | remove every breakpoint |

**Looking around**

| Key | Does |
|---|---|
| `tab` | move focus to the next panel |
| `↑ ↓ ← →` / `h j k l` | move within the focused panel |
| `shift ← →` | jump the cursor to the previous or next instruction |
| `pgup` / `pgdn` | page the focused panel |
| `home` / `end` | jump to the start or the end |
| `m` | memory display: hex, decimal, ASCII |
| `f` | follow the pointer on or off |
| `w` / `W` | watch a cell / stop watching one |
| `G` | scroll memory to a cell address |
| `L` | move the cursor to a source line |

**Program input**

| Key | Does |
|---|---|
| `i` | queue bytes for the program's next `,` |

## Breakpoints are columns, not lines

Most BrainFuck is written one instruction per character and many instructions
per line — `hello_world.bf` is a single line of 106 of them. A line breakpoint
would be almost useless on a program like that, so a breakpoint here names a
specific character.

Move the cursor with the arrow keys and press `b`. The cursor lands wherever
you left it, which is usually a comment character, so `b` snaps to the nearest
instruction on the cursor's line before setting anything. The character itself
is marked, and the gutter shows a `●` for any line holding a breakpoint.

From the command line, `--break` takes `LINE` or `LINE:COLUMN` and snaps the
same way:

```bash
gyrus-debug programs/basic/hello_world.bf --break 1:12 --run
```

`--run` starts the program running rather than stopping at the first
instruction, which is what you want when you have set the breakpoint you care
about.

## Stepping over and out

Step over (`n`) and step out (`o`) are both "run until execution leaves this
range of instructions", where the range is the loop's own extent — from its `[`
to the instruction after its `]`.

They are not expressed in terms of loop depth, and cannot be. At a `[`, the
depth is the same on the iteration that is about to start as it is once the
loop has finished, so a depth-based rule stops on the next iteration rather
than after the loop. The extent comes from the loop metadata the parser records
alongside the debug symbols.

`n` on an instruction that is not a `[` is an ordinary step.

## Where it stops, and where it cannot

The debugger stops **before** each instruction: the tape you are looking at is
the state that instruction is about to act on, and the `▶` marks the character
that has not run yet.

`]` is not a stopping point, because it is not an instruction. gyrus's parser
turns `[` into a `LoopCheck` at the head of the loop body and represents `]` as
the loop's structure rather than as a step — so `]` costs no step count either,
and stopping there would make the step counter appear to stall. Stepping past
the last instruction of a loop body lands on the `[` again, which is where the
condition is actually tested.

## Program input

The debugger owns the keyboard, so the program cannot read from it directly.
Bytes are queued instead, from `--input TEXT`, from `--input-file FILE`, or by
pressing `i` and typing.

When a `,` is reached with nothing queued, execution stops and says so, even in
the middle of a `continue`. Resuming without supplying anything is how you
choose EOF, and it stops asking after that — otherwise `continue` on a program
that reads to the end of its input would stop at every `,`.

Restarting replays what the program already consumed, so a program you fed by
hand does not have to be fed again.

## What it runs

The tree-walking interpreter, not the optimized one. Debugging needs a source
location for every instruction and a hook on every step, and the optimized path
deliberately has neither — `Add(5)` is one operation standing for five source
characters, which is the point of it. Expect debug-mode speed: it is the same
engine `gyrus --debug` uses.

Everything else is the same as `gyrus`: `--memory-size`, `--memory-model`,
`--cell-model`, `--unbounded-initial`, `--unbounded-max`, `--eof-behavior`,
`--max-steps`, and `--timeout` mean what they mean in
[execution models](execution-models.md).

Running under `--cell-model checked` is worth knowing about. The program stops
on the first arithmetic that leaves 0..255, the source panel points at the
instruction that did it, and the tape is still there to look at:

```bash
gyrus-debug programs/warnings/cell_overflow.bf --cell-model checked
```

## How it attaches

The whole debugger is built on the library's public surface, and adding it
required no change to `gyrus` at all. It registers an `ExecutionHook`, and
supplies its own `BfInput` and `BfOutput` so the program's bytes land in a
panel rather than in the middle of the interface.

- `before_instruction` is the stop point for every instruction, and returning
  `HookDecision::Break` unwinds the interpreter with `BfError::ExecutionPaused`
  when the user quits or restarts.
- `after_instruction` is the stop point for `[`, and only for `[`. The
  interpreter runs the `LoopCheck` at the head of a loop body itself and
  dispatches only `after_instruction` for it — *before* the check executes. So
  for that one instruction this hook point means "about to run", which is what
  a debugger needs.
- `on_loop_enter` and `on_loop_exit` maintain the stack of enclosing loops that
  `o` steps out of.
- `on_complete` captures the final tape, which is otherwise gone by the time
  the interpreter has returned.

The interpreter, the hook, and the interface all run on one thread: the
interpreter calls the hook, the hook draws and waits for a key, and only then
does the interpreter continue. Shared state sits behind a mutex because
`ExecutionHook` requires `Send`, not because anything contends for it.

While a program runs freely, the hook checks the clock every few thousand
instructions and redraws about sixteen times a second, which is also when it
takes a copy of the tape — so if the program is about to fail, the state on
screen is at most a few thousand instructions old rather than whatever it was
at the last breakpoint.

## What it does not do

- **No time travel.** Stepping backwards would mean either a snapshot per
  instruction, which a 30,000-cell tape cannot afford, or a deterministic
  replay from the start, which a program reading live input is not. The
  [tutorial](tutorial.md) does keep every step, because a lesson's tape is
  sixteen cells and its programs run for a few hundred steps.
- **No expression evaluation.** Watches are cell addresses. Anything more would
  need a `HookDecision` variant that substitutes an instruction, which does not
  exist.
- **No editing the tape.** `HookContext` is deliberately immutable; hooks
  observe and steer, they do not write.
