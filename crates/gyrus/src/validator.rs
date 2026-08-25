//! BrainFuck program validation
//!
//! This module provides static analysis of BrainFuck programs to detect
//! common issues, suspicious patterns, and potential problems.
//!
//! # Validation Target: U8 Wrapping (Production/JIT)
//!
//! **IMPORTANT**: Validation ALWAYS assumes **u8 wrapping cell arithmetic**.
//! This is the production target for JIT/AOT compilation.
//!
//! ## Why U8 Wrapping Only?
//!
//! Validation is designed to help you write efficient BrainFuck code that will:
//! - Compile efficiently to native code (JIT/AOT)
//! - Follow standard BrainFuck semantics
//! - Avoid common performance pitfalls
//!
//! The cell model (`--cell-model`) is a **runtime debugging tool** and does NOT
//! affect static validation. Validation always targets production (u8 wrapping).
//!
//! ## Cell Arithmetic Assumptions (U8 Wrapping)
//!
//! - **Cell type**: `u8` (0-255)
//! - **Increment overflow**: `255 + 1 = 0` (wraps to zero)
//! - **Decrement underflow**: `0 - 1 = 255` (wraps to 255)
//!
//! **Implications for validation**:
//! - `[+]` loops ~256 times (wraps 255→0, then exits) - inefficient but NOT infinite!
//! - `[++]` loops ~128 times, `[+++]` loops ~85 times, etc. - all terminate via wrapping
//! - `[-]` is valid and idiomatic (decrements until 0, terminates efficiently)
//! - `[--]` is inefficient but valid (warns to use `[-]` instead)
//!
//! ## Memory Model Independence
//!
//! Validation warnings are independent of the memory model (Fixed/Unbounded):
//!
//! - **Extreme nesting** (depth > 10) warns regardless of memory model
//! - **Empty loops** `[]` warn regardless of memory model
//! - **Pointer seeking patterns** like `[>]` and `[<]` are NOT warned about,
//!   as they are idiomatic BrainFuck patterns for seeking non-zero cells
//!
//! # Examples
//!
//! ```rust
//! use gyrus::{parse_with_debug, validate};
//!
//! // Clean program - no warnings
//! let code = "+++[->+++<]>.";
//! let (instructions, debug_info) = parse_with_debug(code).unwrap();
//! let warnings = validate(&instructions, &debug_info);
//! assert_eq!(warnings.len(), 0);
//!
//! // Inefficient pattern - warning (NOT infinite with u8 wrapping, just inefficient)
//! let code = "[+]";  // Loops ~256 times, wraps 255→0, then exits
//! let (instructions, debug_info) = parse_with_debug(code).unwrap();
//! let warnings = validate(&instructions, &debug_info);
//! assert!(!warnings.is_empty());  // Warns about inefficiency
//!
//! // Idiomatic seeking - no warning
//! let code = "[>]";  // Common pattern for finding non-zero cell
//! let (instructions, debug_info) = parse_with_debug(code).unwrap();
//! let warnings = validate(&instructions, &debug_info);
//! assert_eq!(warnings.len(), 0);
//! ```

use crate::config::{CellModel, U8WrappingCells};
use crate::debug::DebugInfo;
use crate::error::BfWarning;
use crate::instruction::Instruction;
use crate::location::SourceLocation;

/// Validate parsed instructions for common issues and potential problems
///
/// **IMPORTANT**: This function ALWAYS assumes **u8 wrapping cell arithmetic**,
/// which is the production target for JIT/AOT compilation.
///
/// The cell model (`--cell-model`) is a runtime debugging tool and does NOT
/// affect static validation. This function validates for standard BrainFuck
/// semantics (u8 wrapping) regardless of what cell model you use at runtime.
///
/// Returns a vector of warnings for suspicious patterns and potential bugs.
/// Programs with warnings may still execute successfully, but likely contain
/// errors or inefficiencies.
///
/// # Validation Assumptions
///
/// - **Cell type**: `u8` with wrapping arithmetic (production target)
/// - **Memory model**: Independent (warnings apply to all memory models)
/// - **Target**: Efficient code for JIT/AOT compilation
///
/// # Warnings Detected
///
/// - **Empty loops** `[]` - Dead code
/// - **Inefficient increment loops** `[+]`, `[++]` - Slow with u8 wrapping
/// - **Inefficient decrement loops** `[--]`, `[---]` - Use `[-]` instead
/// - **Extreme nesting** - Depth > 10 levels
///
/// # Current Limitations
///
/// - Cannot detect all infinite loops (see GCD analysis in code for [+*n] patterns)
/// - Does not track starting cell values (so warnings may be conservative)
/// - Does not perform data flow analysis
/// - Warnings carry the position of the loop's `[`, not the whole `[`..`]` span
///
/// # See Also
///
/// - Module documentation for detailed assumptions about overflow behavior
/// - [`BfWarning`] for the types of warnings that can be produced
pub fn validate(instructions: &[Instruction], debug_info: &DebugInfo) -> Vec<BfWarning> {
    validate_with_cell_model(
        instructions,
        debug_info,
        CellModel::U8Wrapping(U8WrappingCells),
    )
}

