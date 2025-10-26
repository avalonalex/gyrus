use clap::Parser;
use std::fs;
use std::path::PathBuf;

use ferrous_cortex::{
    BfError, CellModel, DebugInfo, EofBehavior, ExecutionConfigBuilder, U8CheckedCells,
    U8WrappingCells, interpret_with_io,
    io::{StdInput, StdOutput},
    minify, parse_with_debug, validate,
};

#[derive(Parser)]
#[command(name = "ferrous-cortex")]
#[command(about = "A BrainFuck interpreter and debugger", long_about = None)]
struct Cli {
    /// BrainFuck source file to execute
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Maximum number of execution steps (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_steps: u64,

    /// Execution timeout in milliseconds (0 = unlimited)
    #[arg(long, default_value = "0")]
    timeout: u64,

    /// Memory size in bytes (for fixed model)
    #[arg(long, default_value = "30000")]
    memory_size: usize,

    /// Memory model: fixed or unbounded
    #[arg(long, default_value = "fixed")]
    memory_model: String,

    /// Cell model: wrapping (default, production) or checked (debugging)
    #[arg(long, default_value = "wrapping")]
    cell_model: String,

    /// Initial memory size for unbounded model
    #[arg(long, default_value = "1000")]
    unbounded_initial: usize,

    /// Maximum memory size for unbounded model
    #[arg(long, default_value = "1000000")]
    unbounded_max: usize,

    /// Show detailed execution information and statistics
    #[arg(short, long)]
    verbose: bool,

    /// Suppress runtime warnings and non-program output (for permissive modes: wrapping cells, unbounded memory)
    #[arg(short, long)]
    quiet: bool,

    /// EOF behavior: zero, neg-one, no-change, or error
    #[arg(long, default_value = "zero")]
    eof_behavior: String,

    /// Validate program and show warnings (does not execute)
    #[arg(long)]
    validate: bool,

    /// Minify the program (strip all comments and output only BF commands)
    #[arg(long)]
    minify: bool,

    /// Output file for minified code (stdout if not specified)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Inspect debug symbols (shows symbol table mapping step indices to source locations)
    #[arg(long)]
    inspect_debug: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e.format_detailed());
        std::process::exit(1);
    }
}

