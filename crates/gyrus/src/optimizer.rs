//! Instruction optimization and IR transformation.
//!
//! This module provides an optimized intermediate representation (IR) that fuses
//! repeated instructions and recognizes common loop patterns. Each optimized
//! instruction preserves source location ranges for debugging.
//!
//! # Optimization Strategies
//!
//! ## Instruction Fusion
//! - `+++` → `Add(3)` - Combine repeated increments
//! - `---` → `Sub(3)` - Combine repeated decrements
//! - `>>>` → `Right(3)` - Combine pointer movements
//! - `<<<` → `Left(3)` - Combine pointer movements
//!
//! ## Loop Pattern Recognition
//! - `[-]` → `Zero` - Clear current cell (not `[+]`: it reaches zero only by
//!   wrapping past 255, which checked cells reject)
//! - `[>]` → `SeekRight(1)` - Find next zero cell
//! - `[>>>]` → `SeekRight(3)` - Find next zero cell, three cells at a time
//! - `[<]` → `SeekLeft(1)` - Find previous zero cell
//! - `[->+<]`, `[>+<-]` → `MultiplyAdd([(1, 1)])` - Move value to next cell;
//!   the source's `-` may sit anywhere in the body
//! - `[->>+++<<]` → `MultiplyAdd([(2, 3)])` - Multiply into an offset
//!
//! # Source Location Tracking
//!
//! Each optimized instruction tracks the range of original instructions it
//! represents via `SourceRange`. This enables:
//! - Mapping runtime errors back to source code
//! - Debugger breakpoints on original source
//! - Profiling original source (not optimized IR)
//!
//! # Example
//!
//! ```rust
//! use gyrus::{parse, optimizer::optimize};
//!
//! # fn main() -> Result<(), gyrus::BfError> {
//! let instructions = parse("+++>---")?;
//! let optimized = optimize(&instructions);
//! // OptimizedProgram with: Add(3, range=0..3), Right(1, range=3..4), Sub(3, range=4..7)
//! # Ok(())
//! # }
//! ```

use crate::config::CellModel;
use crate::instruction::Instruction;

/// Source location range for optimized instructions.
///
/// Maps an optimized instruction back to the original instruction indices
/// it was created from. For example, `+++` (3 instructions) becomes
/// `Add(3)` with range `0..3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    /// Start index in original instruction list (inclusive)
    pub start: usize,
    /// End index in original instruction list (exclusive)
    pub end: usize,
}

impl SourceRange {
    /// Create a new source range
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Create a source range for a single instruction
    pub fn single(index: usize) -> Self {
        Self {
            start: index,
            end: index + 1,
        }
    }

