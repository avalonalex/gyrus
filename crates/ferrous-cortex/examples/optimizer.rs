use ferrous_cortex::{optimize, parse};

fn main() -> Result<(), ferrous_cortex::BfError> {
    // Example 1: Simple instruction fusion
    let source1 = "+++>>>---";
    let instructions1 = parse(source1)?;
    let optimized1 = optimize(&instructions1);

    println!("=== Example 1: Instruction Fusion ===");
    println!("Source: {}", source1);
    println!("Original instructions: {}", optimized1.original_count);
    println!("Optimized instructions: {}", optimized1.optimized_count);
    println!("Compression ratio: {:.2}×", optimized1.compression_ratio());
    println!("Optimized IR:");
    for (i, inst) in optimized1.instructions.iter().enumerate() {
        let range = inst.source_range();
        println!(
            "  [{}] {:?} (original range: {}..{})",
            i, inst, range.start, range.end
        );
    }
    println!();

    // Example 2: Loop pattern recognition
    let source2 = "++[-]>>>[>]";
    let instructions2 = parse(source2)?;
    let optimized2 = optimize(&instructions2);

    println!("=== Example 2: Loop Pattern Recognition ===");
    println!("Source: {}", source2);
    println!("Original instructions: {}", optimized2.original_count);
    println!("Optimized instructions: {}", optimized2.optimized_count);
    println!("Compression ratio: {:.2}×", optimized2.compression_ratio());
    println!("Optimized IR:");
    for (i, inst) in optimized2.instructions.iter().enumerate() {
        let range = inst.source_range();
        println!(
            "  [{}] {:?} (original range: {}..{})",
            i, inst, range.start, range.end
        );
    }
    println!();

    // Example 3: Simple move pattern
    let source3 = "[->+<]";
    let instructions3 = parse(source3)?;
    let optimized3 = optimize(&instructions3);

    println!("=== Example 3: Simple Move Pattern ===");
    println!("Source: {} (move cell value 1 position right)", source3);
    println!("Original instructions: {}", optimized3.original_count);
    println!("Optimized instructions: {}", optimized3.optimized_count);
    println!("Compression ratio: {:.2}×", optimized3.compression_ratio());
    println!("Optimized IR:");
    for (i, inst) in optimized3.instructions.iter().enumerate() {
        let range = inst.source_range();
        println!(
            "  [{}] {:?} (original range: {}..{})",
            i, inst, range.start, range.end
        );
    }
    println!();

    // Example 3b: Multiply pattern
    let source3b = "[->++<]";
    let instructions3b = parse(source3b)?;
    let optimized3b = optimize(&instructions3b);

    println!("=== Example 3b: Multiply Pattern ===");
    println!("Source: {} (multiply by 2 and move)", source3b);
    println!("Original instructions: {}", optimized3b.original_count);
    println!("Optimized instructions: {}", optimized3b.optimized_count);
    println!("Compression ratio: {:.2}×", optimized3b.compression_ratio());
    println!("Optimized IR:");
    for (i, inst) in optimized3b.instructions.iter().enumerate() {
        let range = inst.source_range();
        println!(
            "  [{}] {:?} (original range: {}..{})",
            i, inst, range.start, range.end
        );
    }
    println!();

    // Example 3c: Multi-target multiply
    let source3c = "[->+++>+<<]";
    let instructions3c = parse(source3c)?;
    let optimized3c = optimize(&instructions3c);

    println!("=== Example 3c: Multi-Target Multiply ===");
    println!(
        "Source: {} (multiply by 3 to offset 1, by 1 to offset 2)",
        source3c
    );
    println!("Original instructions: {}", optimized3c.original_count);
    println!("Optimized instructions: {}", optimized3c.optimized_count);
    println!("Compression ratio: {:.2}×", optimized3c.compression_ratio());
    println!("Optimized IR:");
    for (i, inst) in optimized3c.instructions.iter().enumerate() {
        let range = inst.source_range();
        println!(
            "  [{}] {:?} (original range: {}..{})",
            i, inst, range.start, range.end
        );
    }
    println!();

    // Example 4: Complex nested program
    let source4 = "+++++[>+++++<-]>[->>+<<]";
    let instructions4 = parse(source4)?;
    let optimized4 = optimize(&instructions4);

    println!("=== Example 4: Complex Nested Program ===");
    println!("Source: {}", source4);
    println!("Original instructions: {}", optimized4.original_count);
    println!("Optimized instructions: {}", optimized4.optimized_count);
    println!("Compression ratio: {:.2}×", optimized4.compression_ratio());
    println!("Optimized IR:");
    for (i, inst) in optimized4.instructions.iter().enumerate() {
        let range = inst.source_range();
        println!(
            "  [{}] {:?} (original range: {}..{})",
            i, inst, range.start, range.end
        );
    }

    Ok(())
}