/// Validate against a particular cell model.
///
/// The model changes what a pattern *means*, not merely how fast it is. `[+]`
/// under wrapping cells is a slow way to clear a cell: it counts up, wraps
/// through 255 to zero, and stops after about 256 iterations. Under checked
/// cells the same loop does not wrap -- it reports an overflow when it reaches
/// 255, which is the whole point of that model. Calling both "inefficient"
/// would be wrong in one direction or the other, so the warning says which.
///
/// [`validate`] is this with the default model.
pub fn validate_with_cell_model(
    instructions: &[Instruction],
    debug_info: &DebugInfo,
    cell_model: CellModel,
) -> Vec<BfWarning> {
    let mut warnings = Vec::new();
    let mut index = 0;
    validate_instructions(
        instructions,
        &mut warnings,
        0,
        &mut index,
        debug_info,
        cell_model,
    );
    warnings
}

/// The nesting depth past which `ExtremeNesting` is reported.
const DEEP_NESTING: usize = 10;

/// Walk a block, tracking the instruction index so every warning can name the
/// place it is about.
///
/// The index arithmetic is the parser's, not a second guess at it: at `[` the
/// parser records `step_index` for the bracket and then prepends a `LoopCheck`
/// as `body[0]`, and `]` consumes no index at all. So a loop's `[` and its
/// `LoopCheck` share an index, and recursing into the body consumes it on the
/// way past `body[0]`. Every other instruction is one index. That is the whole
/// rule -- there is no special case, which is what makes this safe to keep in
/// step with the parser.
fn validate_instructions(
    instructions: &[Instruction],
    warnings: &mut Vec<BfWarning>,
    depth: usize,
    index: &mut usize,
    debug_info: &DebugInfo,
    cell_model: CellModel,
) {
    for instruction in instructions {
        let Instruction::Loop(body) = instruction else {
            // Everything else, `LoopCheck` included, is one step.
            *index += 1;
            continue;
        };

        // This loop's `[`. Read before recursing, because the body consumes it.
        let loop_index = *index;
        let location = locate(debug_info, loop_index);

        // Reported once, at the outermost loop that crosses the threshold,
        // carrying how deep it actually goes. Warning at every level below it
        // turned one problem into five identical complaints.
        if depth == DEEP_NESTING {
            warnings.push(BfWarning::ExtremeNesting {
                location,
                depth: depth + deepest(body),
            });
        }

        // A loop is empty if it has no instructions, or only the `LoopCheck`
        // the parser prepended.
        let is_empty =
            body.is_empty() || (body.len() == 1 && matches!(body[0], Instruction::LoopCheck));

        if is_empty {
            warnings.push(BfWarning::EmptyLoop { location });
            // No body to walk, but the `LoopCheck` still holds an index.
            *index += body.len().max(1);
        } else {
            check_suspicious_loop_patterns(body, warnings, location, cell_model);
            validate_instructions(body, warnings, depth + 1, index, debug_info, cell_model);
        }
    }
}

/// The instruction's position, or the start of the source if the table has no
/// entry for it -- which happens only when a caller passes debug info that did
/// not come from this program.
fn locate(debug_info: &DebugInfo, index: usize) -> SourceLocation {
    debug_info
        .lookup(index)
        .unwrap_or_else(SourceLocation::start)
}

