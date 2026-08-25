//! Cranelift JIT for gyrus's optimized IR. See `PRD/cranelift-jit.md`.
//!
//! The generated function has the signature `fn(rt: *mut Runtime) -> i32`. It
//! reads the tape's base and length from the runtime on entry, and again after
//! the runtime has grown the tape; it writes the loop-iteration count and the
//! peak cell touched back before returning. It returns 0 on completion or
//! `1 + site` when the site with that index in the translator's side table
//! failed, having stored the position that failed. Every failure is a branch
//! to a cold exit block -- never a Cranelift trap, which under `cranelift-jit`
//! is a SIGILL rather than an error -- so `run` can rebuild the `BfError` the
//! interpreter would have produced, through the interpreter's own constructors.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AbiParam, Block, FuncRef, InstBuilder, MemFlagsData, Value, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use gyrus::io::{BfInput, BfOutput};
use gyrus::optimizer::{OptimizedInstruction, OptimizedProgram};
use gyrus::{
    BfError, CellModel, DebugInfo, EofBehavior, ExecutionConfig, ExecutionStats, MemoryAddress,
    MemoryModel, MemorySize, Result, StepCount, U8CheckedCells,
};
use std::ffi::c_void;
use std::mem::offset_of;

/// Offsets of the fields the generated code touches. `repr(C)` on `Runtime`
/// is what makes these stable.
const OFF_BASE: i32 = offset_of!(Runtime<'static>, base) as i32;
const OFF_LEN: i32 = offset_of!(Runtime<'static>, len) as i32;
const OFF_CURSOR: i32 = offset_of!(Runtime<'static>, cursor) as i32;
const OFF_ITERATIONS: i32 = offset_of!(Runtime<'static>, iterations) as i32;
const OFF_PEAK: i32 = offset_of!(Runtime<'static>, peak) as i32;

/// How often, in loop iterations, the runtime is asked whether to stop. The
/// interpreter reads the clock at most once per this many steps; a loop
/// iteration is at least one step, so this is never coarser.
const TICK_INTERVAL: u64 = 1024;

/// Everything the generated code and its callbacks share.
///
/// `repr(C)` because the generated code reads and writes the first five
/// fields at offsets taken with `offset_of!`. Everything after them is the
/// runtime's own.
#[repr(C)]
struct Runtime<'a> {
    /// Tape base and length, reloaded by the generated code after `bf_grow`.
    base: *mut u8,
    len: u64,
    /// Written by the generated code: the position that failed, on failure;
    /// the loop iterations run and the highest cell touched, on completion.
    cursor: i64,
    iterations: u64,
    peak: u64,

    tape: Vec<u8>,
    memory_model: MemoryModel,
    input: &'a mut dyn BfInput,
    output: &'a mut dyn BfOutput,
    eof: EofBehavior,
    /// The first I/O failure, kept for the error the run will end with.
    io_error: Option<(&'static str, std::io::Error)>,
    bytes_read: u64,
    bytes_written: u64,
    limits: Limits,
    /// Which limit stopped the run, set by `bf_tick`.
    limit_hit: Option<LimitHit>,
}

struct Limits {
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    start: std::time::Instant,
}

impl Limits {
    fn armed(&self) -> bool {
        self.max_steps.is_some() || self.timeout_ms.is_some()
    }

    /// Iterations until the next tick. A tick happens at the start of an
    /// iteration and sees how many completed before it; with `max - completed`
    /// to go, the next tick is the one at the start of the iteration that
    /// would exceed the budget, and it sees exactly `max` completed. Capped
    /// at the interval so the clock is read often enough.
    fn countdown(&self, completed: u64) -> u64 {
        match self.max_steps {
            Some(max) => max.saturating_sub(completed).clamp(1, TICK_INTERVAL),
            None => TICK_INTERVAL,
        }
    }
}

#[derive(Clone, Copy)]
enum LimitHit {
    Steps,
    Timeout,
}

/// The runtime behind the pointer the generated code carries.
///
/// SAFETY, for every callback: the generated code passes back the pointer
/// `run` gave it, which outlives the call, and nothing else touches the
/// runtime while the generated code runs.
fn runtime<'r>(rt: *mut c_void) -> &'r mut Runtime<'static> {
    unsafe { &mut *(rt as *mut Runtime<'static>) }
}

/// `.`: returns 0, or 1 after recording the error in the runtime.
extern "C" fn bf_write(rt: *mut c_void, byte: u32) -> i32 {
    let rt = runtime(rt);
    match rt.output.write_byte(byte as u8) {
        Ok(()) => {
            rt.bytes_written += 1;
            0
        }
        Err(e) => {
            rt.io_error = Some(("writing output", e));
            1
        }
    }
}

/// `,`: returns the byte read (0..=255), -1 for "leave the cell alone", or -2
/// after recording an error. The EOF behaviour is applied here, so the
/// generated code only ever stores or skips.
extern "C" fn bf_read(rt: *mut c_void) -> i32 {
    let rt = runtime(rt);
    match rt.input.read_byte() {
        Ok(Some(byte)) => {
            rt.bytes_read += 1;
            byte as i32
        }
        Ok(None) => match rt.eof {
            EofBehavior::SetZero => 0,
            EofBehavior::SetNegOne => 255,
            EofBehavior::NoChange => -1,
            EofBehavior::Error => {
                rt.io_error = Some((
                    "reading input (EOF reached)",
                    std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "unexpected EOF on input",
                    ),
                ));
                -2
            }
            // `EofBehavior` is non-exhaustive; a variant this crate does not
            // know is treated as the default, set-to-zero.
            _ => 0,
        },
        Err(e) => {
            rt.io_error = Some(("reading input", e));
            -2
        }
    }
}

/// An access at `cursor` failed the bounds check under the unbounded model:
/// grow the tape to cover it, exactly as the interpreter does, and return 1;
/// or return 0 if no growth can, and the access is the error it looks like.
///
/// Growth may move the tape, which is fine: the generated code holds base and
/// length in variables and reloads both from the runtime after this returns.
extern "C" fn bf_grow(rt: *mut c_void, cursor: i64) -> i32 {
    let rt = runtime(rt);
    let MemoryModel::Unbounded(model) = rt.memory_model else {
        return 0;
    };
    match MemoryAddress::new(cursor as isize).index(model.max_size().get()) {
        Some(idx) => {
            rt.tape.resize(idx + 1, 0);
            rt.base = rt.tape.as_mut_ptr();
            rt.len = rt.tape.len() as u64;
            1
        }
        None => 0,
    }
}

/// Called at the start of a loop iteration when the countdown reaches zero
/// and limits are armed. Records how many iterations completed, decides
/// whether a limit has been hit, and returns the next countdown -- or -1 to
/// stop, with the reason left in the runtime.
extern "C" fn bf_tick(rt: *mut c_void, completed: u64) -> i64 {
    let rt = runtime(rt);
    rt.iterations = completed;
    if let Some(max) = rt.limits.max_steps
        && completed >= max
    {
        rt.limit_hit = Some(LimitHit::Steps);
        return -1;
    }
    if let Some(timeout_ms) = rt.limits.timeout_ms
        && rt.limits.start.elapsed().as_millis() as u64 > timeout_ms
    {
        rt.limit_hit = Some(LimitHit::Timeout);
        return -1;
    }
    rt.limits.countdown(completed) as i64
}

/// Flags for every tape access. `trusted` is notrap + aligned: the bounds
/// compare that precedes each access is what makes notrap true.
fn access_flags() -> MemFlagsData {
    MemFlagsData::trusted()
}

/// Why a site can fail, and which original instruction it belongs to.
///
/// The index is the instruction's position in the parsed program -- the
/// start of its `SourceRange` -- which is what `DebugInfo` maps to a source
/// location. The interpreters report their *step count* in the error's
/// `instruction_index` field instead, because that is all they have at the
/// failing access; the JIT knows the instruction, so it reports that, and
/// with debug info can point at the line and column, which the optimized
/// interpreter never could.
#[derive(Clone, Copy)]
enum Site {
    /// A read or write of a cell that is not on the tape, and cannot be.
    Access { instruction_index: usize },
    /// The I/O callback reported an error; the runtime holds it.
    Io { instruction_index: usize },
    /// Checked cells: an `Add` would pass 255.
    Overflow { instruction_index: usize },
    /// Checked cells: a `Sub` would pass 0.
    Underflow { instruction_index: usize },
    /// `bf_tick` said stop, at this loop; the runtime holds which limit.
    Limit { instruction_index: usize },
}

type Entry = extern "C" fn(*mut c_void) -> i32;

/// How much of `ExecutionStats` to track.
///
/// Every counter the JIT keeps costs something on the paths it runs on:
/// the peak-cell notes and the iteration counter together were 8% of
/// mandelbrot and 20% of hanoi, and `--verbose` is their only reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Statistics {
    /// Track nothing that costs a register or an instruction. What is free
    /// is still reported -- bytes in and out, tape length, cells modified --
    /// and `total_steps`, `loop_iterations` and `peak_memory_used` read as
    /// zero, because they were not counted.
    Cheap,
    /// Track everything, exactly as the interpreter does; the statistics
    /// both engines define alike are then equal.
    Full,
}

/// Compile `program` and run it on `config`'s tape with the given I/O,
/// tracking full statistics. See [`run_with`].
pub fn run(
    program: &OptimizedProgram,
    config: &ExecutionConfig,
    input: &mut dyn BfInput,
    output: &mut dyn BfOutput,
    debug_info: Option<&DebugInfo>,
) -> Result<ExecutionStats> {
    run_with(program, config, input, output, debug_info, Statistics::Full)
}

/// Compile `program` and run it on `config`'s tape with the given I/O.
pub fn run_with(
    program: &OptimizedProgram,
    config: &ExecutionConfig,
    input: &mut dyn BfInput,
    output: &mut dyn BfOutput,
    debug_info: Option<&DebugInfo>,
    statistics: Statistics,
) -> Result<ExecutionStats> {
    // A program is only meaningful under the cell model it was optimized
    // for; the interpreter refuses the mismatch for the same reason.
    if program.cell_model != *config.cell_model() {
        return Err(BfError::ConfigurationError {
            message: format!(
                "program was optimized for {} but is being run with {}. \
                 Build it with optimize_with_cell_model(instructions, config.cell_model()).",
                program.cell_model,
                config.cell_model()
            ),
        });
    }
    let memory_model = *config.memory_model();
    let limits = Limits {
        max_steps: config.max_steps(),
        timeout_ms: config.timeout_ms(),
        start: std::time::Instant::now(),
    };
    let options = Options {
        checked: matches!(program.cell_model, CellModel::U8Checked(_)),
        grows: matches!(memory_model, MemoryModel::Unbounded(_)),
        limited: limits.armed(),
        initial_countdown: limits.countdown(0),
        stats: statistics == Statistics::Full,
    };
    let (entry, sites, _module) = compile(program, options)?;

    let mut tape = vec![0u8; memory_model.initial_size().get()];
    let mut rt = Runtime {
        base: tape.as_mut_ptr(),
        len: tape.len() as u64,
        cursor: 0,
        iterations: 0,
        peak: 0,
        tape,
        memory_model,
        input,
        output,
        eof: config.eof_behavior(),
        io_error: None,
        bytes_read: 0,
        bytes_written: 0,
        limits,
        limit_hit: None,
    };
    // Calling the pointer is safe Rust; what made it sound is the transmute
    // in `compile`, and the runtime outliving the call.
    let status = entry(&mut rt as *mut Runtime<'_> as *mut c_void);
    rt.output.flush().ok();

    if status != 0 {
        let site = sites[(status - 1) as usize];
        return Err(match site {
            // The interpreters' constructors, so no message is written twice.
            Site::Access { instruction_index } => rt.memory_model.access_error(
                MemoryAddress::new(rt.cursor as isize),
                &rt.tape,
                StepCount::new(instruction_index as u64),
                debug_info,
                instruction_index,
            ),
            Site::Io { instruction_index } => {
                let (operation, source) = rt
                    .io_error
                    .take()
                    .unwrap_or_else(|| ("I/O", std::io::Error::other("unknown I/O failure")));
                BfError::IoError {
                    operation: operation.to_string(),
                    instruction_index: Some(instruction_index.into()),
                    source,
                }
            }
            Site::Overflow { instruction_index } => U8CheckedCells::overflow_error(
                StepCount::new(instruction_index as u64),
                debug_info,
                instruction_index,
            ),
            Site::Underflow { instruction_index } => U8CheckedCells::underflow_error(
                StepCount::new(instruction_index as u64),
                debug_info,
                instruction_index,
            ),
            Site::Limit { instruction_index } => limit_error(&rt, debug_info, instruction_index),
        });
    }

    Ok(ExecutionStats {
        // One step per loop iteration: the JIT counts nothing finer. The
        // interpreter's count is in optimized-instruction units and is
        // documented as approximate; this is a different approximation.
        total_steps: StepCount::new(rt.iterations),
        loop_iterations: rt.iterations,
        // The `+ 1` that turns a highest index into a count, as
        // `VmState::peak_cells_used` does.
        peak_memory_used: match statistics {
            Statistics::Full => MemorySize::new(rt.peak as usize + 1),
            Statistics::Cheap => MemorySize::new(0),
        },
        cells_modified: rt.tape.iter().filter(|&&c| c != 0).count(),
        bytes_read: rt.bytes_read,
        bytes_written: rt.bytes_written,
        memory_allocated: MemorySize::new(rt.tape.len()),
        ..ExecutionStats::default()
    })
}

/// The error for the limit `bf_tick` reported.
fn limit_error(
    rt: &Runtime<'_>,
    debug_info: Option<&DebugInfo>,
    instruction_index: usize,
) -> BfError {
    let steps = StepCount::new(rt.iterations);
    match rt.limit_hit {
        Some(LimitHit::Timeout) => {
            let limit_ms = rt.limits.timeout_ms.unwrap_or(0);
            BfError::ExecutionTimeout {
                limit_ms,
                actual_steps: Some(steps),
                hint: format!(
                    "Program exceeded {}ms timeout after {} loop iterations under the JIT. \
                     Try increasing the timeout with --timeout {}.",
                    limit_ms,
                    steps.get(),
                    limit_ms * 2
                ),
            }
        }
        _ => BfError::StepLimitExceeded {
            limit: rt.limits.max_steps.unwrap_or(0),
            actual_steps: steps,
            instruction_index,
            source_location: debug_info.and_then(|d| d.lookup(instruction_index)),
            hint: "The JIT counts one step per loop iteration, and --max-steps bounds that count"
                .to_string(),
        },
    }
}

/// What the translation depends on beyond the program itself.
#[derive(Clone, Copy)]
struct Options {
    /// Checked cells: `Add`/`Sub` exit on overflow/underflow.
    checked: bool,
    /// Unbounded memory: a failed bounds check asks the runtime to grow first.
    grows: bool,
    /// Limits armed: loop iterations count down to a `bf_tick`.
    limited: bool,
    initial_countdown: u64,
    /// Full statistics: note the peak cell and count iterations.
    stats: bool,
}

/// Translate and compile. The module is returned so the code stays mapped.
fn compile(program: &OptimizedProgram, options: Options) -> Result<(Entry, Vec<Site>, JITModule)> {
    let cranelift = |e: &dyn std::fmt::Display| BfError::ConfigurationError {
        message: format!("Cranelift: {e}"),
    };

    // `speed` costs little over `none` at compile time here (36 ms vs 26 ms
    // on mandelbrot) and the default register allocator is the right one:
    // `single_pass` compiled hanoi 10% faster and ran mandelbrot 4x slower.
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").map_err(|e| cranelift(&e))?;
    let isa = cranelift_native::builder()
        .map_err(|e| cranelift(&e))?
        .finish(settings::Flags::new(flags))
        .map_err(|e| cranelift(&e))?;
    let mut jit = JITBuilder::with_isa(isa, default_libcall_names());
    jit.symbol("bf_write", bf_write as *const u8);
    jit.symbol("bf_read", bf_read as *const u8);
    jit.symbol("bf_grow", bf_grow as *const u8);
    jit.symbol("bf_tick", bf_tick as *const u8);
    let mut module = JITModule::new(jit);
    let ptr = module.target_config().pointer_type();

    let mut declare = |name: &str, params: &[types::Type], ret: types::Type| {
        let mut sig = module.make_signature();
        for &p in params {
            sig.params.push(AbiParam::new(p));
        }
        sig.returns.push(AbiParam::new(ret));
        module
            .declare_function(name, Linkage::Import, &sig)
            .map_err(|e| cranelift(&e))
    };
    let write_id = declare("bf_write", &[ptr, types::I32], types::I32)?;
    let read_id = declare("bf_read", &[ptr], types::I32)?;
    let grow_id = declare("bf_grow", &[ptr, types::I64], types::I32)?;
    let tick_id = declare("bf_tick", &[ptr, types::I64], types::I64)?;

    let mut ctx = module.make_context();
    ctx.func.signature.params.push(AbiParam::new(ptr));
    ctx.func.signature.returns.push(AbiParam::new(types::I32));
    let main_id = module
        .declare_function("bf_main", Linkage::Export, &ctx.func.signature)
        .map_err(|e| cranelift(&e))?;
    let calls = Calls {
        write: module.declare_func_in_func(write_id, &mut ctx.func),
        read: module.declare_func_in_func(read_id, &mut ctx.func),
        grow: module.declare_func_in_func(grow_id, &mut ctx.func),
        tick: module.declare_func_in_func(tick_id, &mut ctx.func),
    };

    let frontend_config = module.target_config();
    let mut fb_ctx = FunctionBuilderContext::new();
    let sites = {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let rt = b.block_params(entry)[0];

        let vars = Vars {
            cursor: b.declare_var(types::I64),
            peak: b.declare_var(types::I64),
            iterations: b.declare_var(types::I64),
            countdown: b.declare_var(types::I64),
        };
        let zero = b.ins().iconst(types::I64, 0);
        b.def_var(vars.cursor, zero);
        b.def_var(vars.peak, zero);
        b.def_var(vars.iterations, zero);
        let countdown = b.ins().iconst(types::I64, options.initial_countdown as i64);
        b.def_var(vars.countdown, countdown);
        let tape = if options.grows {
            Tape::Growing {
                base: b.declare_var(ptr),
                len: b.declare_var(types::I64),
            }
        } else {
            let base = b.ins().load(ptr, access_flags(), rt, OFF_BASE);
            let len = b.ins().load(types::I64, access_flags(), rt, OFF_LEN);
            Tape::Fixed { base, len }
        };

        let mut t = Translator {
            b,
            rt,
            ptr,
            vars,
            tape,
            calls,
            options,
            sites: Vec::new(),
            exits: Vec::new(),
        };
        if options.grows {
            t.load_tape();
        }
        t.block(&program.instructions, options.stats);
        t.store_counters();
        let ok = t.b.ins().iconst(types::I32, 0);
        t.b.ins().return_(&[ok]);
        t.fill_exits();

        t.b.seal_all_blocks();
        t.b.finalize(frontend_config);
        t.sites
    };

    module
        .define_function(main_id, &mut ctx)
        .map_err(|e| cranelift(&e))?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().map_err(|e| cranelift(&e))?;
    let code = module.get_finalized_function(main_id);
    // SAFETY: `code` is the entry of a function defined with exactly the
    // signature `Entry` describes (one pointer-sized param, an i32 result,
    // the ISA's default calling convention, which is the platform C ABI).
    let entry: Entry = unsafe { std::mem::transmute(code) };
    Ok((entry, sites, module))
}

/// Straight-line instructions: cell operations and moves.
fn in_run(inst: &OptimizedInstruction) -> bool {
    use OptimizedInstruction as I;
    matches!(
        inst,
        I::Add(..)
            | I::Sub(..)
            | I::Zero(..)
            | I::Set(..)
            | I::Output(..)
            | I::Input(..)
            | I::Right(..)
            | I::Left(..)
    )
}

/// Instructions that read the cell under the cursor before anything else.
fn reads_cursor_first(inst: &OptimizedInstruction) -> bool {
    use OptimizedInstruction as I;
    matches!(
        inst,
        I::Loop(..) | I::SeekRight(..) | I::SeekLeft(..) | I::MultiplyAdd(..)
    )
}

/// What a block does to the cursor, statically: its net movement, if that
/// is fixed (a seek, or a loop that does not come back to where it started,
/// makes it not), and the furthest cell it is certain to touch, relative to
/// where it starts -- counting a nested loop's, seek's or multiply's first
/// read of the cursor, but not what happens inside them, which is theirs to
/// note. `Input` touches its cell only when a byte arrives, and is left out.
fn extent(instructions: &[OptimizedInstruction]) -> (Option<i64>, Option<i64>) {
    use OptimizedInstruction as I;
    let mut offset: i64 = 0;
    let mut furthest: Option<i64> = None;
    let touch = |furthest: &mut Option<i64>, at: i64| {
        *furthest = Some(furthest.map_or(at, |f| f.max(at)));
    };
    for inst in instructions {
        match inst {
            I::Right(n, _) => offset += *n as i64,
            I::Left(n, _) => offset -= *n as i64,
            I::Add(..) | I::Sub(..) | I::Zero(..) | I::Set(..) | I::Output(..) => {
                touch(&mut furthest, offset)
            }
            I::Input(..) => {}
            I::MultiplyAdd(..) => touch(&mut furthest, offset),
            I::Loop(body, _) => {
                touch(&mut furthest, offset);
                if extent(body).0 != Some(0) {
                    return (None, furthest);
                }
            }
            I::SeekRight(..) | I::SeekLeft(..) => {
                touch(&mut furthest, offset);
                return (None, furthest);
            }
        }
    }
    (Some(offset), furthest)
}

/// Where the tape's base and length live during translation.
///
/// Under the fixed model they are two values defined at entry and never
/// changed. Under the unbounded model they change whenever the tape grows,
/// so they are variables the frontend threads through the blocks -- which
/// costs SSA construction at every access site, and is why the fixed model
/// does not pay for it.
#[derive(Clone, Copy)]
enum Tape {
    Fixed { base: Value, len: Value },
    Growing { base: Variable, len: Variable },
}

/// The mutable state of the program, as Cranelift variables: SSA values the
/// frontend threads through blocks for us.
#[derive(Clone, Copy)]
struct Vars {
    cursor: Variable,
    peak: Variable,
    iterations: Variable,
    countdown: Variable,
}

#[derive(Clone, Copy)]
struct Calls {
    write: FuncRef,
    read: FuncRef,
    grow: FuncRef,
    tick: FuncRef,
}

struct Translator<'a> {
    b: FunctionBuilder<'a>,
    rt: Value,
    ptr: types::Type,
    vars: Vars,
    tape: Tape,
    calls: Calls,
    options: Options,
    sites: Vec<Site>,
    /// Exit blocks created by `fail_block`, filled by `fill_exits` once the
    /// main translation is done: the frontend insists a block be complete
    /// before switching away from it. Each carries the position to report --
    /// the one that was checked, which for a `MultiplyAdd` target is not the
    /// cursor -- and the site number to return.
    exits: Vec<(Block, Value, i64)>,
}

impl Translator<'_> {
    /// (Re)load the tape's base and length from the runtime, under the
    /// unbounded model, where they can change.
    fn load_tape(&mut self) {
        let Tape::Growing { base, len } = self.tape else {
            unreachable!("a fixed tape is loaded once, at entry");
        };
        let b = self
            .b
            .ins()
            .load(self.ptr, access_flags(), self.rt, OFF_BASE);
        let l = self
            .b
            .ins()
            .load(types::I64, access_flags(), self.rt, OFF_LEN);
        self.b.def_var(base, b);
        self.b.def_var(len, l);
    }

    fn tape_base(&mut self) -> Value {
        match self.tape {
            Tape::Fixed { base, .. } => base,
            Tape::Growing { base, .. } => self.b.use_var(base),
        }
    }

    fn tape_len(&mut self) -> Value {
        match self.tape {
            Tape::Fixed { len, .. } => len,
            Tape::Growing { len, .. } => self.b.use_var(len),
        }
    }

    /// Write the counters back to the runtime: the success path's epilogue.
    /// Failure paths do not need it -- an error returns no statistics, and
    /// the limit error takes its count from what `bf_tick` recorded -- and
    /// with thousands of exit blocks, three stores each was a third of
    /// hanoi's compile time.
    fn store_counters(&mut self) {
        let iterations = self.b.use_var(self.vars.iterations);
        self.b
            .ins()
            .store(access_flags(), iterations, self.rt, OFF_ITERATIONS);
        let peak = self.b.use_var(self.vars.peak);
        self.b.ins().store(access_flags(), peak, self.rt, OFF_PEAK);
    }

    /// Register a failure site and its exit: one small cold block per site,
    /// with one predecessor. One shared exit fed by thousands of sites
    /// through block parameters cost 180 ms of register allocation on hanoi
    /// and, by not being cold, half of mandelbrot's run time.
    fn fail_block(&mut self, site: Site, position: Value) -> Block {
        self.sites.push(site);
        let block = self.b.create_block();
        self.b.set_cold_block(block);
        self.exits.push((block, position, self.sites.len() as i64));
        block
    }

    /// Emit the body of every exit block. `position` was defined in the
    /// block that branches here, its only predecessor, so it dominates.
    fn fill_exits(&mut self) {
        for (block, position, id) in std::mem::take(&mut self.exits) {
            self.b.switch_to_block(block);
            self.b
                .ins()
                .store(access_flags(), position, self.rt, OFF_CURSOR);
            let id = self.b.ins().iconst(types::I32, id);
            self.b.ins().return_(&[id]);
        }
    }

    /// The tape contract: `cursor as usize < len`, else the site's exit --
    /// after, under the unbounded model, one attempt to grow the tape to
    /// cover the cell, which is what the interpreter's access does. Returns
    /// the cell's address. The peak is not noted here; see `note_peak`.
    fn checked_addr(&mut self, cursor: Value, instruction_index: usize) -> Value {
        // Under the unbounded model the check is re-run after growth, so it
        // needs a block of its own to jump back to.
        let check = if self.options.grows {
            let check = self.b.create_block();
            self.b.ins().jump(check, &[]);
            self.b.switch_to_block(check);
            check
        } else {
            self.b.current_block().expect("translating inside a block")
        };
        let len = self.tape_len();
        let on_tape = self.b.ins().icmp(IntCC::UnsignedLessThan, cursor, len);
        let ok = self.b.create_block();
        let fail = self.fail_block(Site::Access { instruction_index }, cursor);
        if self.options.grows {
            // The cold path first asks the runtime to grow; if it could, the
            // tape may have moved, so reload base and length and re-check.
            let grow = self.b.create_block();
            self.b.set_cold_block(grow);
            self.b.ins().brif(on_tape, ok, &[], grow, &[]);
            self.b.switch_to_block(grow);
            let call = self.b.ins().call(self.calls.grow, &[self.rt, cursor]);
            let grew = self.b.inst_results(call)[0];
            let reload = self.b.create_block();
            self.b.ins().brif(grew, reload, &[], fail, &[]);
            self.b.switch_to_block(reload);
            self.load_tape();
            self.b.ins().jump(check, &[]);
        } else {
            self.b.ins().brif(on_tape, ok, &[], fail, &[]);
        }
        self.b.switch_to_block(ok);
        let base = self.tape_base();
        self.b.ins().iadd(base, cursor)
    }

    fn load(&mut self, addr: Value) -> Value {
        self.b.ins().load(types::I8, access_flags(), addr, 0)
    }

    fn store(&mut self, value: Value, addr: Value) {
        self.b.ins().store(access_flags(), value, addr, 0);
    }

    /// Note that the cell at `cursor + offset` is (about to be) touched.
    /// `umax` is a conditional move, but even that was 12% of mandelbrot
    /// when done per access and 9% when done per straight-line run, so it
    /// is done once per loop execution where the body is balanced (see
    /// `Loop`) and once per run elsewhere (see `block`).
    fn note_peak(&mut self, cursor: Value, offset: i64) {
        if !self.options.stats {
            return;
        }
        let position = self.b.ins().iadd_imm_s(cursor, offset);
        let peak = self.b.use_var(self.vars.peak);
        let peak = self.b.ins().umax(peak, position);
        self.b.def_var(self.vars.peak, peak);
    }

    /// Translate a block, one straight-line run at a time.
    ///
    /// A run is a stretch of cell operations and moves with no branch in it,
    /// so either every access in it happens or one of them fails and the run
    /// returns no statistics. The furthest cell it touches is therefore
    /// known statically, and with `note_runs` the peak is noted once at the
    /// run's start instead of once per access. A loop, seek or multiply that
    /// follows the run reads the cell the run ends on before doing anything
    /// else, so that position counts too. Inside a balanced loop body the
    /// loop notes all of this itself, once, and `note_runs` is false.
    fn block(&mut self, instructions: &[OptimizedInstruction], note_runs: bool) {
        let mut i = 0;
        while i < instructions.len() {
            if !in_run(&instructions[i]) {
                self.instruction(&instructions[i]);
                i += 1;
                continue;
            }
            let end = instructions[i..]
                .iter()
                .position(|inst| !in_run(inst))
                .map_or(instructions.len(), |k| i + k);
            if note_runs {
                let (net, mut furthest) = extent(&instructions[i..end]);
                if end < instructions.len() && reads_cursor_first(&instructions[end]) {
                    let net = net.expect("a run has no seeks or loops");
                    furthest = Some(furthest.map_or(net, |f| f.max(net)));
                }
                if let Some(furthest) = furthest {
                    let c = self.b.use_var(self.vars.cursor);
                    self.note_peak(c, furthest);
                }
            }
            for inst in &instructions[i..end] {
                self.instruction(inst);
            }
            i = end;
        }
    }

    /// `Add`/`Sub` on the cell at `addr`: wrapping, or checked with an exit.
    fn arith(&mut self, addr: Value, n: u8, add: bool, index: usize) {
        let v = self.load(addr);
        if !self.options.checked {
            let delta = if add { n as i64 } else { -(n as i64) };
            let v = self.b.ins().iadd_imm_s(v, delta);
            self.store(v, addr);
            return;
        }
        // Widen, so that crossing 255 or 0 is visible. The unfused program
        // would have stepped its way to the boundary before failing, which is
        // why the error reports the boundary value, not this cell's.
        let v = self.b.ins().uextend(types::I32, v);
        let (result, bad, site) = if add {
            let sum = self.b.ins().iadd_imm_s(v, n as i64);
            let over = self
                .b
                .ins()
                .icmp_imm_s(IntCC::UnsignedGreaterThan, sum, 255);
            (
                sum,
                over,
                Site::Overflow {
                    instruction_index: index,
                },
            )
        } else {
            let under = self
                .b
                .ins()
                .icmp_imm_s(IntCC::UnsignedLessThan, v, n as i64);
            let diff = self.b.ins().iadd_imm_s(v, -(n as i64));
            (
                diff,
                under,
                Site::Underflow {
                    instruction_index: index,
                },
            )
        };
        let here = self.b.use_var(self.vars.cursor);
        let fail = self.fail_block(site, here);
        let ok = self.b.create_block();
        self.b.ins().brif(bad, fail, &[], ok, &[]);
        self.b.switch_to_block(ok);
        let result = self.b.ins().ireduce(types::I8, result);
        self.store(result, addr);
    }

    /// The start of a loop body, after the condition has passed: with limits
    /// armed, count down to the next `bf_tick` -- which sees the iterations
    /// completed so far, so a loop that ends exactly at its budget completes,
    /// as it does in the interpreter -- then count this iteration.
    fn body_entry(&mut self, index: usize) {
        let completed = self.b.use_var(self.vars.iterations);
        if self.options.limited {
            let countdown = self.b.use_var(self.vars.countdown);
            let countdown = self.b.ins().iadd_imm_s(countdown, -1);
            self.b.def_var(self.vars.countdown, countdown);
            let tick = self.b.create_block();
            self.b.set_cold_block(tick);
            let go = self.b.create_block();
            self.b.ins().brif(countdown, go, &[], tick, &[]);
            self.b.switch_to_block(tick);
            let call = self.b.ins().call(self.calls.tick, &[self.rt, completed]);
            let next = self.b.inst_results(call)[0];
            let stop = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, next, 0);
            let here = self.b.use_var(self.vars.cursor);
            let fail = self.fail_block(
                Site::Limit {
                    instruction_index: index,
                },
                here,
            );
            let resume = self.b.create_block();
            self.b.ins().brif(stop, fail, &[], resume, &[]);
            self.b.switch_to_block(resume);
            self.b.def_var(self.vars.countdown, next);
            self.b.ins().jump(go, &[]);
            self.b.switch_to_block(go);
        }
        // The counter is a statistic, and the tick's input: with neither
        // wanted it is not kept.
        if !self.options.stats && !self.options.limited {
            return;
        }
        let iterations = self.b.ins().iadd_imm_s(completed, 1);
        self.b.def_var(self.vars.iterations, iterations);
    }

    /// A loop header: read the cell under the cursor and branch on it.
    fn loop_test(&mut self, index: usize, nonzero: Block, zero: Block) {
        let c = self.b.use_var(self.vars.cursor);
        let addr = self.checked_addr(c, index);
        let v = self.load(addr);
        self.b.ins().brif(v, nonzero, &[], zero, &[]);
    }

    fn instruction(&mut self, instruction: &OptimizedInstruction) {
        use OptimizedInstruction as I;
        let index = instruction.source_range().start;
        match instruction {
            I::Add(n, _) | I::Sub(n, _) => {
                let c = self.b.use_var(self.vars.cursor);
                let addr = self.checked_addr(c, index);
                self.arith(addr, *n, matches!(instruction, I::Add(..)), index);
            }
            I::Right(n, _) | I::Left(n, _) => {
                let delta = if matches!(instruction, I::Right(..)) {
                    *n as i64
                } else {
                    -(*n as i64)
                };
                let c = self.b.use_var(self.vars.cursor);
                let c = self.b.ins().iadd_imm_s(c, delta);
                self.b.def_var(self.vars.cursor, c);
            }
            I::Zero(_) | I::Set(_, _) => {
                // A `Set` is only built where its arithmetic could not have
                // faulted, so it is a plain store under either cell model.
                let value = if let I::Set(v, _) = instruction {
                    *v
                } else {
                    0
                };
                let c = self.b.use_var(self.vars.cursor);
                let addr = self.checked_addr(c, index);
                let v = self.b.ins().iconst(types::I8, value as i64);
                self.store(v, addr);
            }
            I::Output(_) => {
                let c = self.b.use_var(self.vars.cursor);
                let addr = self.checked_addr(c, index);
                let v = self.load(addr);
                let v = self.b.ins().uextend(types::I32, v);
                let call = self.b.ins().call(self.calls.write, &[self.rt, v]);
                let status = self.b.inst_results(call)[0];
                let fail = self.fail_block(
                    Site::Io {
                        instruction_index: index,
                    },
                    c,
                );
                let ok = self.b.create_block();
                self.b.ins().brif(status, fail, &[], ok, &[]);
                self.b.switch_to_block(ok);
            }
            I::Input(_) => {
                let call = self.b.ins().call(self.calls.read, &[self.rt]);
                let r = self.b.inst_results(call)[0];
                let here = self.b.use_var(self.vars.cursor);
                let fail = self.fail_block(
                    Site::Io {
                        instruction_index: index,
                    },
                    here,
                );
                let failed = self.b.ins().icmp_imm_s(IntCC::Equal, r, -2);
                let ok = self.b.create_block();
                self.b.ins().brif(failed, fail, &[], ok, &[]);
                self.b.switch_to_block(ok);
                let is_byte = self
                    .b
                    .ins()
                    .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, r, 0);
                let store = self.b.create_block();
                let done = self.b.create_block();
                self.b.ins().brif(is_byte, store, &[], done, &[]);
                self.b.switch_to_block(store);
                // Touched only now, so noted only now.
                let c = self.b.use_var(self.vars.cursor);
                self.note_peak(c, 0);
                let addr = self.checked_addr(c, index);
                let byte = self.b.ins().ireduce(types::I8, r);
                self.store(byte, addr);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
            }
            I::SeekRight(stride, _) | I::SeekLeft(stride, _) => {
                let delta = if matches!(instruction, I::SeekRight(..)) {
                    *stride as i64
                } else {
                    -(*stride as i64)
                };
                let header = self.b.create_block();
                let body = self.b.create_block();
                let after = self.b.create_block();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(header);
                // The read is inside the loop, so a seek that runs off the
                // tape fails at the read, as the interpreter's does.
                self.loop_test(index, body, after);
                self.b.switch_to_block(body);
                let c = self.b.use_var(self.vars.cursor);
                let c = self.b.ins().iadd_imm_s(c, delta);
                self.b.def_var(self.vars.cursor, c);
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(after);
                // The cell a right seek stopped on is the furthest it read; a
                // left seek's furthest is where it started, which whatever
                // came before it has noted.
                if delta > 0 {
                    let c = self.b.use_var(self.vars.cursor);
                    self.note_peak(c, 0);
                }
            }
            I::MultiplyAdd(targets, _) => {
                let c = self.b.use_var(self.vars.cursor);
                let addr = self.checked_addr(c, index);
                let src = self.load(addr);
                let work = self.b.create_block();
                let after = self.b.create_block();
                self.b.ins().brif(src, work, &[], after, &[]);
                self.b.switch_to_block(work);
                // The targets are touched only when the source is non-zero,
                // so they are noted here, not by the enclosing run.
                if let Some(furthest) = targets.iter().map(|(offset, _)| *offset as i64).max()
                    && furthest > 0
                {
                    self.note_peak(c, furthest);
                }
                let src32 = self.b.ins().uextend(types::I32, src);
                for (offset, multiplier) in targets {
                    let tc = self.b.ins().iadd_imm_s(c, *offset as i64);
                    let taddr = self.checked_addr(tc, index);
                    let t = self.load(taddr);
                    let t32 = self.b.ins().uextend(types::I32, t);
                    let product = self.b.ins().imul_imm_s(src32, *multiplier as i64);
                    let sum = self.b.ins().iadd(t32, product);
                    let sum8 = self.b.ins().ireduce(types::I8, sum);
                    self.store(sum8, taddr);
                }
                // The source's address may be stale if a target's check grew
                // the tape; recompute it from the reloaded base.
                let base = self.tape_base();
                let addr = self.b.ins().iadd(base, c);
                let zero = self.b.ins().iconst(types::I8, 0);
                self.store(zero, addr);
                self.b.ins().jump(after, &[]);
                self.b.switch_to_block(after);
            }
            I::Loop(body, _) => {
                // A balanced body touches the same cells every iteration, so
                // its furthest cell is noted once, at the exit, if the loop
                // ran at all -- which the iteration counter tells: a few
                // instructions per loop execution, no new blocks, and nothing
                // on the iteration path, where a note cost 9% of mandelbrot.
                let (net, furthest) = extent(body);
                let balanced = net == Some(0);
                let note = match (self.options.stats, balanced, furthest) {
                    (true, true, Some(furthest)) => {
                        Some((furthest, self.b.use_var(self.vars.iterations)))
                    }
                    _ => None,
                };
                let header = self.b.create_block();
                let body_block = self.b.create_block();
                let after = self.b.create_block();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(header);
                self.loop_test(index, body_block, after);
                self.b.switch_to_block(body_block);
                self.body_entry(index);
                self.block(body, self.options.stats && !balanced);
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(after);
                if let Some((furthest, entered_at)) = note {
                    let now = self.b.use_var(self.vars.iterations);
                    let ran = self.b.ins().icmp(IntCC::NotEqual, now, entered_at);
                    let c = self.b.use_var(self.vars.cursor);
                    let position = self.b.ins().iadd_imm_s(c, furthest);
                    let peak = self.b.use_var(self.vars.peak);
                    let candidate = self.b.ins().select(ran, position, peak);
                    let peak = self.b.ins().umax(peak, candidate);
                    self.b.def_var(self.vars.peak, peak);
                }
            }
        }
    }
}
