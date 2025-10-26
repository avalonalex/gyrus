use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

use ferrous_cortex::{
    BfError, CellModel, DebugInfo, SourceLocation, U8WrappingCells, minify, parse_with_debug,
    syntax::{ColorTheme, SyntaxHighlighter},
    validate,
};

#[derive(Parser)]
#[command(name = "ferrous-cortex-tool")]
#[command(about = "Development and analysis tools for BrainFuck programs", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Minify BF program (strip comments and whitespace)
    Minify {
        /// BrainFuck source file to minify
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show compression statistics
        #[arg(short, long)]
        verbose: bool,
    },

    /// Validate program and show warnings
    Validate {
        /// BrainFuck source file to validate
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Cell model to assume for validation
        #[arg(long, default_value = "wrapping")]
        cell_model: String,

        /// Exit with error if warnings found (for CI/CD)
        #[arg(long)]
        strict: bool,

        /// Show additional validation context
        #[arg(short, long)]
        verbose: bool,
    },

    /// Inspect debug symbols and source locations
    DebugInfo {
        /// BrainFuck source file to inspect
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output format: table, json, csv
        #[arg(long, default_value = "table")]
        format: String,

        /// Include source code context
        #[arg(long)]
        show_source: bool,
    },

    /// Display BF program with syntax highlighting
    View {
        /// BrainFuck source file to view
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Show line numbers
        #[arg(short = 'n', long)]
        line_numbers: bool,

        /// Theme: dark, light
        #[arg(long, default_value = "dark")]
        theme: String,

        /// Plain output (no colors)
        #[arg(long)]
        plain: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), BfError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Minify {
            file,
            output,
            verbose,
        } => run_minify(file, output, verbose),
        Commands::Validate {
            file,
            cell_model,
            strict,
            verbose,
        } => run_validate(file, cell_model, strict, verbose),
        Commands::DebugInfo {
            file,
            format,
            show_source,
        } => run_debug_info(file, format, show_source),
        Commands::View {
            file,
            line_numbers,
            theme,
            plain,
        } => run_view(file, line_numbers, theme, plain),
    }
}

fn run_minify(file: PathBuf, output: Option<PathBuf>, verbose: bool) -> Result<(), BfError> {
    // Read source file
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

    // Parse the program
    let (instructions, _debug_info) = parse_with_debug(&source)?;

    // Minify
    let minified = minify(&instructions);

    // Output
    if let Some(output_path) = &output {
        // Write to file
        fs::write(output_path, &minified).map_err(|source_err| BfError::FileError {
            path: output_path.clone(),
            source: source_err,
            hint: format!(
                "Make sure you have write permission for: {}",
                output_path.display()
            ),
        })?;

        if verbose {
            let reduction = 100.0 * (1.0 - (minified.len() as f64 / source.len() as f64));
            eprintln!(
                "Minified {} bytes to {} bytes ({:.1}% reduction)",
                source.len(),
                minified.len(),
                reduction
            );
            eprintln!("Output written to: {}", output_path.display());
        }
    } else {
        // Write to stdout
        println!("{}", minified);

        if verbose {
            let reduction = 100.0 * (1.0 - (minified.len() as f64 / source.len() as f64));
            eprintln!(
                "Minified {} bytes to {} bytes ({:.1}% reduction)",
                source.len(),
                minified.len(),
                reduction
            );
        }
    }

    Ok(())
}

fn run_validate(
    file: PathBuf,
    cell_model: String,
    strict: bool,
    _verbose: bool,
) -> Result<(), BfError> {
    // Read source file
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

    // Parse the program
    let (instructions, _debug_info) = match parse_with_debug(&source) {
        Ok(result) => result,
        Err(BfError::MultipleBracketErrors { .. }) => {
            // Errors already reported to stderr
            std::process::exit(1);
        }
        Err(e) => return Err(e),
    };

    // Parse cell model (for future use when validation becomes model-aware)
    let _cell_model_parsed = match cell_model.to_lowercase().as_str() {
        "wrapping" | "wrap" | "u8-wrapping" => CellModel::U8Wrapping(U8WrappingCells),
        "checked" | "check" | "u8-checked" => {
            eprintln!(
                "Note: Cell model '{}' specified, but validation currently assumes u8 wrapping.",
                cell_model
            );
            eprintln!("      Model-aware validation is planned for a future release.");
            CellModel::U8Wrapping(U8WrappingCells)
        }
        other => {
            eprintln!(
                "Error: Invalid cell model '{}'. Valid options: wrapping, checked",
                other
            );
            std::process::exit(1);
        }
    };

    // Validate (currently always assumes u8 wrapping)
    let warnings = validate(&instructions);

    if warnings.is_empty() {
        println!("✓ Validation complete: No warnings found");
        Ok(())
    } else {
        eprintln!("Validation found {} warning(s):\n", warnings.len());
        for warning in &warnings {
            eprintln!("{}\n", warning);
        }

        eprintln!("Validation complete: {} warnings", warnings.len());

        if strict {
            std::process::exit(1);
        } else {
            Ok(())
        }
    }
}

