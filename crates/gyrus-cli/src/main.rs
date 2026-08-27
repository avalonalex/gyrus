use clap::Parser;
use std::fs;
use std::path::PathBuf;

use gyrus::hooks::builtin::{ProfilingHook, SharedProfilingHook};
use gyrus::{
    BfError, CellModel, EofBehavior, ExecutionConfigBuilder, interpret_optimized_with_io,
    interpret_with_io,
    io::{StdInput, StdOutput},
    optimize_with_cell_model, parse, parse_with_debug,
};
use gyrus::{DebugInfo, Instruction};
use gyrus_macro::ProgramError;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "gyrus")]
#[command(about = "A BrainFuck interpreter", long_about = None)]
#[command(after_help = "For development tools (minify, validate, debug-info), use 'gyrus-tool'")]
struct Cli {
    /// BrainFuck source file to execute
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Maximum number of execution steps (0 = unlimited). Under --jit a step
    /// is a loop iteration, so the same number is a looser bound there.
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

    /// Compile to native code with Cranelift and run that. Same output, same
    /// errors, with source locations and no --debug slowdown. Pays tens of
    /// milliseconds to compile, so it wins on programs that run longer than
    /// that (mandelbrot: 3x) and loses on ones that finish sooner.
    #[cfg(feature = "jit")]
    #[arg(long, conflicts_with_all = ["debug", "trace"])]
    jit: bool,

    /// Treat the file as macro source whatever it is called. Expansion is
    /// chosen by the `.bfm` extension otherwise, which a temporary file or a
    /// process substitution has no way to carry.
    #[arg(long)]
    r#macro: bool,
}

/// Which engine runs the program.
///
/// Resolved once, from the flags and the file's extension, because the default
/// depends on both. A `.bfm` runs on an engine that can name a source
/// position: it exists so that errors point back at macro source, and the
/// optimized interpreter is the one engine that cannot do that. There is
/// deliberately no flag to ask for it -- `--jit` is faster anyway and keeps
/// the locations.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Optimized,
    Debug,
    Trace,
    Jit,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Optimized => "Optimized",
            Mode::Debug => "Debug (standard + symbols)",
            Mode::Trace => "Trace (profiling + debug)",
            Mode::Jit => "JIT (Cranelift)",
        }
    }

    /// Whether to parse with debug symbols. Everything but the optimized
    /// interpreter uses them: the JIT maps every failure site to its
    /// instruction, so they buy located errors at no run-time cost.
    fn wants_symbols(self) -> bool {
        self != Mode::Optimized
    }
}

/// A program ready to run.
struct Program {
    instructions: Vec<Instruction>,
    debug_info: Option<DebugInfo>,
    /// The text a located error should be rendered against. For a `.bfm` that
    /// is the macro source, not the expansion nobody wrote.
    source: String,
}

/// Read a file, or say why not.
fn read_source(path: &std::path::Path) -> Result<String, BfError> {
    fs::read_to_string(path).map_err(|source| BfError::FileError {
        path: path.to_path_buf(),
        source,
        hint: format!(
            "Make sure the file exists and you have permission to read it. \
             Current path: {}",
            path.display()
        ),
    })
}

/// A BrainFuck file, with debug symbols if the engine will use them.
fn load_bf(path: &std::path::Path, symbols: bool) -> Result<Program, ProgramError> {
    let source = read_source(path)?;
    // MultipleBracketErrors carries every error and formats them all, so it
    // needs no special case here.
    let (instructions, debug_info) = match symbols {
        true => parse_with_debug(&source).map(|(i, d)| (i, Some(d)))?,
        false => (parse(&source)?, None),
    };
    Ok(Program {
        instructions,
        debug_info,
        source,
    })
}

/// A macro file: expanded, then parsed with its symbols rewritten to name the
/// `.bfm` rather than the expansion nobody wrote.
///
/// Symbols unconditionally, because the mode resolution guarantees a macro
/// program never reaches an engine that discards them -- expressed there as a
/// match arm rather than here as an assertion about another function.
fn load_macro(path: &std::path::Path) -> Result<Program, ProgramError> {
    let source = read_source(path)?;
    let expansion = gyrus_macro::expand(&source).map_err(|error| ProgramError::Macro {
        error,
        source: source.clone(),
    })?;
    let (instructions, expanded) = parse_with_debug(expansion.brainfuck())?;
    Ok(Program {
        instructions,
        debug_info: Some(expansion.remap(&expanded)),
        source,
    })
}

