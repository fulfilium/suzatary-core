//! One call's store: the grant, the effects lent to it, WASI with nothing in it, and the fetches
//! the host records.

use std::sync::Arc;
use std::time::Instant;

use suzatary_core::plugin::{MAX_FETCHES, MAX_RESPONSE, Manifest, Refusal, Target};
use suzatary_io::http::{HttpError, Req};
use wasmtime::component::{HasData, HasSelf, Linker, ResourceTable};
use wasmtime_wasi::p2::bindings::{cli, clocks, random, sync::io};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::budget::Limits;
use crate::{Effects, Fetch};

wasmtime::component::bindgen!({
    path: "../wit",
    world: "source",
});

use suzatary::plugin::fetch;

/// What the imports of one call act on. Built fresh per call and dropped with its store.
pub(crate) struct Call {
    pub(crate) manifest: Arc<Manifest>,
    pub(crate) effects: Effects,
    pub(crate) wasi: WasiCtx,
    pub(crate) table: ResourceTable,
    /// When the call began, which its monotonic clock counts from.
    pub(crate) started: Instant,
    /// When the call's time is up, for a fetch's own timeout and every clock subscription.
    pub(crate) deadline: Instant,
    pub(crate) limits: Limits,
    pub(crate) fetches: Vec<Fetch>,
}

impl WasiView for Call {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// The resource table, as WASI's stream and poll interfaces reach it.
struct Table;

impl HasData for Table {
    type Data<'a> = &'a mut ResourceTable;
}

/// Link the WASI interfaces a plugin built for wasm32-wasip2 imports, and only these: no
/// filesystem and no sockets exist to reach for.
pub(crate) fn wasi(linker: &mut Linker<Call>) -> wasmtime::Result<()> {
    use wasmtime_wasi::cli::WasiCli;
    use wasmtime_wasi::clocks::WasiClocks;
    use wasmtime_wasi::random::{WasiRandom, WasiRandomCtx};
    fn random(call: &mut Call) -> &mut WasiRandomCtx {
        call.wasi.random()
    }
    let cli = <Call as wasmtime_wasi::cli::WasiCliView>::cli;
    let clock = <Call as wasmtime_wasi::clocks::WasiClocksView>::clocks;
    clocks::wall_clock::add_to_linker::<Call, WasiClocks>(linker, clock)?;
    clocks::monotonic_clock::add_to_linker::<Call, HasSelf<Call>>(linker, |call| call)?;
    random::random::add_to_linker::<Call, WasiRandom>(linker, random)?;
    random::insecure::add_to_linker::<Call, WasiRandom>(linker, random)?;
    random::insecure_seed::add_to_linker::<Call, WasiRandom>(linker, random)?;
    cli::exit::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::environment::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::stdin::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::stdout::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::stderr::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::terminal_input::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::terminal_output::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::terminal_stdin::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::terminal_stdout::add_to_linker::<Call, WasiCli>(linker, cli)?;
    cli::terminal_stderr::add_to_linker::<Call, WasiCli>(linker, cli)?;
    io::error::add_to_linker::<Call, Table>(linker, |call| &mut call.table)?;
    io::poll::add_to_linker::<Call, Table>(linker, |call| &mut call.table)?;
    io::streams::add_to_linker::<Call, Table>(linker, |call| &mut call.table)
}

impl Call {
    fn send(&mut self, target: Target) -> Result<(u16, Vec<u8>), Refusal> {
        if self.fetches.len() >= MAX_FETCHES {
            return Err(Refusal::TooManyFetches);
        }
        let reason = format!("plugin {}: a declared credential", self.manifest.name());
        let secrets = &self.effects.secrets;
        let outgoing = target.admit(&self.manifest, |name| secrets.get(name.as_str(), &reason))?;
        let left = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(Refusal::TimedOut)?;
        let answered = self
            .effects
            .http
            .send(
                Req::get(outgoing.url().to_string())
                    .timeout(left)
                    .limit(MAX_RESPONSE),
            )
            .map_err(|failed| match failed {
                HttpError::NotSent(_) | HttpError::Private { .. } => Refusal::Unreachable,
                HttpError::TimedOut(_) => Refusal::TimedOut,
                HttpError::Lost(_) => Refusal::Lost,
            })?;
        Ok((answered.status, outgoing.answer(answered.body)?))
    }
}

impl fetch::Host for Call {
    fn fetch(&mut self, target: fetch::Target) -> Result<fetch::Response, String> {
        let target = match target {
            fetch::Target::Url(url) => Target::Url(url),
            fetch::Target::Credential(name) => Target::Credential(name),
        };
        // The ledger keys a link by this name, so only a name the manifest declares is kept.
        let credential = match &target {
            Target::Credential(name) => self.manifest.declared(name).cloned(),
            Target::Url(_) => None,
        };
        let sent = self.send(target);
        self.fetches.push(Fetch {
            credential,
            outcome: sent
                .as_ref()
                .map(|(status, _)| *status)
                .map_err(Clone::clone),
        });
        sent.map(|(status, body)| fetch::Response { status, body })
            .map_err(|refused| refused.to_string())
    }
}
