# The `.bfm` macro language

BrainFuck with names. A `.bfm` file is expanded to ordinary BrainFuck before
anything runs it, and the expansion carries a map from every emitted byte back
to the position that wrote it — so a runtime error reports a line of the file
somebody wrote rather than column 3,847 of a wall of punctuation.

This page is the reference for the language itself. For running and expanding a
`.bfm`, see [the manual](manual.md#run-a-macro-program) and
[development tools](tooling.md#expand); for how the expander is built, see
[architecture](architecture.md#the-macro-expander-cratesgyrus-macro).
Back to the [README](../README.md).

Everything here is checked by `scripts/check-macro-language.py`, which expands
every example on this page and compares the result to the expansion printed
beneath it.

## The shape of it

```bfm
@var a
@var b
@macro put(cell, value) {
@to cell
+{value}
}
@macro while(cell, body) {
@to cell
[
@body
@to cell
]
}
@put(a, 3)
@while(a) {
@put(b, 1)
@to a
-
}
```

```text
+++[>+<-]
```

Two named cells, a macro that sets one, a macro that takes a *block* as its
last argument, and nine instructions out the other end. Nothing in the output
moves further than it has to, because the expander tracks where the cursor is
and emits the difference.

## Values

One grammar, shared by every place a number is wanted — a repeat count, a
`@define`, a `@var`'s cell, a `@field`'s offset, a `@stride`, a `@repeat`
count, and an argument passed to a macro. A value is one of:

| Written | Means |
|---|---|
| `65` | decimal |
| `0x41`, `0X41` | hexadecimal |
| `'A'` | the byte of a character literal |
| `WIDTH` | a name declared by `@define`, or a parameter bound to one |

```bfm
@define NEWLINE 0x0A
+{NEWLINE}
```

```text
++++++++++
```

**There is no arithmetic.** `+{2+3}` and `+{WIDTH+1}` are both errors — not
"unsupported syntax" but "neither a number nor a name", which is what the
expander reports. A value that depends on another value has to be written out,
or given its own `@define`. This is the one gap in the language worth knowing
before you design around it: an offset table has to be spelled, not computed.

## Instructions, repeat counts, and comments

The eight BrainFuck instructions mean what they always mean. Any of them may
carry a repeat count in braces, which is the same instruction that many times:

```bfm
+{5}>{2}<{1}
```

```text
+++++>><
```

Brackets may not: `[{3}` is refused, because a loop repeated is not a loop that
runs three times, and the plausible reading is the wrong one.

A `*` begins a comment that runs to the end of the line. Everything after it is
prose, brackets and instructions included — a comment holds no code and no
character literals, so an apostrophe in it is an apostrophe. Characters that
are not instructions and not directives are ignored wherever they appear, as in
any BrainFuck.

## Directives start their lines

An `@` opens a directive only when nothing but spaces and tabs comes before it
on its line. Indentation is fine:

```bfm
@var a at 3
    @to a
```

```text
>>>
```

An `@` anywhere else on a line is not a directive, and if it spells one it is
refused:

```bfm
@var b at 4
+++
[ @to b + ]
```

```error
Error: '@to' at line 3, column 3 is not first on its line, so it is not a directive
```

That one used to expand to `+++[+]` and say nothing. `@to b` was meant to move
three cells right; it was read as four comment characters, and what was written
as a move became an endless loop adding one to the cell it was already on.
**One directive, one line.**

The same goes for an invocation. `+[@m]+` invoked nothing and said nothing —
the identical silence, in the shape a real `.bfm` writes far more often than it
writes `@to`, and with the identical result: an empty loop that never ends.

```bfm
@macro m {
+
}
+[@m]+
```

```error
Error: '@m' at line 4, column 3 is not first on its line, so it is not an invocation
```

Refused are the thirteen directive names and any macro or block — the `{ ... }`
handed to a macro, which [Macros](#macros) covers — bound at that point.

**What decides it is the character before the `@`, not the one after the
name.** An address has a name in front of it and a directive never does:

```bfm
+ mail bob@here.org
```

```text
+.
```

That is `bob@here.org`, not a `@here`, and `+.` is the `.` in `.org` — still an
ordinary BrainFuck instruction, as every character in a comment always was.
Asking instead what may *follow* a name means answering what a name may be
followed by, and the honest answer is anything: `@m]` is an invocation before a
bracket and `bob@to.com` is an address before a dot. No set of trailing
characters tells those apart.

`programs/third-party` has one program using `@` as a marker inside its
instruction stream and another carrying its author's email address, and
reserving the character outright would mean editing both before either could
become a `.bfm`.

Requiring the name to be *bound* is what keeps `@foo` prose, and it has a
price: a name becomes reserved by being defined above it, so a macro called
`add` makes a bare `@add` in prose an error from that line on. A `*` comment
holds one freely, and prose is what a `*` comment is for.

**One gap is left on purpose.** A directive or invocation spelled mid-line
inside a macro body nobody calls, or inside a branch no `@ifdef` takes, is
never reached and so never refused. Expansion is lazy, and making it otherwise
would mean expanding code to find out whether it is wrong.

For an `@` that really is prose and really does spell a directive, write `@@`.
It is a literal `@`, which is to say a comment character, and emits nothing. It
works anywhere, the start of a line included — an escape that covered one
position and not the other would be a rule to remember rather than a way out:

```bfm
+ @@to a
+
```

```text
++
```

The mistake in the other direction has always been caught, because there the
`@` does begin a directive and the leftovers have nowhere to go:

```text
Error: Malformed @to at line 2, column 7: '+' follows it. A @to takes the rest of its line: move this to a line of its own, or start a comment with '*'
```

There is one exception, and it is the one that makes macro bodies readable. A
body's first line begins after its `{`, so a directive may share that line:

```bfm
@var a at 3
@macro m() { @to a
}
@m()
```

```text
>>>
```

## Named cells

`@var NAME` declares a name for a cell. With `at N` it names that cell; without
it, the expander picks the *lowest* cell no `@var` has taken, so mixing the two
spellings leaves no hole:

```bfm
@var scratch at 9
@var a
@var b
@var c
@to c
```

```text
>>
```

`a`, `b` and `c` are cells 0, 1 and 2 — not 10, 11 and 12 — and reaching `c`
from cell 0 costs two moves. Two names for one explicitly numbered cell is
allowed, because naming the same cell for two phases of a program is a real
thing to want; taking a cell the *expander* chose is not, because that choice
was made on the understanding it was free and nobody saw it made.

`@to NAME` moves the cursor there, emitting the difference between where the
expander believes the cursor is and where the name says. `@here NAME` asserts a
position and emits nothing. Where the expander already knows the position,
`@here` can only agree or be wrong, and being wrong is an error:

```text
Error: '@here' at line 4, column 1 says the cursor is at cell 0, and it is at cell 1
```

That is the point of it. `@here` exists for the places the expander has *lost*
the position — after a scan loop like `[>]`, which stops wherever the data says
— and it can be trusted there precisely because it is checked everywhere else.

## Records

An array of records is walked by scan loops, which means the cursor ends up at
some record without the expander knowing which. `@stride N` declares how many
cells a record occupies, one per file, because it changes what every loop in
the file means. `@field NAME at N` names an offset *within* a record rather
than a cell of the tape:

```bfm
@stride 3
@field marker at 0
@field value at 2
@var base at 0
@to base
@here marker
@to value
```

```text
>>
```

After `@here marker` the expander knows which *field* the cursor is on but not
which record, and `@to value` emits the two moves that get from one field to
the other. An offset outside the record is refused, and `@field` before
`@stride` is refused.

## Constants

```bfm
@define WIDTH 4
>{WIDTH}
```

```text
>>>>
```

A `@define` is a name for a value, usable anywhere a value is. Redeclaring a
name is an error rather than a shadow.

## Macros

`@macro NAME(params) { body }` defines one; `@NAME(args)` expands it. Arguments
are values or names, matched to parameters positionally. A body sees its own
parameters and the file's names — never its caller's, which is what lets a
macro be read on its own.

The last argument may be a **block**, written as `{ ... }` after the argument
list on the same line, and expanded inside the body by naming it like a
directive. That is what makes a loop a macro rather than something every
program writes out, and it is how `lib/idioms.bfm` provides `@while`, `@when`
and `@unless` — see the example at the top of this page.

A block carries the scope it was written in, so it can name the things around
it rather than the things around the macro that expands it.

`@define`, `@var`, `@stride`, `@field` and `@include` are refused inside a
macro body. All five declare, and a declaration that happened once per
invocation would mean something different every time.

## Repeating a block

```bfm
@repeat 3 { +> }
```

```text
+>+>+>
```

The body is expanded in place, in the scope it is written in — it is not an
invocation, so an emitted byte names the instruction that wrote it rather than
the `@repeat` line, and it spends nothing from the invocation budget.

## Text

`@text "..."` asks `gyrus`'s `codegen` for the shortest way to print a string —
a table built by dynamic programming, multiplication loops included, at around
ten instructions a character rather than the hundred that setting each cell
from empty would cost.

```bfm
@text "Hi"
```

```text
[-]>[-]>[-]><<<----[------->++<]>.--[-->+++<]>.<<
```

It empties the cells the generated code walks over, because the table assumes
each starts at zero, and puts the cursor back where it found it. It refuses to
run over a cell somebody named — emptying a named cell is the kind of wrong
that produces a different answer rather than an error. Backslash escapes are
understood inside the quotes.

## Conditionals

`@ifdef NAME` / `@ifndef NAME`, closed by `@endif`. The test is whether the
name is declared — by `@define`, `@var`, `@field` or `@macro` — at that point.

```bfm
@ifdef NOTHING
this branch is never read: ][ @bogus
@endif
@ifndef NOTHING
+
@endif
```

```text
+
```

A branch not taken is never expanded, only stepped over, so it may hold an
unbalanced bracket, a name that does not exist, or a directive that is not one.
That is most of what a conditional is for. Testing a macro's own parameter is
refused, because a parameter is always bound and the answer would always be
yes.

## Including

`@include "path"` reads another `.bfm`, resolved relative to the directory of
the file that wrote the `@include` — not the working directory, so where a
program is run from cannot change what it means. Including the same file twice
is a no-op, so a library may include its own dependencies without a guard.

**An included file declares; it does not emit.** An instruction, a `@text`, or
a `@here` in an included file is an error naming the line that wrote it. The
reason is the source map: it holds one position per emitted byte against one
text, and a second file cannot be written in it — so an instruction from a
library would report either a line of the file that included it or a line
number in a file the reader is not looking at. Refusing to emit is the third
option, and it costs a library nothing, because a macro's bytes already name
the invocation that expanded them.

`programs/macros/lib/` has five libraries written to that rule.

## Limits

Every one of these produces an error naming the limit, rather than a hang.

| Limit | Value | What it bounds |
|---|---|---|
| Expansion | 1,000,000 | instructions a file may emit |
| Repeat | 1,000,000 | `OP{N}` and `@repeat N` |
| Invocations | 100,000 | macro expansions in one run |
| Macro depth | 64 | macros expanding macros |
| Include depth | 32 | files including files |

The expansion limit also bounds `@var NAME at N`: reaching cell N costs N
moves, so a cell past the budget is one no program could move to.

## What the language does not have

Stated because the absences are load-bearing, and because finding out by trying
is a poor way to learn them.

- **Arithmetic on values**, as above. `+{N+1}` is an error.
- **A value that is not a byte-sized idea.** Values are `u64` at expansion
  time, but everything they land in — a cell, a count, an offset — is whatever
  the tape makes of it.
- **Anything at run time.** Every directive here is resolved during expansion.
  There is no macro that depends on what a cell holds; that is what the
  BrainFuck is for.
- **A separate namespace per kind.** Constants, cells, fields and macros share
  one, so a name means one thing in a file.
