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
//! Validation warnings are independent of the memory model (Fixed/Wrapping/Unbounded):
//!
//! - **Extreme nesting** (depth > 10) warns regardless of memory model
//! - **Empty loops** `[]` warn regardless of memory model
//! - **Pointer seeking patterns** like `[>]` and `[<]` are NOT warned about,
//!   as they are idiomatic BrainFuck patterns for seeking non-zero cells
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
//! // Inefficient pattern - warning (NOT infinite with u8 wrapping, just inefficient)
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
            // A loop is considered empty if it has no instructions OR only has LoopCheck
            let is_empty =
                body.is_empty() || (body.len() == 1 && matches!(body[0], Instruction::LoopCheck));

            if is_empty {
                warnings.push(BfWarning::EmptyLoop { location });
            } else {
                // Check for suspicious patterns (assumes u8 wrapping)
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
