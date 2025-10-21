//! I/O abstraction for BrainFuck interpreter.
//!
//! Provides traits for abstracting input and output operations,
//! enabling custom I/O implementations for testing, GUI integration,
//! file operations, and more.
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::io::{StringIo, BfInput, BfOutput};
//!
//! let mut io = StringIo::new("ABC");
//! assert_eq!(io.read_byte().unwrap(), Some(b'A'));
//! io.write_byte(b'X').unwrap();
//! assert_eq!(io.output_string(), "X");
//! ```

use std::io;

/// Input source for BrainFuck `,` (input) instruction.
///
/// Implementations can provide input from stdin, strings, files,
/// network sockets, or any other source.
pub trait BfInput {
    /// Read a single byte.
    ///
    /// Returns `Ok(Some(byte))` if a byte is available,
    /// `Ok(None)` if EOF is reached,
    /// or `Err(e)` on I/O errors.
    fn read_byte(&mut self) -> io::Result<Option<u8>>;
}

/// Output destination for BrainFuck `.` (output) instruction.
///
/// Implementations can write output to stdout, strings, files,
/// network sockets, or any other destination.
pub trait BfOutput {
    /// Write a single byte.
    fn write_byte(&mut self, byte: u8) -> io::Result<()>;

    /// Flush output buffer (optional, default is no-op).
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Combined I/O trait for convenience.
///
/// Types that implement both `BfInput` and `BfOutput` automatically
/// implement this trait.
pub trait BfIo: BfInput + BfOutput {}
impl<T: BfInput + BfOutput> BfIo for T {}

/// Standard input from stdin.
///
/// # Examples
///
/// ```rust,no_run
/// use ferrous_cortex::io::{StdInput, BfInput};
///
/// let mut input = StdInput;
/// if let Ok(Some(byte)) = input.read_byte() {
///     println!("Read: {}", byte as char);
/// }
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct StdInput;

impl BfInput for StdInput {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        use std::io::Read;
        let mut buf = [0u8; 1];
        match io::stdin().read_exact(&mut buf) {
            Ok(_) => Ok(Some(buf[0])),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Standard output to stdout.
///
/// # Examples
///
/// ```rust
/// use ferrous_cortex::io::{StdOutput, BfOutput};
///
/// let mut output = StdOutput;
/// output.write_byte(b'A').unwrap();
/// output.flush().unwrap();
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct StdOutput;

impl BfOutput for StdOutput {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        use std::io::Write;
        io::stdout().write_all(&[byte])
    }

    fn flush(&mut self) -> io::Result<()> {
        use std::io::Write;
        io::stdout().flush()
    }
}

/// Combined standard I/O (stdin + stdout).
///
/// This is a convenience wrapper that provides both input and output
/// using the standard streams.
///
/// # Examples
///
/// ```rust,no_run
/// use ferrous_cortex::io::StdIo;
///
/// let mut io = StdIo::default();
/// // Use for interpreter execution
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct StdIo {
    input: StdInput,
    output: StdOutput,
}

impl StdIo {
    /// Create a new standard I/O instance.
    pub fn new() -> Self {
        Self::default()
    }
}

impl BfInput for StdIo {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        self.input.read_byte()
    }
}

impl BfOutput for StdIo {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        self.output.write_byte(byte)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

/// String-based I/O for testing and library usage.
///
/// Provides input from a string and captures output to a string buffer.
/// This is ideal for unit tests and programmatic use of the interpreter.
///
/// # Examples
///
/// ```rust
/// use ferrous_cortex::io::{StringIo, BfInput, BfOutput};
///
/// let mut io = StringIo::new("Hello");
/// assert_eq!(io.read_byte().unwrap(), Some(b'H'));
/// assert_eq!(io.read_byte().unwrap(), Some(b'e'));
///
/// io.write_byte(b'!').unwrap();
/// assert_eq!(io.output_string(), "!");
/// ```
#[derive(Debug, Clone)]
pub struct StringIo {
    input: Vec<u8>,
    input_pos: usize,
    output: Vec<u8>,
}

impl StringIo {
    /// Create new string-based I/O with given input.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ferrous_cortex::io::StringIo;
    ///
    /// let io = StringIo::new("Test input");
    /// ```
    pub fn new(input: &str) -> Self {
        Self {
            input: input.as_bytes().to_vec(),
            input_pos: 0,
            output: Vec::new(),
        }
    }

