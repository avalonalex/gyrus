//! `gyrus-debug` — step through a BrainFuck program and watch the tape.
//!
//! The debugger is built entirely on the library's public surface: it registers
//! an [`ExecutionHook`](gyrus::hooks::ExecutionHook) that decides when to stop,
//! and supplies its own [`BfInput`](gyrus::io::BfInput) and
//! [`BfOutput`](gyrus::io::BfOutput) so the program's bytes land in a panel
//! rather than in the middle of the interface.
//!
//! It runs the tree-walking interpreter, not the optimized one. Debugging needs
//! source locations for every instruction and a hook on every step, and the
//! optimized path deliberately has neither.

mod hook;
mod program;
mod state;
mod ui;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clap::Parser;
use gyrus::{
    BfError, CellModel, EofBehavior, ExecutionConfig, ExecutionConfigBuilder, SourceLocation,
    interpret_with_io,
};
use gyrus_tui::TerminalGuard;

use hook::{DebugInput, DebugOutput, DebuggerHook, Shared};
use program::Program;
use state::{Exit, Note, Outcome, RunState, Session};

#[derive(Parser)]
#[command(name = "gyrus-debug")]
#[command(about = "Step through a BrainFuck program, with the tape in view", long_about = None)]
#[command(after_help = "Press ? inside the debugger for the full key list.")]
struct Cli {
    /// BrainFuck source file to debug
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Memory size in bytes (for the fixed model)
    #[arg(long, default_value = "30000")]
    memory_size: usize,

    /// Memory model: fixed or unbounded
    #[arg(long, default_value = "fixed")]
    memory_model: String,

    /// Cell model: wrapping (default) or checked (errors on overflow)
    #[arg(long, default_value = "wrapping")]
    cell_model: String,

    /// Initial memory size for the unbounded model
    #[arg(long, default_value = "1000")]
    unbounded_initial: usize,

    /// Maximum memory size for the unbounded model
    #[arg(long, default_value = "1000000")]
    unbounded_max: usize,

    /// EOF behavior: zero, neg-one, no-change, or error
    #[arg(long, default_value = "zero")]
    eof_behavior: String,

    /// Maximum number of execution steps (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_steps: u64,

    /// Execution timeout in milliseconds (0 = unlimited)
    #[arg(long, default_value = "0")]
    timeout: u64,

    /// Bytes to feed the program's `,` instructions. A trailing newline is
    /// added if there is not one, the way a shell would
    #[arg(long, value_name = "TEXT")]
    input: Option<String>,

    /// Read the program's input from a file instead
    #[arg(long, value_name = "FILE", conflicts_with = "input")]
    input_file: Option<PathBuf>,

    /// Set a breakpoint at LINE or LINE:COLUMN. Repeatable
    #[arg(short = 'b', long = "break", value_name = "LINE[:COL]")]
    breakpoints: Vec<String>,

    /// Character in the source that marks a breakpoint. Must not be a
    /// BrainFuck command or `*`
    #[arg(long, value_name = "CHAR", default_value = "@")]
    marker: char,

    /// Ignore breakpoint markers in the source
    #[arg(long, conflicts_with = "marker")]
    no_markers: bool,

    /// Start running instead of stopping at the first instruction
    #[arg(long)]
    run: bool,

