//! Hook dispatching for the BrainFuck interpreter.
//!
//! This module centralizes all hook-related logic, making it easier to test
//! hook behavior in isolation and maintain consistent hook dispatch behavior.

use super::state::VmState;
use crate::config::ExecutionConfig;
use crate::hooks::{ExecutionHook, HookContext, HookDecision};
use crate::instruction::Instruction;

/// Handles all hook dispatching for the interpreter.
///
/// This component centralizes hook-related logic, making it easier to:
/// - Test hook behavior in isolation
/// - Add new hook points without modifying execute_block
/// - Maintain consistent hook behavior across the interpreter
///
/// The dispatcher creates HookContext snapshots and calls the appropriate
/// hook methods on the HookManager, returning the HookDecision.
pub(super) struct HookDispatcher<'a> {
    /// The execution config containing user-registered hooks
    config: &'a mut ExecutionConfig,

    /// Built-in hooks (not in Arc<Mutex>!)
    stats_hook: &'a mut crate::hooks::builtin::StatsTrackerHook,
    warning_hook: &'a mut crate::hooks::builtin::WarningCollectorHook,
    debug_hook: Option<&'a mut crate::hooks::builtin::DebugTrackingHook>,
    limit_hook: Option<&'a mut crate::hooks::builtin::LimitEnforcerHook>,
}

impl<'a> HookDispatcher<'a> {
    /// Create a new hook dispatcher with built-in hooks
    #[inline]
    pub fn new(
        config: &'a mut ExecutionConfig,
        stats_hook: &'a mut crate::hooks::builtin::StatsTrackerHook,
        warning_hook: &'a mut crate::hooks::builtin::WarningCollectorHook,
        debug_hook: Option<&'a mut crate::hooks::builtin::DebugTrackingHook>,
        limit_hook: Option<&'a mut crate::hooks::builtin::LimitEnforcerHook>,
    ) -> Self {
        Self {
            config,
            stats_hook,
            warning_hook,
            debug_hook,
            limit_hook,
        }
    }

    /// Get immutable access to the execution config
    ///
    /// This is safe because we only use it when not actively dispatching hooks
    #[inline]
    pub fn config(&self) -> &ExecutionConfig {
        self.config
    }

    /// Dispatch before_instruction hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    pub fn dispatch_before(
        &mut self,
        instruction: &Instruction,
        state: &VmState,
        instruction_index: usize,
    ) -> HookDecision {
        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            instruction_index,
        );

        // Call built-in hooks first (order matters for correctness)
        // Note: Built-in hooks don't use before_instruction currently,
        // but we keep this for consistency and future extensions

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.before_instruction(instruction, &context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch after_instruction hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    pub fn dispatch_after(
        &mut self,
        instruction: &Instruction,
        state: &VmState,
        instruction_index: usize,
    ) -> HookDecision {
        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            instruction_index,
        );

        // Call built-in hooks first (order matters for correctness)

        // 1. Stats tracking (always runs)
        self.stats_hook.after_instruction(instruction, &context);

        // 2. Warning collection (always runs)
        self.warning_hook.after_instruction(instruction, &context);

        // 3. Limit enforcement (check step limits / timeout)
        if let Some(limit_hook) = &mut self.limit_hook
            && limit_hook.after_instruction(instruction, &context) == HookDecision::Break
        {
            return HookDecision::Break;
        }

        // 4. Debug tracking (updates internal state)
        if let Some(debug_hook) = &mut self.debug_hook {
            debug_hook.after_instruction(instruction, &context);
        }

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.after_instruction(instruction, &context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_loop_enter hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    pub fn dispatch_loop_enter(
        &mut self,
        state: &VmState,
        loop_instruction_index: usize,
        body_start_index: usize,
        body_size: usize,
    ) -> HookDecision {
        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(loop_instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            loop_instruction_index,
        );

        let loop_info =
            crate::hooks::LoopInfo::new(loop_instruction_index, body_start_index, body_size);

        // Call built-in hooks first
        self.stats_hook.on_loop_enter(&context, Some(&loop_info));

        if let Some(debug_hook) = &mut self.debug_hook {
            debug_hook.on_loop_enter(&context, Some(&loop_info));
        }

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.on_loop_enter(&context, Some(&loop_info))
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_loop_exit hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    pub fn dispatch_loop_exit(
        &mut self,
        state: &VmState,
        instruction_index: usize,
    ) -> HookDecision {
        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            instruction_index,
        );

        // Call built-in hooks first
        if let Some(debug_hook) = &mut self.debug_hook {
            debug_hook.on_loop_exit(&context);
        }

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.on_loop_exit(&context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_complete hook (called after execution finishes)
    #[inline]
    pub fn dispatch_complete(&mut self, state: &VmState) {
        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            None, // No source location at completion
            state.loop_depth,
            0, // No meaningful instruction index after completion
        );

        // Call built-in hooks first
        self.stats_hook.on_complete(&context);

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.on_complete(&context);
        }
    }
}
