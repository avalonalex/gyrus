#!/usr/bin/env bash
# Benchmark and profile the interpreter on real BrainFuck programs.
#
# Measures end-to-end wall clock for the `gyrus` binary: parse, optimize,
# execute, write output. For per-phase numbers with warmup and outlier
# rejection, use the criterion benches instead (`cargo bench`).
#
# Every run is diffed against a recorded golden output in benchmarks/expected/.
# A run that got faster while producing different bytes is not a faster
# interpreter, so this script fails instead of reporting the number. Where a
# program is fast enough to also run under --debug, the two interpreters are
# compared against each other, which makes this a differential test of the
# optimizer as well as a benchmark.
#
# Usage:
#   scripts/benchmark.sh                 # time every program, verify output
#   scripts/benchmark.sh --full          # also run --debug on the slow ones
#   scripts/benchmark.sh --record        # regenerate the golden outputs
#   scripts/benchmark.sh --profile [PROG] # execution profile via --trace
set -uo pipefail

cd "$(dirname "$0")/.."

GYRUS=target/release/gyrus
GOLDEN=benchmarks/expected

# path:run_debug_by_default — the debug interpreter is ~40x slower, so for the
# heavy programs it is opt-in via --full rather than part of every run.
PROGRAMS=(
    "programs/basic/hello_world.bf:yes"
    "programs/third-party/advanced/99beer.bf:yes"
    "programs/third-party/advanced/triangle.bf:yes"
    "programs/third-party/advanced/squares.bf:yes"
    "programs/third-party/advanced/bf2c.bf:yes"
    "programs/third-party/advanced/hanoi.bf:no"
    "programs/third-party/advanced/mandelbrot.bf:no"
)

FULL=0; RECORD=0; PROFILE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --full)    FULL=1 ;;
        --record)  RECORD=1 ;;
        --profile) PROFILE="${2:-programs/third-party/advanced/squares.bf}"; shift ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

[ -x "$GYRUS" ] || { echo "error: $GYRUS not built. Run: cargo build --release --workspace" >&2; exit 1; }

ms_now() { python3 -c 'import time; print(time.perf_counter() * 1000)'; }
elapsed() { python3 -c "print(f'{$2 - $1:.0f}')"; }

# --- profile mode -----------------------------------------------------------
if [ -n "$PROFILE" ]; then
    [ -f "$PROFILE" ] || { echo "error: no such program: $PROFILE" >&2; exit 1; }
    echo "Profiling $PROFILE (--trace implies --debug, so this is the slow path)"
    echo
    # The heatmap is ANSI art meant for a terminal; the loop profile is the part
    # worth reading in a transcript, so show that and point at the full output.
    "$GYRUS" --trace "$PROFILE" < /dev/null 2>&1 >/dev/null \
        | sed -n '/BrainFuck Profiling Results/,$p' \
        | sed 's/\x1b\[[0-9;]*m//g'
    echo
    echo "Full heatmap:  $GYRUS --trace $PROFILE"
    exit 0
fi

# --- record mode ------------------------------------------------------------
if [ "$RECORD" = 1 ]; then
    mkdir -p "$GOLDEN"
    for entry in "${PROGRAMS[@]}"; do
        prog="${entry%%:*}"
        name=$(basename "$prog" .bf)
        "$GYRUS" "$prog" < /dev/null > "$GOLDEN/$name.txt" 2>/dev/null
        printf '  %-24s %8s bytes\n' "$name.txt" "$(wc -c < "$GOLDEN/$name.txt" | tr -d ' ')"
    done
    echo "Recorded golden outputs to $GOLDEN/"
    exit 0
fi

# --- benchmark mode ---------------------------------------------------------
printf '%-46s %10s %12s %8s\n' "program" "optimized" "--debug" "speedup"
printf '%-46s %10s %12s %8s\n' "$(printf '%.0s-' {1..46})" "----------" "------------" "--------"

failures=0
for entry in "${PROGRAMS[@]}"; do
    prog="${entry%%:*}"
    want_debug="${entry##*:}"
    name=$(basename "$prog" .bf)
    golden="$GOLDEN/$name.txt"

    t0=$(ms_now); "$GYRUS" "$prog" < /dev/null > /tmp/gyrus-bench.out 2>/dev/null; t1=$(ms_now)
    opt_ms=$(elapsed "$t0" "$t1")

    if [ -f "$golden" ] && ! cmp -s /tmp/gyrus-bench.out "$golden"; then
        printf '%-46s  OUTPUT CHANGED vs %s\n' "$name" "$golden"
        diff "$golden" /tmp/gyrus-bench.out | head -4 | sed 's/^/      /'
        failures=$((failures + 1))
        continue
    fi

    if [ "$want_debug" = "no" ] && [ "$FULL" != 1 ]; then
        printf '%-46s %9sms %12s %8s\n' "$name" "$opt_ms" "(--full)" "-"
        continue
    fi

    t0=$(ms_now); "$GYRUS" --debug "$prog" < /dev/null > /tmp/gyrus-bench.dbg 2>/dev/null; t1=$(ms_now)
    dbg_ms=$(elapsed "$t0" "$t1")

    if ! cmp -s /tmp/gyrus-bench.out /tmp/gyrus-bench.dbg; then
        printf '%-46s  MODES DISAGREE: optimized output != --debug output\n' "$name"
        failures=$((failures + 1))
        continue
    fi

    speedup=$(python3 -c "print(f'{$dbg_ms / max($opt_ms, 1):.1f}x')")
    printf '%-46s %9sms %11sms %8s\n' "$name" "$opt_ms" "$dbg_ms" "$speedup"
done

echo
if [ "$failures" -gt 0 ]; then
    echo "FAIL: $failures program(s) produced unexpected output." >&2
    exit 1
fi
echo "All outputs match benchmarks/expected/, and both interpreters agree."
echo "Profile one with:  scripts/benchmark.sh --profile <program.bf>"
