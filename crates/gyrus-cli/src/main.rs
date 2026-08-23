use clap::Parser;
use std::fs;
use std::path::PathBuf;

use gyrus::hooks::builtin::{ProfilingHook, SharedProfilingHook};
use gyrus::{
    BfError, CellModel, EofBehavior, ExecutionConfigBuilder, U8CheckedCells, U8WrappingCells,
    interpret_optimized_with_io, interpret_with_io,
    io::{StdInput, StdOutput},
    optimize, parse, parse_with_debug,
};
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "gyrus")]
#[command(about = "A BrainFuck interpreter", long_about = None)]
#[command(after_help = "For development tools (minify, validate, debug-info), use 'gyrus-tool'")]
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

    /// Enable debug mode: use standard interpreter with source location tracking
    /// (slower but shows line/column in errors, required for debugging)
    #[arg(long)]
    debug: bool,

    /// Enable trace mode: profile execution and show heatmap at end
    /// (implies --debug, shows hot code regions and loop performance)
    #[arg(long)]
    trace: bool,
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

    // Determine execution mode
    // Default: Optimized interpreter (fast, no tracking)
    // --debug: Standard interpreter with debug symbols
    // --trace: Standard interpreter with debug symbols + profiling
    let use_optimized = !cli.debug && !cli.trace;
    let enable_profiling = cli.trace;

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
    // - Optimized mode: parse without debug symbols (fast)
    // - Debug/trace mode: parse with debug symbols for source tracking
    let (instructions, debug_info) = if cli.debug || cli.trace {
        // Debug mode: parse with debug symbols for source location tracking
        match parse_with_debug(&source) {
            Ok((instructions, debug_info)) => (instructions, Some(debug_info)),
            Err(BfError::MultipleBracketErrors { .. }) => {
                // Errors already reported to stderr, just exit with error code
                std::process::exit(1);
            }
            Err(e) => return Err(e),
        }
    } else {
        // Fast mode (default): parse without debug symbols
        match parse(&source) {
            Ok(instructions) => (instructions, None),
            Err(BfError::MultipleBracketErrors { .. }) => {
                // Errors already reported to stderr, just exit with error code
                std::process::exit(1);
            }
            Err(e) => return Err(e),
        }
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
    let mut config = builder.build();

    // Create profiler hook if --trace flag is set
    let profiler_handle = if enable_profiling {
        let profiler = Arc::new(Mutex::new(ProfilingHook::new()));
        let profiler_clone = Arc::clone(&profiler);
        config.register_hook(Box::new(SharedProfilingHook::new_with_shared(
            profiler_clone,
        )));
        Some(profiler)
    } else {
        None
    };

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
        eprintln!(
            "  Execution mode: {}",
            if use_optimized {
                "Optimized (default)"
            } else if enable_profiling {
                "Trace (profiling + debug)"
            } else {
                "Debug (standard + symbols)"
            }
        );
        eprintln!("  Memory model: {}", config.memory_model());
        eprintln!("  Cell model: {}", config.cell_model());
        eprintln!(
            "  Max steps: {}",
            config
                .max_steps()
                .map_or("unlimited".to_string(), |s| s.to_string())
        );
        eprintln!(
            "  Timeout: {}",
            config
                .timeout_ms()
                .map_or("unlimited".to_string(), |t| format!("{t}ms"))
        );
        eprintln!();
    }

    // Execute the program
    let mut input = StdInput;
    let mut output = StdOutput;
    let stats = if use_optimized {
        // OPTIMIZED MODE (default): Fast execution, no tracking
        let optimized = optimize(&instructions);

        if cli.verbose && !cli.quiet {
            eprintln!("=== Optimization Results ===");
            eprintln!("Original instructions: {}", optimized.original_count);
            eprintln!("Optimized instructions: {}", optimized.optimized_count);
            eprintln!("Compression ratio: {:.2}×", optimized.compression_ratio());
            eprintln!();
        }

        match interpret_optimized_with_io(&optimized.instructions, config, &mut input, &mut output)
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: {}", e.format_detailed());
                eprintln!(
                    "\nHint: Use --debug for source location tracking and better error messages"
                );
                std::process::exit(1);
            }
        }
    } else {
        // DEBUG/TRACE MODE: Standard interpreter with debug symbols
        match interpret_with_io(
            &instructions,
            config,
            &mut input,
            &mut output,
            debug_info.as_ref(),
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}", e.format_with_source(&source));
                std::process::exit(1);
            }
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

        // If debug mode is enabled (not trace-only), show where execution completed
        if !enable_profiling
            && cli.debug
            && let Some(ref debug_info) = debug_info
        {
            let total_instructions = debug_info.len();
            if total_instructions > 0 {
                eprintln!("\n=== Debug Information ===");
                eprintln!("Total instructions: {}", total_instructions);

                let last_instruction_index = total_instructions - 1;
                if let Some(location) = debug_info.lookup(last_instruction_index) {
                    eprintln!(
                        "Program completed at: line {}, column {} (offset {})",
                        location.line, location.column, location.offset
                    );
                    eprintln!("✓ Debug tracking verified through program completion");
                } else {
                    eprintln!("Warning: Could not determine final source location");
                }
            }
        }
    }

    // Display profiling results if --trace flag was used (unless --quiet)
    if !cli.quiet
        && let Some(profiler) = &profiler_handle
    {
        let profiler = profiler.lock().unwrap();
        eprintln!("\n{}", "=".repeat(80));
        eprintln!(
            "{}",
            profiler.format_source_heatmap(&source, debug_info.as_ref())
        );
        eprintln!("{}", "=".repeat(80));
        eprintln!();
        eprintln!("{}", profiler.format_ascii_tree());
        eprintln!("{}", "=".repeat(80));
    }

    Ok(())
}
