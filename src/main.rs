use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod bf;

use bf::{BfError, ExecutionConfig, MemoryModel, interpret_with_config, minify, parse, validate};

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

    /// Initial memory size for unbounded model
    #[arg(long, default_value = "1000")]
    unbounded_initial: usize,

    /// Maximum memory size for unbounded model
    #[arg(long, default_value = "1000000")]
    unbounded_max: usize,

    /// Show detailed execution information
    #[arg(short, long)]
    verbose: bool,

    /// Validate program and show warnings
    #[arg(long)]
    validate: bool,

    /// Treat warnings as errors (implies --validate)
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
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), BfError> {
    let cli = Cli::parse();

    // Read the source file
    let source = fs::read_to_string(&cli.file)
        .map_err(|e| BfError::FileError(format!("Failed to read file: {}", e)))?;

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
            fs::write(output_path, &minified)
                .map_err(|e| BfError::FileError(format!("Failed to write output: {}", e)))?;
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

    // Validate if requested
    let should_validate = cli.validate || cli.strict;
    if should_validate {
        let warnings = validate(&instructions);

        if !warnings.is_empty() {
            eprintln!("Validation found {} warning(s):\n", warnings.len());
            for warning in &warnings {
                eprintln!("{}\n", warning);
            }

            if cli.strict {
                eprintln!("Error: Warnings treated as errors in strict mode");
                std::process::exit(1);
            }
        } else if cli.verbose {
            eprintln!("Validation: No warnings found\n");
        }
    }

    // Build execution config
    let memory_model = match cli.memory_model.to_lowercase().as_str() {
        "fixed" => MemoryModel::Fixed(cli.memory_size),
        "wrapping" => MemoryModel::Wrapping(cli.memory_size),
        "unbounded" => MemoryModel::Unbounded {
            initial_size: cli.unbounded_initial,
            max_size: cli.unbounded_max,
        },
        other => {
            eprintln!(
                "Error: Invalid memory model '{}'. Valid options: fixed, wrapping, unbounded",
                other
            );
            std::process::exit(1);
        }
    };

    let mut config = ExecutionConfig::default().with_memory_model(memory_model);

    if cli.max_steps > 0 {
        config = config.with_max_steps(cli.max_steps);
    }

    if cli.timeout > 0 {
        config = config.with_timeout_ms(cli.timeout);
    }

    if cli.verbose {
        eprintln!("Configuration:");
        match &config.memory_model {
            MemoryModel::Fixed(size) => {
                eprintln!("  Memory model: Fixed({} bytes)", size);
            }
            MemoryModel::Wrapping(size) => {
                eprintln!("  Memory model: Wrapping({} bytes)", size);
            }
            MemoryModel::Unbounded {
                initial_size,
                max_size,
            } => {
                eprintln!(
                    "  Memory model: Unbounded(initial: {}, max: {})",
                    initial_size, max_size
                );
            }
        }
        eprintln!(
            "  Max steps: {}",
            config
                .max_steps
                .map_or("unlimited".to_string(), |s| s.to_string())
        );
        eprintln!(
            "  Timeout: {}ms",
            config
                .timeout_ms
                .map_or("unlimited".to_string(), |t| t.to_string())
        );
        eprintln!();
    }

    // Execute the program
    interpret_with_config(&instructions, config)?;

    Ok(())
}
