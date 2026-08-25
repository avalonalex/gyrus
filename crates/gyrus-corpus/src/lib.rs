//! The program corpus manifest, parsed.
//!
//! `programs/test_manifest.toml` is the single description of what each
//! bundled BrainFuck program should do, and both test suites read it from
//! here: `gyrus`'s corpus test runs every case through the tree-walking
//! interpreter, and `gyrus-jit`'s runs the same cases through the JIT beside
//! the optimized interpreter. That is the point of this crate existing. When
//! the two suites each kept their own idea of the corpus, the tree-walker's
//! was hand-written to *mirror* the manifest, and the mirror drifted: eleven
//! programs were tested by one engine and not the other, in both directions.
//!
//! This is test support. It is not part of what gyrus offers as a library,
//! which is why it lives in its own crate rather than in `gyrus` behind a
//! feature -- a `test-support` feature would put these helpers in the public
//! API, and they were deliberately taken out of it before the crate went
//! public.
//!
//! The TOML parser here is deliberately small rather than a dependency: it
//! understands exactly the manifest's shape and treats anything else as an
//! error, so an expectation can never be dropped silently by being
//! misspelled.

use gyrus::{BfError, EofBehavior, ExecutionConfig, ExecutionConfigBuilder};
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct Case {
    pub name: String,
    pub file: String,
    pub input: String,
    pub expected_output: Option<Vec<u8>>,
    /// For programs that never terminate on purpose: the bytes the run must
    /// begin with. The step limit stands in for the Ctrl-C a human would type,
    /// so there is a prefix to check but never a whole output.
    pub expected_output_prefix: Option<Vec<u8>>,
    pub expected_exit: String,
    pub expected_error_type: Option<String>,
    /// The quine: its output is its own source. Weaker expectations (a length
    /// floor) are what this replaces.
    pub output_is_source: bool,
    pub memory_size: Option<usize>,
    pub max_steps: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub eof_behavior: Option<EofBehavior>,
}

/// A line of the manifest without its trailing comment. A `#` inside a
/// quoted string is not a comment.
fn uncommented(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if quoted => escaped = !escaped,
            '"' if !escaped => quoted = !quoted,
            '#' if !quoted => return line[..i].trim(),
            _ => escaped = false,
        }
    }
    line.trim()
}

/// Just enough TOML for the manifest: `[[test]]` blocks of `key = value`,
/// where a value is a basic string, an integer, an array (ignored), or the
/// inline table `config = { ... }` of integers and strings. Anything this
/// does not understand is an error, not a silently dropped expectation.
pub fn manifest(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    for (number, raw) in text.lines().enumerate() {
        let line = uncommented(raw);
        if line.is_empty() {
            continue;
        }
        if line == "[[test]]" {
            cases.push(Case::default());
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("manifest line {}: not `key = value`: {raw}", number + 1));
        let case = cases
            .last_mut()
            .unwrap_or_else(|| panic!("manifest line {}: before any [[test]]", number + 1));
        let (key, value) = (key.trim(), value.trim());
        if key == "config" {
            let inner = value
                .strip_prefix('{')
                .and_then(|v| v.strip_suffix('}'))
                .unwrap_or_else(|| panic!("manifest line {}: config is not a table", number + 1));
            for pair in inner.split(',').filter(|p| !p.trim().is_empty()) {
                let (k, v) = pair
                    .split_once('=')
                    .unwrap_or_else(|| panic!("manifest line {}: bad pair {pair:?}", number + 1));
                let v = v.trim();
                match k.trim() {
                    "memory_size" => case.memory_size = Some(integer(v)),
                    "max_steps" => case.max_steps = Some(integer(v)),
                    "timeout_ms" => case.timeout_ms = Some(integer(v)),
                    "eof_behavior" => {
                        let spelling = string(v);
                        case.eof_behavior = Some(spelling.parse().unwrap_or_else(|()| {
                            panic!(
                                "manifest line {}: unknown eof_behavior {spelling:?}",
                                number + 1
                            )
                        }));
                    }
                    other => panic!("manifest line {}: unknown config key {other}", number + 1),
                }
            }
            continue;
        }
        match key {
            "name" => case.name = string(value),
            "file" => case.file = string(value),
            "input" => case.input = string(value),
            // Expected bytes: the manifest writes them as a string, and a
            // character up to U+00FF is one byte of output.
            "expected_output" => case.expected_output = Some(bytes(&string(value))),
            "expected_output_prefix" => case.expected_output_prefix = Some(bytes(&string(value))),
            "expected_exit" => case.expected_exit = string(value),
            "expected_error_type" => {
                let kind = string(value);
                assert!(
                    matches!(kind.as_str(), "parse" | "limit" | "runtime"),
                    "manifest line {}: unknown expected_error_type {kind:?}",
                    number + 1
                );
                case.expected_error_type = Some(kind);
            }
            "timeout_ms" => case.timeout_ms = Some(integer(value)),
            "output_is_source" => {
                case.output_is_source = value == "true";
                assert!(
                    case.output_is_source,
                    "manifest line {}: output_is_source is only ever true",
                    number + 1
                );
            }
            "description" | "tags" | "expected_warnings" => {}
            other => panic!("manifest line {}: unknown key {other}", number + 1),
        }
    }
    cases
}

/// Manifest text as the bytes a program emits. One character is one byte, so
/// anything above U+00FF is a mistake in the manifest rather than something to
/// truncate quietly.
fn bytes(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| {
            u8::try_from(c as u32)
                .unwrap_or_else(|_| panic!("{c:?} is not a single byte; write it as \\uXXXX"))
        })
        .collect()
}

