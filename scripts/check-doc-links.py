#!/usr/bin/env python3
"""Verify that every relative link in the Markdown files points at something real.

Links rot silently whenever a file moves, and this repository moved most of its
documentation at once in August 2026. A broken link is a small thing that reads
as carelessness, and it is trivially checkable, so it is checked.

External URLs (http, https, mailto) and bare anchors are skipped: this only
validates links into the repository itself.

Usage: scripts/check-doc-links.py
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
SKIP_DIRS = {".git", "target", "node_modules"}


def markdown_files():
    for path in sorted(ROOT.rglob("*.md")):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        yield path


def main():
    broken = []
    checked = 0
    for path in markdown_files():
        for lineno, line in enumerate(path.read_text(errors="replace").split("\n"), 1):
            for target in LINK.findall(line):
                target = target.split()[0].strip("<>")  # drop link titles
                if target.startswith(("http://", "https://", "mailto:", "#")):
                    continue
                checked += 1
                resolved = (path.parent / target.split("#")[0]).resolve()
                if not resolved.exists():
                    broken.append((path.relative_to(ROOT), lineno, target))

    print(f"Checked {checked} relative link(s) across Markdown files")
    if broken:
        print(f"\nFAIL: {len(broken)} broken link(s):\n", file=sys.stderr)
        for path, lineno, target in broken:
            print(f"  {path}:{lineno} -> {target}", file=sys.stderr)
        return 1
    print("OK: every relative link resolves.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