    /// Number of original instructions this range covers
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Check if range is empty
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// Optimized intermediate representation instruction.
///
/// Each instruction is either:
/// - A fused operation (Add, Sub, Right, Left) with a count
/// - A recognized loop pattern (Zero, Seek, Move)
/// - An I/O operation (Input, Output)
/// - A general loop (for patterns we don't optimize yet)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizedInstruction {
    /// Add N to current cell (fused +++ → Add(3))
    Add(u8, SourceRange),
    /// Subtract N from current cell (fused --- → Sub(3))
    Sub(u8, SourceRange),
    /// Move pointer right by N (fused >>> → Right(3))
    Right(usize, SourceRange),
    /// Move pointer left by N (fused <<< → Left(3))
    Left(usize, SourceRange),
    /// Output current cell value (.)
    Output(SourceRange),
    /// Read input into current cell (,)
    Input(SourceRange),
    /// Set current cell to zero (optimized `[-]`)
    Zero(SourceRange),
    /// Seek right, `stride` cells at a time, to the next zero cell
    /// (optimized `[>]`, `[>>]`, ...).
    ///
    /// The stride is what makes this pay on real programs: a program that
    /// keeps records of N cells scans for the end of its table with `[>>>>>>>>>]`,
    /// and mandelbrot spends 47% of the instructions it executes inside 124
    /// such loops. Each iteration of one was a whole `Loop` block call for a
    /// single fused move.
    SeekRight(usize, SourceRange),
    /// Seek left, `stride` cells at a time, to the previous zero cell
    /// (optimized `[<]`, `[<<]`, ...). See [`SeekRight`](Self::SeekRight).
    SeekLeft(usize, SourceRange),
    /// Multiply current cell by multipliers and add to target offsets, then zero current cell
    /// Example: `[->+++>+<<]` → `MultiplyAdd(vec![(1, 3), (2, 1)])`
    /// Semantics:
    /// - For each (offset, multiplier): `cell[ptr + offset] += cell[ptr] * multiplier`
    /// - Then: `cell[ptr] = 0`
    ///
    /// Special cases:
    /// - `[->+<]` → `MultiplyAdd(vec![(1, 1)])` - simple move
    /// - `[->++<]` → `MultiplyAdd(vec![(1, 2)])` - move with multiply by 2
    /// - `[->+++>+<<]` → `MultiplyAdd(vec![(1, 3), (2, 1)])` - multi-target multiply
    MultiplyAdd(Vec<(isize, i32)>, SourceRange),
    /// General loop (not optimized yet, contains optimized body)
    Loop(Vec<OptimizedInstruction>, SourceRange),
}

impl OptimizedInstruction {
    /// Get the source range for this instruction
    pub fn source_range(&self) -> SourceRange {
        match self {
            OptimizedInstruction::Add(_, r) => *r,
            OptimizedInstruction::Sub(_, r) => *r,
            OptimizedInstruction::Right(_, r) => *r,
            OptimizedInstruction::Left(_, r) => *r,
            OptimizedInstruction::Output(r) => *r,
            OptimizedInstruction::Input(r) => *r,
            OptimizedInstruction::Zero(r) => *r,
            OptimizedInstruction::SeekRight(_, r) => *r,
            OptimizedInstruction::SeekLeft(_, r) => *r,
            OptimizedInstruction::MultiplyAdd(_, r) => *r,
            OptimizedInstruction::Loop(_, r) => *r,
        }
    }
}

/// Optimized program with source mapping.
///
/// Contains optimized instructions and metadata for debugging/profiling.
#[derive(Debug, Clone)]
pub struct OptimizedProgram {
    /// Optimized instruction sequence
    pub instructions: Vec<OptimizedInstruction>,
    /// Original instruction count (before optimization)
    pub original_count: usize,
    /// Optimized instruction count (after optimization)
    pub optimized_count: usize,
    /// The cell model these instructions were optimized for.
    ///
    /// Not every fold is valid under every cell model, so a program is only
    /// meaningful under the model it was built for. Carrying it here lets
    /// `interpret_optimized` reject a mismatch instead of silently running a
    /// program whose folds do not hold. See [`optimize_with_cell_model`].
    pub cell_model: CellModel,
}

impl OptimizedProgram {
    /// Create a new optimized program built for `cell_model`.
    pub fn new(
        instructions: Vec<OptimizedInstruction>,
        original_count: usize,
        cell_model: CellModel,
    ) -> Self {
        let optimized_count = count_instructions(&instructions);
        Self {
            instructions,
            original_count,
            optimized_count,
            cell_model,
        }
    }

    /// Optimization ratio (original / optimized)
    pub fn compression_ratio(&self) -> f64 {
        if self.optimized_count == 0 {
            1.0
        } else {
            self.original_count as f64 / self.optimized_count as f64
        }
    }
}

/// Count total instructions (including nested loops)
fn count_instructions(instructions: &[OptimizedInstruction]) -> usize {
    instructions
        .iter()
        .map(|inst| match inst {
            OptimizedInstruction::Loop(body, _) => 1 + count_instructions(body),
            _ => 1,
        })
        .sum()
}

