#!/usr/bin/env bash
# Benchmark and profile the interpreter on real BrainFuck programs.
#
# Measures end-to-end wall clock for the `gyrus` binary: parse, optimize,
# execute (or JIT-compile and execute), write output. For per-phase numbers with warmup and outlier
# rejection, use the criterion benches instead (`cargo bench`).
#
# Every run is diffed against a recorded golden output in benchmarks/expected/.
# A run that got faster while producing different bytes is not a faster
# engine, so this script fails instead of reporting the number. The JIT is
# held to the interpreter's bytes on every program; where a program is fast
# enough to also run under --debug, the tree-walker is compared too, which
# makes this a differential test of the optimizer and the JIT as well as a
# benchmark.
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

# One run of "$@" with stdout in the file named by OUT, printed as
# "<milliseconds> <exit status>". Timed with bash's own `time`: a python3
# timestamp on either side cost ~24 ms of its own, more than most of the
# corpus takes to run. Called in a command substitution, so it prints rather
# than sets its results.
timed_run() {
    local rc
    TIMEFORMAT=%R
    { time "$@" < /dev/null > "$OUT" 2>/dev/null; rc=$?; } 2> /tmp/gyrus-bench.time
    python3 -c "print(round(float(open('/tmp/gyrus-bench.time').read().strip().replace(',', '.')) * 1000), $rc)"
}

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
# jit x = optimized / --jit, so a JIT slower than the interpreter shows as
# less than 1x; speedup = --debug / optimized, the two interpreters.
printf '%-38s %10s %10s %7s %12s %8s\n' "program" "optimized" "--jit" "jit x" "--debug" "speedup"
printf '%-38s %10s %10s %7s %12s %8s\n' "$(printf '%.0s-' {1..38})" "----------" "----------" "-------" "------------" "--------"

failures=0
for entry in "${PROGRAMS[@]}"; do
    prog="${entry%%:*}"
    want_debug="${entry##*:}"
    name=$(basename "$prog" .bf)
    golden="$GOLDEN/$name.txt"

    OUT=/tmp/gyrus-bench.out; read -r opt_ms _ <<< "$(timed_run "$GYRUS" "$prog")"

    if [ -f "$golden" ] && ! cmp -s /tmp/gyrus-bench.out "$golden"; then
        printf '%-38s  OUTPUT CHANGED vs %s\n' "$name" "$golden"
        diff "$golden" /tmp/gyrus-bench.out | head -4 | sed 's/^/      /'
        failures=$((failures + 1))
        continue
    fi

    # The JIT is a third engine, held to the same bytes and the same exit
    # status; compile time is in the number, as it is for the user.
    OUT=/tmp/gyrus-bench.jit; read -r jit_ms jit_rc <<< "$(timed_run "$GYRUS" --jit "$prog")"
    if [ "$jit_rc" != 0 ]; then
        printf '%-38s  --jit FAILED (exit %s; is the jit feature built?)\n' "$name" "$jit_rc"
        failures=$((failures + 1))
        continue
    fi
    if ! cmp -s /tmp/gyrus-bench.out /tmp/gyrus-bench.jit; then
        printf '%-38s  MODES DISAGREE: optimized output != --jit output\n' "$name"
        failures=$((failures + 1))
        continue
    fi
    jit_x=$(python3 -c "print(f'{$opt_ms / max($jit_ms, 1):.2f}x')")

    if [ "$want_debug" = "no" ] && [ "$FULL" != 1 ]; then
        printf '%-38s %9sms %9sms %7s %12s %8s\n' "$name" "$opt_ms" "$jit_ms" "$jit_x" "(--full)" "-"
        continue
    fi

    OUT=/tmp/gyrus-bench.dbg; read -r dbg_ms _ <<< "$(timed_run "$GYRUS" --debug "$prog")"

    if ! cmp -s /tmp/gyrus-bench.out /tmp/gyrus-bench.dbg; then
        printf '%-38s  MODES DISAGREE: optimized output != --debug output\n' "$name"
        failures=$((failures + 1))
        continue
    fi

    speedup=$(python3 -c "print(f'{$dbg_ms / max($opt_ms, 1):.1f}x')")
    printf '%-38s %9sms %9sms %7s %11sms %8s\n' "$name" "$opt_ms" "$jit_ms" "$jit_x" "$dbg_ms" "$speedup"
done

# --- cell-model differential ------------------------------------------------
# Everything above runs the default cell model, which is how a real bug lived
# here undetected: the optimizer's multiply-loop fold assumes wrapping
# arithmetic, so under --cell-model checked it computed `target += source * n`
# in one step and wrapped past 255 silently, where the unfused program raises
# CellOverflow. The two interpreters have to agree under *every* model, not
# just the default one.
#
# Error text legitimately differs between the modes (--debug carries source
# locations), so compare what the program produced and whether it failed --
# not the diagnostic wording.
echo
echo "Cell-model differential (optimized vs --debug vs --jit under --cell-model checked)"
checked_bytes=0
for entry in "${PROGRAMS[@]}"; do
    prog="${entry%%:*}"
    want_debug="${entry##*:}"
    name=$(basename "$prog" .bf)
    [ "$want_debug" = "yes" ] || [ "$FULL" = 1 ] || continue

    "$GYRUS" --cell-model checked "$prog" < /dev/null > /tmp/gyrus-chk.opt 2>/dev/null
    opt_rc=$?
    "$GYRUS" --debug --cell-model checked "$prog" < /dev/null > /tmp/gyrus-chk.dbg 2>/dev/null
    dbg_rc=$?
    "$GYRUS" --jit --cell-model checked "$prog" < /dev/null > /tmp/gyrus-chk.jit 2>/dev/null
    jit_rc=$?

    if ! cmp -s /tmp/gyrus-chk.opt /tmp/gyrus-chk.dbg || [ "$opt_rc" != "$dbg_rc" ] \
        || ! cmp -s /tmp/gyrus-chk.opt /tmp/gyrus-chk.jit || [ "$opt_rc" != "$jit_rc" ]; then
        printf '  %-44s MODES DISAGREE (exit %s vs %s vs %s)\n' "$name" "$opt_rc" "$dbg_rc" "$jit_rc"
        failures=$((failures + 1))
    else
        printf '  %-44s agree (exit %s)\n' "$name" "$opt_rc"
        checked_bytes=$((checked_bytes + $(wc -c < /tmp/gyrus-chk.opt)))
    fi
done

# Guard against the comparison silently testing nothing. If --cell-model were
# renamed, every run would fail identically with empty output, cmp would find
# two empty files equal, and every program would report "agree".
if [ "$checked_bytes" -eq 0 ]; then
    echo "  FAIL: no program produced output under checked cells, so nothing was" >&2
    echo "        actually compared. Does '--cell-model checked' still exist?" >&2
    failures=$((failures + 1))
fi

echo
if [ "$failures" -gt 0 ]; then
    echo "FAIL: $failures program(s) produced unexpected output." >&2
    exit 1
fi
echo "All outputs match benchmarks/expected/, and all three engines agree"
echo "under the default and checked cell models."
echo "Profile one with:  scripts/benchmark.sh --profile <program.bf>"
