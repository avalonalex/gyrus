// Module declarations
mod config;
mod debug;
mod error;
mod instruction;
mod interpreter;
pub mod io;
mod location;
mod minify;
mod parser;
mod stats;
#[cfg(test)]
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
