#!/usr/bin/env python3
"""Verify the mandelbrot statistics the macro crate's design rests on.

`crates/gyrus-macro` grew `@stride` and `@field` because of three measurements
of `programs/third-party/advanced/mandelbrot.bf`: how many loops it has, how
many of those are textually unbalanced, and how many are scans over its
nine-cell records. Those numbers are quoted in the design and in the code that
implements it, and nothing was checking them.

They are not going to change -- the file is a fixed third-party program -- which
is exactly why an unexecuted claim about it would rot unnoticed if the file were
ever replaced or re-formatted.

Usage: scripts/check-mandelbrot-claims.py
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROGRAM = ROOT / "programs/third-party/advanced/mandelbrot.bf"

# The claims, as they are written in the design and the code.
EXPECTED_LOOPS = 686
EXPECTED_UNBALANCED_PERCENT = 35
EXPECTED_SCANS = 124
STRIDE = 9


def main():
    code = "".join(c for c in PROGRAM.read_text() if c in "+-<>[],.")

    # A loop is unbalanced when the '>' and '<' between its brackets do not
    # cancel -- which is what the expander measures, and why it cannot follow
    # the cursor through one.
    net, stack, unbalanced, loops = 0, [], 0, 0
    for c in code:
        if c == "[":
            stack.append(net)
        elif c == "]":
            loops += 1
            if stack and net != stack.pop():
                unbalanced += 1
        elif c == ">":
            net += 1
        elif c == "<":
            net -= 1

    scans = len(re.findall(rf"\[>{{{STRIDE}}}\]|\[<{{{STRIDE}}}\]", code))
    percent = 100 * unbalanced // loops

    failures = []
    for name, got, want in [
        ("loops", loops, EXPECTED_LOOPS),
        ("unbalanced percent", percent, EXPECTED_UNBALANCED_PERCENT),
        (f"scans of {STRIDE} cells", scans, EXPECTED_SCANS),
    ]:
        print(f"  {name:24} {got}")
        if got != want:
            failures.append(f"{name}: claimed {want}, measured {got}")

    if failures:
        print("\nFAIL: the design cites numbers this program no longer has:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        print(
            "\nThey appear in PRD/macro-preprocessor-design.md and "
            "crates/gyrus-macro/src/expand.rs.",
            file=sys.stderr,
        )
        return 1
    print("\nOK: mandelbrot is still the program the macro design was measured against.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
