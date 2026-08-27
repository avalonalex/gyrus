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

/// Answers the source does not contain.
///
/// `hello_world.bfm` and `variables.bfm` transcribe values a reader could
/// check by eye. This one computes them: nothing in it says 72 or 105, so the
/// program has to be right for the output to be "Hi". That is the property a
/// *generated* program needs before its expected output proves anything, which
/// is what makes this the shape to generate rather than a bigger example.
#[test]
fn the_arithmetic_example_computes_its_answers() {
    let (_, printed) = run("programs/macros/arithmetic.bfm", 1_000_000);
    assert_eq!(printed, "Hi");
}

/// A loop opened by one macro and closed by another.
///
/// The standard library the design proposes is built on this -- `while_not_zero`
/// and `end_while` are separate macros bracketing a region -- and it is not
/// obvious that the bracket stack and the loop-balance check survive it, since
/// both are maintained across a body boundary the macro knows nothing about.
#[test]
fn the_control_example_brackets_a_loop_across_two_macros() {
    let (brainfuck, printed) = run("programs/macros/control.bfm", 100_000);
    assert_eq!(printed, "ABC");
    // And it comes out as the idiom somebody would have written by hand.
    assert_eq!(brainfuck.split('[').next_back().expect("a loop"), ">.+<-]");
}

/// The scan idiom, and the way back from it.
///
/// `[.>]` moves the cursor by one each iteration, so after it the expander
/// cannot say where the cursor is -- and `@here` is the only construct that
/// can tell it. This is the one path where a wrong answer is silent, so it is
/// worth a program that runs rather than only a unit test that expands.
#[test]
fn the_scan_example_finds_its_way_back() {
    let (_, printed) = run("programs/macros/scan.bfm", 100_000);
    assert_eq!(printed, "Hi!\n");
}

/// An array of records walked by a scan, with its fields named.
///
/// The shape large BrainFuck programs are built from -- mandelbrot's tape and
/// the standard BF library's array idiom are both this, at different strides;
/// `scripts/check-mandelbrot-claims.py` measures the first. A scan stops
/// wherever the data says, so the cursor's cell is unknowable -- but it moved
/// a whole record each time, so which field it is on is not.
#[test]
fn the_records_example_walks_an_array_by_field_name() {
    let (brainfuck, printed) = run("programs/macros/records.bfm", 100_000);
    assert_eq!(printed, "A1B2C3");
    // The loop is written entirely in field names and still comes out as a
    // scan over the array, which is the whole claim.
    assert!(brainfuck.ends_with("[>.>.<<>>>]"), "{brainfuck}");
}

/// The standard idiom catalogue, transcribed rather than adapted.
///
/// <https://esolangs.org/wiki/Brainfuck_algorithms> writes its idioms in a
/// notation where `x`, `y` and `temp0` are cell names and juxtaposition is
/// movement -- `y[x+temp0+y-]` means "at y, loop, add at x, add at temp0, back
/// to y". That is what `@var` and `@to` are, so the equality test transcribes
/// line for line. Every loop body in it returns to the cell it tested, which
/// is why the expander can follow the cursor through the whole thing.
///
/// Five comparisons, chosen to cover both answers and both ends of a cell.
#[test]
fn the_compare_example_transcribes_a_catalogued_idiom() {
    let (_, printed) = run("programs/macros/compare.bfm", 1_000_000);
    assert_eq!(printed, "10110");
}

/// Conditional compilation, and what it means for it to be *compilation*.
///
/// The marks are not skipped at run time when `TRACE` is off -- they are
/// absent from the BrainFuck. Expanding the same file with the `@define`
/// removed is the whole demonstration, so the test does it both ways.
#[test]
fn the_conditional_example_compiles_its_tracing_in_and_out() {
    let (with_trace, printed) = run("programs/macros/conditional.bfm", 100_000);
    assert_eq!(printed, "H.i.!");

    let source = read("programs/macros/conditional.bfm");
    let without = source.replace("@define TRACE 1", "* TRACE is off");
    let expansion = gyrus_macro::expand(&without)
        .unwrap_or_else(|e| panic!("{}", e.format_with_source(&without)));

    // Shorter, not merely quieter: the marks left no instructions behind.
    assert!(
        expansion.brainfuck().len() < with_trace.len(),
        "turning tracing off did not shrink the program"
    );

    let (instructions, expanded) = parse_with_debug(expansion.brainfuck()).expect("parses");
    let debug_info = expansion.remap(&expanded);
    let (mut input, mut output) = (StringIo::empty(), StringIo::empty());
    interpret_with_io(
        &instructions,
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_max_steps(100_000)
            .build(),
        &mut input,
        &mut output,
        Some(&debug_info),
    )
    .expect("runs");
    assert_eq!(output.output_string(), "Hi!");
}