/// Optimize a sequence of instructions for the default (wrapping) cell model.
///
/// Applies instruction fusion and pattern recognition to create an optimized IR.
/// Preserves source location ranges for debugging.
///
/// The result records [`CellModel::U8Wrapping`], and `interpret_optimized`
/// rejects it under any other model rather than running folds that do not hold
/// there. Use [`optimize_with_cell_model`] when the cell model is not the
/// default.
pub fn optimize(instructions: &[Instruction]) -> OptimizedProgram {
    optimize_with_cell_model(instructions, CellModel::default())
}

/// Optimize a sequence of instructions for a specific cell model.
///
/// Every pattern here preserves program meaning under wrapping cells. One of
/// them does not survive checked cells:
///
/// `[->+++<]` folds to `MultiplyAdd`, which computes `target += source * 3` in
/// a single step. The loop it replaces reaches that total by incrementing one
/// at a time, so it can cross 255 partway through and raise `CellOverflow` --
/// the whole point of [`CellModel::U8Checked`]. Worse, the fold is not
/// reversible: the optimizer folds the source's `-` or `+` direction into the
/// *sign* of the multiplier, so `MultiplyAdd` no longer records which one it
/// was, and the interpreter cannot replay the original loop to find out.
///
/// So under checked cells the fold is not applied at all. The loop stays a
/// general `Loop`, whose body executes one instruction at a time and reports
/// overflow exactly where the unoptimized program does. Checked cells are a
/// debugging model; correct diagnostics beat throughput there.
pub fn optimize_with_cell_model(
    instructions: &[Instruction],
    cell_model: CellModel,
) -> OptimizedProgram {
    let original_count = count_original_instructions(instructions);
    let optimized = optimize_block(instructions, 0, cell_model).0;
    OptimizedProgram::new(optimized, original_count, cell_model)
}

/// Count original instructions (including nested loops)
fn count_original_instructions(instructions: &[Instruction]) -> usize {
    instructions.iter().map(count_original_instruction).sum()
}

/// Same count over a borrowed view, so callers holding `&[&Instruction]` do not
/// have to deep-clone the block (nested loop bodies included) just to size it.
fn count_original_refs(instructions: &[&Instruction]) -> usize {
    instructions
        .iter()
        .map(|inst| count_original_instruction(inst))
        .sum()
}

fn count_original_instruction(instruction: &Instruction) -> usize {
    match instruction {
        Instruction::Loop(body) => 1 + count_original_instructions(body),
        Instruction::LoopCheck => 0, // Internal, counted as part of its Loop
        _ => 1,
    }
}

