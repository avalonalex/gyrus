# Fibonacci Sequence Generator (fib.b)

## Overview

This program computes and outputs Fibonacci numbers (https://oeis.org/A000045) indefinitely, one per line, in decimal format:

```
0
1
1
2
3
5
8
13
21
34
55
89
144
...
```

The sequence starts with F(0)=0, F(1)=1, then each subsequent number is the sum of the previous two.

**Author**: Original implementation circa 2001 (https://brainfuck.org/ggab.html)
**Design**: Infinite loop - outputs Fibonacci numbers until interrupted (Ctrl-C)

## Key Features

- **No input required** - runs indefinitely
- **Decimal output** - human-readable ASCII numbers (not raw bytes)
- **Arbitrary precision** - handles Fibonacci numbers of any size by storing one decimal digit per cell
- **Cell-size portable** - doesn't rely on large cell values (works with 8-bit cells)

## Algorithm Overview

The algorithm maintains the last two Fibonacci numbers (A and B):
1. Output B in decimal format
2. Replace A with B
3. Replace B with A+B (the next Fibonacci number)
4. Repeat

## Memory Layout Innovation

Because Fibonacci numbers grow very large, this program uses a sophisticated multi-cell representation where **each decimal digit gets its own cell**. The challenge is organizing these digits in BrainFuck's one-dimensional memory.

### Interleaved Storage

Rather than storing numbers sequentially (which would cause them to collide as they grow), the program uses **interleaved storage** - splitting the array conceptually into parallel arrays:

```
     5 1 6 6 1 4 7 1 9 6 1 0 0 1 1 0 0 0 0 0 0 ...
0 10 A T B A T B A T B A T B A T B A T B A T B ...
```

Where:
- **Cell 0**: Always 0 (placeholder)
- **Cell 1**: Linefeed (10) for output formatting
- **A cells**: Previous Fibonacci number (one digit per cell)
- **B cells**: Current Fibonacci number (one digit per cell)
- **T cells**: Temporary/length markers (1 = digit exists, 0 = past end of number)

### Why Interleaved?

This layout has several advantages:
1. **Uniform distance**: Corresponding digits of different numbers are always the same distance apart
2. **No collisions**: As numbers grow, they don't bump into each other
3. **Natural growth**: Numbers grow rightward into reserved space (1/3 of the array for each number)

### Backward Storage

Numbers are stored **backwards** (most significant digit to the right) because:
- Numbers grow to the left when written normally
- BrainFuck's array is unbounded to the right
- This allows rightward growth into available space

## The Addition Algorithm

Computing A+B with multi-digit numbers requires handling carries (e.g., 9+6=15, carry the 1).

### Digit-by-Digit Update

The program updates one digit at a time from left to right:
1. Move digit of A to T (temporary storage)
2. Add A digit + B digit, store sum in T
3. Move sum to B cell, handling carries if sum ≥ 10

### Carry Handling - The "Case Statement"

The clever part is the carry logic. For sums 0-9, no carry is needed. For sums ≥10:

```brainfuck
[>+<-[>+<-[>+<-[>+<-[>+<-[>+<-[>+<-[>+<-[>+<-[
  >[-]      Set B = 0 (the "0" in "10")
  >+        Add 1 to next A digit (carry)
  >+        Mark next T as nonzero (ensure next iteration)
  <<<-      Continue moving sum
  [>+<-]    Finish moving sum (handles 11-19)
]]]]]]]]]]
```

This is a **nested loop structure** that acts like a switch/case:
- Cases 0-5: Skip forward immediately after moving
- Cases 6-9: Execute a few more moves then skip forward
- Case 10+: Execute the carry logic

## Output Process

For each Fibonacci number:
1. Scan right-to-left through T markers
2. For each T=1:
   - Add 48 to B digit (convert 0-9 to ASCII '0'-'9')
   - Output the ASCII character
   - Subtract 48 to restore original value
3. Output linefeed character

This outputs the number in normal left-to-right order (because we're scanning backward through the backward-stored number).

## Key Techniques Demonstrated

1. **Variable-sized data handling** - Numbers grow dynamically
2. **Breadcrumb technique** - Using markers (T cells) to scan through data
3. **Interleaved storage** - Preventing data collisions in one-dimensional array
4. **Temporary value reuse** - Don't keep data longer than needed
5. **Case statement pattern** - Nested loops to handle multiple cases
6. **Reversible transformations** - x+48-48=x (avoid temporary copies)

## Complexity

- **Time**: O(log n) operations per Fibonacci number (optimal for multi-digit arithmetic)
- **Space**: O(log n) cells per number (one cell per decimal digit)
- **Commands per output character**: <450 (very efficient)

## Testing

Because this program runs indefinitely, testing requires a step limit to simulate Ctrl-C:

```bash
# Run manually (Ctrl-C to stop)
cargo run -- programs/advanced/fibonacci.bf

# Automated test (see tests/program_corpus.rs)
# - Let program run with step limit
# - Verify output starts with "1\n1\n2\n3\n5\n8\n13\n..."
# - Program "fails" with step limit (expected behavior)
```

## Full Program

```brainfuck
>++++++++++>+>+[
  [+++++[>++++++++<-]>.<++++++[>--------<-]+<<<]>.>>[
    [-]<[>+<-]>>[<<+>+>-]<[>+<-[>+<-[>+<-[>+<-[>+<-[>+<-
      [>+<-[>+<-[>+<-[>[-]>+>+<<<-[>+<-]]]]]]]]]]]+>>>
  ]<<<
]
```

## Historical Note

This was one of the first nontrivial BrainFuck programs written (circa 2001), demonstrating that BrainFuck can handle sophisticated algorithms efficiently with proper memory organization.