fn main() {
    if let Err(failure) = run() {
        eprintln!("{}", failure.report());
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

fn run() -> Result<(), ProgramError> {
    let cli = Cli::parse();

    #[cfg(feature = "jit")]
    let asked_for_jit = cli.jit;
    #[cfg(not(feature = "jit"))]
    let asked_for_jit = false;

    let requested = if cli.trace {
        Mode::Trace
    } else if asked_for_jit {
        Mode::Jit
    } else if cli.debug {
        Mode::Debug
    } else {
        Mode::Optimized
    };
    // What was asked for, then what the format requires. A macro program only
    // ever runs on an engine that can name a source position, and stating that
    // as one arm is what keeps it true: as an ordering of `else if`s it held
    // only because two other branches happened to be tested first.
    let expanding = cli.r#macro || gyrus_macro::is_macro_path(&cli.file);
    let mode = match (expanding, requested) {
        (true, engine) if !engine.wants_symbols() => Mode::Debug,
        (_, engine) => engine,
    };

    let Program {
        instructions,
        debug_info,
        source,
    } = match expanding {
        true => load_macro(&cli.file)?,
        false => load_bf(&cli.file, mode.wants_symbols())?,
    };

    // Parse cell model
    let cell_model = parse_or_exit(
        &cli.cell_model,
        |s| s.parse::<CellModel>().ok(),
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
    // The spellings live with the type (`impl FromStr for EofBehavior`), as
    // the cell model's do, so the CLI, the test manifest and any other reader
    // accept the same ones.
    let eof_behavior = parse_or_exit(
        &cli.eof_behavior,
        |s| s.parse::<EofBehavior>().ok(),
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
    let profiler_handle = if mode == Mode::Trace {
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
        eprintln!("  Execution mode: {}", mode.name());
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
    // The optimized interpreter and the JIT run the same optimized program;
    // it is built once, and reported once, for both.
    let optimized = if matches!(mode, Mode::Optimized | Mode::Jit) {
        let optimized = optimize_with_cell_model(&instructions, *config.cell_model());
        if cli.verbose && !cli.quiet {
            eprintln!("=== Optimization Results ===");
            eprintln!("Original instructions: {}", optimized.original_count);
            eprintln!("Optimized instructions: {}", optimized.optimized_count);
            eprintln!("Compression ratio: {:.2}×", optimized.compression_ratio());
            eprintln!();
        }
        Some(optimized)
    } else {
        None
    };

    let stats = match mode {
        Mode::Optimized => {
            // OPTIMIZED MODE (default): Fast execution, no tracking
            let optimized = optimized.as_ref().expect("built above");
            match interpret_optimized_with_io(optimized, config, &mut input, &mut output) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error: {}", e.format_detailed());
                    eprintln!(
                        "\nHint: Use --jit or --debug for source location tracking and better error messages"
                    );
                    std::process::exit(1);
                }
            }
        }
        Mode::Jit => {
            #[cfg(feature = "jit")]
            {
                let optimized = optimized.as_ref().expect("built above");
                // Counting costs; --verbose is the only thing that reads the counts.
                let statistics = if cli.verbose {
                    gyrus_jit::Statistics::Full
                } else {
                    gyrus_jit::Statistics::Cheap
                };
                match gyrus_jit::run_with(
                    optimized,
                    &config,
                    &mut input,
                    &mut output,
                    debug_info.as_ref(),
                    statistics,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("{}", e.format_with_source(&source));
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(feature = "jit"))]
            unreachable!("--jit is not a flag without the jit feature")
        }
        // The tree-walker, with or without the profiler attached to it.
        Mode::Debug | Mode::Trace => match interpret_with_io(
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
        },
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
        if mode == Mode::Jit {
            // The JIT counts loop iterations, nothing finer; see docs/execution-models.md.
            eprintln!(
                "Total steps executed: {} (loop iterations)",
                stats.total_steps
            );
        } else {
            eprintln!("Total steps executed: {}", stats.total_steps);
        }
        eprintln!("Loop iterations: {}", stats.loop_iterations);
        eprintln!("Peak memory used: {} cells", stats.peak_memory_used);
        eprintln!("Memory allocated: {} bytes", stats.memory_allocated);
        eprintln!("Cells modified: {}", stats.cells_modified);
        eprintln!("Bytes read: {}", stats.bytes_read);
        eprintln!("Bytes written: {}", stats.bytes_written);

        // Where execution completed, when the mode kept enough to say. Asked
        // of the mode rather than of `--debug`: a `.bfm` runs in Debug mode
        // without the flag, and this was the one reader still consulting the
        // flag, so it silently printed nothing for every macro program.
        if mode == Mode::Debug
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
