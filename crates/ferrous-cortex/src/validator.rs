//! BrainFuck program validation
//!
//! This module provides static analysis of BrainFuck programs to detect
//! common issues, suspicious patterns, and potential problems.
//!
//! # Validation Assumptions
//!
//! The validator currently makes specific assumptions about the execution model:
//!
//! ## Cell Arithmetic (Currently Hardcoded)
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
//! ## Memory Model (Mostly Independent)
//!
//! Validation warnings are mostly independent of the memory model
//! (Fixed/Wrapping/Unbounded), with a few exceptions:
//!
//! - **Extreme nesting** (depth > 10) warns regardless of memory model
//! - **Empty loops** `[]` warn regardless of memory model
//! - **Pointer seeking patterns** like `[>]` and `[<]` are NOT warned about,
//!   as they are idiomatic BrainFuck patterns for seeking non-zero cells
//!
//! ## Future: Configuration-Aware Validation
//!
//! When cell models become configurable (e.g., `CellModel::U8Checked`,
//! `CellModel::U8Saturating`), validation will become model-aware:
//!
//! - **U8Wrapping**: `[+]` → inefficient (~256 iterations), not infinite (current behavior)
//! - **U8Checked**: `[+]` → will error on overflow (255+1 panics)
//! - **U8Saturating**: `[+]` → TRULY infinite (stuck at 255, never reaches 0)
//! - **I8Wrapping**: Similar to u8 (~256 iterations, wraps -128→127)
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::{parse, validate};
//!
//! // Clean program - no warnings
//! let code = "+++[->+++<]>.";
//! let instructions = parse(code).unwrap();
//! let warnings = validate(&instructions);
//! assert_eq!(warnings.len(), 0);
//!
//! // Inefficient pattern - warning (NOT infinite, just inefficient)
//! let code = "[+]";  // Loops ~256 times, wraps 255→0, then exits
//! let instructions = parse(code).unwrap();
//! let warnings = validate(&instructions);
//! assert!(!warnings.is_empty());  // Warns about inefficiency
//!
//! // Idiomatic seeking - no warning
//! let code = "[>]";  // Common pattern for finding non-zero cell
//! let instructions = parse(code).unwrap();
//! let warnings = validate(&instructions);
//! assert_eq!(warnings.len(), 0);
//! ```

use crate::error::BfWarning;
use crate::instruction::Instruction;
use crate::location::SourceLocation;

/// Validate parsed instructions for common issues and potential problems
///
/// Returns a vector of warnings for suspicious patterns and potential bugs.
/// Programs with warnings may still execute successfully, but likely contain
/// errors or inefficiencies.
///
/// # Current Limitations
///
/// - Assumes `u8` cells with wrapping arithmetic
/// - Cannot detect all infinite loops (see GCD analysis in code for [+*n] patterns)
/// - Does not track starting cell values (so warnings may be conservative)
/// - Does not perform data flow analysis
/// - Location tracking is placeholder (always reports start location)
///
/// # See Also
///
/// - Module documentation for detailed assumptions about overflow behavior
/// - [`BfWarning`] for the types of warnings that can be produced
pub fn validate(instructions: &[Instruction]) -> Vec<BfWarning> {
    let mut warnings = Vec::new();
    let location = SourceLocation::start(); // Placeholder - would need actual tracking

    validate_instructions(instructions, &mut warnings, 0, location);
    warnings
}

fn validate_instructions(
    instructions: &[Instruction],
    warnings: &mut Vec<BfWarning>,
    depth: usize,
    location: SourceLocation,
) {
    // Check for extreme nesting
    if depth > 10 {
        warnings.push(BfWarning::ExtremeNesting { location, depth });
    }

    for instruction in instructions {
        if let Instruction::Loop(body) = instruction {
            // Check for empty loops
            if body.is_empty() {
                warnings.push(BfWarning::EmptyLoop { location });
            } else {
                // Check for suspicious patterns
                check_suspicious_loop_patterns(body, warnings, location);

                // Recursively validate nested loops
                validate_instructions(body, warnings, depth + 1, location);
            }
        }
    }
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
) {
    // Note: [>] and [<] are common patterns for seeking in BF, so we don't warn about them
    // Note: [-] is a common pattern for clearing a cell, so we don't warn about it

    // Check for [+] and variants - complex termination behavior with u8 wrapping
    //
    // IMPORTANT: Termination depends on mathematical properties (GCD) and starting value!
    //
    // [+]: Always terminates (gcd(1,256)=1, visits all values including 0)
    // [++]: May be infinite! Depends on starting value:
    //   - From even: terminates (visits 0,2,4,...,254,0)
    //   - From odd: INFINITE (visits 1,3,5,...,255,1 - never 0!)
    // [+++]: Always terminates (gcd(3,256)=1)
    // [++++]: May terminate depending on starting value (gcd(4,256)=4)
    //
    // Since we don't know starting values at validation time, we warn about all of them.
    let all_increments = body
        .iter()
        .all(|i| matches!(i, Instruction::IncrementValue));
    if all_increments && !body.is_empty() {
        let gcd = gcd(body.len(), 256);
        let reason = if body.len() == 1 {
            "Inefficient pattern: loops ~256 times before reaching zero. Use [-] to clear a cell."
                .to_string()
        } else if gcd > 1 {
            format!(
                "Suspicious pattern: may be infinite or inefficient depending on starting cell value. \
                 Increment by {} only visits multiples of {} (gcd={}).",
                body.len(),
                gcd,
                gcd
            )
        } else {
            format!(
                "Inefficient pattern: loops ~{} times before reaching zero. Use [-] to clear a cell.",
                256 / body.len()
            )
        };

        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "+".repeat(body.len())),
            reason,
        });
    }

    // Check for [--] and variants - inefficient compared to [-]
    // Unlike [+], these are legitimately less efficient than the single-decrement form
    let all_decrements = body
        .iter()
        .all(|i| matches!(i, Instruction::DecrementValue));
    if all_decrements && body.len() > 1 {
        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "-".repeat(body.len())),
            reason: "Multiple decrements in a loop is inefficient. Consider using [-] to clear the cell.".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_validate_empty_loop() {
        let source = "[]";
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert_eq!(warnings.len(), 1);
        assert!(matches!(warnings[0], BfWarning::EmptyLoop { .. }));
    }

    #[test]
    fn test_validate_inefficient_increment_loop() {
        // [+] is inefficient (loops ~256 times), not infinite with u8 wrapping
        let source = "+[+]";
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(
            |w| matches!(w, BfWarning::SuspiciousPattern { pattern, .. } if pattern.contains("+"))
        ));
    }

    #[test]
    fn test_validate_extreme_nesting() {
        let source = "+[+[+[+[+[+[+[+[+[+[+[+[-]]]]]]]]]]]]"; // 12 levels
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, BfWarning::ExtremeNesting { depth, .. } if *depth > 10))
        );
    }

    #[test]
    fn test_validate_clean_program() {
        let source = "+++[->+++<]>."; // Simple clean program
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        // Should have no warnings
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_validate_multiple_warnings() {
        let source = "[]++[+]"; // Empty loop + inefficient increment loop
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert!(warnings.len() >= 2);
    }
}
