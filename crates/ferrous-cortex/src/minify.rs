use crate::instruction::Instruction;

/// Convert instructions back to BrainFuck source code (minified - no comments)
pub fn minify(instructions: &[Instruction]) -> String {
    let mut output = String::new();
    minify_instructions(instructions, &mut output);
    output
}

fn minify_instructions(instructions: &[Instruction], output: &mut String) {
    for instruction in instructions {
        match instruction {
            Instruction::IncrementPointer => output.push('>'),
            Instruction::DecrementPointer => output.push('<'),
            Instruction::IncrementValue => output.push('+'),
            Instruction::DecrementValue => output.push('-'),
            Instruction::Output => output.push('.'),
            Instruction::Input => output.push(','),
            Instruction::Loop(body) => {
                output.push('[');
                minify_instructions(body, output);
                output.push(']');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_minify_simple() {
        let source = "+++  Comments here\n>++ More comments\n.";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "+++>++.");
    }

    #[test]
    fn test_minify_with_line_comments() {
        let source = "* Line comment\n+++  * Inline comment\n[>++<-]  * Loop comment";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "+++[>++<-]");
    }

    #[test]
    fn test_minify_nested_loops() {
        let source = "[[+]]";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "[[+]]");
    }

    #[test]
    fn test_minify_all_commands() {
        let source = "* Test all commands\n><+-.,[]";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "><+-.,[]");
    }

    #[test]
    fn test_minify_round_trip() {
        // Parse, minify, parse again should give same result
        let source = "+++[>++<-]>.";
        let instructions1 = parse(source).unwrap();
        let minified = minify(&instructions1);
        let instructions2 = parse(&minified).unwrap();
        assert_eq!(instructions1, instructions2);
    }
}
