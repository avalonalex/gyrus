use crate::error::BfWarning;
use crate::instruction::Instruction;
use crate::location::SourceLocation;

/// Validate parsed instructions for common issues and potential problems
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
        match instruction {
            Instruction::Loop(body) => {
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
            _ => {}
        }
    }
}

fn check_suspicious_loop_patterns(
    body: &[Instruction],
    warnings: &mut Vec<BfWarning>,
    location: SourceLocation,
) {
    // Note: [>] and [<] are common patterns for seeking in BF, so we don't warn about them
    // Note: [-] is a common pattern for clearing a cell, so we don't warn about it

    // Check for [+] which creates an infinite loop (cell can never reach zero by incrementing)
    if body.len() == 1 && matches!(body[0], Instruction::IncrementValue) {
        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: "[+]".to_string(),
            reason:
                "This pattern creates an infinite loop (cell will never reach zero by incrementing)"
                    .to_string(),
        });
    }

    // Check for [-] followed only by other decrements (e.g., [--])
    // This is suspicious because it's not as efficient as [-]
    let all_decrements = body
        .iter()
        .all(|i| matches!(i, Instruction::DecrementValue));
    if all_decrements && body.len() > 1 {
        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "-".repeat(body.len())),
            reason: format!("Multiple decrements in a loop is inefficient. Consider using [-] to clear the cell."),
        });
    }

    // Check for [+] followed by other increments (e.g., [++])
    let all_increments = body
        .iter()
        .all(|i| matches!(i, Instruction::IncrementValue));
    if all_increments && body.len() >= 1 {
        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "+".repeat(body.len())),
            reason:
                "This pattern creates an infinite loop (cell will never reach zero by incrementing)"
                    .to_string(),
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
    fn test_validate_infinite_loop() {
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
        let source = "[]++[+]"; // Empty loop + infinite loop
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert!(warnings.len() >= 2);
    }
}
