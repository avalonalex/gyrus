//! Execution engine for the BrainFuck interpreter.
//!
//! This module contains the core execution logic including:
//! - Single instruction execution
//! - Block execution with loop handling
//! - Pointer manipulation helpers

use super::dispatch::HookDispatcher;
use super::state::{ExecutionFlow, ExecutionResult, VmState};
use crate::config::{EofBehavior, ExecutionConfig};
use crate::debug::DebugInfo;
use crate::error::BfError;
use crate::hooks::HookDecision;
use crate::instruction::Instruction;
use crate::io::{BfInput, BfOutput};
use crate::types::StepCount;
use std::io;

/// Move the cursor one cell right.
///
/// Infallible under the tape contract: a cursor may sit anywhere, on the tape
/// or off it, and only using it can fail. The bound is enforced in
/// `VmState::cell`, at the access.
#[inline]
fn increment_pointer(state: &mut VmState) {
    state.pointer.increment();
}

/// Move the cursor one cell left. Infallible; see [`increment_pointer`].
#[inline]
fn decrement_pointer(state: &mut VmState) {
    state.pointer.decrement();
}

/// Execute a single non-loop instruction
///
/// This function handles the execution of individual BrainFuck instructions,
/// excluding loops which are handled by `execute_block()`.
///
/// This separation provides:
/// - Cleaner code organization (control flow vs execution)
/// - Better testability
/// - Clearer hook integration points (future)
///
/// # Arguments
/// * `instruction` - The instruction to execute
/// * `state` - Mutable VM state
/// * `config` - Execution configuration
/// * `input` - Input source
/// * `output` - Output destination
///
/// # Panics
/// Panics if called with `Instruction::Loop` - loops must be handled by `execute_block()`
#[inline]
pub(super) fn execute_single_instruction<I: BfInput, O: BfOutput>(
    instruction: &Instruction,
    state: &mut VmState,
    config: &ExecutionConfig,
    input: &mut I,
    output: &mut O,
    instruction_index: usize, // Current instruction index for error reporting
    debug_info: Option<&DebugInfo>, // Optional debug info for error messages
) -> ExecutionResult {
    match instruction {
        Instruction::IncrementPointer => {
            increment_pointer(state);
        }

        Instruction::DecrementPointer => {
            decrement_pointer(state);
        }

        // Cell arithmetic: Delegated to CellModel (configurable!)
        //
        // IMPORTANT: Cell arithmetic is configurable via ExecutionConfig.cell_model().
        // Different models provide different overflow/underflow behaviors:
        // - U8Wrapping: 255+1=0, 0-1=255 (default, most compatible)
        // - U8Checked: Overflow/underflow returns error
        //
        // This is INDEPENDENT of MemoryModel, which only controls pointer movement.
        // See config.rs module docs for CellModel and MemoryModel orthogonality.
        // See validator.rs module docs for cell-model-aware validation.
        Instruction::IncrementValue => {
            let steps = state.step_count;
            let cell = state.cell(debug_info, instruction_index)?;
            config
                .cell_model()
                .behavior()
                .try_increment(cell, steps, debug_info)?;
        }

        Instruction::DecrementValue => {
            let steps = state.step_count;
            let cell = state.cell(debug_info, instruction_index)?;
            config
                .cell_model()
                .behavior()
                .try_decrement(cell, steps, debug_info)?;
        }

        Instruction::Output => {
            let byte = *state.cell(debug_info, instruction_index)?;
            output.write_byte(byte).map_err(|source| BfError::IoError {
                operation: "writing output".to_string(),
                instruction_index: Some(state.step_count.into()),
                source,
            })?;
            output.flush().map_err(|source| BfError::IoError {
                operation: "flushing output".to_string(),
                instruction_index: Some(state.step_count.into()),
                source,
            })?;
            // Bytes written tracking moved to StatsTrackerHook
        }

        Instruction::Input => {
            match input.read_byte() {
                Ok(Some(byte)) => {
                    *state.cell(debug_info, instruction_index)? = byte;
                    // Bytes read tracking moved to StatsTrackerHook
                }
                Ok(None) => {
                    // Handle EOF based on configuration
                    match config.eof_behavior() {
                        EofBehavior::SetZero => {
                            *state.cell(debug_info, instruction_index)? = 0;
                        }
                        EofBehavior::SetNegOne => {
                            *state.cell(debug_info, instruction_index)? = 255; // -1 as u8
                        }
                        EofBehavior::NoChange => {
                            // Do nothing, leave cell as-is
                        }
                        EofBehavior::Error => {
                            return Err(BfError::IoError {
                                operation: "reading input (EOF reached)".to_string(),
                                instruction_index: Some(state.step_count.into()),
                                source: io::Error::new(io::ErrorKind::UnexpectedEof, "EOF reached"),
                            });
                        }
                    }
                }
                Err(source) => {
                    return Err(BfError::IoError {
                        operation: "reading input".to_string(),
                        instruction_index: Some(state.step_count.into()),
                        source,
                    });
                }
            }
        }

        Instruction::LoopCheck => {
            // LoopCheck checks the loop condition and exits if cell is zero
            // This is the BrainFuck '[' instruction logic
            // The step counter is already incremented in execute_block before this is called
            // The loop condition reads the cell, so it is an access and the
            // cursor has to be on the tape for it.
            if *state.cell(debug_info, instruction_index)? == 0 {
                return Ok(ExecutionFlow::LoopExit);
            }
            // If cell is non-zero, continue with the rest of the loop body
        }

        Instruction::Loop(_) => {
            panic!("execute_single_instruction() cannot handle loops - use execute_block()");
        }
    }

    Ok(ExecutionFlow::Continue)
}