    /// Initial memory display: hex, decimal, or ascii
    #[arg(long, default_value = "hex")]
    display: String,
}

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    let program = Arc::new(Program::load(&cli.file).map_err(|error| error.format_detailed())?);
    let display = ui::parse_display(&cli.display).ok_or_else(|| {
        format!(
            "Error: Invalid display '{}'. Valid options: hex, decimal, ascii",
            cli.display
        )
    })?;
    let initial_input = read_input(&cli)?;
    let initial_memory = initial_memory_size(&cli)?;

    // Validate everything before taking over the terminal, so a typo in a flag
    // prints an error instead of flashing an empty screen at it.
    build_config(&cli, None)?;
    let marker = marker_char(&cli)?;
    let breakpoints: Vec<(usize, usize)> = cli
        .breakpoints
        .iter()
        .map(|spec| parse_breakpoint(spec))
        .collect::<Result<_, _>>()?;

    let (_guard, terminal) = TerminalGuard::enter()
        .map_err(|error| format!("Error: could not set up the terminal: {error}"))?;

    let mut session = Session::new(Arc::clone(&program), terminal, initial_memory);
    session.ui.memory_display = display;
    session.pending_input = initial_input.into();
    for position in breakpoints {
        apply_breakpoint(&mut session, position)?;
    }
    apply_markers(marker, &mut session);
    let initial_run = if cli.run {
        RunState::Continue
    } else {
        RunState::Step
    };
    session.run = initial_run;

    let shared: Shared = Arc::new(Mutex::new(session));

    loop {
        let config = build_config(&cli, Some(Arc::clone(&shared)))?;
        let mut input = DebugInput::new(Arc::clone(&shared));
        let mut output = DebugOutput::new(Arc::clone(&shared));

        let result = interpret_with_io(
            &program.instructions,
            config,
            &mut input,
            &mut output,
            Some(&program.debug),
        );

        let mut session = shared.lock().expect("debugger session poisoned");
        match result {
            Ok(stats) => session.outcome = Some(Outcome::Completed(Box::new(stats))),
            // The only thing that pauses execution here is this debugger asking
            // it to, so an `ExecutionPaused` is the user having pressed quit or
            // restart, not a failure.
            Err(BfError::ExecutionPaused { .. }) if session.exit.is_some() => {}
            Err(error) => {
                // Point the source panel at the instruction that failed. The
                // interpreter stopped there, so the tape on screen is the state
                // it failed in — which is the reason to be in a debugger at all.
                if let Some(location) = error_location(&error) {
                    session.snapshot.location = Some(location);
                }
                session.outcome = Some(Outcome::Failed(error.format_detailed()));
            }
        }

        if session.exit.is_none() {
            // The program stopped on its own. Show the final state and let the
            // user look around before deciding what to do.
            ui::post_mortem(&mut session)
                .map_err(|error| format!("Error: terminal failure: {error}"))?;
        }

        match session.exit.take() {
            Some(Exit::Restart) => {
                session.reset(initial_memory);
                session.run = initial_run;
            }
            // The interface could not draw. Report it the way every other
            // failure is reported, rather than exiting zero in silence.
            Some(Exit::Failed(error)) => {
                return Err(format!("Error: terminal failure: {error}"));
            }
            _ => break,
        }
    }

    Ok(())
}

/// The marker character to scan for, or `None` when markers are off.
///
/// Rejects the eight commands and `*`: a marker that is also an instruction
/// would put a breakpoint on every one of them, and `*` starts a comment, so
/// every marker would be inside one and none would ever bind.
fn marker_char(cli: &Cli) -> Result<Option<char>, String> {
    if cli.no_markers {
        return Ok(None);
    }
    if "><+-.,[]*".contains(cli.marker) {
        return Err(format!(
            "Error: --marker '{}' is a BrainFuck command or a comment character, \
             so it cannot mark anything",
            cli.marker
        ));
    }
    // Whitespace would put a breakpoint on nearly every instruction of a
    // normally formatted program, so `continue` would become single-stepping;
    // a newline never appears in a line at all, so it would silently do nothing.
    if cli.marker.is_whitespace() || cli.marker.is_control() {
        return Err(format!(
            "Error: --marker {:?} is whitespace or a control character, which \
             cannot mark a position in the source",
            cli.marker
        ));
    }
    Ok(Some(cli.marker))
}

