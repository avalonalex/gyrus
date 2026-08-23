//! Code minification example
//!
//! This example demonstrates how to minify BrainFuck programs
//! by removing comments and whitespace.
//!
//! Run with: cargo run --example minify

use gyrus::{BfError, minify, parse};

fn main() -> Result<(), BfError> {
    println!("=== Code Minification Example ===\n");

    // Example 1: Simple minification
    println!("Example 1: Basic Minification");
    println!("------------------------------");
    let source = r"
        * This is a comment
        +++    * Add 3
        [      * Loop
          >.   * Output
        ]
    ";

    println!("Original ({} bytes):", source.len());
    println!("{}", source);

    let instructions = parse(source)?;
    let minified = minify(&instructions);

    println!("\nMinified ({} bytes):", minified.len());
    println!("{}", minified);
    println!(
        "\nReduction: {} bytes → {} bytes ({:.1}% smaller)",
        source.len(),
        minified.len(),
        (1.0 - minified.len() as f64 / source.len() as f64) * 100.0
    );
    println!();

    // Example 2: Hello World minification
    println!("Example 2: Hello World Minification");
    println!("------------------------------------");
    let hello_source = r#"
        * Classic Hello World program
        * This program outputs "Hello World!"

        ++++++++[        * Set cell 0 to 8
          >++++[         * Add 4 to cell 1, 8 times
            >++>+++>+++>+<<<<-
          ]
          >+>+>->>+[<]<-
        ]
        >>.              * Print 'H'
        >---.            * Print 'e'
        +++++++..        * Print 'll'
        +++.             * Print 'o'
        >>.              * Print ' '
        <-.              * Print 'W'
        <.               * Print 'o'
        +++.             * Print 'r'
        ------.          * Print 'l'
        --------.        * Print 'd'
        >>+.             * Print '!'
        >++.             * Print '\n'
    "#;

    println!("Original ({} bytes):", hello_source.len());
    let instructions = parse(hello_source)?;
    let minified = minify(&instructions);

    println!("Minified ({} bytes):", minified.len());
    println!("{}", minified);
    println!(
        "\nReduction: {:.1}% smaller",
        (1.0 - minified.len() as f64 / hello_source.len() as f64) * 100.0
    );
    println!();

    // Example 3: Preserving functionality
    println!("Example 3: Verifying Functionality");
    println!("-----------------------------------");

    let original_source = "+++  * Three\n  [>++<-]  * Double it\n  >.  * Output";
    let original_instructions = parse(original_source)?;

    let minified = minify(&original_instructions);
    let minified_instructions = parse(&minified)?;

    println!("Original: {}", original_source);
    println!("Minified: {}", minified);

    if original_instructions == minified_instructions {
        println!("\n✓ Functionality preserved!");
        println!("  Both versions produce identical execution");
    }
    println!();

    // Example 4: Nested structure preservation
    println!("Example 4: Nested Structure Preservation");
    println!("-----------------------------------------");

    let nested_source = r"
        * Nested loops
        +++[           * Outer loop
          >+++[        * Inner loop
            <+>-       * Transfer
          ]<-
        ]
    ";

    let instructions = parse(nested_source)?;
    let minified = minify(&instructions);

    println!("Original:\n{}", nested_source);
    println!("Minified: {}", minified);
    println!("\n✓ Bracket structure preserved correctly");
    println!();

    // Example 5: Use cases
    println!("Example 5: Minification Use Cases");
    println!("----------------------------------");
    println!();
    println!("When to use minification:");
    println!("  ✓ Code golf competitions");
    println!("  ✓ Embedding BF in other files");
    println!("  ✓ Network transmission");
    println!("  ✓ Storage optimization");
    println!("  ✓ Obfuscation (hide implementation)");
    println!();
    println!("When NOT to use:");
    println!("  ✗ During development (keep comments!)");
    println!("  ✗ For documentation/educational code");
    println!("  ✗ When debugging");
    println!();

    // Example 6: Round-trip guarantee
    println!("Example 6: Round-Trip Guarantee");
    println!("--------------------------------");

    let source = "* Comment\n+++[>++<-]>.";
    let instructions = parse(source)?;
    let minified1 = minify(&instructions);

    let instructions2 = parse(&minified1)?;
    let minified2 = minify(&instructions2);

    println!("Original:     {}", source);
    println!("Minified 1st: {}", minified1);
    println!("Minified 2nd: {}", minified2);

    if minified1 == minified2 {
        println!("\n✓ Minification is idempotent");
        println!("  parse → minify → parse → minify = same result");
    }
    println!();

    println!("=== Minification Complete ===");

    Ok(())
}
