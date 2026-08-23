#!/usr/bin/env python3
"""Verify that every command line the README documents is one the binaries accept.

Documentation of a CLI rots the moment a flag moves, and nothing complains: the
README here documented `gyrus --validate` and `gyrus --minify` long after both
became `gyrus-tool` subcommands, so ten copy-pasteable examples failed outright.

This extracts every `gyrus ...` / `gyrus-tool ...` invocation from the README
and checks its flags against what clap actually reports in --help. It is
deliberately dumb about semantics: it does not run the commands, only checks
that the flags exist.

Usage: scripts/check-readme-commands.py [--readme PATH]
Exits non-zero if the README documents a flag the binary does not have.
"""
import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FLAG = re.compile(r"(--[a-z][a-z0-9-]*)")


def help_flags(argv):
    """Every long flag clap prints for a command."""
    try:
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as exc:
        sys.exit(f"error: could not run {' '.join(argv)}: {exc}")
    return set(FLAG.findall(proc.stdout + proc.stderr))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--readme", default=str(ROOT / "README.md"))
    args = ap.parse_args()

    gyrus = ROOT / "target" / "release" / "gyrus"
    tool = ROOT / "target" / "release" / "gyrus-tool"
    for binary in (gyrus, tool):
        if not binary.exists():
            sys.exit(f"error: {binary} not built. Run: cargo build --release --workspace")

    known = {"gyrus": help_flags([str(gyrus), "--help"])}
    tool_help = subprocess.run([str(tool), "--help"], capture_output=True, text=True).stdout
    # clap lists subcommands two-space-indented under "Commands:"
    subcommands = re.findall(r"^  ([a-z][a-z-]*)\s{2,}\S", tool_help, re.M)
    tool_flags = help_flags([str(tool), "--help"])
    for sub in subcommands:
        if sub != "help":
            tool_flags |= help_flags([str(tool), sub, "--help"])
    known["gyrus-tool"] = tool_flags

    text = Path(args.readme).read_text()
    problems = []
    checked = 0
    for lineno, raw in enumerate(text.split("\n"), 1):
        line = raw.strip().lstrip("$").strip()
        line = re.sub(r"^\./target/release/", "", line)
        for prog in ("gyrus-tool", "gyrus"):  # longest first
            if line.startswith(prog + " "):
                break
        else:
            continue
        checked += 1
        unknown = sorted(set(FLAG.findall(line)) - known[prog])
        if unknown:
            problems.append((lineno, raw.strip(), unknown))

    print(f"Checked {checked} documented command line(s) in {Path(args.readme).name}")
    if problems:
        print("\nFAIL: the README documents flags these binaries do not accept:\n", file=sys.stderr)
        for lineno, line, unknown in problems:
            print(f"  {Path(args.readme).name}:{lineno}: {line}", file=sys.stderr)
            print(f"      unknown: {', '.join(unknown)}\n", file=sys.stderr)
        return 1
    print("OK: every documented flag exists.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
