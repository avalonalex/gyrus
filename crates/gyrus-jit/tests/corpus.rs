//! The program corpus, under the JIT, against `programs/test_manifest.toml`:
//! the same expectations `program_corpus.rs` holds the tree-walker to, the
//! manifest's own configuration, and the optimized interpreter beside the
//! JIT for the byte-for-byte comparison -- under both cell models.

use gyrus::io::StringIo;
use gyrus::{
    BfError, EofBehavior, ExecutionConfig, ExecutionConfigBuilder, interpret_optimized_with_io,
    optimize_with_cell_model, parse,
};
use std::path::Path;

#[derive(Default, Debug)]
struct Case {
    name: String,
    file: String,
    input: String,
    expected_output: Option<Vec<u8>>,
    expected_exit: String,
    expected_error_type: Option<String>,
    memory_size: Option<usize>,
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    eof_behavior: Option<EofBehavior>,
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
fn manifest(text: &str) -> Vec<Case> {
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
            "expected_output" => {
                case.expected_output = Some(string(value).chars().map(|c| c as u32 as u8).collect())
            }
            "expected_exit" => case.expected_exit = string(value),
            "expected_error_type" => case.expected_error_type = Some(string(value)),
            "timeout_ms" => case.timeout_ms = Some(integer(value)),
            // The quine: its output is its own source, which this test does
            // not know how to compare, so no expectation is taken.
            "skip_output_check" => {}
            "description" | "tags" | "expected_warnings" => {}
            other => panic!("manifest line {}: unknown key {other}", number + 1),
        }
    }
    cases
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

/// A run that never returns is a failure with a name, not a hung suite:
/// every case runs under a step budget and a timeout, the manifest's or
/// this default.
const DEFAULT_MAX_STEPS: u64 = 10_000_000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

fn config(case: &Case, checked: bool) -> ExecutionConfig {
    let b = ExecutionConfigBuilder::new().with_memory_size(case.memory_size.unwrap_or(30_000));
    let b = if checked { b.with_checked_cells() } else { b };
    let b = b
        .with_max_steps(case.max_steps.unwrap_or(DEFAULT_MAX_STEPS))
        .with_timeout_ms(case.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    match case.eof_behavior {
        Some(eof) => b.with_eof_behavior(eof).build(),
        None => b.build(),
    }
}

/// Both engines' (result, output) on the case.
fn both(
    case: &Case,
    program: &gyrus::optimizer::OptimizedProgram,
    checked: bool,
) -> [(Result<(), BfError>, Vec<u8>); 2] {
    let run = |jit: bool| {
        let (mut i, mut o) = (StringIo::new(&case.input), StringIo::empty());
        let r = if jit {
            gyrus_jit::run(program, &config(case, checked), &mut i, &mut o, None)
        } else {
            interpret_optimized_with_io(program, config(case, checked), &mut i, &mut o)
        };
        (r.map(|_| ()), o.output_bytes().to_vec())
    };
    [run(false), run(true)]
}

#[test]
fn the_corpus_behaves_the_same_under_the_jit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let cases =
        manifest(&std::fs::read_to_string(root.join("programs/test_manifest.toml")).unwrap());
    assert!(
        cases.len() >= 10,
        "manifest parsed to only {} cases",
        cases.len()
    );
    for case in &cases {
        let source = std::fs::read_to_string(root.join("programs").join(&case.file))
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let parsed = parse(&source);
        if case.expected_error_type.as_deref() == Some("parse") {
            assert!(parsed.is_err(), "{}: expected a parse error", case.name);
            continue;
        }
        let instructions = parsed.unwrap_or_else(|e| panic!("{}: {e}", case.name));
        for checked in [false, true] {
            let what = format!(
                "{} ({})",
                case.name,
                if checked { "checked" } else { "wrapping" }
            );
            let program =
                optimize_with_cell_model(&instructions, *config(case, checked).cell_model());
            let [(interp, out_i), (jit, out_j)] = both(case, &program, checked);
            // Whatever happened, the bytes emitted before it are the same.
            assert_eq!(out_j, out_i, "{what}: output differs between engines");
            match (&interp, &jit) {
                (Ok(()), Ok(())) => {}
                (Err(a), Err(b)) => assert_eq!(
                    std::mem::discriminant(a),
                    std::mem::discriminant(b),
                    "{what}: {a:?} vs {b:?}"
                ),
                (a, b) => panic!("{what}: interpreter {a:?}, jit {b:?}"),
            }
            // The manifest's expectations are for the model it was written
            // for, the default; under checked cells a program may fail where
            // it did not, and then only the agreement above is required.
            if checked {
                continue;
            }
            match (case.expected_exit.as_str(), &jit) {
                ("success", Ok(())) => {
                    if let Some(expected) = &case.expected_output {
                        assert_eq!(&out_j, expected, "{what}: output");
                    }
                }
                ("error", Err(e)) => match case.expected_error_type.as_deref() {
                    Some("limit") => assert!(
                        matches!(
                            e,
                            BfError::StepLimitExceeded { .. } | BfError::ExecutionTimeout { .. }
                        ),
                        "{what}: {e:?}"
                    ),
                    Some("runtime") => {
                        assert!(
                            matches!(e, BfError::MemoryOutOfBounds { .. }),
                            "{what}: {e:?}"
                        )
                    }
                    _ => {}
                },
                (want, got) => panic!("{what}: expected {want}, got {got:?}"),
            }
        }
    }
}
