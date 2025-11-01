#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rust-lang/rust/master/src/doc/rust-logo-128x128.png"
)]
#![doc(html_favicon_url = "https://www.rust-lang.org/favicon.ico")]
#![doc(html_root_url = "https://docs.rs/ferrous-cortex/latest/")]

//! # FerrousCortex
//!
//! A production-grade BrainFuck interpreter and debugger written in Rust.
//!
//! FerrousCortex provides a fast, safe, and extensible BrainFuck interpreter
//! with rich error messages, configurable memory models, comprehensive
//! validation, and source location tracking for superior debugging.
//!
//! ## Features
//!
//! - **Multiple memory models**: Fixed, wrapping, and unbounded memory
//! - **Multiple cell models**: Wrapping (standard) and checked (debugging)
//! - **Configurable EOF behavior**: Zero, -1, no-change, or error
//! - **Rich error messages**: Syntax highlighting, source context, memory dumps
//! - **Debug symbols**: Source location tracking for runtime errors and warnings
//! - **Validation**: Detect suspicious patterns and potential issues
//! - **Custom I/O**: Use string-based, file-based, or custom I/O implementations
//! - **Minification**: Remove comments and produce minimal valid BrainFuck
//! - **Type safety**: Newtype pattern prevents type confusion
//! - **Execution statistics**: Track steps, memory usage, and I/O operations
//!
//! ## Quick Start
//!
//! ```rust
//! use ferrous_cortex::{parse, interpret_with_config, ExecutionConfigBuilder};
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! // Parse a simple "Hello World" program
//! let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
//! let instructions = parse(source)?;
//!
//! // Configure and run the interpreter
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(30000)
//!     .build();
//!
//! let stats = interpret_with_config(&instructions, config, None)?;
//! println!("Executed {} steps", stats.total_steps);
//! # Ok(())
//! # }
//! ```
//!
//! ## Using Custom I/O
//!
//! For testing and library usage, use string-based I/O:
//!
//! ```rust
//! use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfigBuilder};
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let source = ",[.,]";  // Echo program
//! let instructions = parse(source)?;
//!
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(100)
//!     .build();
//!
//! let mut input = StringIo::new("Hello");
//! let mut output = StringIo::empty();
//! interpret_with_io(&instructions, config, &mut input, &mut output, None)?;
//! assert_eq!(output.output_string(), "Hello");
//! # Ok(())
//! # }
//! ```
//!
//! ## Memory Models
//!
//! Choose from two memory models based on your needs:
//!
//! ```rust
//! use ferrous_cortex::ExecutionConfigBuilder;
//!
//! // Fixed: Traditional bounds-checked array (default)
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(30000)
//!     .build();
//!
//! // Unbounded: Dynamic growth from initial size up to max limit
//! let config = ExecutionConfigBuilder::new()
//!     .with_unbounded_memory(1000, 100000)
//!     .unwrap()
//!     .build();
//! ```
//!
//! ## Cell Models
//!
//! Control how cell arithmetic behaves:
//!
//! ```rust
//! use ferrous_cortex::ExecutionConfigBuilder;
//!
//! // Wrapping: Standard BrainFuck (255 + 1 = 0)
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(30000)
//!     .with_wrapping_cells()
//!     .build();
//!
//! // Checked: Strict debugging mode (255 + 1 = error)
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(30000)
//!     .with_checked_cells()
//!     .build();
//! ```
//!
//! ## Validation
//!
//! Detect common issues before execution:
//!
//! ```rust
//! use ferrous_cortex::{parse, validate};
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let source = "[+]";  // Inefficient pattern
//! let instructions = parse(source)?;
//! let warnings = validate(&instructions);
//!
//! for warning in warnings {
//!     eprintln!("Warning: {}", warning);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Examples
//!
//! See the `examples/` directory for complete usage examples:
//! - `basic_usage` - Core parsing and execution workflows
//! - `custom_io` - Custom I/O trait implementations
//! - `memory_models` - Different memory model configurations
//! - `validation` - Program validation and warning handling
//! - `minify` - Code minification
//!
//! Run with: `cargo run --example basic_usage`

// Module declarations
pub mod codegen;
mod config;
mod debug;
mod error;
pub mod hooks;
mod instruction;
mod interpreter;
pub mod io;
mod location;
mod minify;
mod parser;
mod stats;
pub mod syntax;
// Test utilities - exposed publicly to allow integration tests to use them
// Integration tests are separate crates and need public access to test utilities
pub mod test_utils;
mod types;
mod validator;

// Re-exports for public API
pub use config::{
    CellBehavior, CellModel, EofBehavior, ExecutionConfig, ExecutionConfigBuilder, MEMORY_SIZE,
    MemoryBehavior, MemoryModel, ReadyToBuild, U8CheckedCells, U8WrappingCells, Unbuilt,
};
pub use debug::DebugInfo;
pub use error::{BfError, BfWarning, MemoryDump, Result, RuntimeWarning};
pub use instruction::Instruction;
pub use interpreter::{interpret, interpret_with_config, interpret_with_io};
pub use location::SourceLocation;
pub use minify::minify;
pub use parser::{parse, parse_with_debug};
pub use stats::ExecutionStats;
pub use types::{InstructionIndex, MemoryAddress, MemorySize, StepCount};
pub use validator::validate;
