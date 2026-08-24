use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

use gyrus::{
    BfError, CellModel, DebugInfo, SourceLocation, U8WrappingCells,
    codegen::compile_string,
    minify, optimize_with_cell_model, parse, parse_with_debug,
    random::{RandomProgramConfig, generate_random_program},
    syntax::{ColorTheme, SyntaxHighlighter},
    validate,
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
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), BfError> {
    let cli = Cli::parse();

    match cli.command {
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
            output,
            verbose,
        } => run_generate(length, max_depth, loop_probability, output, verbose),
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

fn run_minify(file: PathBuf, output: Option<PathBuf>, verbose: bool) -> Result<(), BfError> {
    // Read source file
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

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
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

    // Parse the program
    let (instructions, _debug_info) = parse_with_debug(&source)?;

    // Parse cell model (for future use when validation becomes model-aware)
    let _cell_model_parsed = match cell_model.parse::<CellModel>() {
        Ok(CellModel::U8Checked(_)) => {
            eprintln!(
                "Note: Cell model '{}' specified, but validation currently assumes u8 wrapping.",
                cell_model
            );
            eprintln!("      Model-aware validation is planned for a future release.");
            CellModel::U8Wrapping(U8WrappingCells)
        }
        Ok(model) => model,
        Err(()) => {
            eprintln!(
                "Error: Invalid cell model '{}'. Valid options: wrapping, checked",
                cell_model
            );
            std::process::exit(1);
        }
    };

    // Validate (currently always assumes u8 wrapping)
    let warnings = validate(&instructions);

    if warnings.is_empty() {
        println!("✓ Validation complete: No warnings found");
        Ok(())
    } else {
        eprintln!("Validation found {} warning(s):\n", warnings.len());
        for warning in &warnings {
            eprintln!("{}\n", warning);
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
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

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
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

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

    // Create configuration
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

    // Generate the program
    let mut rng = rand::rng();
    let program = generate_random_program(&mut rng, &config);

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
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

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

            // Extract the source substring using character offsets
            let source_bytes = source.as_bytes();
            let substring = if char_end <= source_bytes.len() && char_start <= char_end {
                String::from_utf8_lossy(&source_bytes[char_start..char_end]).to_string()
            } else {
                "".to_string()
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
