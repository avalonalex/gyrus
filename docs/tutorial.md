# The Tutorial (`gyrus-tutorial`)

Thirteen lessons, numbered 0 to 12, that teach BrainFuck by running it. Each
one explains an idea, hands you a program that demonstrates it, and asks for a
variation.

Back to the [README](../README.md).

```bash
cargo build --release
./target/release/gyrus-tutorial
./target/release/gyrus-tutorial --lesson 3     # start partway in
./target/release/gyrus-tutorial --list         # the table of contents
```

```
 gyrus-tutorial │ 4 of 13 · Loops │ not yet
┌ Loops  ↑↓ to read on ──────────────────┐┌ Your program ──────────────────────┐
│[ and ] are the only branch and the only││▶    1 │ ++[>+<-]                   │
│jump in the language.                   ││                                    │
│                                        │└────────────────────────────────────┘
│  [   if the cell is zero, skip past ]  │┌ Tape · step 4 of 13 ───────────────┐
│  ]   go back to the [                  ││cell    0   1   2   3   4   5   6   │
│                                        ││value   2   0   0   0   0   0   0   │
│Run ++[>+<-] and step through it slowly ││char    ·   ·   ·   ·   ·   ·   ·   │
│watching two cells:                     ││            ▲                       │
│                                        │└────────────────────────────────────┘
│  cell 0 counts down    2, 1, 0         │┌ Output  0 bytes ───────────────────┐
│  cell 1 counts up      0, 1, 2         ││                                    │
└────────────────────────────────────────┘└────────────────────────────────────┘
 not yet: cell 2 holds 0, and the lesson asked for 5
 ← → step  home first step  tab edit  ctrl-r run again  F2 hint  ctrl-q quit
```

## The lessons

| | | |
|---|---|---|
| 0 | Welcome | the tape, the pointer, the eight commands |
| 1 | Counting | `+` and `-`, and what a byte does at its edges |
| 2 | The pointer | `>` and `<`, and why losing track of it is the classic bug |
| 3 | Loops | `[` and `]`; `[>+<-]` moves a value rather than copying it |
| 4 | Clearing | `[-]`, why it is idiomatic, and why `[+]` is not |
| 5 | Multiplication | a loop that adds, and the counter it consumes |
| 6 | Why this is enough | what Turing completeness costs and does not buy |
| 7 | Input and output | `.` and `,`, character codes, and EOF being a choice |
| 8 | Nested loops | building large numbers without typing them |
| 9 | Making a decision | an `if` is a loop that must not go round twice |
| 10 | Copying | a spare cell and two passes; what passes for a subroutine |
| 11 | Walking the tape | `[>]` and `[<]`, and building a string |
| 12 | The halting problem | why every tool here has a cutoff instead of an answer |

## Keys

Press `F1` inside the tutorial for this list.

| Key | Does |
|---|---|
| `ctrl-r` / `F5` | run your program and record every step |
| `tab` | move between typing and stepping |
| `← →` | step through the run |
| `home` / `end` | jump to the first or last step |
| `pgup` / `pgdn` | ten steps at a time |
| `F2` | reveal one more hint |
| `F3` | show an answer |
| `F4` | load that answer into the editor |
| `F6` | put the lesson's starting program back |
| `ctrl-n` / `ctrl-p` | next and previous lesson |
| `↑ ↓` | scroll the lesson text, while stepping |
| `ctrl-q` / `ctrl-c` | quit |

The chords work whether the caret is in the editor or not, so running a program
never depends on which panel has focus.

## Stepping backwards

Running a lesson program records every step of it: the tape, the pointer, the
source position, and how much had been printed. `←` and `→` then move through
that recording instantly, in both directions.

This is the opposite of what the [debugger](debugger.md) does, and deliberately
so. The debugger stops a live interpreter because a real program's 30,000-cell
tape cannot afford a copy per instruction. A lesson's tape is sixteen cells and
its programs run for a few hundred steps, so keeping all of it is cheap — and
walking backwards through `[->+<]` is the thing that makes it legible.

A run stops after 20,000 steps. A lesson snippet that runs longer than that has
gone wrong, and lesson 12 is about that cutoff being the only answer available.

## Being marked right

Each lesson states what it wants: particular cells holding particular values,
particular output, or both. Cells the lesson does not name can hold anything,
and there is usually more than one right program — `F3` shows the one the
lesson was written around, not the only one.

A few lessons ask for nothing and simply want reading; the header says `read`
rather than `solved` for those.

Two lessons add a constraint beyond the result. Lesson 8 has a character budget,
because getting 100 into a cell with a hundred `+` characters is correct and
misses the point. Lesson 9 checks the flag cell as well as the output, because
a program that prints `y` and leaves the flag set got out of the loop some
other way.

Progress is not saved between runs. `--lesson N` is how you come back to where
you were.

## How it works

The tutorial runs the same interpreter as everything else in gyrus, through the
same public API. Its only addition is an `ExecutionHook` that records the state
before each instruction, plus the same `after_instruction` special case for `[`
that the debugger uses — the interpreter dispatches only that hook point for
the `LoopCheck` standing for `[`, and does so before the check runs.

The lesson text, the starting programs, the answers, the hints, and the checks
are `crates/gyrus-tutorial/course.toml`, compiled into the binary with
`include_str!` and read by a small strict parser in `src/lesson.rs` — the same
arrangement, and the same reasoning, as the program manifest `gyrus-corpus`
reads. A key the parser does not recognise is an error rather than a lesson
quietly missing its check.

Five tests hold the course together: the file must parse; every answer must
satisfy its own lesson's check; every starting program must at least parse and
run; no starting program may already satisfy the lesson — a lesson whose
starter is the answer teaches nothing; and every starter must do what the
course says it does.

That last one is why each lesson carries a `shows_ending` and usually a
`shows_cells`. A body that says "run it and watch cell 1 reach 12" is a claim
about the code beside it, and before the course was pinned this way a starter
could be edited into disagreeing with its own paragraph without failing
anything.
