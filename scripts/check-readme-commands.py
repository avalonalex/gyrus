#!/usr/bin/env python3
"""Verify that every command line the docs contain is one the binaries accept.

Documentation of a CLI rots the moment a flag moves, and nothing complains: the
README here documented `gyrus --validate` and `gyrus --minify` long after both
became `gyrus-tool` subcommands, so ten copy-pasteable examples failed outright.

This extracts every `gyrus ...` / `gyrus-tool ...` invocation from the README
and checks its flags against what clap actually reports in --help. It is
deliberately dumb about semantics: it does not run the commands, only checks
that the flags exist.

Checks README.md and docs/*.md. PRD/ is deliberately excluded: those documents
describe features that do not exist yet, so their command lines are aspirational
by design.

Usage: scripts/check-readme-commands.py [FILE ...]
Exits non-zero if the docs use a flag the binary does not have.
"""
import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FLAG = re.compile(r"(--[a-z][a-z0-9-]*)")

# `cargo run [-p CRATE] [--release] -- ` addresses a binary in this workspace.
CARGO_RUN = re.compile(
    r"^cargo run\s+(?:-p\s+(?P<crate>[a-z-]+)\s+)?(?:--release\s+)?--\s+"
)
# Which binary each crate produces. An unqualified `cargo run` in a workspace
# with several binaries is ambiguous, but every such line in these docs means
# the interpreter, and that is the one whose flags rot.
CRATE_BINARY = {
    "": "gyrus",
    "gyrus-cli": "gyrus",
    "gyrus-tool": "gyrus-tool",
    "gyrus-debug": "gyrus-debug",
    "gyrus-tutorial": "gyrus-tutorial",
}


def help_flags(argv):
    """Every long flag clap prints for a command."""
    try:
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as exc:
        sys.exit(f"error: could not run {' '.join(argv)}: {exc}")
    return set(FLAG.findall(proc.stdout + proc.stderr))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*", help="Markdown files (default: README.md and docs/*.md)")
    args = ap.parse_args()
    targets = [Path(f) for f in args.files] or [ROOT / "README.md", *sorted((ROOT / "docs").glob("*.md"))]

    gyrus = ROOT / "target" / "release" / "gyrus"
    tool = ROOT / "target" / "release" / "gyrus-tool"
    debug = ROOT / "target" / "release" / "gyrus-debug"
    tutorial = ROOT / "target" / "release" / "gyrus-tutorial"
    for binary in (gyrus, tool, debug, tutorial):
        if not binary.exists():
            sys.exit(f"error: {binary} not built. Run: cargo build --release --workspace")

    known = {
        "gyrus": help_flags([str(gyrus), "--help"]),
        "gyrus-debug": help_flags([str(debug), "--help"]),
        "gyrus-tutorial": help_flags([str(tutorial), "--help"]),
    }
    tool_help = subprocess.run([str(tool), "--help"], capture_output=True, text=True).stdout
    # clap lists subcommands two-space-indented under "Commands:"
    subcommands = re.findall(r"^  ([a-z][a-z-]*)\s{2,}\S", tool_help, re.M)
    tool_flags = help_flags([str(tool), "--help"])
    for sub in subcommands:
        if sub != "help":
            tool_flags |= help_flags([str(tool), sub, "--help"])
    known["gyrus-tool"] = tool_flags

    problems = []
    checked = 0
    for target_file in targets:
      for lineno, raw in enumerate(target_file.read_text().split("\n"), 1):
        line = raw.strip().lstrip("$").strip()
        line = re.sub(r"^\./target/release/", "", line)
        # `cargo run -p gyrus-cli -- program.bf --flag` is the same claim as
        # `gyrus program.bf --flag`, and the docs use both spellings. Only the
        # bare one used to be checked, which is how a dozen lines in
        # docs/tooling.md kept documenting `--inspect-debug` for however long
        # it had been since that became `gyrus-tool debug-info`.
        line = CARGO_RUN.sub(lambda m: CRATE_BINARY[m.group("crate") or ""] + " ", line)
        for prog in ("gyrus-tutorial", "gyrus-debug", "gyrus-tool", "gyrus"):  # longest first
            if line.startswith(prog + " "):
                break
        else:
            continue
        checked += 1
        unknown = sorted(set(FLAG.findall(line)) - known[prog])
        if unknown:
            problems.append((target_file, lineno, raw.strip(), unknown))

    print(f"Checked {checked} documented command line(s) across {len(targets)} file(s)")
    if problems:
        print("\nFAIL: the docs use flags these binaries do not accept:\n", file=sys.stderr)
        for path, lineno, line, unknown in problems:
            rel = path.relative_to(ROOT) if path.is_absolute() else path
            print(f"  {rel}:{lineno}: {line}", file=sys.stderr)
            print(f"      unknown: {', '.join(unknown)}\n", file=sys.stderr)
        return 1
    print("OK: every documented flag exists.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
