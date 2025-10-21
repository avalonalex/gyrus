// Module declarations
mod config;
mod error;
mod instruction;
mod interpreter;
mod location;
mod minify;
mod parser;
mod stats;
mod types;
mod validator;

// Re-exports for public API
pub use config::{
    EofBehavior, ExecutionConfig, ExecutionConfigBuilder, MEMORY_SIZE, MemoryModel, ReadyToBuild,
    Unbuilt,
};
pub use error::{BfError, BfWarning, Result};
pub use instruction::Instruction;
pub use interpreter::{interpret, interpret_with_config};
pub use location::SourceLocation;
pub use minify::minify;
pub use parser::parse;
pub use stats::ExecutionStats;
pub use types::{InstructionIndex, MemoryAddress, MemorySize, StepCount};
pub use validator::validate;
