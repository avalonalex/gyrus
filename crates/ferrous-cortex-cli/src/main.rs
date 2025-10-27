use clap::Parser;
use std::fs;
use std::path::PathBuf;

use ferrous_cortex::{
    BfError, CellModel, EofBehavior, ExecutionConfigBuilder, U8CheckedCells, U8WrappingCells,
    interpret_with_io,
    io::{StdInput, StdOutput},
    parse_with_debug,
};

#[derive(Parser)]
#[command(name = "ferrous-cortex")]
#[command(about = "A BrainFuck interpreter", long_about = None)]
#[command(
    after_help = "For development tools (minify, validate, debug-info), use 'ferrous-cortex-tool'"
)]
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
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e.format_detailed());
        std::process::exit(1);
    }
}

/// Helper function to parse an enum-like CLI argument or exit with error
fn parse_or_exit<T, F>(value: &str, parser: F, param_name: &str, valid_options: &str) -> T
where
    F: FnOnce(&str) -> Option<T>,
{
    parser(value).unwrap_or_else(|| {
        eprintln!(
            "Error: Invalid {} '{}'. Valid options: {}",
            param_name, value, valid_options
        );
        std::process::exit(1);
    })
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

    // Parse cell model
    let cell_model = parse_or_exit(
        &cli.cell_model,
        |s| match s.to_lowercase().as_str() {
            "wrapping" | "wrap" | "u8-wrapping" => Some(CellModel::U8Wrapping(U8WrappingCells)),
            "checked" | "check" | "u8-checked" => Some(CellModel::U8Checked(U8CheckedCells)),
            _ => None,
        },
        "cell model",
        "wrapping, checked",
    );

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
    let eof_behavior = parse_or_exit(
        &cli.eof_behavior,
        |s| match s.to_lowercase().as_str() {
            "zero" => Some(EofBehavior::SetZero),
            "neg-one" | "negone" | "-1" | "255" => Some(EofBehavior::SetNegOne),
            "no-change" | "nochange" | "unchanged" => Some(EofBehavior::NoChange),
            "error" => Some(EofBehavior::Error),
            _ => None,
        },
        "EOF behavior",
        "zero, neg-one, no-change, error",
    );
    let builder = builder.with_eof_behavior(eof_behavior);

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
    let stats = match interpret_with_io(
        &instructions,
        config,
        &mut input,
        &mut output,
        Some(&debug_info),
    ) {
        Ok(s) => s,
        Err(e) => {
            // For runtime errors, use format_with_source to show source location
            eprintln!("{}", e.format_with_source(&source));
            std::process::exit(1);
        }
    };

    // Display runtime warnings only in verbose mode (cell wrapping is common in BF)
    if cli.verbose && !cli.quiet && !stats.warnings.is_empty() {
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
