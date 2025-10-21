/// Default memory size (30,000 bytes - traditional BrainFuck)
pub const MEMORY_SIZE: usize = 30000;

/// Behavior when input (,) encounters EOF
#[derive(Debug, Clone, PartialEq)]
pub enum EofBehavior {
    /// Set cell to 0 on EOF (most common, used by many interpreters)
    SetZero,

    /// Set cell to 255 (-1 as unsigned byte) on EOF
    SetNegOne,

    /// Leave cell unchanged on EOF
    NoChange,

    /// Return error on EOF (strictest, prevents silent bugs)
    Error,
}

impl Default for EofBehavior {
    fn default() -> Self {
        EofBehavior::SetZero // Most common behavior
    }
}

/// Memory model for interpreter execution
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryModel {
    /// Fixed-size memory array (current behavior)
    /// Out-of-bounds access returns an error
    Fixed(usize),

    /// Wrapping memory: pointer wraps around at boundaries
    /// e.g., at size 30000: pointer 30000 -> 0, pointer -1 -> 29999
    Wrapping(usize),

    /// Unbounded memory: grows as needed up to a maximum limit
    /// Starts small and expands when accessed beyond current size
    Unbounded {
        initial_size: usize,
        max_size: usize,
    },
}

impl MemoryModel {
    /// Get the initial memory size for this model
    pub fn initial_size(&self) -> usize {
        match self {
            MemoryModel::Fixed(size) => *size,
            MemoryModel::Wrapping(size) => *size,
            MemoryModel::Unbounded { initial_size, .. } => *initial_size,
        }
    }
}

/// Configuration for BrainFuck interpreter execution
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub memory_model: MemoryModel,
    pub max_steps: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub allow_negative_pointer: bool,
    pub eof_behavior: EofBehavior,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            memory_model: MemoryModel::Fixed(MEMORY_SIZE),
            max_steps: None,
            timeout_ms: None,
            allow_negative_pointer: false,
            eof_behavior: EofBehavior::default(),
        }
    }
}

impl ExecutionConfig {
    /// Create a new ExecutionConfig with default values (same as Default::default())
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the memory model
    pub fn with_memory_model(mut self, model: MemoryModel) -> Self {
        self.memory_model = model;
        self
    }

    /// Set fixed memory size (convenience method)
    #[allow(dead_code)]
    pub fn with_memory_size(mut self, size: usize) -> Self {
        self.memory_model = MemoryModel::Fixed(size);
        self
    }

    /// Set wrapping memory model
    #[allow(dead_code)]
    pub fn with_wrapping_memory(mut self, size: usize) -> Self {
        self.memory_model = MemoryModel::Wrapping(size);
        self
    }

    /// Set unbounded memory model
    #[allow(dead_code)]
    pub fn with_unbounded_memory(mut self, initial_size: usize, max_size: usize) -> Self {
        self.memory_model = MemoryModel::Unbounded {
            initial_size,
            max_size,
        };
        self
    }

    pub fn with_max_steps(mut self, steps: u64) -> Self {
        self.max_steps = Some(steps);
        self
    }

    pub fn with_timeout_ms(mut self, timeout: u64) -> Self {
        self.timeout_ms = Some(timeout);
        self
    }

    /// Set EOF behavior for input operations
    pub fn with_eof_behavior(mut self, behavior: EofBehavior) -> Self {
        self.eof_behavior = behavior;
        self
    }

    /// Allow pointer to go negative (not yet fully implemented)
    #[allow(dead_code)]
    pub fn with_negative_pointer(mut self, allow: bool) -> Self {
        self.allow_negative_pointer = allow;
        self
    }
}
