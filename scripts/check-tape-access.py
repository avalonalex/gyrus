#!/usr/bin/env python3
"""Check that nothing indexes the tape by the cursor except the one place allowed to.

The tape contract says a cursor may sit anywhere and only *using* it can fail.
That only holds if every read and write goes through `VmState::cell`/`cell_at`,
which is the single place the bound is enforced. `docs/architecture.md` states
this as an imperative -- "Never index `state.memory` by the cursor" -- and this
script is what makes it true rather than hoped for.

Scalar indexing of the tape (`memory[i]`) is what gets checked. Slicing
(`memory[a..b]`) is a different operation with its own bounds check and is left
alone. A line that genuinely needs to index directly says so:

    // tape-access-ok: <why this one is sound>

Run: scripts/check-tape-access.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCAN = [
    ROOT / "crates/gyrus/src/interpreter",
    ROOT / "crates/gyrus/src/config/memory_model.rs",
]

# `memory[...]` where the subscript is not a range. Ranges slice, and a slice
# carries its own check; a scalar subscript is the thing that must not be
# derived from a cursor outside the one accessor.
SCALAR_INDEX = re.compile(r"memory\[(?![^\]]*\.\.)[^\]]+\]")
MARKER = "tape-access-ok"


def rust_files():
    for target in SCAN:
        if target.is_file():
            yield target
        else:
            yield from sorted(target.rglob("*.rs"))


def main() -> int:
    violations = []
    checked = 0
    allowed = 0

    for path in rust_files():
        lines = path.read_text().splitlines()
        in_tests = False
        for n, line in enumerate(lines, start=1):
            if re.match(r"\s*#\[cfg\(test\)\]", line):
                in_tests = True
            if in_tests:
                continue
            stripped = line.strip()
            if stripped.startswith("//") or stripped.startswith("///"):
                continue
            if not SCALAR_INDEX.search(line):
                continue
            checked += 1
            context = line + (lines[n - 2] if n >= 2 else "")
            if MARKER in context:
                allowed += 1
                continue
            violations.append((path.relative_to(ROOT), n, stripped))

    print(f"Checked {checked} scalar tape index(es); {allowed} carry a {MARKER!r} note.")
    if violations:
        print()
        print("FAIL: the tape was indexed by something other than the one accessor.")
        print("Every read and write must go through VmState::cell / cell_at, which is")
        print("where the tape contract is enforced. If a site is genuinely sound, say")
        print(f"why on the line above it with a `// {MARKER}: ...` note.")
        print()
        for path, n, text in violations:
            print(f"  {path}:{n}: {text}")
        return 1

    print("OK: the tape is only indexed where the contract is enforced.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
