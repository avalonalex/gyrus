use clap::{Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use gyrus::{
    BfError, CellModel, DebugInfo, SourceLocation,
    codegen::compile_string,
    minify, optimize_with_cell_model, parse, parse_with_debug,
    random::{
        IdiomaticConfig, RandomProgramConfig, generate_idiomatic_program, generate_random_program,
    },
    syntax::{CharClass, ColorTheme, SyntaxHighlighter, classify_line},
    validate_with_cell_model,
};

#[derive(Parser)]
#[command(name = "gyrus-tool")]
#[command(about = "Development and analysis tools for BrainFuck programs", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Expand a macro program (.bfm) into BrainFuck
    Expand {
        /// Macro source file to expand
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show what the expansion cost
        #[arg(short, long)]
        verbose: bool,
    },

    /// Minify BF program (strip comments and whitespace)
    Minify {
        /// BrainFuck source file to minify
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show compression statistics
        #[arg(short, long)]
        verbose: bool,
    },

    /// Validate program and show warnings
    Validate {
        /// BrainFuck source file to validate
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Cell model to assume for validation
        #[arg(long, default_value = "wrapping")]
        cell_model: String,

        /// Exit with error if warnings found (for CI/CD)
        #[arg(long)]
        strict: bool,

        /// Show additional validation context
        #[arg(short, long)]
        verbose: bool,
    },

    /// Inspect debug symbols and source locations
    DebugInfo {
        /// BrainFuck source file to inspect
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output format: table, json, csv
        #[arg(long, default_value = "table")]
        format: String,

        /// Include source code context
        #[arg(long)]
        show_source: bool,
    },

    /// Display BF program with syntax highlighting
    View {
        /// BrainFuck source file to view
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Show line numbers
        #[arg(short = 'n', long)]
        line_numbers: bool,

        /// Theme: dark, light
        #[arg(long, default_value = "dark")]
        theme: String,

        /// Plain output (no colors)
        #[arg(long)]
        plain: bool,
    },

    /// Generate random BrainFuck program
    Generate {
        /// Average number of commands in the program
        #[arg(short = 'l', long, default_value = "50")]
        length: usize,

        /// Maximum loop nesting depth
        #[arg(short = 'd', long, default_value = "3")]
        max_depth: usize,

        /// Probability of adding loops (0.0 to 1.0)
        #[arg(short = 'p', long, default_value = "0.3")]
        loop_probability: f64,

        /// Compose the program from idioms (clears, multiplies, scans,
        /// counted loops) on a 32-cell tape, so that it terminates and stays
        /// on the tape. --length is then the number of fragments, and
        /// --loop-probability is ignored.
        #[arg(long)]
        idiomatic: bool,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show generation statistics
        #[arg(short, long)]
        verbose: bool,
    },

    /// Compile string to BrainFuck program
    Compile {
        /// Text to compile into a BrainFuck program
        text: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show compilation statistics
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show optimization mapping (visual)
    Optimize {
        /// BrainFuck source file to optimize
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Theme: dark, light
        #[arg(long, default_value = "dark")]
        theme: String,

        /// Plain output (no colors)
        #[arg(long)]
        plain: bool,

        /// Cell model the program will run under: wrapping (default) or
        /// checked. Checked cells disable multiply-loop folding, so the
        /// mapping and the compression ratio differ between the two.
        #[arg(long, default_value = "wrapping")]
        cell_model: String,
    },
}

fn main() {
    if let Err(e) = run() {
        // `format_detailed` rather than `{}`: it prints the hint the error
        // carries and the io::Error underneath it, both of which the bare
        // Display drops -- so a failed write reported only its path and not
        // why it failed.
        eprintln!("{}", e.format_detailed());
        std::process::exit(1);
    }
}

fn run() -> Result<(), BfError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Expand {
            file,
            output,
            verbose,
        } => run_expand(file, output, verbose),
        Commands::Minify {
            file,
            output,
            verbose,
        } => run_minify(file, output, verbose),
        Commands::Validate {
            file,
            cell_model,
            strict,
            verbose,
        } => run_validate(file, cell_model, strict, verbose),
        Commands::DebugInfo {
            file,
            format,
            show_source,
        } => run_debug_info(file, format, show_source),
        Commands::View {
            file,
            line_numbers,
            theme,
            plain,
        } => run_view(file, line_numbers, theme, plain),
        Commands::Generate {
            length,
            max_depth,
            loop_probability,
            idiomatic,
            output,
            verbose,
        } => run_generate(
            length,
            max_depth,
            loop_probability,
            idiomatic,
            output,
            verbose,
        ),
        Commands::Compile {
            text,
            output,
            verbose,
        } => run_compile(text, output, verbose),
        Commands::Optimize {
            file,
            theme,
            plain,
            cell_model,
        } => run_optimize(file, theme, plain, &cell_model),
    }
}
/// Whether two paths name the same file, as far as the filesystem will say.
///
/// Falls back to comparing the paths when either cannot be canonicalized --
/// an output file that does not exist yet, most obviously.
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Read a file, or say why not.
///
/// The same six lines were written at each subcommand that reads one. They had
/// not drifted, which is luck rather than design: six copies of a user-facing
/// hint is six places for the next edit to reach five of.
fn read_file(file: &std::path::Path) -> Result<String, BfError> {
    fs::read_to_string(file).map_err(|source| BfError::FileError {
        path: file.to_path_buf(),
        source,
        hint: format!(
            "Make sure the file exists and you have permission to read it. \
             Current path: {}",
            file.display()
        ),
    })
}

