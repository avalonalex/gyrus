use clap::Parser;
use std::fs;
use std::path::PathBuf;

use ferrous_cortex::{
    BfError, CellModel, EofBehavior, ExecutionConfigBuilder, U8CheckedCells, U8WrappingCells,
    interpret_with_config, minify, parse, validate_with_cell_model,
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

    /// Memory size in bytes (for fixed and wrapping models)
    #[arg(long, default_value = "30000")]
    memory_size: usize,

    /// Memory model: fixed, wrapping, or unbounded
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

    /// EOF behavior: zero, neg-one, no-change, or error
    #[arg(long, default_value = "zero")]
    eof_behavior: String,

    /// Validate program and show warnings (does not execute)
    #[arg(long)]
    validate: bool,

    /// Treat warnings as errors and fail (executes only if no warnings)
    #[arg(long)]
    strict: bool,

    /// Minify the program (strip all comments and output only BF commands)
    #[arg(long)]
    minify: bool,

    /// Output file for minified code (stdout if not specified)
    #[arg(short, long)]
    output: Option<PathBuf>,
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

    // Parse the program
    let instructions = match parse(&source) {
        Ok(instr) => instr,
        Err(BfError::MultipleBracketErrors { .. }) => {
            // Errors already reported to stderr, just exit with error code
            std::process::exit(1);
        }
        Err(e) => return Err(e),
    };

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

    // Validate if requested (cell-model-aware)
    if cli.validate || cli.strict {
        let warnings = validate_with_cell_model(&instructions, &cell_model);

        if !warnings.is_empty() {
            eprintln!("Validation found {} warning(s):\n", warnings.len());
            for warning in &warnings {
                eprintln!("{}\n", warning);
            }

            if cli.strict {
                eprintln!("Error: Warnings treated as errors in strict mode");
                std::process::exit(1);
            } else {
                // --validate mode: exit without executing
                return Ok(());
            }
        } else {
            // No warnings found
            if cli.verbose {
                eprintln!("Validation: No warnings found\n");
            }

            if cli.validate {
                // --validate mode: exit without executing even if clean
                eprintln!("Validation: No warnings found");
                return Ok(());
            }
            // --strict mode with no warnings: continue to execution
        }
    }

    // Build execution config using enhanced builder
    let builder = ExecutionConfigBuilder::new();

    // Set memory model (required)
    let builder = match cli.memory_model.to_lowercase().as_str() {
        "fixed" => builder.with_memory_size(cli.memory_size),
        "wrapping" => builder.with_wrapping_memory(cli.memory_size),
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
                "Error: Invalid memory model '{}'. Valid options: fixed, wrapping, unbounded",
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

    if cli.verbose {
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

    // Execute the program
    let stats = interpret_with_config(&instructions, config)?;

    // Display statistics in verbose mode
    if cli.verbose {
        eprintln!("\n=== Execution Statistics ===");
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