/// Optimize a block of instructions, returning (optimized instructions, next index)
fn optimize_block(
    instructions: &[Instruction],
    start_index: usize,
    cell_model: CellModel,
) -> (Vec<OptimizedInstruction>, usize) {
    let mut result = Vec::new();
    let mut i = 0;
    let mut current_index = start_index;

    while i < instructions.len() {
        match &instructions[i] {
            // Skip internal LoopCheck instructions.
            //
            // LoopCheck does not occupy an index of its own: `count_original_instructions`
            // scores it 0 and scores the enclosing `Loop` as 1, because the `Loop`
            // container and its LoopCheck are the same `[` in the source. Advancing
            // here as well would count that `[` twice.
            Instruction::LoopCheck => {
                i += 1;
            }

            // Try to recognize loop patterns
            Instruction::Loop(body) => {
                let loop_start = current_index;

                // Try to recognize common patterns
                if let Some(optimized) = recognize_loop_pattern(body, loop_start, cell_model) {
                    result.push(optimized);
                    current_index += 1 + count_original_instructions(body);
                    i += 1;
                } else {
                    // General loop - recursively optimize body.
                    // body_start already includes the +1 for the '[' itself, so the
                    // index the body ends on is the index the loop ends on.
                    let body_start = current_index + 1; // +1 for the loop instruction itself
                    let (optimized_body, next_index) = optimize_block(body, body_start, cell_model);
                    current_index = next_index;
                    result.push(OptimizedInstruction::Loop(
                        optimized_body,
                        SourceRange::new(loop_start, current_index),
                    ));
                    i += 1;
                }
            }

            // Fuse repeated increments
            Instruction::IncrementValue => {
                let start = current_index;
                let mut count = 0u8;
                while i < instructions.len()
                    && matches!(instructions[i], Instruction::IncrementValue)
                    && count < 255
                {
                    count = count.saturating_add(1);
                    i += 1;
                    current_index += 1;
                }
                result.push(OptimizedInstruction::Add(
                    count,
                    SourceRange::new(start, current_index),
                ));
            }

            // Fuse repeated decrements
            Instruction::DecrementValue => {
                let start = current_index;
                let mut count = 0u8;
                while i < instructions.len()
                    && matches!(instructions[i], Instruction::DecrementValue)
                    && count < 255
                {
                    count = count.saturating_add(1);
                    i += 1;
                    current_index += 1;
                }
                result.push(OptimizedInstruction::Sub(
                    count,
                    SourceRange::new(start, current_index),
                ));
            }

            // Fuse repeated right movements
            Instruction::IncrementPointer => {
                let start = current_index;
                let mut count = 0usize;
                while i < instructions.len()
                    && matches!(instructions[i], Instruction::IncrementPointer)
                {
                    count += 1;
                    i += 1;
                    current_index += 1;
                }
                result.push(OptimizedInstruction::Right(
                    count,
                    SourceRange::new(start, current_index),
                ));
            }

            // Fuse repeated left movements
            Instruction::DecrementPointer => {
                let start = current_index;
                let mut count = 0usize;
                while i < instructions.len()
                    && matches!(instructions[i], Instruction::DecrementPointer)
                {
                    count += 1;
                    i += 1;
                    current_index += 1;
                }
                result.push(OptimizedInstruction::Left(
                    count,
                    SourceRange::new(start, current_index),
                ));
            }

            // I/O operations (no fusion)
            Instruction::Output => {
                result.push(OptimizedInstruction::Output(SourceRange::single(
                    current_index,
                )));
                current_index += 1;
                i += 1;
            }

            Instruction::Input => {
                result.push(OptimizedInstruction::Input(SourceRange::single(
                    current_index,
                )));
                current_index += 1;
                i += 1;
            }
        }
    }

    (result, current_index)
}

/// Recognize common loop patterns and convert to optimized instructions
fn recognize_loop_pattern(
    body: &[Instruction],
    loop_start: usize,
    cell_model: CellModel,
) -> Option<OptimizedInstruction> {
    // Filter out LoopCheck for pattern matching
    let body: Vec<_> = body
        .iter()
        .filter(|inst| !matches!(inst, Instruction::LoopCheck))
        .collect();

    let loop_end = loop_start + 1 + count_original_refs(&body);

    // Pattern: [-] → Zero
    //
    // Deliberately not [+]: reaching zero by incrementing relies on the cell
    // wrapping past 255, which `CellModel::U8Checked` rejects. Folding it to a
    // store of 0 would silently succeed where the debug interpreter reports
    // CellOverflow. [+] is an anti-pattern the validator already warns about,
    // so leaving it as a real loop costs nothing worth having.
    if body.len() == 1 && matches!(body[0], Instruction::DecrementValue) {
        return Some(OptimizedInstruction::Zero(SourceRange::new(
            loop_start, loop_end,
        )));
    }

    // Pattern: [>], [>>], ... → SeekRight(stride). A body that is nothing but
    // moves in one direction is a scan; the number of moves is the stride.
    if !body.is_empty()
        && body
            .iter()
            .all(|inst| matches!(inst, Instruction::IncrementPointer))
    {
        return Some(OptimizedInstruction::SeekRight(
            body.len(),
            SourceRange::new(loop_start, loop_end),
        ));
    }

    // Pattern: [<], [<<], ... → SeekLeft(stride)
    if !body.is_empty()
        && body
            .iter()
            .all(|inst| matches!(inst, Instruction::DecrementPointer))
    {
        return Some(OptimizedInstruction::SeekLeft(
            body.len(),
            SourceRange::new(loop_start, loop_end),
        ));
    }

    // Pattern: Generalized multiplication loops
    // Examples:
    // - [->+<] → MultiplyAdd(vec![(1, 1)])
    // - [->++<] → MultiplyAdd(vec![(1, 2)])
    // - [->+++>+<<] → MultiplyAdd(vec![(1, 3), (2, 1)])
    // Not valid under checked cells; see `optimize_with_cell_model`.
    if !matches!(cell_model, CellModel::U8Wrapping(_)) {
        return None;
    }
    recognize_multiply_loop(&body, loop_start, loop_end)
}

