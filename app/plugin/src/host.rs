//! The host: one engine and one linker, and the check a component passes to be loaded.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use prodrome_core::declared::{Declaration, Refusal};
use suzatary_core::plugin::{Manifest, Said};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, Trap};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;

use crate::budget::{Limits, TICK};
use crate::call::{Call, Source as World, SourcePre};
use crate::{Budget, Effects, Run, RunError, Synced};

/// The most bytes of standard error a call may write; past it the write traps the plugin.
const MAX_STDERR: usize = 64 * 1024;

/// The most standard-error lines kept per call.
const MAX_LINES: usize = 100;

/// One wasmtime engine, shared by every plugin, and the linker that holds its imports.
///
/// The engine counts epochs on a thread of its own, one tick every 10 ms; a call's deadline is a
/// number of ticks, so a plugin in a loop is interrupted without the host waiting on it. The
/// thread ends with the last value holding the engine.
pub struct Host {
    engine: Engine,
    linker: Linker<Call>,
    budget: Budget,
}

/// Why the host could not start.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The engine refused its configuration.
    #[error("the plugin engine could not start: {0}")]
    Engine(String),
    /// The thread that advances the epoch could not start.
    #[error("the plugin clock could not start: {0}")]
    Ticker(String),
    /// The imports could not be linked.
    #[error("the plugin imports could not be linked: {0}")]
    Link(String),
}

/// Why a component was not loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// Not a WebAssembly component this engine compiles.
    #[error("not a component: {0}")]
    NotAComponent(String),
    /// It imports what the host does not link, or does not export `sync`, so it never runs.
    #[error("not a source this host runs: {0}")]
    NotASource(String),
    /// Its declared schema breaks a law of admission, which the refusal names.
    #[error("its schema is {0}")]
    Schema(#[from] Refusal),
}

/// A component that passed [`Host::load`], ready to instantiate.
pub struct Source {
    manifest: Arc<Manifest>,
    schema: Declaration,
    pre: SourcePre<Call>,
    budget: Budget,
}

impl Host {
    /// Start the engine and link the imports, once; every call gets `budget`.
    pub fn new(budget: Budget) -> Result<Self, HostError> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| HostError::Engine(e.to_string()))?;
        let weak = engine.weak();
        thread::Builder::new()
            .name("plugin-epoch".to_owned())
            .spawn(move || {
                while let Some(engine) = weak.upgrade() {
                    engine.increment_epoch();
                    drop(engine);
                    thread::sleep(TICK);
                }
            })
            .map_err(|e| HostError::Ticker(e.to_string()))?;
        let mut linker = Linker::new(&engine);
        World::add_to_linker::<Call, HasSelf<Call>>(&mut linker, |call| call)
            .and_then(|()| crate::call::wasi(&mut linker))
            .map_err(|e| HostError::Link(e.to_string()))?;
        Ok(Self {
            engine,
            linker,
            budget,
        })
    }

    /// Admit `schema`, the plugin's declared schema, then compile `component` and prepare it.
    ///
    /// A component that imports anything the linker does not hold is refused here, before
    /// anything of it runs: the linker's imports are the plugin's whole authority, and its schema
    /// the whole shape of what it may write.
    pub fn load(
        &self,
        manifest: Manifest,
        component: &[u8],
        schema: &str,
    ) -> Result<Source, LoadError> {
        let schema = Declaration::admit(schema)?;
        let compiled = Component::from_binary(&self.engine, component)
            .map_err(|e| LoadError::NotAComponent(e.to_string()))?;
        let pre = self
            .linker
            .instantiate_pre(&compiled)
            .and_then(SourcePre::new)
            .map_err(|e| LoadError::NotASource(format!("{e:#}")))?;
        Ok(Source {
            manifest: Arc::new(manifest),
            schema,
            pre,
            budget: self.budget,
        })
    }
}

impl Source {
    /// The manifest it was loaded with.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Its declared schema, admitted: what every item it emits must parse into.
    #[must_use]
    pub fn schema(&self) -> &Declaration {
        &self.schema
    }

    /// Read the outside world from `cursor`, lending the plugin the names of the credentials its
    /// manifest declares, in a fresh store under the host's [`Budget`].
    #[must_use]
    pub fn sync(&self, effects: &Effects, cursor: Option<&[u8]>) -> Run {
        let stderr = MemoryOutputPipe::new(MAX_STDERR);
        let wasi = WasiCtxBuilder::new().stderr(stderr.clone()).build();
        let call = Call {
            manifest: Arc::clone(&self.manifest),
            effects: effects.clone(),
            wasi,
            table: ResourceTable::new(),
            started: Instant::now(),
            deadline: Instant::now() + self.budget.time,
            limits: Limits::new(self.budget),
            fetches: Vec::new(),
        };
        let mut store = Store::new(self.pre.engine(), call);
        store.limiter(|call| &mut call.limits);
        store.set_epoch_deadline(self.budget.ticks());
        store.epoch_deadline_trap();
        let result = self.call(&mut store, cursor);
        let said = String::from_utf8_lossy(&stderr.contents())
            .lines()
            .take(MAX_LINES)
            .map(Said::clip)
            .collect();
        Run {
            result,
            fetches: store.into_data().fetches,
            said,
        }
    }

    fn call(&self, store: &mut Store<Call>, cursor: Option<&[u8]>) -> Result<Synced, RunError> {
        let stopped = |store: &Store<Call>, e: &wasmtime::Error, or: fn(Said) -> RunError| {
            if store.data().limits.exceeded {
                RunError::Memory
            } else if e.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
                RunError::Deadline
            } else {
                or(Said::clip(format!("{e:#}")))
            }
        };
        let instance = self
            .pre
            .instantiate(&mut *store)
            .map_err(|e| stopped(store, &e, RunError::Instantiate))?;
        let lent: Vec<String> = self
            .manifest
            .credentials()
            .map(ToString::to_string)
            .collect();
        let answer = instance
            .suzatary_plugin_sync()
            .call_sync(&mut *store, cursor, &lent)
            .map_err(|e| stopped(store, &e, RunError::Trapped))?;
        // An answer after the deadline came from time past the budget, as a clock wait can end
        // there and return before the epoch next stops the plugin.
        if Instant::now() >= store.data().deadline {
            return Err(RunError::Deadline);
        }
        answer
            .map(|synced| Synced {
                items: synced.items,
                cursor: synced.cursor,
            })
            .map_err(|said| RunError::Plugin(Said::clip(said)))
    }
}
