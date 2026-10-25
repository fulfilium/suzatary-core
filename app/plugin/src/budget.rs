//! What one call may spend: wall-clock time and linear memory.

use std::time::Duration;

use wasmtime::{ResourceLimiter, Result};

/// How often the engine's epoch advances; a deadline is rounded up to whole ticks.
pub(crate) const TICK: Duration = Duration::from_millis(10);

/// The most core instances, tables and memories one call's component may create. A component
/// built from one guest has a handful of each.
const MAX_OBJECTS: usize = 16;

/// The most elements of all tables together.
const MAX_TABLE: usize = 100_000;

/// One call's budget. Past `time` the call is interrupted at its next instruction, a fetch's
/// own timeout being whatever is left; growing past `memory` stops it on the spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Wall-clock time, fetches included.
    pub time: Duration,
    /// Bytes of linear memory, all memories together.
    pub memory: usize,
}

impl Default for Budget {
    /// Thirty seconds and 64 MiB: a calendar feed parses in a fraction of either.
    fn default() -> Self {
        Self {
            time: Duration::from_secs(30),
            memory: 64 * 1024 * 1024,
        }
    }
}

impl Budget {
    /// The deadline in epoch ticks, at least one.
    pub(crate) fn ticks(self) -> u64 {
        let ticks = self.time.as_nanos().div_ceil(TICK.as_nanos()).max(1);
        u64::try_from(ticks).unwrap_or(u64::MAX)
    }
}

/// The memory cap as wasmtime asks it, remembering whether the plugin hit it so the call's
/// error can say so as a value.
#[derive(Debug)]
pub(crate) struct Limits {
    memory: usize,
    used: usize,
    pub(crate) exceeded: bool,
}

impl Limits {
    pub(crate) fn new(budget: Budget) -> Self {
        Self {
            memory: budget.memory,
            used: 0,
            exceeded: false,
        }
    }
}

impl ResourceLimiter for Limits {
    /// Past the cap the growth is an error, which traps the plugin where it stands rather than
    /// handing it a failed `memory.grow` to retry.
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool> {
        let used = self.used.saturating_sub(current) + desired;
        if used > self.memory {
            self.exceeded = true;
            return Err(wasmtime::format_err!(
                "the plugin asked for {used} bytes of memory, past its {} byte cap",
                self.memory
            ));
        }
        self.used = used;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool> {
        Ok(desired <= MAX_TABLE)
    }

    fn instances(&self) -> usize {
        MAX_OBJECTS
    }

    fn tables(&self) -> usize {
        MAX_OBJECTS
    }

    fn memories(&self) -> usize {
        MAX_OBJECTS
    }
}
