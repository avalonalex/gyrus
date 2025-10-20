use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod bf;

use bf::{parse, interpret, BfError};

#[derive(Parser)]
#[command(name = "ferrous-cortex")]
#[command(about = "A BrainFuck interpreter and debugger", long_about = None)]
struct Cli {
    /// BrainFuck source file to execute
    #[arg(value_name = "FILE")]
    file: PathBuf,
}

fn main() -> Result<(), BfError> {
    let cli = Cli::parse();

    // Read the source file
    let source = fs::read_to_string(&cli.file)
        .map_err(|e| BfError::IoError(format!("Failed to read file: {}", e)))?;

    // Parse the program
    let instructions = parse(&source)?;

    // Execute the program
    interpret(&instructions)?;

    Ok(())
}