/// Read a *BrainFuck* source file: [`read_file`], and macro source refused.
///
/// Every subcommand but `expand` parses BrainFuck, and a `.bfm` parsed as
/// BrainFuck is not an error -- it is a different program, so `validate`
/// reported no warnings and `minify` produced twenty-four bytes bearing no
/// relation to the file. Saying so is the whole fix.
fn read_source(file: &std::path::Path) -> Result<String, BfError> {
    if gyrus_macro::is_macro_path(file) {
        return Err(BfError::ConfigurationError {
            message: format!(
                "{} is macro source. Expand it first -- `gyrus-tool expand {}` -- or run it \
                 directly with `gyrus`.",
                file.display(),
                file.display()
            ),
        });
    }
    read_file(file)
}

/// Expand a macro program into BrainFuck.
///
/// The counterpart to running one, and the only subcommand here that takes
/// macro source rather than refusing it. What it is *for* is in
/// `docs/tooling.md`; what is worth knowing in the code is below.
fn run_expand(file: PathBuf, output: Option<PathBuf>, verbose: bool) -> Result<(), BfError> {
    // A macro error is rendered against the macro source, with a caret, the
    // way a parse error is. It is not a `BfError`, so it is reported and
    // exited on here rather than returned. Through `expand_file` because
    // `@include` resolves against the directory holding the file.
    let expansion = gyrus_macro::expand_file(&file).unwrap_or_else(|failure| {
        eprintln!("{}", failure.report());
        std::process::exit(1);
    });
    let source = expansion.source().to_string();
    let brainfuck = expansion.brainfuck();

    if verbose {
        // Instructions written against instructions emitted, not bytes
        // against instructions: a `.bfm` is mostly prose and declarations, so
        // comparing its size to the output says more about how well it is
        // commented than about what the macros did. What they did is expand
        // the instructions somebody typed into more of them.
        //
        // Through the library's own classifier rather than a fourth spelling
        // of what counts as code. `CharClass` exists because that rule had
        // three encodings once, and its doc comment says two too many; a
        // `*`-comment rule and an instruction alphabet written out here would
        // have been the fourth, in the one place getting it wrong is
        // invisible -- which is how this counted every sentence-ending '.' in
        // the prose and reported hello_world.bfm as a 0.9x expansion.
        let written = source
            .lines()
            .flat_map(classify_line)
            .filter(|class| !matches!(class, CharClass::Comment | CharClass::Whitespace))
            .count();
        let emitted = brainfuck.len();
        eprintln!("Expanded {}", file.display());
        eprintln!("  Macro source:        {} bytes", source.len());
        eprintln!("  Instructions written: {written}");
        eprintln!("  Instructions emitted: {emitted}");
        if written > 0 {
            eprintln!(
                "  Expansion:            {:.1}x",
                emitted as f64 / written as f64
            );
        }
        eprintln!();
    }

    let Some(path) = output else {
        println!("{brainfuck}");
        return Ok(());
    };

    // Refused rather than done: the source was read into memory, so this would
    // succeed and replace the only copy of a hand-written program with
    // generated BrainFuck. `-o prog.bf` is one character away.
    if same_file(&file, &path) {
        return Err(BfError::ConfigurationError {
            message: format!(
                "{} is the macro source; expanding over it would destroy it",
                path.display()
            ),
        });
    }

    // `FileError` says "Failed to read source file", which this is not.
    // Writing the newline separately rather than through a `format!` also
    // spares a second copy of an expansion that is the one thing this tool is
    // meant to make large.
    let failed = |operation: String| {
        move |source| BfError::IoError {
            operation,
            instruction_index: None,
            source,
        }
    };
    let mut out =
        fs::File::create(&path).map_err(failed(format!("creating {}", path.display())))?;
    out.write_all(brainfuck.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .map_err(failed(format!("writing {}", path.display())))?;
    if verbose {
        eprintln!("Written to {}", path.display());
    }
    Ok(())
}

fn run_minify(file: PathBuf, output: Option<PathBuf>, verbose: bool) -> Result<(), BfError> {
    // Read source file
    let source = read_source(&file)?;

    // Parse the program
    let (instructions, _debug_info) = parse_with_debug(&source)?;

    // Minify
    let minified = minify(&instructions);

    // Output
    if let Some(output_path) = &output {
        // Write to file
        fs::write(output_path, &minified).map_err(|source_err| BfError::FileError {
            path: output_path.clone(),
            source: source_err,
            hint: format!(
                "Make sure you have write permission for: {}",
                output_path.display()
            ),
        })?;

        if verbose {
            let reduction = 100.0 * (1.0 - (minified.len() as f64 / source.len() as f64));
            eprintln!(
                "Minified {} bytes to {} bytes ({:.1}% reduction)",
                source.len(),
                minified.len(),
                reduction
            );
            eprintln!("Output written to: {}", output_path.display());
        }
    } else {
        // Write to stdout
        println!("{}", minified);

        if verbose {
            let reduction = 100.0 * (1.0 - (minified.len() as f64 / source.len() as f64));
            eprintln!(
                "Minified {} bytes to {} bytes ({:.1}% reduction)",
                source.len(),
                minified.len(),
                reduction
            );
        }
    }

    Ok(())
}

fn run_validate(
    file: PathBuf,
    cell_model: String,
    strict: bool,
    _verbose: bool,
) -> Result<(), BfError> {
    // Read source file
    let source = read_source(&file)?;

    // Parse the program
    let (instructions, debug_info) = parse_with_debug(&source)?;

    let model = match cell_model.parse::<CellModel>() {
        Ok(model) => model,
        Err(()) => {
            eprintln!(
                "Error: Invalid cell model '{}'. Valid options: wrapping, checked",
                cell_model
            );
            std::process::exit(1);
        }
    };

    let warnings = validate_with_cell_model(&instructions, &debug_info, model);

    if warnings.is_empty() {
        println!("✓ Validation complete: No warnings found");
        Ok(())
    } else {
        eprintln!("Validation found {} warning(s):\n", warnings.len());
        for warning in &warnings {
            // With source context and a caret, the way errors are shown. A
            // warning that names a line the reader then has to go and find is
            // doing half the job.
            eprintln!("{}\n", warning.format_with_source(&source));
        }

        eprintln!("Validation complete: {} warnings", warnings.len());

        if strict {
            std::process::exit(1);
        } else {
            Ok(())
        }
    }
}

fn run_debug_info(file: PathBuf, format: String, show_source: bool) -> Result<(), BfError> {
    // Read source file
    let source = read_source(&file)?;

    // Parse the program with debug symbols
    let (_instructions, debug_info) = parse_with_debug(&source)?;

    // Output in requested format
    match format.to_lowercase().as_str() {
        "table" => display_debug_table(&source, &debug_info, show_source),
        "json" => display_debug_json(&source, &debug_info),
        "csv" => display_debug_csv(&debug_info),
        other => {
            eprintln!(
                "Error: Invalid format '{}'. Valid options: table, json, csv",
                other
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

fn run_view(file: PathBuf, line_numbers: bool, theme: String, plain: bool) -> Result<(), BfError> {
    // Read source file
    let source = read_source(&file)?;

    // Select theme
    let color_theme = match theme.to_lowercase().as_str() {
        "dark" => ColorTheme::dark(),
        "light" => ColorTheme::light(),
        other => {
            eprintln!(
                "Error: Invalid theme '{}'. Valid options: dark, light",
                other
            );
            std::process::exit(1);
        }
    };

    // Create highlighter
    let highlighter = SyntaxHighlighter::with_theme(color_theme).show_line_numbers(line_numbers);

    // Highlight the code
    let highlighted = highlighter.highlight(&source);

    // Output
    if plain {
        print!("{}", highlighted.to_plain());
    } else {
        print!("{}", highlighted.to_ansi());
    }

    Ok(())
}

fn run_generate(
    length: usize,
    max_depth: usize,
    loop_probability: f64,
    idiomatic: bool,
    output: Option<PathBuf>,
    verbose: bool,
) -> Result<(), BfError> {
    // Validate parameters
    if !(0.0..=1.0).contains(&loop_probability) {
        eprintln!(
            "Error: loop-probability must be between 0.0 and 1.0, got {}",
            loop_probability
        );
        std::process::exit(1);
    }

    let mut rng = rand::rng();
    let program = if idiomatic {
        let config = IdiomaticConfig {
            fragments: length,
            max_depth,
            ..IdiomaticConfig::default()
        };
        if verbose {
            eprintln!("Generating an idiomatic BrainFuck program...");
            eprintln!("  Fragments: {}", length);
            eprintln!("  Max depth: {} levels", max_depth);
            eprintln!("  Tape: {} cells", config.tape);
        }
        generate_idiomatic_program(&mut rng, &config)
    } else {
        let config = RandomProgramConfig {
            max_depth,
            avg_commands: length,
            loop_probability,
        };
        if verbose {
            eprintln!("Generating random BrainFuck program...");
            eprintln!("  Length: {} commands (average)", length);
            eprintln!("  Max depth: {} levels", max_depth);
            eprintln!("  Loop probability: {:.1}%", loop_probability * 100.0);
        }
        generate_random_program(&mut rng, &config)
    };

    // Output
    if let Some(output_path) = &output {
        // Write to file
        fs::write(output_path, &program).map_err(|source_err| BfError::FileError {
            path: output_path.clone(),
            source: source_err,
            hint: format!(
                "Make sure you have write permission for: {}",
                output_path.display()
            ),
        })?;

        if verbose {
            eprintln!("Generated {} bytes", program.len());
            eprintln!("Output written to: {}", output_path.display());
        }
    } else {
        // Write to stdout
        println!("{}", program);

        if verbose {
            eprintln!("Generated {} bytes", program.len());
        }
    }

    Ok(())
}

fn run_compile(text: String, output: Option<PathBuf>, verbose: bool) -> Result<(), BfError> {
    if verbose {
        eprintln!("Compiling string to BrainFuck...");
        eprintln!("  Input: {} characters", text.len());
    }

    // Compile to BF
    let program = compile_string(&text);

    // Output
    if let Some(output_path) = &output {
        // Write to file
        fs::write(output_path, &program).map_err(|source_err| BfError::FileError {
            path: output_path.clone(),
            source: source_err,
            hint: format!(
                "Make sure you have write permission for: {}",
                output_path.display()
            ),
        })?;

        if verbose {
            eprintln!("Generated {} bytes of BrainFuck code", program.len());
            eprintln!("Output written to: {}", output_path.display());
        }
    } else {
        // Write to stdout
        println!("{}", program);

        if verbose {
            eprintln!("Generated {} bytes of BrainFuck code", program.len());
        }
    }

    Ok(())
}

fn display_debug_table(source: &str, debug_info: &DebugInfo, show_source: bool) {
    println!("=== Debug Symbol Table ===\n");

    if show_source {
        println!("Source code ({} bytes):", source.len());
        println!();

        // Use syntax highlighter to display source
        let highlighter = SyntaxHighlighter::new().show_line_numbers(true);
        let highlighted = highlighter.highlight(source);
        print!("{}", highlighted.to_ansi());
        println!();
    }

    println!("Symbol table ({} entries):", debug_info.len());
    println!(
        "{:<8} {:<13} {:<8} {:<8} {:<10}",
        "Step", "Instruction", "Line", "Column", "Offset"
    );
    println!("{}", "-".repeat(60));

    // Display all entries in order
    for idx in 0..debug_info.len() {
        if let Some(loc) = debug_info.lookup(idx) {
            // Get the character at this location
            let char_at_loc = get_char_at_location(source, &loc);
            let char_display = match char_at_loc {
                '\n' => "\\n".to_string(),
                '\r' => "\\r".to_string(),
                '\t' => "\\t".to_string(),
                c if c.is_control() => format!("\\x{:02x}", c as u8),
                c => c.to_string(),
            };

            println!(
                "{:<8} {:<13} {:<8} {:<8} {:<10}",
                idx,
                format!("'{}'", char_display),
                loc.line,
                loc.column,
                loc.offset
            );
        }
    }

    println!("\n=== Summary ===");
    println!("Total instructions: {}", debug_info.len());
}

fn display_debug_json(source: &str, debug_info: &DebugInfo) {
    use serde_json::json;

    let mut symbols = Vec::new();
    for idx in 0..debug_info.len() {
        if let Some(loc) = debug_info.lookup(idx) {
            let char_at_loc = get_char_at_location(source, &loc);
            symbols.push(json!({
                "step": idx,
                "instruction": char_at_loc,
                "line": loc.line,
                "column": loc.column,
                "offset": loc.offset,
            }));
        }
    }

    let output = json!({
        "source_bytes": source.len(),
        "total_instructions": debug_info.len(),
        "symbols": symbols,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn display_debug_csv(debug_info: &DebugInfo) {
    println!("step,instruction,line,column,offset");
    for idx in 0..debug_info.len() {
        if let Some(loc) = debug_info.lookup(idx) {
            println!("{},?,{},{},{}", idx, loc.line, loc.column, loc.offset);
        }
    }
}

/// Get the character at a specific source location
fn get_char_at_location(source: &str, loc: &SourceLocation) -> char {
    source.chars().nth(loc.offset).unwrap_or('?')
}

fn run_optimize(
    file: PathBuf,
    theme: String,
    plain: bool,
    cell_model: &str,
) -> Result<(), BfError> {
    // The mapping shown has to be the one the interpreter will actually run,
    // and that depends on the cell model: checked cells decline the
    // multiply-loop fold, which changes both the instructions and the ratio.
    let cell_model = cell_model.parse::<CellModel>().unwrap_or_else(|()| {
        eprintln!("Error: Invalid cell model '{cell_model}'. Valid options: wrapping, checked");
        std::process::exit(1);
    });

    // Read source file
    let source = read_source(&file)?;

    // Parse the program
    let instructions = parse(&source)?;

    // Minify first to remove comments and get clean BF code
    let minified_source = minify(&instructions);

    // Parse the minified code WITH debug info to get instruction->character mapping
    let (minified_instructions, debug_info) = parse_with_debug(&minified_source)?;

    // Optimize the minified instructions
    let optimized = optimize_with_cell_model(&minified_instructions, cell_model);

    // Select theme
    let color_theme = match theme.to_lowercase().as_str() {
        "dark" => ColorTheme::dark(),
        "light" => ColorTheme::light(),
        other => {
            eprintln!(
                "Error: Invalid theme '{}'. Valid options: dark, light",
                other
            );
            std::process::exit(1);
        }
    };

    // Display optimization mapping
    println!("=== Optimization Mapping ===\n");
    println!("Original: {} instructions", optimized.original_count);
    println!("Optimized: {} instructions", optimized.optimized_count);
    println!(
        "Compression: {:.2}× ({:.1}% reduction)\n",
        optimized.compression_ratio(),
        (1.0 - 1.0 / optimized.compression_ratio()) * 100.0
    );

    display_optimization_mapping(
        &minified_source,
        &optimized.instructions,
        &debug_info,
        &color_theme,
        plain,
    );

    Ok(())
}

fn display_optimization_mapping(
    source: &str,
    optimized: &[gyrus::optimizer::OptimizedInstruction],
    debug_info: &DebugInfo,
    theme: &ColorTheme,
    plain: bool,
) {
    // Build the grouped output
    let mut output = String::new();

    for (i, inst) in optimized.iter().enumerate() {
        let range = inst.source_range();
        let instr_start = range.start; // Instruction index (not character offset)
        let instr_end = range.end; // Instruction index (not character offset)

        // Convert instruction indices to character offsets using debug info
        if let Some(start_loc) = debug_info.lookup(instr_start) {
            let char_start = start_loc.offset;

            // Find the end character offset
            let char_end = if instr_end < debug_info.len() {
                // End is the start of the next instruction
                if let Some(end_loc) = debug_info.lookup(instr_end) {
                    end_loc.offset
                } else {
                    source.len()
                }
            } else {
                // Last instruction - go to end of source
                source.len()
            };

            // `SourceLocation::offset` counts characters, not bytes -- the
            // parser advances it once per `char`. Slicing the byte array with
            // those indices silently produces the wrong text the moment the
            // source contains a multi-byte character, which a comment easily
            // can, and `from_utf8_lossy` then hides it behind replacement
            // characters instead of failing. Take characters.
            let substring: String = if char_start <= char_end {
                source
                    .chars()
                    .skip(char_start)
                    .take(char_end - char_start)
                    .collect()
            } else {
                String::new()
            };

            output.push_str(&substring);
        }

        // Add 3 spaces after each group (except the last)
        if i < optimized.len() - 1 {
            output.push_str("   ");
        }
    }

    // Display with syntax highlighting
    if plain {
        println!("{}", output);
    } else {
        let highlighter = SyntaxHighlighter::with_theme(theme.clone());
        let highlighted = highlighter.highlight(&output);
        print!("{}", highlighted.to_ansi());
        println!();
    }
}
