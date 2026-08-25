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

/// The spellings the CLI, the test manifest and every other reader accept.
/// Written once, here, as the cell model's are: the CLI and a test had grown
/// their own tables, and they had drifted.
impl std::str::FromStr for EofBehavior {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s.to_lowercase().as_str() {
            "zero" | "set-zero" | "set_zero" | "setzero" | "0" => Ok(EofBehavior::SetZero),
            "neg-one" | "neg_one" | "negone" | "set-neg-one" | "set_neg_one" | "-1" | "255" => {
                Ok(EofBehavior::SetNegOne)
            }
            "no-change" | "no_change" | "nochange" | "unchanged" => Ok(EofBehavior::NoChange),
            "error" => Ok(EofBehavior::Error),
            _ => Err(()),
        }
    }
}
