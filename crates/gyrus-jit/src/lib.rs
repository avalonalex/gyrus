//! Cranelift JIT for gyrus's optimized IR. Spike scope: wrapping cells, fixed
//! memory, no limits; see `PRD/cranelift-jit.md` for the design this is the
//! first step of.
//!
//! The generated function has the signature
//! `fn(rt: *mut Runtime, tape: *mut u8, len: i64, cursor_out: *mut i64) -> i32`
//! and returns 0 on completion or `1 + site` when the site with that index in
//! the translator's side table failed. Every failure is a branch to one shared
//! exit block -- never a Cranelift trap, which under `cranelift-jit` is a
//! SIGILL rather than an error -- so the runtime can rebuild the `BfError` the
//! interpreter would have produced.

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
    MemoryDump, MemorySize, Result,
};
use std::ffi::c_void;

/// What the generated code hands back to the runtime through the callbacks.
struct Runtime<'a> {
    input: &'a mut dyn BfInput,
    output: &'a mut dyn BfOutput,
    eof: EofBehavior,
    /// The first I/O failure, kept for the error the run will end with.
    io_error: Option<(&'static str, std::io::Error)>,
    bytes_read: u64,
    bytes_written: u64,
}

/// `.`: returns 0, or 1 after recording the error in the runtime.
extern "C" fn bf_write(rt: *mut c_void, byte: u32) -> i32 {
    // SAFETY: the generated code passes back the pointer `run` gave it, which
    // outlives the call, and nothing else holds the runtime while it runs.
    let rt = unsafe { &mut *(rt as *mut Runtime<'_>) };
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
    // SAFETY: as for bf_write.
    let rt = unsafe { &mut *(rt as *mut Runtime<'_>) };
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

/// Flags for every tape access. `trusted` is notrap + aligned: the bounds
/// compare that precedes each access is what makes notrap true.
fn access_flags() -> MemFlagsData {
    MemFlagsData::trusted()
}

/// Why a site can fail, and which original instruction it belongs to.
#[derive(Clone, Copy)]
enum Site {
    /// A read or write of the cell under the cursor, off the tape.
    Access { instruction_index: usize },
    /// The I/O callback reported an error; the runtime holds it.
    Io,
}

type Entry = extern "C" fn(*mut c_void, *mut u8, i64, *mut i64) -> i32;

/// Compile `program` and run it on `config`'s tape with the given I/O.
pub fn run(
    program: &OptimizedProgram,
    config: &ExecutionConfig,
    input: &mut dyn BfInput,
    output: &mut dyn BfOutput,
    debug_info: Option<&DebugInfo>,
) -> Result<ExecutionStats> {
    if !matches!(program.cell_model, CellModel::U8Wrapping(_)) {
        return Err(BfError::ConfigurationError {
            message: "the JIT spike supports wrapping cells only".to_string(),
        });
    }
    let (entry, sites, _module) = compile(program)?;

    let mut tape = vec![0u8; config.memory_model().initial_size().get()];
    let mut rt = Runtime {
        input,
        output,
        eof: config.eof_behavior(),
        io_error: None,
        bytes_read: 0,
        bytes_written: 0,
    };
    let mut cursor: i64 = 0;
    // Calling the pointer is safe Rust; what made it sound is the transmute
    // in `compile`, and the tape and cursor slot outliving the call.
    let status = entry(
        &mut rt as *mut Runtime<'_> as *mut c_void,
        tape.as_mut_ptr(),
        tape.len() as i64,
        &mut cursor,
    );
    rt.output.flush().ok();

    if status != 0 {
        let site = sites[(status - 1) as usize];
        return Err(match site {
            Site::Access { instruction_index } => BfError::MemoryOutOfBounds {
                instruction_index: instruction_index.into(),
                attempted: cursor as isize,
                max: MemorySize::new(tape.len() - 1),
                memory_dump: Some(Box::new(MemoryDump::from_memory(
                    &tape,
                    MemoryAddress::new(cursor as isize),
                ))),
                source_location: debug_info.and_then(|d| d.lookup(instruction_index)),
                loop_call_stack: None,
                hint: format!(
                    "Attempted to use cell {cursor}, outside the {}-cell tape. Moving the cursor \
                     there is allowed; reading or writing it is not.",
                    tape.len()
                ),
            },
            Site::Io => {
                let (operation, source) = rt
                    .io_error
                    .take()
                    .unwrap_or_else(|| ("I/O", std::io::Error::other("unknown I/O failure")));
                BfError::IoError {
                    operation: operation.to_string(),
                    instruction_index: None,
                    source,
                }
            }
        });
    }

    Ok(ExecutionStats {
        bytes_read: rt.bytes_read,
        bytes_written: rt.bytes_written,
        memory_allocated: MemorySize::new(tape.len()),
        cells_modified: tape.iter().filter(|&&c| c != 0).count(),
        ..ExecutionStats::default()
    })
}

/// Translate and compile. The module is returned so the code stays mapped.
fn compile(program: &OptimizedProgram) -> Result<(Entry, Vec<Site>, JITModule)> {
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
    let mut module = JITModule::new(jit);
    let ptr = module.target_config().pointer_type();

    let mut write_sig = module.make_signature();
    write_sig.params.push(AbiParam::new(ptr));
    write_sig.params.push(AbiParam::new(types::I32));
    write_sig.returns.push(AbiParam::new(types::I32));
    let write_id = module
        .declare_function("bf_write", Linkage::Import, &write_sig)
        .map_err(|e| cranelift(&e))?;
    let mut read_sig = module.make_signature();
    read_sig.params.push(AbiParam::new(ptr));
    read_sig.returns.push(AbiParam::new(types::I32));
    let read_id = module
        .declare_function("bf_read", Linkage::Import, &read_sig)
        .map_err(|e| cranelift(&e))?;

    let mut ctx = module.make_context();
    for _ in 0..4 {
        ctx.func.signature.params.push(AbiParam::new(ptr));
    }
    ctx.func.signature.returns.push(AbiParam::new(types::I32));
    let main_id = module
        .declare_function("bf_main", Linkage::Export, &ctx.func.signature)
        .map_err(|e| cranelift(&e))?;
    let write_ref = module.declare_func_in_func(write_id, &mut ctx.func);
    let read_ref = module.declare_func_in_func(read_id, &mut ctx.func);

    let frontend_config = module.target_config();
    let mut fb_ctx = FunctionBuilderContext::new();
    let sites = {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let params = b.block_params(entry).to_vec();
        let (rt, base, len, cursor_out) = (params[0], params[1], params[2], params[3]);

        let cursor = b.declare_var(types::I64);
        let zero = b.ins().iconst(types::I64, 0);
        b.def_var(cursor, zero);

        let mut t = Translator {
            b,
            rt,
            base,
            len,
            cursor,
            cursor_out,
            write_ref,
            read_ref,
            sites: Vec::new(),
            exits: Vec::new(),
        };
        t.block(&program.instructions);
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
    // signature `Entry` describes (four pointer-sized params, an i32 result,
    // the ISA's default calling convention, which is the platform C ABI).
    let entry: Entry = unsafe { std::mem::transmute(code) };
    Ok((entry, sites, module))
}

struct Translator<'a> {
    b: FunctionBuilder<'a>,
    rt: Value,
    base: Value,
    len: Value,
    cursor: Variable,
    cursor_out: Value,
    write_ref: FuncRef,
    read_ref: FuncRef,
    sites: Vec<Site>,
    /// Exit blocks created by `fail_block`, filled by `fill_exits` once the
    /// main translation is done: the frontend insists a block be complete
    /// before switching away from it. Each carries the position to report --
    /// the one that was checked, which for a `MultiplyAdd` target is not the
    /// cursor -- and the site number to return.
    exits: Vec<(Block, Value, i64)>,
}

impl Translator<'_> {
    /// Register a failure site and build its exit: record the cursor, return
    /// the site's number. One small block per site, with one predecessor,
    /// rather than one shared exit with thousands: the shared form needed
    /// block parameters from every site and cost 180 ms of register
    /// allocation on hanoi, three times its run time.
    fn fail_block(&mut self, site: Site, position: Value) -> Block {
        self.sites.push(site);
        let block = self.b.create_block();
        self.b.set_cold_block(block);
        self.exits.push((block, position, self.sites.len() as i64));
        block
    }

    /// Emit the body of every exit block: record the position, return the
    /// site. `position` was defined in the block that branches here, its only
    /// predecessor, so it dominates.
    fn fill_exits(&mut self) {
        for (block, position, id) in std::mem::take(&mut self.exits) {
            self.b.switch_to_block(block);
            self.b
                .ins()
                .store(access_flags(), position, self.cursor_out, 0);
            let id = self.b.ins().iconst(types::I32, id);
            self.b.ins().return_(&[id]);
        }
    }

    /// The tape contract, in two instructions: `cursor as usize < len`, and a
    /// branch to the site's exit if not. Returns the cell's address.
    fn checked_addr(&mut self, cursor: Value, instruction_index: usize) -> Value {
        let on_tape = self.b.ins().icmp(IntCC::UnsignedLessThan, cursor, self.len);
        let fail = self.fail_block(Site::Access { instruction_index }, cursor);
        let ok = self.b.create_block();
        self.b.ins().brif(on_tape, ok, &[], fail, &[]);
        self.b.switch_to_block(ok);
        self.b.ins().iadd(self.base, cursor)
    }

    fn load(&mut self, addr: Value) -> Value {
        self.b.ins().load(types::I8, access_flags(), addr, 0)
    }

    fn store(&mut self, value: Value, addr: Value) {
        self.b.ins().store(access_flags(), value, addr, 0);
    }

    fn block(&mut self, instructions: &[OptimizedInstruction]) {
        for instruction in instructions {
            self.instruction(instruction);
        }
    }

    fn instruction(&mut self, instruction: &OptimizedInstruction) {
        use OptimizedInstruction as I;
        let index = instruction.source_range().start;
        match instruction {
            I::Add(n, _) | I::Sub(n, _) => {
                let delta = if matches!(instruction, I::Add(..)) {
                    *n as i64
                } else {
                    -(*n as i64)
                };
                let c = self.b.use_var(self.cursor);
                let addr = self.checked_addr(c, index);
                let v = self.load(addr);
                let v = self.b.ins().iadd_imm_s(v, delta);
                self.store(v, addr);
            }
            I::Right(n, _) | I::Left(n, _) => {
                let delta = if matches!(instruction, I::Right(..)) {
                    *n as i64
                } else {
                    -(*n as i64)
                };
                let c = self.b.use_var(self.cursor);
                let c = self.b.ins().iadd_imm_s(c, delta);
                self.b.def_var(self.cursor, c);
            }
            I::Zero(_) | I::Set(_, _) => {
                let value = if let I::Set(v, _) = instruction {
                    *v
                } else {
                    0
                };
                let c = self.b.use_var(self.cursor);
                let addr = self.checked_addr(c, index);
                let v = self.b.ins().iconst(types::I8, value as i64);
                self.store(v, addr);
            }
            I::Output(_) => {
                let c = self.b.use_var(self.cursor);
                let addr = self.checked_addr(c, index);
                let v = self.load(addr);
                let v = self.b.ins().uextend(types::I32, v);
                let call = self.b.ins().call(self.write_ref, &[self.rt, v]);
                let status = self.b.inst_results(call)[0];
                let fail = self.fail_block(Site::Io, c);
                let ok = self.b.create_block();
                self.b.ins().brif(status, fail, &[], ok, &[]);
                self.b.switch_to_block(ok);
            }
            I::Input(_) => {
                let call = self.b.ins().call(self.read_ref, &[self.rt]);
                let r = self.b.inst_results(call)[0];
                let here = self.b.use_var(self.cursor);
                let fail = self.fail_block(Site::Io, here);
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
                let c = self.b.use_var(self.cursor);
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
                let c = self.b.use_var(self.cursor);
                // The read is inside the loop, so a seek that runs off the
                // tape fails at the read, as the interpreter's does.
                let addr = self.checked_addr(c, index);
                let v = self.load(addr);
                self.b.ins().brif(v, body, &[], after, &[]);
                self.b.switch_to_block(body);
                let c = self.b.use_var(self.cursor);
                let c = self.b.ins().iadd_imm_s(c, delta);
                self.b.def_var(self.cursor, c);
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(after);
            }
            I::MultiplyAdd(targets, _) => {
                let c = self.b.use_var(self.cursor);
                let addr = self.checked_addr(c, index);
                let src = self.load(addr);
                let work = self.b.create_block();
                let after = self.b.create_block();
                self.b.ins().brif(src, work, &[], after, &[]);
                self.b.switch_to_block(work);
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
                let zero = self.b.ins().iconst(types::I8, 0);
                self.store(zero, addr);
                self.b.ins().jump(after, &[]);
                self.b.switch_to_block(after);
            }
            I::Loop(body, _) => {
                let header = self.b.create_block();
                let body_block = self.b.create_block();
                let after = self.b.create_block();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(header);
                let c = self.b.use_var(self.cursor);
                let addr = self.checked_addr(c, index);
                let v = self.load(addr);
                self.b.ins().brif(v, body_block, &[], after, &[]);
                self.b.switch_to_block(body_block);
                self.block(body);
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(after);
            }
        }
    }
}