/// Recognize multiplication loop patterns
///
/// A multiplication loop is a balanced body -- moves and arithmetic only, the
/// cursor back where it started -- that touches the source cell (position 0)
/// exactly once, with a single `-` or `+`. Every other `+`/`-` run adds to
/// some other cell, and the loop runs once per unit of the source, so the
/// whole thing is `cell[k] += cell[0] * m` for each target, then `cell[0] = 0`.
///
/// The source's `-` may sit anywhere in the body. `[->+++<]` and `[>+++<-]`
/// are the same loop -- a BrainFuck loop body is circular, and a `-` at the
/// end of one iteration is a `-` at the start of the next, with the loop
/// condition read in between either way. Requiring it first, as the first
/// version of this function did, missed 56% of what squares executes and a
/// third of triangle; hand-written programs put the decrement last at least
/// as often as first.
///
/// Example: `[->+++>+<<]` and `[>+++>+<<-]`
/// - cell[1] += cell[0] * 3; cell[2] += cell[0] * 1; cell[0] = 0
fn recognize_multiply_loop(
    body: &[&Instruction],
    loop_start: usize,
    loop_end: usize,
) -> Option<OptimizedInstruction> {
    // -1 for a source decremented by `-`, +1 for one incremented by `+`
    // (which reaches zero by wrapping, 256 - n iterations, so every
    // multiplier flips sign). None until the source's one op is seen.
    let mut decrement_factor: Option<i32> = None;
    let mut position: isize = 0;
    let mut adds: Vec<(isize, i32)> = Vec::new();
    let mut i = 0;

    while i < body.len() {
        match body[i] {
            Instruction::IncrementPointer => {
                position += 1;
                i += 1;
            }
            Instruction::DecrementPointer => {
                position -= 1;
                i += 1;
            }
            Instruction::IncrementValue | Instruction::DecrementValue => {
                let sign = if matches!(body[i], Instruction::IncrementValue) {
                    1
                } else {
                    -1
                };
                // Count the run of the same instruction.
                let mut count = 1;
                while i + count < body.len() && body[i + count] == body[i] {
                    count += 1;
                }
                if position == 0 {
                    // The source may be touched once, by one instruction: a
                    // second op, or a run of two, makes the iteration count
                    // something other than the source's value.
                    if count != 1 || decrement_factor.is_some() {
                        return None;
                    }
                    decrement_factor = Some(sign);
                } else {
                    adds.push((position, count as i32 * sign));
                }
                i += count;
            }
            _ => {
                // Other instructions (Output, Input, Loop) invalidate the pattern
                return None;
            }
        }
    }

    // Balanced, one source op, and something to multiply into.
    let factor = decrement_factor?;
    if position != 0 || adds.is_empty() {
        return None;
    }
    // A `+` on the source flips every multiplier: the loop runs 256 - n times.
    for (_, multiplier) in &mut adds {
        *multiplier *= -factor;
    }
    Some(OptimizedInstruction::MultiplyAdd(
        adds,
        SourceRange::new(loop_start, loop_end),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[->+++<]` folds to a single MultiplyAdd under the default cell model.
    #[test]
    fn multiply_loop_folds_under_wrapping_cells() {
        let instructions = crate::parser::parse("[->+++<]").unwrap();
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        assert!(
            matches!(
                optimized.instructions[0],
                OptimizedInstruction::MultiplyAdd(_, _)
            ),
            "expected MultiplyAdd, got {:?}",
            optimized.instructions[0]
        );
    }

    /// The source's `-` may be anywhere in the body: `[>+++<-]` is the same
    /// loop as `[->+++<]`, and folds to the same instruction.
    #[test]
    fn multiply_loop_folds_with_the_decrement_anywhere() {
        let expect = |src: &str| {
            let optimized = optimize(&crate::parser::parse(src).unwrap());
            assert_eq!(optimized.instructions.len(), 1, "{src}");
            match &optimized.instructions[0] {
                OptimizedInstruction::MultiplyAdd(adds, _) => adds.clone(),
                other => panic!("{src}: expected MultiplyAdd, got {other:?}"),
            }
        };
        assert_eq!(expect("[->+++<]"), vec![(1, 3)]);
        assert_eq!(expect("[>+++<-]"), vec![(1, 3)]);
        assert_eq!(expect("[>+++<->+<]"), vec![(1, 3), (1, 1)]);
        // mandelbrot's shape: two targets, the decrement in the middle.
        assert_eq!(expect("[<->-<<<<<<+>>>>>>]"), vec![(-1, -1), (-6, 1)]);
        // A `+` on the source flips the multipliers, wherever it sits.
        assert_eq!(expect("[+>+<]"), vec![(1, -1)]);
        assert_eq!(expect("[>+<+]"), vec![(1, -1)]);
    }

    /// What is not a multiply loop: the source touched twice, or by a run,
    /// or a body that does not come back to it.
    #[test]
    fn multiply_loop_needs_exactly_one_source_op_and_balance() {
        for src in ["[->+<-]", "[-->+<]", "[>+<--]", "[->+]", "[>+<]", "[-]"] {
            let optimized = optimize(&crate::parser::parse(src).unwrap());
            assert!(
                !matches!(
                    optimized.instructions[0],
                    OptimizedInstruction::MultiplyAdd(..)
                ),
                "{src} must not fold to MultiplyAdd, got {:?}",
                optimized.instructions[0]
            );
        }
    }

    /// ...but not under checked cells; see [`optimize_with_cell_model`] for why
    /// the fold is both invalid there and unreplayable afterwards.
    #[test]
    fn multiply_loop_is_not_folded_under_checked_cells() {
        let instructions = crate::parser::parse("[->+++<]").unwrap();
        let optimized = optimize_with_cell_model(
            &instructions,
            CellModel::U8Checked(crate::config::U8CheckedCells),
        );
        assert!(
            !optimized
                .instructions
                .iter()
                .any(|i| matches!(i, OptimizedInstruction::MultiplyAdd(_, _))),
            "checked cells must not fold multiply loops, got {:?}",
            optimized.instructions
        );
        assert!(
            matches!(optimized.instructions[0], OptimizedInstruction::Loop(_, _)),
            "expected a general Loop, got {:?}",
            optimized.instructions[0]
        );
    }

    /// A scan's stride is the number of moves in its body; `[>]` is stride 1.
    #[test]
    fn scan_loops_fold_with_their_stride() {
        for (src, expect) in [
            (
                "[>]",
                OptimizedInstruction::SeekRight(1, SourceRange::new(0, 2)),
            ),
            (
                "[>>>]",
                OptimizedInstruction::SeekRight(3, SourceRange::new(0, 4)),
            ),
            (
                "[<<]",
                OptimizedInstruction::SeekLeft(2, SourceRange::new(0, 3)),
            ),
        ] {
            let optimized = optimize(&crate::parser::parse(src).unwrap());
            assert_eq!(optimized.instructions, vec![expect], "{src}");
        }
    }

    /// Patterns that carry no cell arithmetic are unaffected by the cell model.
    #[test]
    fn seek_and_clear_still_fold_under_checked_cells() {
        let checked = CellModel::U8Checked(crate::config::U8CheckedCells);
        for (src, ok) in [("[>]", "SeekRight"), ("[<]", "SeekLeft"), ("[-]", "Zero")] {
            let instructions = crate::parser::parse(src).unwrap();
            let optimized = optimize_with_cell_model(&instructions, checked);
            assert_eq!(optimized.instructions.len(), 1, "{src} -> {ok}");
            assert!(
                !matches!(optimized.instructions[0], OptimizedInstruction::Loop(_, _)),
                "{src} should still fold to {ok} under checked cells"
            );
        }
    }

    #[test]
    fn test_fuse_increments() {
        let instructions = vec![
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::IncrementValue,
        ];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        assert!(matches!(
            optimized.instructions[0],
            OptimizedInstruction::Add(3, _)
        ));
        assert_eq!(optimized.original_count, 3);
        assert_eq!(optimized.optimized_count, 1);
    }

    #[test]
    fn test_fuse_pointer_movement() {
        let instructions = vec![
            Instruction::IncrementPointer,
            Instruction::IncrementPointer,
            Instruction::DecrementPointer,
        ];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 2);
        assert!(matches!(
            optimized.instructions[0],
            OptimizedInstruction::Right(2, _)
        ));
        assert!(matches!(
            optimized.instructions[1],
            OptimizedInstruction::Left(1, _)
        ));
    }

    #[test]
    fn test_recognize_zero_pattern() {
        let instructions = vec![Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::DecrementValue,
        ])];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        assert!(matches!(
            optimized.instructions[0],
            OptimizedInstruction::Zero(_)
        ));
    }

    #[test]
    fn test_recognize_seek_right() {
        let instructions = vec![Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::IncrementPointer,
        ])];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        assert!(matches!(
            optimized.instructions[0],
            OptimizedInstruction::SeekRight(..)
        ));
    }

    #[test]
    fn test_recognize_multiply_add_simple() {
        // [->+<] → MultiplyAdd(vec![(1, 1)])
        let instructions = vec![Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::DecrementValue,
            Instruction::IncrementPointer,
            Instruction::IncrementValue,
            Instruction::DecrementPointer,
        ])];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        match &optimized.instructions[0] {
            OptimizedInstruction::MultiplyAdd(adds, _) => {
                assert_eq!(adds, &vec![(1, 1)]);
            }
            _ => panic!("Expected MultiplyAdd"),
        }
    }

    #[test]
    fn test_recognize_multiply_add_with_multiplier() {
        // [->++<] → MultiplyAdd(vec![(1, 2)])
        let instructions = vec![Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::DecrementValue,
            Instruction::IncrementPointer,
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::DecrementPointer,
        ])];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        match &optimized.instructions[0] {
            OptimizedInstruction::MultiplyAdd(adds, _) => {
                assert_eq!(adds, &vec![(1, 2)]);
            }
            _ => panic!("Expected MultiplyAdd"),
        }
    }

    #[test]
    fn test_recognize_multiply_add_multi_target() {
        // [->+++>+<<] → MultiplyAdd(vec![(1, 3), (2, 1)])
        let instructions = vec![Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::DecrementValue,
            Instruction::IncrementPointer,
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::IncrementPointer,
            Instruction::IncrementValue,
            Instruction::DecrementPointer,
            Instruction::DecrementPointer,
        ])];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        match &optimized.instructions[0] {
            OptimizedInstruction::MultiplyAdd(adds, _) => {
                assert_eq!(adds, &vec![(1, 3), (2, 1)]);
            }
            _ => panic!("Expected MultiplyAdd"),
        }
    }

    #[test]
    fn test_source_range_tracking() {
        let instructions = vec![
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::Output,
        ];
        let optimized = optimize(&instructions);
        assert_eq!(
            optimized.instructions[0].source_range(),
            SourceRange::new(0, 2)
        );
        assert_eq!(
            optimized.instructions[1].source_range(),
            SourceRange::single(2)
        );
    }

    /// Source ranges are indices into the flat instruction stream the interpreter
    /// and debug symbols use, where a `[` occupies exactly one index (its LoopCheck).
    /// An unfused loop used to advance the cursor twice for that single `[` - once
    /// for the Loop container and once for its LoopCheck - so everything after a loop
    /// was reported two indices too far, and everything inside it one too far.
    #[test]
    fn test_source_ranges_survive_an_unfused_loop() {
        // "[>.]+" -> LoopCheck=0, '>'=1, '.'=2, '+'=3
        let optimized = optimize(&crate::parser::parse("[>.]+").unwrap());

        let OptimizedInstruction::Loop(body, loop_range) = &optimized.instructions[0] else {
            panic!(
                "expected a general Loop, got {:?}",
                optimized.instructions[0]
            );
        };
        assert_eq!(*loop_range, SourceRange::new(0, 3));
        assert_eq!(body[0].source_range(), SourceRange::new(1, 2), "the '>'");
        assert_eq!(body[1].source_range(), SourceRange::new(2, 3), "the '.'");
        assert_eq!(
            optimized.instructions[1].source_range(),
            SourceRange::new(3, 4),
            "the '+' after the loop"
        );
    }

    /// The same accounting has to hold through nesting.
    #[test]
    fn test_source_ranges_survive_nested_unfused_loops() {
        // "+[>[.]<]-" -> '+'=0, outer '['=1, '>'=2, inner '['=3, '.'=4, '<'=5, '-'=6
        let optimized = optimize(&crate::parser::parse("+[>[.]<]-").unwrap());

        assert_eq!(
            optimized.instructions[0].source_range(),
            SourceRange::new(0, 1)
        );
        assert_eq!(
            optimized.instructions[1].source_range(),
            SourceRange::new(1, 6)
        );
        assert_eq!(
            optimized.instructions[2].source_range(),
            SourceRange::new(6, 7)
        );

        let OptimizedInstruction::Loop(outer, _) = &optimized.instructions[1] else {
            panic!("expected outer Loop");
        };
        assert_eq!(outer[0].source_range(), SourceRange::new(2, 3), "the '>'");
        assert_eq!(
            outer[1].source_range(),
            SourceRange::new(3, 5),
            "inner loop"
        );
        assert_eq!(outer[2].source_range(), SourceRange::new(5, 6), "the '<'");
    }

    /// `[-]` folds to a store of zero, but `[+]` must not: it only reaches zero by
    /// wrapping past 255, which `CellModel::U8Checked` reports as an overflow.
    #[test]
    fn test_only_decrement_clear_loop_folds_to_zero() {
        let zeroed = optimize(&[Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::DecrementValue,
        ])]);
        assert!(matches!(
            zeroed.instructions[0],
            OptimizedInstruction::Zero(_)
        ));

        let kept = optimize(&[Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::IncrementValue,
        ])]);
        assert!(
            matches!(kept.instructions[0], OptimizedInstruction::Loop(_, _)),
            "[+] must stay a real loop, got {:?}",
            kept.instructions[0]
        );
    }

    #[test]
    fn test_nested_loop_optimization() {
        let instructions = vec![Instruction::Loop(vec![
            Instruction::LoopCheck,
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::Loop(vec![Instruction::LoopCheck, Instruction::DecrementValue]),
        ])];
        let optimized = optimize(&instructions);
        assert_eq!(optimized.instructions.len(), 1);
        if let OptimizedInstruction::Loop(body, _) = &optimized.instructions[0] {
            assert_eq!(body.len(), 2);
            assert!(matches!(body[0], OptimizedInstruction::Add(2, _)));
            assert!(matches!(body[1], OptimizedInstruction::Zero(_)));
        } else {
            panic!("Expected Loop instruction");
        }
    }
}
