# Benchmark Baseline Results

This document tracks baseline performance metrics for the interpreter before and after optimizations.

## Commands

### Run All Benchmarks
```bash
cargo bench -p ferrous-cortex --bench interpreter
```

### Run Specific Benchmarks
```bash
# Quick mode (faster, less accurate)
cargo bench -p ferrous-cortex --bench interpreter -- --quick hanoi
cargo bench -p ferrous-cortex --bench interpreter -- --quick mandelbrot

# Full mode (slower, more accurate)
cargo bench -p ferrous-cortex --bench interpreter -- hanoi
cargo bench -p ferrous-cortex --bench interpreter -- mandelbrot
```

### Run Simple Benchmarks (Fast)
```bash
cargo bench -p ferrous-cortex --bench interpreter -- simple_arithmetic
cargo bench -p ferrous-cortex --bench interpreter -- nested_loops
cargo bench -p ferrous-cortex --bench interpreter -- pointer_movement
cargo bench -p ferrous-cortex --bench interpreter -- hello_world
```

## Baseline Results (Unoptimized Interpreter)

Date: `YYYY-MM-DD`
Commit: `<commit-hash>`
Hardware: `<CPU model, RAM>`

### Simple Benchmarks

| Benchmark | Time (μs) | Notes |
|-----------|-----------|-------|
| simple_arithmetic | TBD | `+++++[>++++[>++<-]<-]` |
| nested_loops | TBD | `+++[>+++[>+++[>+++<-]<-]<-]` |
| pointer_movement | TBD | `>>>>>>>>>><<<<<<<<<<...` (40 ops) |
| io_operations | TBD | `,[.,]` echo 13 chars |
| hello_world | TBD | Classic "Hello World!" |

### Compute-Intensive Benchmarks

| Benchmark | Time (ms) | Steps | Notes |
|-----------|-----------|-------|-------|
| hanoi | TBD | TBD | Towers of Hanoi |
| mandelbrot | TBD | TBD | Mandelbrot set renderer |

## Optimized Results (After IR + Optimization Pass)

Date: `YYYY-MM-DD`
Commit: `<commit-hash>`

### Simple Benchmarks

| Benchmark | Time (μs) | Speedup | Notes |
|-----------|-----------|---------|-------|
| simple_arithmetic | TBD | TBD× | |
| nested_loops | TBD | TBD× | |
| pointer_movement | TBD | TBD× | |
| io_operations | TBD | TBD× | |
| hello_world | TBD | TBD× | |

### Compute-Intensive Benchmarks

| Benchmark | Time (ms) | Speedup | Steps Reduced | Notes |
|-----------|-----------|---------|---------------|-------|
| hanoi | TBD | TBD× | TBD% | |
| mandelbrot | TBD | TBD× | TBD% | |

## Expected Optimizations

### Instruction Fusion
- `+++` → `Add(3)` - Combine repeated increments
- `---` → `Sub(3)` - Combine repeated decrements
- `>>>` → `Right(3)` - Combine pointer movements
- `<<<` → `Left(3)` - Combine pointer movements

### Loop Pattern Recognition
- `[-]` → `Zero` - Clear cell
- `[>]` → `SeekRight` - Find next zero cell
- `[<]` → `SeekLeft` - Find previous zero cell
- `[->>+<<]` → `Move(2)` - Move value to offset

### Expected Speedups
- Simple arithmetic: 5-10× (heavy instruction fusion)
- Pointer movement: 10-20× (pointer op fusion)
- Nested loops: 3-5× (fusion + loop overhead reduction)
- Hanoi: 2-4× (loop patterns + fusion)
- Mandelbrot: 2-4× (compute-bound, but fusion helps)

## Notes

- All benchmarks use `ExecutionConfig::default()` (30,000 byte fixed memory, u8 wrapping cells)
- Benchmarks exclude parsing time (only interpreter execution)
- Results may vary by hardware and system load
- Use `--quick` for faster iteration during development
- Full benchmarks should be run on dedicated hardware for reproducibility