/// Count the total number of instructions in a block, including nested loops.
/// This is used to compute loop body sizes for LoopInfo.
pub(super) fn count_instructions(instructions: &[Instruction]) -> usize {
    let mut count = 0;
    for instruction in instructions {
        if let Instruction::Loop(body) = instruction {
            count += count_instructions(body); // Recursively count loop body
        } else {
            count += 1; // Count this instruction
        }
    }
    count
}

/// Execute a block of BrainFuck instructions
///
/// This is the main execution engine that handles:
/// - Sequential instruction execution
/// - Loop handling with proper nesting
/// - Hook dispatching at appropriate points
/// - Step counting and state management
///
/// # Arguments
/// * `instructions` - Slice of instructions to execute
/// * `state` - Mutable VM state
/// * `dispatcher` - Hook dispatcher for execution events
/// * `input` - Input source
/// * `output` - Output destination
/// * `start_index` - Flat instruction index where this block starts
/// * `debug_info` - Optional debug information for error messages
///
/// # Returns
/// * `Ok(ExecutionFlow::Continue)` - Block completed normally
/// * `Ok(ExecutionFlow::LoopExit)` - LoopCheck signaled loop exit
/// * `Err(BfError)` - An error occurred during execution
pub(super) fn execute_block<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    state: &mut VmState,
    dispatcher: &mut HookDispatcher,
    input: &mut I,
    output: &mut O,
    start_index: usize,             // flat index where this block starts
    debug_info: Option<&DebugInfo>, // Optional debug info for error messages
) -> ExecutionResult {
    // Helper to check if hook requested execution pause
    let check_pause =
        |decision: HookDecision, step_count: StepCount, context: &str| -> ExecutionResult {
            match decision {
                HookDecision::Break => Err(BfError::ExecutionPaused {
                    instruction_index: step_count.into(),
                    source_location: None,
                    message: Some(format!("Execution paused by hook {}", context)),
                }),
                _ => Ok(ExecutionFlow::Continue),
            }
        };

    let mut local_index = 0;

    for instruction in instructions {
        // Compute current instruction index
        let instruction_index = start_index + local_index;

        // Increment step counter BEFORE calling hooks so hooks see the updated count.
        //
        // Skipped for Loop for the same reason its hooks are skipped below: Loop is
        // an AST container with no source character of its own - the '[' it stands
        // for is executed as the LoopCheck at the head of its body, which counts its
        // own step. Counting here too inflated total_steps by one per loop reached
        // and tripped --max-steps early.
        if !matches!(instruction, Instruction::Loop(_)) {
            state.step_count.increment();
        }

        // Hook: before_instruction
        // Skip before_instruction for Loop since it's just an AST container.
        // The actual '[' instruction (LoopCheck) is what gets executed, not the Loop wrapper.
        // This prevents double-counting and maintains consistency with after_instruction.
        if !matches!(instruction, Instruction::Loop(_)) {
            let decision = dispatcher.dispatch_before(instruction, state, instruction_index);
            check_pause(decision, state.step_count, "at instruction")?;
            if decision == HookDecision::Skip {
                // Skip this instruction, increment local_index and continue
                local_index += 1;
                continue;
            }
        }

        // Execute instruction: delegate to specialized handlers
        match instruction {
            // Loops require special handling for recursion and depth tracking
            Instruction::Loop(body) => {
                // Compute loop body information for hooks
                // The body starts at instruction_index (which is the LoopCheck)
                // With the new parsing model, Loop is just an AST container - LoopCheck IS the '['
                let body_start_index = instruction_index;
                let body_size = count_instructions(body);

                // The loop body always starts with LoopCheck (parser invariant)
                // We execute LoopCheck first to determine if we should enter the loop
                // This avoids calling on_loop_enter when the loop immediately exits
                loop {
                    // Execute LoopCheck instruction (first in body)
                    // We need to manually increment step counter and call hooks since we're bypassing execute_block
                    let loop_check = &body[0];
                    debug_assert!(matches!(loop_check, Instruction::LoopCheck));

                    // Increment step counter (normally done in execute_block)
                    state.step_count.increment();

                    // Hook: after_instruction for LoopCheck (needed for limit checking)
                    let decision = dispatcher.dispatch_after(loop_check, state, body_start_index);
                    check_pause(decision, state.step_count, "at LoopCheck")?;
                    // Skip doesn't make sense for LoopCheck - it's required

                    // Execute LoopCheck to determine if we should continue
                    match execute_single_instruction(
                        loop_check,
                        state,
                        dispatcher.config(),
                        input,
                        output,
                        body_start_index, // LoopCheck is at body_start_index
                        debug_info,
                    )? {
                        ExecutionFlow::Continue => {
                            // Cell is non-zero, enter loop body
                        }
                        ExecutionFlow::LoopExit => {
                            // Cell is zero, exit loop without calling on_loop_enter
                            break;
                        }
                    }

                    // Now we know we're actually entering the loop body
                    state.loop_depth += 1;

                    // Hook: on_loop_enter
                    let decision = dispatcher.dispatch_loop_enter(
                        state,
                        instruction_index,
                        body_start_index,
                        body_size,
                    );
                    if let Err(e) = check_pause(decision, state.step_count, "at loop enter") {
                        state.loop_depth -= 1;
                        return Err(e);
                    }
                    if decision == HookDecision::Skip {
                        // Skip means skip the loop: leave it entirely rather than
                        // re-testing the condition. `continue` would jump back to the
                        // LoopCheck without running the body, so nothing could ever
                        // change the condition cell and the loop would spin forever.
                        state.loop_depth -= 1;
                        break;
                    }

                    // Execute rest of loop body (skip LoopCheck since we already executed it)
                    // body[1..] contains everything after LoopCheck
                    if body.len() > 1 {
                        execute_block(
                            &body[1..],
                            state,
                            dispatcher,
                            input,
                            output,
                            body_start_index + 1, // Skip LoopCheck
                            debug_info,
                        )?;
                        // execute_block returns Continue (LoopExit already handled above)
                    }

                    state.loop_depth -= 1;

                    // Hook: on_loop_exit
                    let decision = dispatcher.dispatch_loop_exit(state, instruction_index);
                    check_pause(decision, state.step_count, "at loop exit")?;
                    // Skip doesn't make sense at loop exit, treat as Continue
                }
            }

            // All other instructions are handled by execute_single_instruction
            non_loop_instruction => {
                match execute_single_instruction(
                    non_loop_instruction,
                    state,
                    dispatcher.config(),
                    input,
                    output,
                    instruction_index,
                    debug_info,
                )? {
                    ExecutionFlow::Continue => {
                        // Normal execution, continue
                    }
                    ExecutionFlow::LoopExit => {
                        // LoopCheck returned LoopExit, propagate it up
                        return Ok(ExecutionFlow::LoopExit);
                    }
                }
            }
        }

        // Hook: after_instruction
        // Skip after_instruction for Loop since it's just an AST container.
        // The actual '[' instruction (LoopCheck) already calls after_instruction
        // from inside the loop handler (line 285), and they share the same instruction_index.
        // Calling it here would cause double-counting in profilers and other hooks.
        if !matches!(instruction, Instruction::Loop(_)) {
            let decision = dispatcher.dispatch_after(instruction, state, instruction_index);
            check_pause(decision, state.step_count, "after instruction")?;
            // Skip doesn't make sense after instruction has already executed
        }

        // Increment local index for next instruction
        // For loops, we need to account for all instructions in the body
        match instruction {
            Instruction::Loop(body) => {
                local_index += count_instructions(body);
            }
            _ => {
                local_index += 1;
            }
        }
    }

    // Block completed normally (no LoopExit encountered)
    Ok(ExecutionFlow::Continue)
}
