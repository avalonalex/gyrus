use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod bf;

use bf::{BfError, ExecutionConfig, interpret_with_config, parse};

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
}

fn main() -> Result<(), BfError> {
    let cli = Cli::parse();

    // Read the source file
    let source = fs::read_to_string(&cli.file)
        .map_err(|e| BfError::FileError(format!("Failed to read file: {}", e)))?;

    // Parse the program
    let instructions = parse(&source)?;

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