/// Read the source's breakpoint markers, and say what was found.
///
/// On by default. Every BrainFuck implementation ignores every character that
/// is not one of the eight commands, so a marked program runs identically
/// everywhere and a breakpoint becomes something you can commit.
///
/// The count is announced rather than applied quietly: a program you did not
/// write may contain the marker character for its own reasons, and an
/// unexplained stop is worse than an unwanted one. `B` clears them all.
fn apply_markers(marker: Option<char>, session: &mut Session) {
    let Some(marker) = marker else {
        return;
    };

    let program = Arc::clone(&session.program);
    let (positions, unbound) = program.markers(marker);
    // Count what was actually added: a marker can land where `--break` already
    // put one, and claiming it twice would overstate what `B` is about to clear.
    let added = positions
        .iter()
        .filter(|position| session.set_breakpoint(**position))
        .count();

    let mut notes = Vec::new();
    if added > 0 {
        notes.push(format!(
            "{} breakpoint{} from {} markers — B clears them",
            added,
            if added == 1 { "" } else { "s" },
            marker
        ));
    }
    if !unbound.is_empty() {
        let lines: Vec<String> = unbound.iter().map(usize::to_string).collect();
        notes.push(format!(
            "no instruction after the {} on line{} {}",
            marker,
            if unbound.len() == 1 { "" } else { "s" },
            lines.join(", ")
        ));
    }
    if !notes.is_empty() {
        session.note(notes.join("; "), Note::Info);
    }
}

/// Where a runtime error happened, when the error knows.
fn error_location(error: &BfError) -> Option<SourceLocation> {
    match error {
        BfError::MemoryOutOfBounds {
            source_location, ..
        }
        | BfError::StepLimitExceeded {
            source_location, ..
        }
        | BfError::CellOverflow {
            source_location, ..
        }
        | BfError::CellUnderflow {
            source_location, ..
        } => *source_location,
        _ => None,
    }
}

/// The tape length the debugger should show before the first instruction runs.
fn initial_memory_size(cli: &Cli) -> Result<usize, String> {
    match cli.memory_model.to_lowercase().as_str() {
        "fixed" => Ok(cli.memory_size),
        "unbounded" => Ok(cli.unbounded_initial),
        other => Err(format!(
            "Error: Invalid memory model '{other}'. Valid options: fixed, unbounded"
        )),
    }
}

fn read_input(cli: &Cli) -> Result<Vec<u8>, String> {
    match (&cli.input, &cli.input_file) {
        // `--input 1234` means what `echo 1234 |` means. Programs that read a
        // number read until the newline, so without one they stop one byte
        // short and look broken -- and the `i` prompt already appends one, so
        // the two ways of supplying input disagreed.
        (Some(text), _) => {
            let mut bytes = text.as_bytes().to_vec();
            if !bytes.ends_with(b"\n") {
                bytes.push(b'\n');
            }
            Ok(bytes)
        }
        // A file is exact bytes: it is the way to say "no trailing newline".
        (_, Some(path)) => std::fs::read(path)
            .map_err(|error| format!("Error: could not read {}: {error}", path.display())),
        _ => Ok(Vec::new()),
    }
}

/// Build the execution config, optionally with the debugger hook attached.
///
/// A config owns its hooks and is consumed by the interpreter, so restarting
/// means building a fresh one. Everything that has to survive a restart lives
/// in the [`Session`] instead.
fn build_config(cli: &Cli, session: Option<Shared>) -> Result<ExecutionConfig, String> {
    let cell_model: CellModel = cli.cell_model.parse().map_err(|_| {
        format!(
            "Error: Invalid cell model '{}'. Valid options: wrapping, checked",
            cli.cell_model
        )
    })?;
    let eof_behavior: EofBehavior = cli.eof_behavior.parse().map_err(|_| {
        format!(
            "Error: Invalid EOF behavior '{}'. Valid options: zero, neg-one, no-change, error",
            cli.eof_behavior
        )
    })?;

    let builder = ExecutionConfigBuilder::new();
    let builder = match cli.memory_model.to_lowercase().as_str() {
        "fixed" => builder.with_memory_size(cli.memory_size),
        "unbounded" => builder
            .with_unbounded_memory(cli.unbounded_initial, cli.unbounded_max)
            .map_err(|error| format!("Error: {error}"))?,
        other => {
            return Err(format!(
                "Error: Invalid memory model '{other}'. Valid options: fixed, unbounded"
            ));
        }
    };

    let builder = builder
        .with_cell_model(cell_model)
        .with_eof_behavior(eof_behavior);
    let builder = if cli.max_steps > 0 {
        builder.with_max_steps(cli.max_steps)
    } else {
        builder
    };
    let builder = if cli.timeout > 0 {
        builder.with_timeout_ms(cli.timeout)
    } else {
        builder
    };
    let builder = match session {
        Some(session) => builder.with_hook(Box::new(DebuggerHook::new(session))),
        None => builder,
    };

    Ok(builder.build())
}

