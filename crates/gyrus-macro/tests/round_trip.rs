//! The proof that the expander produces real BrainFuck: a program from
//! `programs/` rewritten as `.bfm`, expanded, and held against the original.
//!
//! This is the PRD's own first success criterion, and it is a better first
//! test than anything hand-written because the target already exists and is
//! already pinned down -- `hello_world` is a case in
//! `programs/test_manifest.toml`, so its output is checked on the tree-walker,
//! the optimized interpreter and the JIT. Matching it character for character
//! inherits all of that.
//!
//! It is also what keeps `programs/macros/` from quietly emptying out: the
//! corpus-directory test counts `.bf` files, and this directory holds none.

use gyrus::{
    ExecutionConfigBuilder, interpret_with_io, io::StringIo, minify, parse, parse_with_debug,
};

fn read(relative: &str) -> String {
    let path = gyrus_corpus::workspace_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

#[test]
fn the_macro_hello_world_expands_to_the_program_it_was_written_from() {
    let bfm = read("programs/macros/hello_world.bfm");
    let original = read("programs/basic/hello_world.bf");

    let expansion =
        gyrus_macro::expand(&bfm).unwrap_or_else(|e| panic!("{}", e.format_with_source(&bfm)));

    // The `.bf` file ends with a newline and an expansion does not. That is a
    // fact about files rather than about programs; everything else has to
    // match character for character.
    assert_eq!(
        expansion.brainfuck(),
        original.trim_end(),
        "the expansion is not the program it was written from"
    );

    // The same claim stated about programs rather than about text, which is
    // the half that would survive either side being reformatted.
    assert_eq!(
        minify(&parse(expansion.brainfuck()).expect("expansion parses")),
        minify(&parse(&original).expect("original parses")),
    );
}

/// Expand a `.bfm`, remap its debug info, run it, and return what it printed.
///
/// The sequence -- expand, `parse_with_debug`, `remap`, `interpret_with_io` --
/// is what using this crate looks like end to end, so it is written once here
/// rather than per test.
fn run(relative: &str, max_steps: u64) -> (String, String) {
    let bfm = read(relative);
    let expansion =
        gyrus_macro::expand(&bfm).unwrap_or_else(|e| panic!("{}", e.format_with_source(&bfm)));

    let (instructions, expanded) = parse_with_debug(expansion.brainfuck()).expect("parses");
    let debug_info = expansion.remap(&expanded);

    let (mut input, mut output) = (StringIo::empty(), StringIo::empty());
    interpret_with_io(
        &instructions,
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_max_steps(max_steps)
            .build(),
        &mut input,
        &mut output,
        Some(&debug_info),
    )
    .expect("runs");

    (expansion.brainfuck().to_string(), output.output_string())
}

#[test]
fn the_expansion_prints_hello_world() {
    let (_, printed) = run("programs/macros/hello_world.bfm", 1_000_000);
    assert_eq!(printed, "Hello World!\n");
}

/// Named cells end to end. `hello_world.bfm` proves the expander against a
/// program that already exists; this one is written the way a `.bfm` would be
/// written from scratch, so what pins it down is its output.
#[test]
fn the_variables_example_prints_hi() {
    let (brainfuck, printed) = run("programs/macros/variables.bfm", 100_000);
    // The multiply idiom, which is also what the optimizer folds into a
    // MultiplyAdd -- so this exercises a shape worth exercising.
    assert_eq!(
        brainfuck,
        "++++++++[>+++++++++<-]>.+++++++++++++++++++++++++++++++++."
    );
    assert_eq!(printed, "Hi");
}

/// Macros end to end, and the point of them: the same idiom three times over,
/// written once. Pinned by its output, like the variables example.
#[test]
fn the_macros_example_prints_hi() {
    let (brainfuck, printed) = run("programs/macros/macros.bfm", 100_000);
    assert_eq!(printed, "Hi!");
    // A macro is not a subroutine: each invocation expands in place, so the
    // three multiply loops are all there in the output.
    assert_eq!(brainfuck.matches('[').count(), 6, "{brainfuck}");
}
