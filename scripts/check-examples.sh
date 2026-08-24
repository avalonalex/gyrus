#!/usr/bin/env bash
# Run every example in crates/gyrus/examples and fail if any does not exit 0.
#
# `cargo build --examples` and clippy both compile these, so a type error is
# caught already. A *runtime* failure is not: when MemoryAddress became signed,
# hooks_execution_tracer kept compiling and panicked on its first instruction
# (`ptr - 2` at cell 0 is negative, and indexing with it is a very large usize).
# Nothing noticed, because nothing ran them.
#
# Examples are documentation that executes. This is the script that checks it.
set -uo pipefail

cd "$(dirname "$0")/.."

# `mapfile` is bash 4; macOS ships 3.2, so build the list the portable way.
EXAMPLES=""
while IFS= read -r name; do
    EXAMPLES="$EXAMPLES $name"
done < <(find crates/gyrus/examples -maxdepth 1 -name '*.rs' -exec basename {} .rs \; | sort)

if [ -z "$EXAMPLES" ]; then
    echo "error: no examples found under crates/gyrus/examples" >&2
    exit 1
fi

echo "Running examples..."
failures=0
for name in $EXAMPLES; do
    if out=$(cargo run --quiet --release --example "$name" 2>&1); then
        printf '  %-28s ok\n' "$name"
    else
        printf '  %-28s FAILED\n' "$name"
        printf '%s\n' "$out" | tail -5 | sed 's/^/      /'
        failures=$((failures + 1))
    fi
done

echo
if [ "$failures" -gt 0 ]; then
    echo "FAIL: $failures example(s) did not run cleanly." >&2
    exit 1
fi
echo "OK: every example runs."
