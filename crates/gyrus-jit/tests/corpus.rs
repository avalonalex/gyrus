//! The program corpus, under the JIT, against `programs/test_manifest.toml`:
//! the same expectations `program_corpus.rs` holds the tree-walker to, and
//! the optimized interpreter beside it for the byte-for-byte comparison.

use gyrus::io::StringIo;
use gyrus::{
    BfError, EofBehavior, ExecutionConfigBuilder, interpret_optimized_with_io, optimize, parse,
};
use std::path::Path;

#[derive(Default, Debug)]
struct Case {
    name: String,
    file: String,
    input: String,
    expected_output: Option<String>,
    expected_exit: String,
    expected_error_type: Option<String>,
    memory_size: Option<usize>,
    max_steps: Option<u64>,
    eof_behavior: Option<String>,
}

/// Just enough TOML for the manifest: `[[test]]` blocks of `key = "string"`.
fn manifest(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "[[test]]" {
            cases.push(Case::default());
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(case) = cases.last_mut() else {
            continue;
        };
        let value = value.trim();
        // `config = { memory_size = 100 }`, `{ max_steps = 1000 }`,
        // `{ eof_behavior = "zero" }`: the inline table the manifest uses.
        if key.trim() == "config" {
            let inner = value.trim_start_matches('{').trim_end_matches('}');
            for pair in inner.split(',') {
                let Some((k, v)) = pair.split_once('=') else {
                    continue;
                };
                let v = v.trim().trim_matches('"');
                match k.trim() {
                    "memory_size" => case.memory_size = v.parse().ok(),
                    "max_steps" => case.max_steps = v.parse().ok(),
                    "eof_behavior" => case.eof_behavior = Some(v.to_string()),
                    _ => {}
                }
            }
            continue;
        }
        let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
            continue;
        };
        let value = unescape(value);
        match key.trim() {
            "name" => case.name = value,
            "file" => case.file = value,
            "input" => case.input = value,
            "expected_output" => case.expected_output = Some(value),
            "expected_exit" => case.expected_exit = value,
            "expected_error_type" => case.expected_error_type = Some(value),
            _ => {}
        }
    }
    cases
}

/// TOML basic-string escapes the manifest uses: `\n`, `\t`, `\"`, `\uXXXX`.
fn unescape(raw: &str) -> String {
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
    for case in cases {
        let source = std::fs::read_to_string(root.join("programs").join(&case.file)).unwrap();
        let build = || {
            let b =
                ExecutionConfigBuilder::new().with_memory_size(case.memory_size.unwrap_or(30_000));
            // Under the JIT a step is a loop iteration, so the manifest's
            // budget stops the looping-forever cases just as surely.
            let b = match case.max_steps {
                Some(max) => b.with_max_steps(max),
                None => b,
            };
            let b = match case.eof_behavior.as_deref() {
                Some("no_change") => b.with_eof_behavior(EofBehavior::NoChange),
                Some("negone") | Some("neg_one") => b.with_eof_behavior(EofBehavior::SetNegOne),
                Some("error") => b.with_eof_behavior(EofBehavior::Error),
                _ => b,
            };
            b.build()
        };
        let parsed = parse(&source);
        if case.expected_error_type.as_deref() == Some("parse") {
            assert!(parsed.is_err(), "{}: expected a parse error", case.name);
            continue;
        }
        let program = optimize(&parsed.unwrap());
        let run = |jit: bool| {
            let (mut i, mut o) = (StringIo::new(&case.input), StringIo::empty());
            let r = if jit {
                gyrus_jit::run(&program, &build(), &mut i, &mut o, None)
            } else {
                interpret_optimized_with_io(&program, build(), &mut i, &mut o)
            };
            r.map(|_| o.output_bytes().to_vec())
        };
        let (interp, jit) = (run(false), run(true));
        match (case.expected_exit.as_str(), &jit) {
            ("success", Ok(out)) => {
                if let Some(expected) = &case.expected_output {
                    assert_eq!(out, expected.as_bytes(), "{}: output", case.name);
                }
                assert_eq!(
                    out,
                    interp.as_ref().unwrap(),
                    "{}: interpreter and JIT differ",
                    case.name
                );
            }
            ("error", Err(e)) => {
                let interp_err = interp.expect_err("interpreter should fail too");
                assert_eq!(
                    std::mem::discriminant(e),
                    std::mem::discriminant(&interp_err),
                    "{}: {e:?} vs {interp_err:?}",
                    case.name
                );
                match case.expected_error_type.as_deref() {
                    Some("limit") => assert!(
                        matches!(e, BfError::StepLimitExceeded { .. }),
                        "{}: {e:?}",
                        case.name
                    ),
                    Some("runtime") => assert!(
                        matches!(e, BfError::MemoryOutOfBounds { .. }),
                        "{}: {e:?}",
                        case.name
                    ),
                    _ => {}
                }
            }
            (want, got) => panic!("{}: expected {want}, got {got:?}", case.name),
        }
    }
}
