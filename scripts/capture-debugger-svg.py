#!/usr/bin/env python3
"""Regenerate the debugger screenshot in the README.

A screenshot is a claim about what the program looks like, and it is the kind
that rots in total silence: the panel titles, the status row and the key hints
have all changed at least once since the debugger was written, and a stale
image would still look perfectly plausible.

So the image is not a screenshot anyone took. It is generated: this drives the
real binary in a pty, keeps the bytes it writes, and renders them -- with the
colours the program actually emitted -- to an SVG. Text in, text out, so the
result is diffable and no binary blob enters the repository.

Usage:
    scripts/capture-debugger-svg.py            # rewrite docs/images/debugger.svg
    scripts/capture-debugger-svg.py --check    # fail if it is out of date

Needs `cargo build --release --workspace` first.
"""
import argparse
import fcntl
import html
import os
import pty
import re
import select
import struct
import sys
import termios
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target" / "release" / "gyrus-debug"
OUTPUT = ROOT / "docs" / "images" / "debugger.svg"

# The captured session. `line_comments.bf` is the program to show: it is one of
# ours, it is commented well enough to read without knowing BrainFuck, and its
# comments narrate exactly what the tape panel is displaying.
PROGRAM = "programs/basic/line_comments.bf"
ARGS = ["--break", "11:1"]
# Watch the two cells the program talks about, run to the breakpoint, and stop
# three instructions in -- with 72 built and not yet printed.
KEYS = ["w", "\x7f", "0", "\r", "w", "\x7f", "1", "\r", "c", " ", " ", " "]
COLS, ROWS = 112, 26
SETTLE = 0.35

CSI = re.compile(rb"\x1b\[([0-9;?]*)([A-Za-z])")
OSC = re.compile(rb"\x1b\][^\x07]*\x07")
CHARSET = re.compile(rb"\x1b[()][A-Za-z0-9]")

DEFAULT_FG = (0xC8, 0xC8, 0xD2)
DEFAULT_BG = (0x14, 0x16, 0x1B)
NAMED = {
    30: (0, 0, 0), 31: (220, 80, 80), 32: (120, 200, 120), 33: (220, 190, 90),
    34: (80, 150, 240), 35: (200, 120, 220), 36: (100, 200, 220), 37: (200, 200, 210),
    90: (110, 110, 120), 91: (240, 110, 110), 92: (140, 230, 140), 93: (240, 210, 120),
    94: (120, 180, 250), 95: (220, 150, 240), 96: (140, 220, 240), 97: (240, 240, 245),
}


class Cell:
    __slots__ = ("ch", "fg", "bg", "bold")

    def __init__(self):
        self.ch, self.fg, self.bg, self.bold = " ", DEFAULT_FG, DEFAULT_BG, False


def capture(argv, keys, cols, rows):
    """Run `argv` in a pty of the given size, sending `keys`, and keep its output."""
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.execv(str(argv[0]), [str(a) for a in argv])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    out = bytearray()

    def drain(seconds):
        end = time.time() + seconds
        while time.time() < end:
            ready, _, _ = select.select([fd], [], [], 0.05)
            if ready:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    return False
                if not chunk:
                    return False
                out.extend(chunk)
        return True

    drain(SETTLE)
    for key in keys:
        os.write(fd, key.encode())
        if not drain(SETTLE):
            break
    os.write(fd, b"q")
    drain(SETTLE)
    for closer in (lambda: os.close(fd), lambda: os.waitpid(pid, 0)):
        try:
            closer()
        except OSError:
            pass
    return bytes(out)


def parse(data, cols, rows):
    """Replay the escape sequences into a grid of coloured cells."""
    grid = [[Cell() for _ in range(cols)] for _ in range(rows)]
    fg, bg, bold, reverse = DEFAULT_FG, DEFAULT_BG, False, False
    row = col = i = 0
    while i < len(data):
        byte = data[i]
        if byte == 0x1B:
            match = CSI.match(data, i)
            if match:
                params, cmd = match.group(1), match.group(2)
                nums = [int(x) for x in params.split(b";") if x.isdigit()]
                if cmd == b"H":
                    row = (nums[0] - 1) if nums else 0
                    col = (nums[1] - 1) if len(nums) > 1 else 0
                elif cmd == b"J":
                    grid = [[Cell() for _ in range(cols)] for _ in range(rows)]
                    row = col = 0
                elif cmd == b"K":
                    for x in range(col, cols):
                        grid[row][x] = Cell()
                elif cmd == b"m":
                    fg, bg, bold, reverse = sgr(nums or [0], fg, bg, bold, reverse)
                i = match.end()
                continue
            for pattern in (OSC, CHARSET):
                match = pattern.match(data, i)
                if match:
                    i = match.end()
                    break
            else:
                i += 2
            continue
        if byte == 0x0D:
            col = 0
            i += 1
            continue
        if byte == 0x0A:
            row += 1
            col = 0
            i += 1
            continue
        width = 4 if byte >= 0xF0 else 3 if byte >= 0xE0 else 2 if byte >= 0xC0 else 1
        char = data[i:i + width].decode("utf-8", "replace")
        if 0 <= row < rows and 0 <= col < cols:
            cell = grid[row][col]
            cell.ch = char
            cell.fg, cell.bg = (bg, fg) if reverse else (fg, bg)
            cell.bold = bold
        col += 1
        i += width
    return grid


