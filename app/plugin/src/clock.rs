//! The monotonic clock a plugin reads, whose every subscription ends by the call's deadline.

use std::time::{Duration, Instant};

use wasmtime::component::Resource;
use wasmtime_wasi::p2::bindings::clocks::monotonic_clock;
use wasmtime_wasi::p2::{DynPollable, Pollable, subscribe};

use crate::call::Call;

/// A subscription's end. The host blocks in `pollable.block`, where no epoch reaches, so a
/// sleep past the deadline would outlive the call's budget; this one wakes at the deadline, and
/// the epoch then stops the plugin at its next instruction.
struct Until(tokio::time::Instant);

#[wasmtime_wasi::async_trait]
impl Pollable for Until {
    async fn ready(&mut self) {
        tokio::time::sleep_until(self.0).await;
    }
}

impl Call {
    fn until(&mut self, wait: Duration) -> wasmtime::Result<Resource<DynPollable>> {
        let wanted = Instant::now().checked_add(wait).unwrap_or(self.deadline);
        let until = Until(tokio::time::Instant::from_std(wanted.min(self.deadline)));
        let resource = self.table.push(until)?;
        subscribe(&mut self.table, resource)
    }

    fn nanos(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

impl monotonic_clock::Host for Call {
    fn now(&mut self) -> wasmtime::Result<u64> {
        Ok(self.nanos())
    }

    fn resolution(&mut self) -> wasmtime::Result<u64> {
        Ok(1)
    }

    fn subscribe_instant(&mut self, when: u64) -> wasmtime::Result<Resource<DynPollable>> {
        self.until(Duration::from_nanos(when.saturating_sub(self.nanos())))
    }

    fn subscribe_duration(&mut self, wait: u64) -> wasmtime::Result<Resource<DynPollable>> {
        self.until(Duration::from_nanos(wait))
    }
}
