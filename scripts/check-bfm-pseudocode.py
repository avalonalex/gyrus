#!/usr/bin/env python3
"""Every `.bfm` with a loop in it says what the loop is for, and the docs can count.

BrainFuck reads a cell at a time and an algorithm does not. Naming the cells --
which is what `gyrus-macro` is for -- makes a line legible without making the
*program* legible: `@to ones` `[` `-` says what is happening and not why. So
every macro program in `programs/macros/` that contains a loop carries a block
of pseudocode in its comments, indented under a `*`, saying what it would be in
a language that has numbers and an `if`.

The check is mechanical on purpose: a comment line indented four spaces or more
is a pseudocode line. It cannot tell whether the pseudocode is *right* -- only
a reader can -- but it can tell when a program with logic in it has none, which
is the way this convention would otherwise rot.

Usage: scripts/check-bfm-pseudocode.py
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MACROS = ROOT / "programs/macros"

# A comment line carrying indented pseudocode: `*` then four spaces then text.
PSEUDOCODE = re.compile(r"^\*\s{4,}\S")


def main() -> int:
    missing = []
    checked = 0
    for path in sorted(MACROS.rglob("*.bfm")):
        text = path.read_text()
        code = "".join(c for c in text if c in "+-<>[],.")
        if "[" not in code:
            # A straight line of macro invocations is its own explanation.
            continue
        checked += 1
        if not any(PSEUDOCODE.match(line) for line in text.splitlines()):
            missing.append(path.relative_to(ROOT))

    # The counts in prose. `docs/manual.md` and `programs/README.md` both say
    # how many programs and libraries there are, and both had to be bumped by
    # hand every time one was added -- which is the kind of claim this
    # repository writes a script for rather than a comment.
    stale = count_claims()

    if missing:
        print("FAIL: a macro program with loops in it and no pseudocode:", file=sys.stderr)
        for path in missing:
            print(f"  {path}", file=sys.stderr)
        print(
            "\nAdd a block to its comments -- indented under a `*` -- saying what it\n"
            "would be in a language that has numbers. See programs/README.md.",
            file=sys.stderr,
        )
        return 1

    if stale:
        print("FAIL: a count in the documentation is stale:", file=sys.stderr)
        for line in stale:
            print(f"  {line}", file=sys.stderr)
        return 1

    print(f"OK: all {checked} macro programs with loops say what the loops are for,")
    print(f"    and the documentation still counts {len(programs())} of them.")
    return 0


WORDS = {
    1: "one", 2: "two", 3: "three", 4: "four", 5: "five", 6: "six", 7: "seven",
    8: "eight", 9: "nine", 10: "ten", 11: "eleven", 12: "twelve",
    13: "thirteen", 14: "fourteen", 15: "fifteen", 16: "sixteen",
    17: "seventeen", 18: "eighteen", 19: "nineteen", 20: "twenty",
}


def programs():
    return sorted(p for p in MACROS.glob("*.bfm"))


def libraries():
    return sorted((MACROS / "lib").glob("*.bfm"))


def count_claims():
    """The sentences in the docs that restate a directory listing."""
    wrong = []
    programs_word = WORDS[len(programs())]
    libraries_word = WORDS[len(libraries())]
    for relative, wanted in (
        ("docs/manual.md", [f"{programs_word} programs", f"{libraries_word} libraries"]),
        ("programs/README.md", [f"All {programs_word} are load-bearing"]),
    ):
        text = (ROOT / relative).read_text()
        for phrase in wanted:
            if phrase not in text:
                wrong.append(f"{relative} does not say {phrase!r}")
    return wrong


if __name__ == "__main__":
    sys.exit(main())