def sgr(nums, fg, bg, bold, reverse):
    """Apply one `ESC [ ... m` to the current attributes."""
    i = 0
    while i < len(nums):
        n = nums[i]
        if n == 0:
            fg, bg, bold, reverse = DEFAULT_FG, DEFAULT_BG, False, False
        elif n == 1:
            bold = True
        elif n == 22:
            bold = False
        elif n == 7:
            reverse = True
        elif n == 27:
            reverse = False
        elif n == 39:
            fg = DEFAULT_FG
        elif n == 49:
            bg = DEFAULT_BG
        elif 40 <= n <= 47 and n - 10 in NAMED:
            bg = NAMED[n - 10]
        elif n in NAMED:
            fg = NAMED[n]
        elif n in (38, 48) and i + 4 < len(nums) and nums[i + 1] == 2:
            colour = (nums[i + 2], nums[i + 3], nums[i + 4])
            fg, bg = (colour, bg) if n == 38 else (fg, colour)
            i += 4
        i += 1
    return fg, bg, bold, reverse


def to_svg(grid, cols, rows, size=15, pad=14):
    """One `<text>` per row, one `<tspan>` per run of identical styling.

    `textLength` pins every run to the monospace advance, so the columns line up
    even where the reader's font is not the one this was generated against.
    """
    advance, leading = size * 0.6, size * 1.32
    width, height = cols * advance + 2 * pad, rows * leading + 2 * pad
    colour = lambda triple: "#%02x%02x%02x" % triple

    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0f}" '
        f'height="{height:.0f}" viewBox="0 0 {width:.1f} {height:.1f}" '
        f'font-family="ui-monospace,SFMono-Regular,Menlo,DejaVu Sans Mono,Consolas,'
        f'monospace" font-size="{size}">',
        f'<rect width="{width:.1f}" height="{height:.1f}" rx="8" '
        f'fill="{colour(DEFAULT_BG)}"/>',
    ]

    for y, line in enumerate(grid):
        x = 0
        while x < cols:
            if line[x].bg == DEFAULT_BG:
                x += 1
                continue
            start, bg = x, line[x].bg
            while x < cols and line[x].bg == bg:
                x += 1
            out.append(
                f'<rect x="{pad + start * advance:.2f}" y="{pad + y * leading:.2f}" '
                f'width="{(x - start) * advance:.2f}" height="{leading:.2f}" '
                f'fill="{colour(bg)}"/>')

    for y, line in enumerate(grid):
        spans, x = [], 0
        while x < cols:
            blank = line[x].ch == " " and line[x].bg == DEFAULT_BG
            if blank:
                x += 1
                continue
            start, fg, bold = x, line[x].fg, line[x].bold
            while (x < cols and line[x].fg == fg and line[x].bold == bold
                   and not (line[x].ch == " " and line[x].bg == DEFAULT_BG)):
                x += 1
            run = "".join(cell.ch for cell in line[start:x])
            weight = ' font-weight="bold"' if bold else ""
            spans.append(
                f'<tspan x="{pad + start * advance:.2f}" fill="{colour(fg)}"{weight} '
                f'textLength="{(x - start) * advance:.2f}" '
                f'lengthAdjust="spacingAndGlyphs">{html.escape(run)}</tspan>')
        if spans:
            out.append(
                f'<text y="{pad + y * leading + size * 0.92:.2f}" '
                f'xml:space="preserve">{"".join(spans)}</text>')

    out.append("</svg>")
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true",
                        help="fail if the committed image is out of date")
    args = parser.parse_args()

    if not BINARY.exists():
        sys.exit(f"error: {BINARY} not built. Run: cargo build --release --workspace")

    data = capture([BINARY, ROOT / PROGRAM, *ARGS], KEYS, COLS, ROWS)
    svg = to_svg(parse(data, COLS, ROWS), COLS, ROWS)

    if args.check:
        if not OUTPUT.exists():
            sys.exit(f"FAIL: {OUTPUT.relative_to(ROOT)} does not exist. "
                     "Run: scripts/capture-debugger-svg.py")
        if OUTPUT.read_text() != svg:
            sys.exit(f"FAIL: {OUTPUT.relative_to(ROOT)} no longer matches what the "
                     "debugger draws.\nRun: scripts/capture-debugger-svg.py")
        print(f"OK: {OUTPUT.relative_to(ROOT)} matches what the debugger draws.")
        return

    OUTPUT.write_text(svg)
    print(f"Wrote {OUTPUT.relative_to(ROOT)} ({len(svg)} bytes, {COLS}x{ROWS})")


if __name__ == "__main__":
    main()