fn integer<T: std::str::FromStr>(v: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    v.replace('_', "")
        .parse()
        .unwrap_or_else(|e| panic!("bad integer {v:?}: {e:?}"))
}

/// A TOML basic string: quotes stripped, `\n`, `\t`, `\"`, `\\`, `\uXXXX`.
fn string(v: &str) -> String {
    let raw = v
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_else(|| panic!("not a quoted string: {v}"));
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                assert_eq!(hex.len(), 4, "\\u needs four hex digits, got {hex:?}");
                out.push(
                    char::from_u32(u32::from_str_radix(&hex, 16).expect("\\u escape"))
                        .expect("scalar"),
                );
            }
            other => panic!("unsupported escape \\{other:?} in manifest"),
        }
    }
    out
}

/// The workspace root, found relative to this crate rather than the working
/// directory, so tests run the same under `cargo test` from anywhere.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Every case in `programs/test_manifest.toml`.
///
/// Panics rather than returning an error: a manifest this cannot read is a
/// broken test suite, and the panic names the line.
pub fn cases() -> Vec<Case> {
    let path = workspace_root().join("programs/test_manifest.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let cases = manifest(&text);
    assert!(
        cases.len() >= 20,
        "manifest parsed to only {} cases -- the parser is probably wrong",
        cases.len()
    );
    for case in &cases {
        assert!(
            case.expected_exit == "error"
                || case.expected_output.is_some()
                || case.expected_output_prefix.is_some()
                || case.output_is_source,
            "{}: a success case must say what it produces -- expected_output, \
             expected_output_prefix, or output_is_source. A case that only \
             asserts it exited cleanly is a program nobody is checking.",
            case.name
        );
    }
    cases
}

/// A run that never returns is a failure with a name, not a hung suite: every
/// case runs under a step budget and a timeout, the manifest's or this default.
pub const DEFAULT_MAX_STEPS: u64 = 10_000_000;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

impl Case {
    /// The program's source, read from `programs/`.
    pub fn source(&self) -> String {
        let path = workspace_root().join("programs").join(&self.file);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: cannot read {}: {e}", self.name, path.display()))
    }

    /// The configuration this case asks for, under either cell model.
    pub fn config(&self, checked: bool) -> ExecutionConfig {
        let b = ExecutionConfigBuilder::new().with_memory_size(self.memory_size.unwrap_or(30_000));
        let b = if checked { b.with_checked_cells() } else { b };
        let b = b
            .with_max_steps(self.max_steps.unwrap_or(DEFAULT_MAX_STEPS))
            .with_timeout_ms(self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
        match self.eof_behavior {
            Some(eof) => b.with_eof_behavior(eof).build(),
            None => b.build(),
        }
    }

    /// Whether the manifest expects this program to fail to parse.
    pub fn expects_parse_error(&self) -> bool {
        self.expected_error_type.as_deref() == Some("parse")
    }

    /// Whether the manifest expects a limit to stop this program.
    pub fn expects_limit(&self) -> bool {
        self.expected_error_type.as_deref() == Some("limit")
    }

    /// Hold a run to everything the manifest says about it.
    ///
    /// Both suites call this rather than writing their own assertions, so the
    /// tree-walker and the JIT are held to identical expectations. They were
    /// not before: one checked `expected_output` for every case and the other
    /// only for cases that exited successfully, so a case declaring both an
    /// error and an output went unchecked under the JIT.
    ///
    /// `engine` names who is being checked, so a failure says which one.
    pub fn check(&self, engine: &str, result: &Result<(), BfError>, output: &[u8]) {
        let name = &self.name;
        match (self.expected_exit.as_str(), result) {
            ("success", Ok(())) => {}
            ("error", Err(e)) => match self.expected_error_type.as_deref() {
                Some("limit") => assert!(
                    matches!(
                        e,
                        BfError::StepLimitExceeded { .. } | BfError::ExecutionTimeout { .. }
                    ),
                    "{name} ({engine}): expected a limit, got {e:?}"
                ),
                Some("runtime") => assert!(
                    matches!(e, BfError::MemoryOutOfBounds { .. }),
                    "{name} ({engine}): expected a runtime error, got {e:?}"
                ),
                // Rejected by the parser, so this is unreachable rather than
                // permissive.
                other => panic!("{name}: expected_error_type {other:?} on an error case"),
            },
            (want, got) => panic!("{name} ({engine}): manifest expects {want}, got {got:?}"),
        }

        // Output is checked whatever the exit was: a program its step limit
        // stopped still has to have emitted the right bytes first.
        if let Some(expected) = &self.expected_output {
            assert_eq!(
                Bytes(output),
                Bytes(expected),
                "{name} ({engine}): output differs from the manifest"
            );
        }
        if let Some(prefix) = &self.expected_output_prefix {
            assert!(
                output.starts_with(prefix),
                "{name} ({engine}): output does not start with the expected prefix\n                   expected: {:?}\n  got:      {:?}",
                Bytes(prefix),
                Bytes(&output[..output.len().min(prefix.len() + 16)]),
            );
        }
        if self.output_is_source {
            assert_eq!(
                Bytes(output),
                Bytes(self.source().as_bytes()),
                "{name} ({engine}): a quine must reproduce its own source"
            );
        }
    }
}

/// Bytes that print as text where they can, so a failed comparison is readable
/// rather than a wall of integers.
pub struct Bytes<'a>(pub &'a [u8]);

impl PartialEq for Bytes<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::fmt::Debug for Bytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(self.0))?;
        if self.0.len() > 40 {
            write!(f, " ({} bytes)", self.0.len())?;
        }
        Ok(())
    }
}