    /// Create with empty input.
    ///
    /// Useful when no input is needed, only output capture.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ferrous_cortex::io::StringIo;
    ///
    /// let io = StringIo::empty();
    /// ```
    pub fn empty() -> Self {
        Self::new("")
    }

    /// Get output as string (lossy UTF-8 conversion).
    ///
    /// Non-UTF-8 bytes will be replaced with the Unicode replacement character (�).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ferrous_cortex::io::{StringIo, BfOutput};
    ///
    /// let mut io = StringIo::empty();
    /// io.write_byte(b'H').unwrap();
    /// io.write_byte(b'i').unwrap();
    /// assert_eq!(io.output_string(), "Hi");
    /// ```
    pub fn output_string(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    /// Get output as raw bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ferrous_cortex::io::{StringIo, BfOutput};
    ///
    /// let mut io = StringIo::empty();
    /// io.write_byte(65).unwrap();
    /// assert_eq!(io.output_bytes(), &[65]);
    /// ```
    pub fn output_bytes(&self) -> &[u8] {
        &self.output
    }

    /// Clear the output buffer.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ferrous_cortex::io::{StringIo, BfOutput};
    ///
    /// let mut io = StringIo::empty();
    /// io.write_byte(b'X').unwrap();
    /// io.clear_output();
    /// assert_eq!(io.output_string(), "");
    /// ```
    pub fn clear_output(&mut self) {
        self.output.clear();
    }

    /// Reset input position to the beginning.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ferrous_cortex::io::{StringIo, BfInput};
    ///
    /// let mut io = StringIo::new("AB");
    /// assert_eq!(io.read_byte().unwrap(), Some(b'A'));
    /// io.reset_input();
    /// assert_eq!(io.read_byte().unwrap(), Some(b'A'));
    /// ```
    pub fn reset_input(&mut self) {
        self.input_pos = 0;
    }
}

impl BfInput for StringIo {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        if self.input_pos < self.input.len() {
            let byte = self.input[self.input_pos];
            self.input_pos += 1;
            Ok(Some(byte))
        } else {
            Ok(None)
        }
    }
}

impl BfOutput for StringIo {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        self.output.push(byte);
        Ok(())
    }
}

impl Default for StringIo {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_io_input() {
        let mut io = StringIo::new("ABC");
        assert_eq!(io.read_byte().unwrap(), Some(b'A'));
        assert_eq!(io.read_byte().unwrap(), Some(b'B'));
        assert_eq!(io.read_byte().unwrap(), Some(b'C'));
        assert_eq!(io.read_byte().unwrap(), None);
        assert_eq!(io.read_byte().unwrap(), None); // EOF is idempotent
    }

    #[test]
    fn test_string_io_output() {
        let mut io = StringIo::empty();
        io.write_byte(b'H').unwrap();
        io.write_byte(b'i').unwrap();
        assert_eq!(io.output_string(), "Hi");
        assert_eq!(io.output_bytes(), b"Hi");
    }

    #[test]
    fn test_string_io_combined() {
        let mut io = StringIo::new("Test");
        assert_eq!(io.read_byte().unwrap(), Some(b'T'));
        io.write_byte(b'O').unwrap();
        assert_eq!(io.read_byte().unwrap(), Some(b'e'));
        io.write_byte(b'K').unwrap();
        assert_eq!(io.output_string(), "OK");
    }

    #[test]
    fn test_string_io_reset() {
        let mut io = StringIo::new("AB");
        assert_eq!(io.read_byte().unwrap(), Some(b'A'));
        assert_eq!(io.read_byte().unwrap(), Some(b'B'));
        io.reset_input();
        assert_eq!(io.read_byte().unwrap(), Some(b'A'));
    }

    #[test]
    fn test_string_io_clear_output() {
        let mut io = StringIo::empty();
        io.write_byte(b'X').unwrap();
        assert_eq!(io.output_string(), "X");
        io.clear_output();
        assert_eq!(io.output_string(), "");
    }

    #[test]
    fn test_string_io_empty() {
        let mut io = StringIo::empty();
        assert_eq!(io.read_byte().unwrap(), None);
        io.write_byte(b'A').unwrap();
        assert_eq!(io.output_string(), "A");
    }

    #[test]
    fn test_string_io_non_utf8() {
        let mut io = StringIo::empty();
        io.write_byte(0xFF).unwrap();
        io.write_byte(0xFE).unwrap();
        // Non-UTF8 bytes, but output_string should handle it
        assert!(io.output_string().contains('�'));
        assert_eq!(io.output_bytes(), &[0xFF, 0xFE]);
    }
}
