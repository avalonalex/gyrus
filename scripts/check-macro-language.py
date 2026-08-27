#!/usr/bin/env python3
"""Verify that every example in docs/macro-language.md expands to what it says.

A language reference is a page of claims about what a program does, and the
`.bfm` language had no reference at all until one was written -- the rule for
what a repeat count may contain lived in an error string. A page of untested
claims would have been the same problem one layer up, so every example is a
```bfm block followed by the ```text block holding its expansion, and this
runs them.

A ```bfm block with no expansion beneath it is a failure rather than a
skip: an unchecked example is the one that rots.

Needs `cargo build --release -p gyrus-tool` first.

Usage: scripts/check-macro-language.py
"""
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PAGE = ROOT / "docs" / "macro-language.md"
TOOL = ROOT / "target" / "release" / "gyrus-tool"
BLOCK = re.compile(r"^```(bfm|text)\n(.*?)^```\n", re.MULTILINE | re.DOTALL)


def examples(text):
    """Each ```bfm block paired with the block that follows it, if any."""
    blocks = [(m.group(1), m.group(2), text[: m.start()].count("\n") + 1)
              for m in BLOCK.finditer(text)]
    for index, (kind, body, line) in enumerate(blocks):
        if kind != "bfm":
            continue
        following = blocks[index + 1] if index + 1 < len(blocks) else None
        expected = following[1] if following and following[0] == "text" else None
        yield line, body, expected


def expand(source):
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "example.bfm"
        path.write_text(source)
        done = subprocess.run(
            [str(TOOL), "expand", str(path)], capture_output=True, text=True
        )
        if done.returncode != 0:
            return None, done.stderr.strip() or done.stdout.strip()
        return done.stdout.strip(), None


def main():
    if not TOOL.exists():
        print(f"FAIL: {TOOL} is missing. Run: cargo build --release -p gyrus-tool",
              file=sys.stderr)
        return 1

    failures = []
    checked = 0
    for line, source, expected in examples(PAGE.read_text()):
        if expected is None:
            failures.append((line, "no expansion block follows this example", ""))
            continue
        checked += 1
        got, error = expand(source)
        if error is not None:
            failures.append((line, "the expander refused it", error))
        elif got != expected.strip():
            failures.append((line, f"expected {expected.strip()!r}", f"got {got!r}"))

    print(f"Checked {checked} example(s) in {PAGE.relative_to(ROOT)}")
    if failures:
        print(f"\nFAIL: {len(failures)} example(s) do not match:\n", file=sys.stderr)
        for line, what, detail in failures:
            print(f"  {PAGE.relative_to(ROOT)}:{line}: {what}", file=sys.stderr)
            if detail:
                print(f"    {detail}", file=sys.stderr)
        return 1
    print("OK: every example expands to the BrainFuck printed beneath it.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