/// How many further levels of loop nest inside this block.
fn deepest(instructions: &[Instruction]) -> usize {
    instructions
        .iter()
        .filter_map(|i| match i {
            Instruction::Loop(body) => Some(1 + deepest(body)),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// Calculate greatest common divisor using Euclidean algorithm
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let temp = b;
        b = a % b;
        a = temp;
    }
    a
}

fn check_suspicious_loop_patterns(
    body: &[Instruction],
    warnings: &mut Vec<BfWarning>,
    location: SourceLocation,
    cell_model: CellModel,
) {
    // Note: [>] and [<] are common patterns for seeking in BF, so we don't warn about them
    // Note: [-] is a common pattern for clearing a cell, so we don't warn about it

    // Filter out LoopCheck when analyzing patterns (it's internal bookkeeping)
    let real_body: Vec<&Instruction> = body
        .iter()
        .filter(|i| !matches!(i, Instruction::LoopCheck))
        .collect();

    // Check for [+] and variants - assumes u8 wrapping (production target)
    let all_increments = real_body
        .iter()
        .all(|i| matches!(i, Instruction::IncrementValue));
    if all_increments && !real_body.is_empty() {
        // Under checked cells this loop does not wrap: it climbs to 255 and
        // reports the overflow rather than coming back to zero. That is an
        // error waiting to happen, not an inefficiency, and the GCD reasoning
        // below -- which is entirely about wrapping -- does not apply.
        if matches!(cell_model, CellModel::U8Checked(_)) {
            warnings.push(BfWarning::SuspiciousPattern {
                location,
                pattern: format!("[{}]", "+".repeat(real_body.len())),
                reason: format!(
                    "Will fail under checked cells: incrementing by {} never reaches zero, and \
                     the cell overflows at 255. Use [-] to clear a cell.",
                    real_body.len()
                ),
            });
            return;
        }
        // With u8 wrapping arithmetic, termination depends on GCD
        let gcd_value = gcd(real_body.len(), 256);
        let reason = if real_body.len() == 1 {
            "Inefficient pattern: loops ~256 times before reaching zero. Use [-] to clear a cell."
                .to_string()
        } else if gcd_value > 1 {
            format!(
                "Suspicious pattern: may be infinite or inefficient depending on starting cell value. \
                 Increment by {} only visits multiples of {} (gcd={}).",
                real_body.len(),
                gcd_value,
                gcd_value
            )
        } else {
            format!(
                "Inefficient pattern: loops ~{} times before reaching zero. Use [-] to clear a cell.",
                256 / real_body.len()
            )
        };

        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "+".repeat(real_body.len())),
            reason,
        });
    }

    // Check for [--] and variants - inefficient compared to [-]
    let all_decrements = real_body
        .iter()
        .all(|i| matches!(i, Instruction::DecrementValue));
    if all_decrements && real_body.len() > 1 {
        let reason =
            "Multiple decrements in a loop is inefficient. Consider using [-] to clear the cell."
                .to_string();

        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "-".repeat(real_body.len())),
            reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_with_debug;

    #[test]
    fn test_validate_empty_loop() {
        let source = "[]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();
        let warnings = validate(&instructions, &debug_info);
        assert_eq!(warnings.len(), 1);
        assert!(matches!(warnings[0], BfWarning::EmptyLoop { .. }));
    }

    #[test]
    fn test_validate_inefficient_increment_loop() {
        // [+] is inefficient (loops ~256 times), not infinite with u8 wrapping
        let source = "+[+]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();
        let warnings = validate(&instructions, &debug_info);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(
            |w| matches!(w, BfWarning::SuspiciousPattern { pattern, .. } if pattern.contains("+"))
        ));
    }

    #[test]
    fn test_validate_extreme_nesting() {
        let source = "+[+[+[+[+[+[+[+[+[+[+[+[-]]]]]]]]]]]]"; // 12 levels
        let (instructions, debug_info) = parse_with_debug(source).unwrap();
        let warnings = validate(&instructions, &debug_info);
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, BfWarning::ExtremeNesting { depth, .. } if *depth > 10))
        );
    }

    #[test]
    fn test_validate_clean_program() {
        let source = "+++[->+++<]>."; // Simple clean program
        let (instructions, debug_info) = parse_with_debug(source).unwrap();
        let warnings = validate(&instructions, &debug_info);
        // Should have no warnings
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_validate_multiple_warnings() {
        let source = "[]++[+]"; // Empty loop + inefficient increment loop
        let (instructions, debug_info) = parse_with_debug(source).unwrap();
        let warnings = validate(&instructions, &debug_info);
        assert!(warnings.len() >= 2);
    }
}
