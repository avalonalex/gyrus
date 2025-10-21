// Module declarations
mod config;
mod error;
mod instruction;
mod interpreter;
mod location;
mod minify;
mod parser;
mod stats;
mod validator;

// Re-exports for public API
pub use config::{EofBehavior, ExecutionConfig, MEMORY_SIZE, MemoryModel};
pub use error::{BfError, BfWarning};
pub use instruction::Instruction;
pub use interpreter::{interpret, interpret_with_config};
pub use location::SourceLocation;
pub use minify::minify;
pub use parser::parse;
pub use stats::ExecutionStats;
pub use validator::validate;