fn run() -> Result<(), BfError> {
    let cli = Cli::parse();

    // Read the source file
    let source = fs::read_to_string(&cli.file).map_err(|source| BfError::FileError {
        path: cli.file.clone(),
        source,
        hint: format!(
            "Make sure the file exists and you have permission to read it. \
             Current path: {}",
            cli.file.display()
        ),
    })?;

    // Parse the program (with debug symbols)
    let (instructions, debug_info) = match parse_with_debug(&source) {
        Ok(result) => result,
        Err(BfError::MultipleBracketErrors { .. }) => {
            // Errors already reported to stderr, just exit with error code
            std::process::exit(1);
        }
        Err(e) => return Err(e),
    };

    // Inspect debug symbols if requested (and exit without executing)
    if cli.inspect_debug {
        display_debug_symbols(&source, &debug_info);
        return Ok(());
    }

    // Minify if requested (and exit without executing)
    if cli.minify {
        let minified = minify(&instructions);

        if let Some(output_path) = &cli.output {
            // Write to file
            fs::write(output_path, &minified).map_err(|source| BfError::FileError {
                path: output_path.clone(),
                source,
                hint: format!(
                    "Make sure you have write permission for: {}",
                    output_path.display()
                ),
            })?;
            if cli.verbose {
                eprintln!(
                    "Minified {} bytes to {} bytes (saved to {})",
                    source.len(),
                    minified.len(),
                    output_path.display()
                );
            }
        } else {
            // Write to stdout
            println!("{}", minified);
        }

        return Ok(());
    }

    // Parse cell model for validation
    let cell_model = match cli.cell_model.to_lowercase().as_str() {
        "wrapping" | "wrap" | "u8-wrapping" => CellModel::U8Wrapping(U8WrappingCells),
        "checked" | "check" | "u8-checked" => CellModel::U8Checked(U8CheckedCells),
        other => {
            eprintln!(
                "Error: Invalid cell model '{}'. Valid options: wrapping, checked",
                other
            );
            std::process::exit(1);
        }
    };

    // Validate if requested (always assumes u8 wrapping - production target)
    if cli.validate {
        let warnings = validate(&instructions);

        if !warnings.is_empty() {
            eprintln!("Validation found {} warning(s):\n", warnings.len());
            for warning in &warnings {
                eprintln!("{}\n", warning);
            }
        } else {
            // No warnings found
            eprintln!("Validation: No warnings found");
        }

        // --validate mode: always exit without executing
        return Ok(());
    }

    // Build execution config using enhanced builder
    let builder = ExecutionConfigBuilder::new();

    // Set memory model (required)
    let builder = match cli.memory_model.to_lowercase().as_str() {
        "fixed" => builder.with_memory_size(cli.memory_size),
        "unbounded" => {
            match builder.with_unbounded_memory(cli.unbounded_initial, cli.unbounded_max) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Configuration error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!(
                "Error: Invalid memory model '{}'. Valid options: fixed, unbounded",
                other
            );
            std::process::exit(1);
        }
    };

    // Set cell model
    let builder = builder.with_cell_model(cell_model);

    // Set EOF behavior
    let builder = match cli.eof_behavior.to_lowercase().as_str() {
        "zero" => builder.with_eof_behavior(EofBehavior::SetZero),
        "neg-one" | "negone" | "-1" | "255" => builder.with_eof_behavior(EofBehavior::SetNegOne),
        "no-change" | "nochange" | "unchanged" => builder.with_eof_behavior(EofBehavior::NoChange),
        "error" => builder.with_eof_behavior(EofBehavior::Error),
        other => {
            eprintln!(
                "Error: Invalid EOF behavior '{}'. Valid options: zero, neg-one, no-change, error",
                other
            );
            std::process::exit(1);
        }
    };

    // Set optional parameters
    let builder = if cli.max_steps > 0 {
        builder.with_max_steps(cli.max_steps)
    } else {
        builder
    };

    let builder = if cli.timeout > 0 {
        builder.with_timeout_ms(cli.timeout)
    } else {
        builder
    };

    // Build the final config
    let config = builder.build();

    // Warn if --quiet is used with checked mode (contradictory)
    if cli.quiet && matches!(cell_model, CellModel::U8Checked(_)) {
        eprintln!(
            "Warning: --quiet suppresses runtime warnings, but --cell-model checked produces errors (not warnings)."
        );
        eprintln!(
            "         Consider using --cell-model wrapping if you want warnings that can be suppressed."
        );
        eprintln!();
    }

    if cli.verbose && !cli.quiet {
        eprintln!("Configuration:");
        eprintln!("  Memory model: {}", config.memory_model());
        eprintln!("  Cell model: {}", config.cell_model());
        eprintln!(
            "  Max steps: {}",
            config
                .max_steps()
                .map_or("unlimited".to_string(), |s| s.to_string())
        );
        eprintln!(
            "  Timeout: {}ms",
            config
                .timeout_ms()
                .map_or("unlimited".to_string(), |t| t.to_string())
        );
        eprintln!();
    }

    // Execute the program (with debug symbols)
    let mut input = StdInput;
    let mut output = StdOutput;
    let stats = interpret_with_io(
        &instructions,
        config,
        &mut input,
        &mut output,
        Some(&debug_info),
    )?;

    // Display runtime warnings (unless --quiet)
    if !cli.quiet && !stats.warnings.is_empty() {
        eprintln!("\n=== Runtime Warnings ===");
        eprintln!("Detected {} runtime event(s):\n", stats.warnings.len());
        for warning in &stats.warnings {
            eprintln!("{}\n", warning.format_with_source(&source));
        }
    }

    // Display statistics in verbose mode (unless --quiet)
    if cli.verbose && !cli.quiet {
        eprintln!("=== Execution Statistics ===");
        eprintln!("Total steps executed: {}", stats.total_steps);
        eprintln!("Loop iterations: {}", stats.loop_iterations);
        eprintln!("Peak memory used: {} cells", stats.peak_memory_used);
        eprintln!("Memory allocated: {} bytes", stats.memory_allocated);
        eprintln!("Cells modified: {}", stats.cells_modified);
        eprintln!("Bytes read: {}", stats.bytes_read);
        eprintln!("Bytes written: {}", stats.bytes_written);
    }

    Ok(())
}

/// Display debug symbol table in a formatted way
fn display_debug_symbols(source: &str, debug_info: &DebugInfo) {
    println!("=== Debug Symbol Table ===\n");
    println!("Source code ({} bytes):", source.len());
    println!("{:?}\n", source);

    println!("Symbol table ({} entries):", debug_info.len());
    println!(
        "{:<12} {:<15} {:<8} {:<8} {:<10}",
        "Step Index", "Character", "Line", "Column", "Offset"
    );
    println!("{}", "=".repeat(65));

    // Display all entries in order
    for idx in 0..debug_info.len() {
        if let Some(loc) = debug_info.lookup(idx) {
            // Get the character at this location
            let char_at_loc = get_char_at_location(source, loc);
            let char_display = match char_at_loc {
                '\n' => "\\n".to_string(),
                '\r' => "\\r".to_string(),
                '\t' => "\\t".to_string(),
                c if c.is_control() => format!("\\x{:02x}", c as u8),
                c => c.to_string(),
            };

            println!(
                "{:<12} {:<15} {:<8} {:<8} {:<10}",
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
    println!("Source bytes: {}", source.len());
    println!(
        "Compression ratio: {:.1}%",
        (debug_info.len() as f64 / source.len() as f64) * 100.0
    );
}

/// Get the character at a given source location
fn get_char_at_location(source: &str, loc: ferrous_cortex::SourceLocation) -> char {
    source.chars().nth(loc.offset).unwrap_or('\0')
}
