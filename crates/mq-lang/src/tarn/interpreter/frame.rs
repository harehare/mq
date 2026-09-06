//! Per-execution frame state: deadline/call-depth tracking and the `Locals`/operand-stack
//! pools that let repeated calls reuse allocations instead of hitting the allocator.
use super::errors::{VmError, VmResult};
use crate::runtime::env::Env;
use crate::runtime::host::HostFunctions;
use crate::tarn::value::{Locals, StackValue};
use crate::{Shared, SharedCell};
use std::time::{Duration, Instant};

/// Instructions between deadline checks.
const TIMEOUT_CHECK_INTERVAL: u32 = 1024;

/// Per-execution deadline and call-depth state.
pub(super) struct ExecutionLimits {
    deadline: Option<Instant>,
    timeout: Option<Duration>,
    step: u32,
    call_depth: u32,
    max_call_stack_depth: u32,
    pools: ExecutionPools,
}

/// Reusable non-capturing frame storage.
#[derive(Default)]
pub(crate) struct ExecutionPools {
    local_pool: Vec<Vec<Locals>>,
    stack_pool: Vec<Vec<StackValue>>,
}

const MAX_POOLED_LOCAL_COUNT: usize = 256;

impl ExecutionLimits {
    pub(super) fn new(timeout: Option<Duration>, max_call_stack_depth: u32, pools: ExecutionPools) -> Self {
        Self {
            deadline: timeout.map(|t| Instant::now() + t),
            timeout,
            step: 0,
            call_depth: 0,
            max_call_stack_depth,
            pools,
        }
    }

    pub(super) fn into_pools(self) -> ExecutionPools {
        self.pools
    }

    #[inline(always)]
    pub(super) fn check(&mut self) -> VmResult<()> {
        let Some(deadline) = self.deadline else {
            return Ok(());
        };
        self.step = self.step.wrapping_add(1);
        if self.step & (TIMEOUT_CHECK_INTERVAL - 1) != 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            Err(VmError::Timeout(self.timeout.unwrap_or_default()))
        } else {
            Ok(())
        }
    }

    #[inline(always)]
    pub(super) fn has_deadline(&self) -> bool {
        self.deadline.is_some()
    }

    #[inline(always)]
    pub(super) fn enter_call(&mut self) -> VmResult<()> {
        if self.call_depth >= self.max_call_stack_depth {
            return Err(VmError::RecursionError(self.max_call_stack_depth));
        }
        self.call_depth += 1;
        Ok(())
    }

    #[inline(always)]
    pub(super) fn exit_call(&mut self) {
        self.call_depth = self.call_depth.saturating_sub(1);
    }

    pub(super) fn take_locals(&mut self, count: u16, captures: bool) -> Locals {
        self.take_locals_with_initialized_prefix(count, 0, captures)
    }

    pub(super) fn take_locals_with_initialized_prefix(
        &mut self,
        count: u16,
        initialized: usize,
        captures: bool,
    ) -> Locals {
        let locals = if captures {
            None
        } else {
            self.pools
                .local_pool
                .get_mut(count as usize)
                .and_then(|bucket| bucket.pop())
        };
        let locals = locals.unwrap_or_else(|| fresh_locals(count as usize, captures));
        locals.reset_from(initialized.min(count as usize));
        locals
    }

    pub(super) fn recycle_locals(&mut self, locals: Locals) {
        const MAX_RETAINED_PER_LENGTH: usize = 8;
        let count = locals.len();
        if count >= MAX_POOLED_LOCAL_COUNT {
            return;
        }
        if count >= self.pools.local_pool.len() {
            self.pools.local_pool.resize_with(count + 1, Vec::new);
        }
        let bucket = &mut self.pools.local_pool[count];
        if bucket.len() < MAX_RETAINED_PER_LENGTH {
            bucket.push(locals);
        }
    }

    pub(super) fn take_stack(&mut self) -> Vec<StackValue> {
        self.pools.stack_pool.pop().unwrap_or_else(|| Vec::with_capacity(8))
    }

    pub(super) fn recycle_stack(&mut self, mut stack: Vec<StackValue>) {
        const MAX_RETAINED_FRAMES: usize = 32;
        stack.clear();
        if self.pools.stack_pool.len() < MAX_RETAINED_FRAMES {
            self.pools.stack_pool.push(stack);
        }
    }
}

fn fresh_locals(count: usize, captures: bool) -> Locals {
    if captures {
        Locals::boxed(count)
    } else {
        Locals::flat(count)
    }
}

/// Mutable services shared by all frames of one VM evaluation.
pub(super) struct ExecutionContext<'a> {
    pub(super) env: &'a Shared<SharedCell<Env>>,
    pub(super) limits: &'a mut ExecutionLimits,
    pub(super) host_functions: &'a HostFunctions,
}
