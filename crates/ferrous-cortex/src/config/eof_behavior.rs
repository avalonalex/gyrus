//! EOF (End-Of-File) behavior configuration

/// Behavior when input (,) encounters EOF
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EofBehavior {
    /// Set cell to 0 on EOF (most common, used by many interpreters)
    #[default]
    SetZero,

    /// Set cell to 255 (-1 as unsigned byte) on EOF
    SetNegOne,

    /// Leave cell unchanged on EOF
    NoChange,

    /// Return error on EOF (strictest, prevents silent bugs)
    Error,
}
