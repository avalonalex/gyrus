//! Spike driver: `gyrus-jit program.bf` -- optimize, JIT, run on stdin/stdout.
use gyrus::io::{StdInput, StdOutput};
use gyrus::{ExecutionConfig, optimize, parse};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: gyrus-jit program.bf");
    let source = std::fs::read_to_string(&path).expect("read program");
    let instructions = parse(&source).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let program = optimize(&instructions);
    let config = ExecutionConfig::default();
    let mut input = StdInput;
    let mut output = StdOutput;
    match gyrus_jit::run_with(
        &program,
        &config,
        &mut input,
        &mut output,
        None,
        gyrus_jit::Statistics::Cheap,
    ) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1)
        }
    }
}
