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
    CellModel, ExecutionConfigBuilder, interpret_optimized_with_io, interpret_with_io,
    io::StringIo, minify, optimize_with_cell_model, parse, parse_with_debug,
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

/// Expand a `.bfm` from `programs/`, run it, and return what it printed.
fn run(relative: &str, max_steps: u64) -> (String, String) {
    run_source(&read(relative), max_steps)
}

/// The same, for source that is not a file: the conditional test expands one
/// program twice, with a line taken out the second time.
///
/// The sequence -- expand, `parse_with_debug`, `remap`, `interpret_with_io` --
/// is what using this crate looks like end to end, so it is written once here
/// rather than per test.
fn run_source(bfm: &str, max_steps: u64) -> (String, String) {
    let expansion =
        gyrus_macro::expand(bfm).unwrap_or_else(|e| panic!("{}", e.format_with_source(bfm)));
    run_expansion(&expansion, max_steps)
}

/// The same, for an expansion already in hand: `@include` needs a file, so the
/// program that has one is expanded before it gets here.
fn run_expansion(expansion: &gyrus_macro::Expansion, max_steps: u64) -> (String, String) {
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

/// The same, on the optimized interpreter.
///
/// For the one program here that does enough work to care which engine runs
/// it: 8.6 million steps, which the tree-walker takes forty seconds over and
/// this finishes in fifty milliseconds. The expansion is ordinary BrainFuck,
/// so every engine is entitled to run it -- and that they agree is what the
/// differential suite in `gyrus` is for, not this one.
fn run_optimized(expansion: &gyrus_macro::Expansion, max_steps: u64) -> String {
    let instructions = parse(expansion.brainfuck()).expect("parses");
    let optimized = optimize_with_cell_model(&instructions, CellModel::default());
    let (mut input, mut output) = (StringIo::empty(), StringIo::empty());
    interpret_optimized_with_io(
        &optimized,
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_max_steps(max_steps)
            .build(),
        &mut input,
        &mut output,
    )
    .expect("runs");
    output.output_string()
}

/// Arithmetic on numbers too big for a cell, which is what a factoring program
/// is mostly made of.
///
/// 13911 is 3 times 4637, and 4637 is prime. `factor.bf` -- a program written
/// by somebody else -- says `13911: 3 4637` when given that number, and so
/// does this. The answer is also a fact about 13911 rather than about either
/// program, which is the better half of the reason to check it.
#[test]
fn the_factors_of_13911_come_out_as_three_and_4637() {
    let path = gyrus_corpus::workspace_root().join("programs/macros/factor.bfm");
    let source = read("programs/macros/factor.bfm");
    let expansion = gyrus_macro::expand_at(&source, &path)
        .unwrap_or_else(|failure| panic!("{}", failure.report()));

    assert_eq!(run_optimized(&expansion, 50_000_000), "13911: 3 4637\n");

    // Thirty thousand instructions, every one of them still pointing at a line
    // of the two hundred somebody wrote.
    let lines = source.lines().count();
    for offset in 0..expansion.brainfuck().len() {
        let origin = expansion.origin(offset).expect("every byte has an origin");
        assert!(
            origin.line <= lines,
            "byte {offset} names line {}",
            origin.line
        );
    }
}

/// The catalogue's own division, pasted in whole.
///
/// `lib/fast.bfm` is the one place in the corpus where an idiom is *not*
/// expressed in the macro language: the algorithm is a pointer walking a fixed
/// workspace, so the cells are pinned and the snippet goes in verbatim. This
/// pins its contract, including the two divisors it cannot do on its own --
/// one, which walks it off the end of its workspace, and zero, which has no
/// answer. Both were found by testing every divisor rather than a convenient
/// one, and neither is mentioned on the wiki.
#[test]
fn the_pasted_in_division_divides() {
    let path = gyrus_corpus::workspace_root().join("programs/macros/divide.bfm");
    let source = read("programs/macros/divide.bfm");
    let expansion = gyrus_macro::expand_at(&source, &path)
        .unwrap_or_else(|failure| panic!("{}", failure.report()));

    let (_, printed) = run_expansion(&expansion, 1_000_000);
    assert_eq!(
        printed,
        "9/2=4,1 7/7=1,0 5/9=0,5 6/3=2,0 0/4=0,0 8/1=8,0 4/0=0,4 \n"
    );
}

/// The one that is a program rather than a demonstration.
///
/// 199 lines of `.bfm` against 11,354 bytes of output, checked byte for byte
/// against `benchmarks/expected/99beer.txt` -- which is what
/// `programs/third-party/advanced/99beer.bf` prints, a program written by
/// somebody else years ago. Nothing here and nothing in this crate had a hand
/// in deciding what that file says, which is the whole point of testing
/// against it: agreement with it is not agreement with ourselves.
#[test]
fn ninety_nine_bottles_prints_what_the_program_it_was_written_from_prints() {
    let path = gyrus_corpus::workspace_root().join("programs/macros/99bottles.bfm");
    let source = read("programs/macros/99bottles.bfm");
    let expansion = gyrus_macro::expand_at(&source, &path)
        .unwrap_or_else(|failure| panic!("{}", failure.report()));

    let (brainfuck, printed) = run_expansion(&expansion, 20_000_000);
    assert_eq!(printed, read("benchmarks/expected/99beer.txt"));

    // Every byte of eleven thousand still names a line of the file somebody
    // wrote. A macro's bytes name the invocation, so the lines they name are
    // the eleven in the verse rather than the ones inside `@say`.
    let lines = source.lines().count();
    for offset in 0..brainfuck.len() {
        let origin = expansion.origin(offset).expect("every byte has an origin");
        assert!(
            origin.line <= lines,
            "byte {offset} names line {}",
            origin.line
        );
    }
}

/// A program whose vocabulary comes from another file.
///
/// Through `expand_file` rather than `run_source`, because the path in the
/// `@include` resolves against the file holding it -- which is the behaviour
/// under test as much as the output is.
#[test]
fn the_include_example_takes_its_vocabulary_from_a_library() {
    let path = gyrus_corpus::workspace_root().join("programs/macros/include.bfm");
    let source = read("programs/macros/include.bfm");
    let expansion = gyrus_macro::expand_at(&source, &path)
        .unwrap_or_else(|failure| panic!("{}", failure.report()));

    let (brainfuck, printed) = run_expansion(&expansion, 100_000);
    assert_eq!(printed, "hi\n");

    // Nothing of the library survives as *text*: what it contributed is three
    // macros and two names, and the instructions are the ones this file asked
    // for. That is what makes the source map still hold.
    assert!(brainfuck.chars().all(|c| "+-<>.,[]".contains(c)));
    for offset in 0..brainfuck.len() {
        let origin = expansion.origin(offset).expect("every byte has an origin");
        assert!(
            expansion.source().lines().count() >= origin.line,
            "byte {offset} names line {} of a file with fewer",
            origin.line
        );
    }
}

/// Conditional compilation, and what it means for it to be *compilation*.
///
/// The marks are not skipped at run time when `TRACE` is off -- they are
/// absent from the BrainFuck. Expanding the same file with the `@define`
/// removed is the whole demonstration, so the test does it both ways.
#[test]
fn the_conditional_example_compiles_its_tracing_in_and_out() {
    let source = read("programs/macros/conditional.bfm");
    let (with_trace, printed) = run_source(&source, 100_000);
    assert_eq!(printed, "H.i.!");

    let without = source.replace("@define TRACE 1", "* TRACE is off");
    let (without_trace, printed) = run_source(&without, 100_000);
    assert_eq!(printed, "Hi!");

    // Shorter, not merely quieter: the marks left no instructions behind.
    // The two numbers are the ones `programs/README.md` quotes, asserted here
    // so the file cannot drift away from the claim made about it.
    assert_eq!(
        (with_trace.len(), without_trace.len()),
        (322, 222),
        "the instruction counts in programs/README.md are stale"
    );
}