/// Parse one `--break LINE[:COL]` argument.
///
/// Separate from applying it so a malformed one is caught before the terminal
/// is taken, alongside every other flag.
fn parse_breakpoint(spec: &str) -> Result<(usize, usize), String> {
    let bad_line = || format!("Error: not a line number: {spec:?}");
    match spec.split_once(':') {
        Some((line, column)) => Ok((
            line.parse::<usize>().map_err(|_| bad_line())?,
            column
                .parse::<usize>()
                .map_err(|_| format!("Error: not a column: {spec:?}"))?,
        )),
        None => Ok((spec.parse::<usize>().map_err(|_| bad_line())?, 1)),
    }
}

/// Apply one parsed `--break` position.
fn apply_breakpoint(session: &mut Session, position: (usize, usize)) -> Result<(), String> {
    match session.program.nearest_on_line(position) {
        Some((snapped, _)) => {
            session.set_breakpoint(snapped);
            Ok(())
        }
        None => Err(format!(
            "Error: no instruction on line {} to break at",
            position.0
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(input: Option<&str>, file: Option<&str>) -> Cli {
        Cli {
            file: PathBuf::from("test.bf"),
            memory_size: 30000,
            memory_model: "fixed".into(),
            cell_model: "wrapping".into(),
            unbounded_initial: 1000,
            unbounded_max: 1_000_000,
            eof_behavior: "zero".into(),
            max_steps: 0,
            timeout: 0,
            input: input.map(str::to_owned),
            input_file: file.map(PathBuf::from),
            breakpoints: Vec::new(),
            marker: '@',
            no_markers: false,
            run: false,
            display: "hex".into(),
        }
    }

    #[test]
    fn input_gains_the_newline_a_shell_would_add() {
        // `factor.bf` reads digits until a newline. Without one it stops a byte
        // short of starting, which looks like the debugger ignoring --input.
        assert_eq!(
            read_input(&cli(Some("1234567"), None)).unwrap(),
            b"1234567\n"
        );
    }

    #[test]
    fn a_newline_that_is_already_there_is_not_doubled() {
        assert_eq!(read_input(&cli(Some("hi\n"), None)).unwrap(), b"hi\n");
    }

    #[test]
    fn a_malformed_break_is_rejected_before_anything_else_happens() {
        assert!(parse_breakpoint("abc").is_err());
        assert!(parse_breakpoint("12:x").is_err());
        assert_eq!(parse_breakpoint("12").unwrap(), (12, 1));
        assert_eq!(parse_breakpoint("12:5").unwrap(), (12, 5));
    }

    #[test]
    fn a_marker_that_is_also_an_instruction_is_refused() {
        let mut c = cli(None, None);
        c.marker = '+';
        assert!(marker_char(&c).is_err());
        c.marker = '*';
        assert!(marker_char(&c).is_err());
        c.marker = ' ';
        assert!(marker_char(&c).is_err(), "whitespace is not a marker");
        c.marker = '\n';
        assert!(marker_char(&c).is_err(), "a newline is not a marker");
        c.marker = '@';
        assert_eq!(marker_char(&c).unwrap(), Some('@'));
        c.no_markers = true;
        assert_eq!(marker_char(&c).unwrap(), None);
    }

    #[test]
    fn no_input_flag_queues_nothing() {
        assert!(read_input(&cli(None, None)).unwrap().is_empty());
    }
}
