//! Running a lesson's snippet and keeping every step of it.
//!
//! The debugger stops a live interpreter because a real program's tape is too
//! big to keep a copy of per instruction. A lesson's tape is a few dozen cells
//! and its programs run for a few hundred steps, so the tutorial does the
//! opposite: run the whole thing, record every step, and let the learner move
//! back and forth through it instantly. Stepping backwards is the thing that
//! makes `[->+<]` legible, and replaying is the cheapest way to get it.

use std::sync::{Arc, Mutex};

use gyrus::hooks::{ExecutionHook, HookContext, HookDecision};
use gyrus::io::{BfOutput, StringIo};
use gyrus::{
    BfError, ExecutionConfigBuilder, Instruction, SourceLocation, interpret_with_io,
    parse_with_debug,
};

/// The interpreter's state just before one instruction ran.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Where in the source that instruction is.
    pub location: Option<SourceLocation>,
    /// The tape at that moment.
    pub memory: Vec<u8>,
    /// The cursor position. Signed: off the tape is a legal place to be.
    pub pointer: isize,
    /// How much of the output had been written by then.
    pub output_len: usize,
    /// Loop nesting depth.
    pub loop_depth: usize,
}

/// How a recorded run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// The program finished on its own.
    Finished,
    /// It hit the recorder's step cap, which is what an endless loop looks like.
    TooManySteps(usize),
    /// It stopped on an error.
    Failed(String),
}

/// Everything a run of a lesson snippet produced.
#[derive(Debug, Clone)]
pub struct Trace {
    /// One entry per instruction executed, in order.
    pub frames: Vec<Frame>,
    /// The tape after the last instruction.
    pub memory: Vec<u8>,
    /// The pointer after the last instruction.
    pub pointer: isize,
    /// Everything the program wrote.
    pub output: Vec<u8>,
    /// How it ended.
    pub ending: Ending,
}

impl Trace {
    /// The frame at `step`, or the final state when `step` is past the end.
    pub fn frame(&self, step: usize) -> Frame {
        self.frames.get(step).cloned().unwrap_or(Frame {
            location: None,
            memory: self.memory.clone(),
            pointer: self.pointer,
            output_len: self.output.len(),
            loop_depth: 0,
        })
    }

    /// Cells that changed between `step - 1` and `step`.
    pub fn changed_at(&self, step: usize) -> std::collections::HashSet<usize> {
        let Some(previous) = step.checked_sub(1).and_then(|s| self.frames.get(s)) else {
            return std::collections::HashSet::new();
        };
        let current = self.frame(step);
        previous
            .memory
            .iter()
            .zip(current.memory.iter())
            .enumerate()
            .filter(|(_, (before, after))| before != after)
            .map(|(address, _)| address)
            .collect()
    }

    /// The number of scrub positions: one per instruction, plus the end state.
    pub fn positions(&self) -> usize {
        self.frames.len() + 1
    }
}

type SharedOutput = Arc<Mutex<Vec<u8>>>;

/// What the recorder collects. Shared with the caller because the hook itself
/// is moved into the execution config, which the interpreter consumes.
#[derive(Debug, Default)]
struct Recording {
    frames: Vec<Frame>,
    /// Set when the step cap stopped the run, which is what a program that
    /// never terminates looks like from out here.
    exhausted: bool,
    /// The tape after the last instruction, when the program got that far.
    final_state: Option<(Vec<u8>, isize)>,
}

struct Recorder {
    shared: Arc<Mutex<Recording>>,
    output: SharedOutput,
    limit: usize,
}

impl Recorder {
    fn record(&mut self, context: &HookContext) -> HookDecision {
        let output_len = self.output.lock().expect("output poisoned").len();
        let mut recording = self.shared.lock().expect("recording poisoned");
        if recording.frames.len() >= self.limit {
            recording.exhausted = true;
            return HookDecision::Break;
        }
        recording.frames.push(Frame {
            location: context.source_location().copied(),
            memory: context.memory().to_vec(),
            pointer: context.pointer().0,
            output_len,
            loop_depth: context.loop_depth(),
        });
        HookDecision::Continue
    }
}

impl ExecutionHook for Recorder {
    fn before_instruction(
        &mut self,
        _instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        self.record(context)
    }

    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        // The interpreter runs the `LoopCheck` standing for `[` itself and
        // dispatches only this hook point for it -- before the check executes.
        // Without this, `[` would be the one instruction the tutorial never
        // shows a learner stopping on, which is the instruction they most need
        // to stop on.
        if matches!(instruction, Instruction::LoopCheck) {
            self.record(context)
        } else {
            HookDecision::Continue
        }
    }

    fn on_complete(&mut self, context: &HookContext) {
        // Frames are states *before* each instruction, so the tape as the
        // program left it is not among them -- and that is the one the lesson
        // check looks at.
        self.shared.lock().expect("recording poisoned").final_state =
            Some((context.memory().to_vec(), context.pointer().0));
    }
}

struct Collect {
    output: SharedOutput,
}

impl BfOutput for Collect {
    fn write_byte(&mut self, byte: u8) -> std::io::Result<()> {
        self.output.lock().expect("output poisoned").push(byte);
        Ok(())
    }
}

/// Run `source` on a `cells`-cell tape, recording at most `limit` steps.
///
/// Parse errors come back as `Err`; anything that goes wrong at run time is an
/// [`Ending`] on an otherwise usable trace, because the steps leading up to a
/// failure are exactly what a learner needs to see.
pub fn record(source: &str, input: &str, cells: usize, limit: usize) -> Result<Trace, BfError> {
    let (instructions, debug) = parse_with_debug(source)?;

    let output: SharedOutput = Arc::new(Mutex::new(Vec::new()));
    let shared: Arc<Mutex<Recording>> = Arc::new(Mutex::new(Recording::default()));

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(cells)
        .with_hook(Box::new(Recorder {
            shared: Arc::clone(&shared),
            output: Arc::clone(&output),
            limit,
        }))
        .build();

    // `StringIo` is already the input half of this; only the output side needs
    // a custom adapter, because the recorder reads the length mid-run.
    let mut feed = StringIo::new(input);
    let mut collect = Collect {
        output: Arc::clone(&output),
    };

    let result = interpret_with_io(&instructions, config, &mut feed, &mut collect, Some(&debug));

    let recording = std::mem::take(&mut *shared.lock().expect("recording poisoned"));
    let ending = match &result {
        Ok(_) => Ending::Finished,
        Err(_) if recording.exhausted => Ending::TooManySteps(limit),
        Err(error) => Ending::Failed(error.format_detailed()),
    };

    // Without a clean finish there is no "after" state, so the last recorded
    // step is as far as the tape got.
    let (memory, pointer) = recording.final_state.clone().unwrap_or_else(|| {
        recording
            .frames
            .last()
            .map(|frame| (frame.memory.clone(), frame.pointer))
            .unwrap_or_else(|| (vec![0; cells], 0))
    });

    Ok(Trace {
        frames: recording.frames,
        memory,
        pointer,
        output: output.lock().expect("output poisoned").clone(),
        ending,
    })
}
