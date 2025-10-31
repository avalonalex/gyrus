use clap::Parser;
use std::fs;
use std::path::PathBuf;

use ferrous_cortex::hooks::builtin::{ProfilingHook, SharedProfilingHook};
use ferrous_cortex::{
    BfError, CellModel, EofBehavior, ExecutionConfigBuilder, U8CheckedCells, U8WrappingCells,
    interpret_with_io,
    io::{StdInput, StdOutput},
    parse, parse_with_debug,
};
use std::sync::{Arc, Mutex};

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

    /// Enable debug symbols for source location tracking (slower, shows line/column in errors)
    #[arg(long)]
    debug: bool,

    /// Enable profiling to identify hot code regions and loop performance (implies --debug)
    #[arg(long)]
    profile: bool,

    /// Generate flamegraph SVG output file (implies --profile)
    #[arg(long, value_name = "FILE")]
    flamegraph: Option<PathBuf>,

    /// Generate HTML heatmap output file (implies --profile)
    #[arg(long, value_name = "FILE")]
    profile_html: Option<PathBuf>,
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

    // Parse the program (with or without debug symbols based on --debug flag)
    // Note: --profile, --flamegraph, and --profile-html imply --debug for source location tracking
    let (instructions, debug_info) =
        if cli.debug || cli.profile || cli.flamegraph.is_some() || cli.profile_html.is_some() {
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

    // Create profiler hook if --profile flag is set or --flamegraph/--profile-html is requested
    let profiler_handle = if cli.profile || cli.flamegraph.is_some() || cli.profile_html.is_some() {
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

    // Execute the program (with or without debug symbols)
    let mut input = StdInput;
    let mut output = StdOutput;
    let stats = match interpret_with_io(
        &instructions,
        config,
        &mut input,
        &mut output,
        debug_info.as_ref(),
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

        // If debug mode is enabled, show where execution completed
        if cli.debug
            && let Some(ref debug_info) = debug_info
        {
            // Find the last instruction that was executed
            // Program completed, so we executed through all instructions
            let total_instructions = debug_info.len();
            if total_instructions > 0 {
                eprintln!("\n=== Debug Information ===");
                eprintln!("Total instructions: {}", total_instructions);

                // Look up the last instruction's location
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

    // Display profiling results if --profile flag was used (unless --quiet)
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

    // Generate flamegraph SVG if requested
    if let Some(flamegraph_path) = &cli.flamegraph
        && let Some(profiler) = &profiler_handle
    {
        let profiler = profiler.lock().unwrap();
        let folded_stacks = profiler.generate_flamegraph_data();

        // Generate SVG using inferno
        use inferno::flamegraph;
        let mut options = flamegraph::Options::default();
        options.title = "BrainFuck Profile".to_string();

        let mut svg_output = Vec::new();
        if let Err(e) = flamegraph::from_lines(&mut options, folded_stacks.lines(), &mut svg_output)
        {
            eprintln!("Error generating flamegraph: {}", e);
            std::process::exit(1);
        }

        // Write to file
        if let Err(e) = fs::write(flamegraph_path, svg_output) {
            eprintln!(
                "Error writing flamegraph to {}: {}",
                flamegraph_path.display(),
                e
            );
            std::process::exit(1);
        }

        if !cli.quiet {
            eprintln!("\n✅ Flamegraph saved to: {}", flamegraph_path.display());
        }
    }

    // Generate HTML heatmap if requested
    if let Some(html_path) = &cli.profile_html
        && let Some(profiler) = &profiler_handle
    {
        let profiler = profiler.lock().unwrap();
        let html = profiler.generate_html_heatmap(&source, debug_info.as_ref());

        // Write to file
        if let Err(e) = fs::write(html_path, html) {
            eprintln!(
                "Error writing HTML heatmap to {}: {}",
                html_path.display(),
                e
            );
            std::process::exit(1);
        }

        if !cli.quiet {
            eprintln!("✅ HTML heatmap saved to: {}", html_path.display());
        }
    }

    Ok(())
}
