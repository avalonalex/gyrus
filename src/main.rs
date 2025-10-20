use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod bf;

use bf::{BfError, ExecutionConfig, interpret_with_config, minify, parse, validate};

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

    /// Memory size in bytes
    #[arg(long, default_value = "30000")]
    memory_size: usize,

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
    let mut config = ExecutionConfig::default().with_memory_size(cli.memory_size);

    if cli.max_steps > 0 {
        config = config.with_max_steps(cli.max_steps);
    }

    if cli.timeout > 0 {
        config = config.with_timeout_ms(cli.timeout);
    }

    if cli.verbose {
        eprintln!("Configuration:");
        eprintln!("  Memory size: {} bytes", config.memory_size);
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