fn run_debug_info(file: PathBuf, format: String, show_source: bool) -> Result<(), BfError> {
    // Read source file
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

    // Parse the program with debug symbols
    let (_instructions, debug_info) = parse_with_debug(&source)?;

    // Output in requested format
    match format.to_lowercase().as_str() {
        "table" => display_debug_table(&source, &debug_info, show_source),
        "json" => display_debug_json(&source, &debug_info),
        "csv" => display_debug_csv(&debug_info),
        other => {
            eprintln!(
                "Error: Invalid format '{}'. Valid options: table, json, csv",
                other
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

fn run_view(
    file: PathBuf,
    line_numbers: bool,
    theme: String,
    plain: bool,
) -> Result<(), BfError> {
    // Read source file
    let source = fs::read_to_string(&file).map_err(|source_err| BfError::FileError {
        path: file.clone(),
        source: source_err,
        hint: format!(
            "Make sure the file exists and you have permission to read it. Current path: {}",
            file.display()
        ),
    })?;

    // Select theme
    let color_theme = match theme.to_lowercase().as_str() {
        "dark" => ColorTheme::dark(),
        "light" => ColorTheme::light(),
        other => {
            eprintln!(
                "Error: Invalid theme '{}'. Valid options: dark, light",
                other
            );
            std::process::exit(1);
        }
    };

    // Create highlighter
    let highlighter = SyntaxHighlighter::with_theme(color_theme).show_line_numbers(line_numbers);

    // Highlight the code
    let highlighted = highlighter.highlight(&source);

    // Output
    if plain {
        print!("{}", highlighted.to_plain());
    } else {
        print!("{}", highlighted.to_ansi());
    }

    Ok(())
}

fn display_debug_table(source: &str, debug_info: &DebugInfo, show_source: bool) {
    println!("=== Debug Symbol Table ===\n");

    if show_source {
        println!("Source code ({} bytes):", source.len());
        println!("{:?}\n", source);
    }

    println!("Symbol table ({} entries):", debug_info.len());
    println!(
        "{:<8} {:<13} {:<8} {:<8} {:<10}",
        "Step", "Instruction", "Line", "Column", "Offset"
    );
    println!("{}", "-".repeat(60));

    // Display all entries in order
    for idx in 0..debug_info.len() {
        if let Some(loc) = debug_info.lookup(idx) {
            // Get the character at this location
            let char_at_loc = get_char_at_location(source, &loc);
            let char_display = match char_at_loc {
                '\n' => "\\n".to_string(),
                '\r' => "\\r".to_string(),
                '\t' => "\\t".to_string(),
                c if c.is_control() => format!("\\x{:02x}", c as u8),
                c => c.to_string(),
            };

            println!(
                "{:<8} {:<13} {:<8} {:<8} {:<10}",
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
}

fn display_debug_json(source: &str, debug_info: &DebugInfo) {
    use serde_json::json;

    let mut symbols = Vec::new();
    for idx in 0..debug_info.len() {
        if let Some(loc) = debug_info.lookup(idx) {
            let char_at_loc = get_char_at_location(source, &loc);
            symbols.push(json!({
                "step": idx,
                "instruction": char_at_loc,
                "line": loc.line,
                "column": loc.column,
                "offset": loc.offset,
            }));
        }
    }

    let output = json!({
        "source_bytes": source.len(),
        "total_instructions": debug_info.len(),
        "symbols": symbols,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn display_debug_csv(debug_info: &DebugInfo) {
    println!("step,instruction,line,column,offset");
    for idx in 0..debug_info.len() {
        if let Some(loc) = debug_info.lookup(idx) {
            println!("{},{},{},{},{}", idx, "?", loc.line, loc.column, loc.offset);
        }
    }
}

/// Get the character at a specific source location
fn get_char_at_location(source: &str, loc: &SourceLocation) -> char {
    source.chars().nth(loc.offset).unwrap_or('?')
}
