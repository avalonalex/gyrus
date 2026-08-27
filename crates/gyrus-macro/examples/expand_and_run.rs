//! Expand a `.bfm` and run what comes out.
//!
//! What `gyrus prog.bfm` does, in the four lines in the middle of this file:
//! expand, parse the expansion, rewrite its debug symbols to name the macro
//! source, run. Kept as an example rather than folded into the CLI because it
//! is the shortest statement of how a program uses this crate, and because
//! `scripts/check-examples.sh` then runs the whole pipeline on every commit.
//!
//! ```text
//! cargo run -p gyrus-macro --example expand_and_run -- programs/macros/scan.bfm
//! ```
//!
//! With no argument it runs `programs/macros/arithmetic.bfm`, so that
//! `scripts/check-examples.sh` exercises the whole pipeline.

use gyrus::{ExecutionConfigBuilder, interpret_with_io, io::StringIo, parse_with_debug};

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the workspace root");
    let path = match std::env::args().nth(1) {
        Some(given) => std::path::PathBuf::from(given),
        None => root.join("programs/macros/arithmetic.bfm"),
    };

    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    // Expand, and report a macro error against the macro source.
    let expansion = gyrus_macro::expand(&source).unwrap_or_else(|e| {
        eprintln!("{}", e.format_with_source(&source));
        std::process::exit(1);
    });

    // Parse the expansion, then rewrite its debug info to name the `.bfm`.
    // This is the whole trick: from here on, every located error reports the
    // line and column somebody wrote.
    let (instructions, expanded) = parse_with_debug(expansion.brainfuck())
        .unwrap_or_else(|e| panic!("the expansion does not parse: {e}"));
    let debug_info = expansion.remap(&expanded);

    println!("{}", expansion.brainfuck());

    let (mut input, mut output) = (StringIo::empty(), StringIo::empty());
    let config = ExecutionConfigBuilder::new()
        .with_memory_size(30_000)
        .with_max_steps(10_000_000)
        .build();
    match interpret_with_io(
        &instructions,
        config,
        &mut input,
        &mut output,
        Some(&debug_info),
    ) {
        Ok(stats) => println!(
            "{:?}  ({} steps)",
            output.output_string(),
            stats.total_steps
        ),
        Err(e) => {
            eprintln!("{}", e.format_with_source(expansion.source()));
            std::process::exit(1);
        }
    }
}
