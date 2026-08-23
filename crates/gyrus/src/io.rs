//! I/O abstraction for BrainFuck interpreter.
//!
//! Provides traits for abstracting input and output operations,
//! enabling custom I/O implementations for testing, GUI integration,
//! file operations, and more.
//!
//! # Examples
//!
//! ```rust
//! use gyrus::io::{StringIo, BfInput, BfOutput};
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
/// use gyrus::io::{StdInput, BfInput};
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
/// use gyrus::io::{StdOutput, BfOutput};
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
/// use gyrus::io::StdIo;
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
/// use gyrus::io::{StringIo, BfInput, BfOutput};
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
    /// use gyrus::io::StringIo;
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
    /// use gyrus::io::StringIo;
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
    /// use gyrus::io::{StringIo, BfOutput};
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
    /// use gyrus::io::{StringIo, BfOutput};
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
    /// use gyrus::io::{StringIo, BfOutput};
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
    /// use gyrus::io::{StringIo, BfInput};
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

/// Debug I/O for testing: infinite input, discarded output.
///
/// This I/O implementation is designed for testing scenarios where:
/// - Programs may request arbitrary amounts of input (`,` instruction)
/// - Output is not needed for the test
/// - Programs should never block on input
///
/// `DebugIo` cycles through a predefined set of characters indefinitely,
/// ensuring that programs always get input when requested. All output
/// is silently discarded.
///
/// # Use Cases
///
/// - Property-based testing with random programs
/// - Stress testing without input/output bottlenecks
/// - Testing programs with unknown input requirements
///
/// # Examples
///
/// ```rust
/// use gyrus::io::{DebugIo, BfInput, BfOutput};
///
/// let mut io = DebugIo::new();
///
/// // Infinite input - never returns EOF
/// assert_eq!(io.read_byte().unwrap(), Some(b'X'));
/// assert_eq!(io.read_byte().unwrap(), Some(b'\n'));
/// assert_eq!(io.read_byte().unwrap(), Some(b'A'));
/// assert_eq!(io.read_byte().unwrap(), Some(b'B'));
/// assert_eq!(io.read_byte().unwrap(), Some(b'0'));
/// assert_eq!(io.read_byte().unwrap(), Some(b'X')); // Cycles back
///
/// // Output is discarded
/// io.write_byte(b'H').unwrap();
/// io.write_byte(b'i').unwrap();
/// // No way to retrieve output - it's gone
/// ```
#[derive(Debug, Clone, Default)]
pub struct DebugIo {
    /// Counter for cycling through input characters
    counter: usize,
}

impl DebugIo {
    /// Create a new debug I/O instance.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gyrus::io::DebugIo;
    ///
    /// let io = DebugIo::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }
}

impl BfInput for DebugIo {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        // Cycle through a few characters: 'X', '\n', 'A', 'B', '0'
        // This provides variety while keeping programs progressing
        const INPUT_CHARS: &[u8] = b"X\nAB0";
        let byte = INPUT_CHARS[self.counter % INPUT_CHARS.len()];
        self.counter += 1;
        Ok(Some(byte))
    }
}

impl BfOutput for DebugIo {
    fn write_byte(&mut self, _byte: u8) -> io::Result<()> {
        // Discard all output - we don't care about it in debug/test scenarios
        Ok(())
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

    #[test]
    fn test_debug_io_infinite_input() {
        let mut io = DebugIo::new();
        // Read more than the cycle length to verify it cycles
        assert_eq!(io.read_byte().unwrap(), Some(b'X'));
        assert_eq!(io.read_byte().unwrap(), Some(b'\n'));
        assert_eq!(io.read_byte().unwrap(), Some(b'A'));
        assert_eq!(io.read_byte().unwrap(), Some(b'B'));
        assert_eq!(io.read_byte().unwrap(), Some(b'0'));
        // Should cycle back to 'X'
        assert_eq!(io.read_byte().unwrap(), Some(b'X'));
        assert_eq!(io.read_byte().unwrap(), Some(b'\n'));
    }

    #[test]
    fn test_debug_io_discards_output() {
        let mut io = DebugIo::new();
        // Output operations should succeed but discard data
        io.write_byte(b'H').unwrap();
        io.write_byte(b'i').unwrap();
        io.write_byte(b'!').unwrap();
        // No way to verify output was discarded (which is the point)
        // Just verify the operations succeeded
    }

    #[test]
    fn test_debug_io_never_eof() {
        let mut io = DebugIo::new();
        // Read many bytes to ensure we never get EOF
        for _ in 0..100 {
            assert!(io.read_byte().unwrap().is_some());
        }
    }
}
