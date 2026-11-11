//! A route as data, and the one adapter that mounts it.

use axum::Router;
use axum::extract::{FromRequestParts, Query, RawPathParams, Request, State as AxumState};
use axum::response::Response;
use axum::routing::{MethodRouter, get, post};
use suzatary_base::refusal::Refusal;

use super::State;
use super::body::Body;
use super::call::Call;
use super::gate::who;
use super::guard::{Asks, Guard, Guarded, Proof, act_headers};
use super::params::{Asked, Params};
use super::reply::{Outcome, refused};

/// One route: where it is mounted, who may call it, and what it runs.
pub struct Route {
    pub method: Method,
    /// The whole path of a box route; a chain route's path after `/api` or `/api/{chain}`.
    pub path: &'static str,
    pub scope: Scope,
    /// Taken from the type of witness its work asks for.
    pub guard: Guard,
    /// Taken from the same witness.
    pub asks: Asks,
    handler: MethodRouter<State>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    /// A write, or a read too large for a query string.
    Post,
}

/// Whose route it is, which decides where it is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Box,
    /// A chain's: bare under `/api` for the first chain, and under `/api/{chain}` for any.
    ///
    /// matchit prefers a static segment, so `instance.rs` refuses a chain named like one.
    Chain,
    /// A chain's under `/api/{chain}` only, because its bare path predates chains.
    Named,
}

/// The work a route runs, on the blocking pool.
pub type Run<W, P, B, O> = fn(&Call<W, P, B>) -> O;

impl Route {
    pub fn get<W: Guarded, P: Params, B: Body, O: Outcome>(
        path: &'static str,
        scope: Scope,
        run: Run<W, P, B, O>,
    ) -> Self {
        Self::adapted(Method::Get, path, scope, run)
    }

    pub fn post<W: Guarded, P: Params, B: Body, O: Outcome>(
        path: &'static str,
        scope: Scope,
        run: Run<W, P, B, O>,
    ) -> Self {
        Self::adapted(Method::Post, path, scope, run)
    }

    /// A read the adapter cannot express (a stream, a refusal of a whole prefix), handed
    /// to axum as it is; only a read may be special, so no write skips the adapter.
    pub fn special(path: &'static str, scope: Scope, handler: MethodRouter<State>) -> Self {
        Self {
            method: Method::Get,
            path,
            scope,
            guard: Guard::Read,
            asks: Asks::Nothing,
            handler,
        }
    }

    fn adapted<W: Guarded, P: Params, B: Body, O: Outcome>(
        method: Method,
        path: &'static str,
        scope: Scope,
        run: Run<W, P, B, O>,
    ) -> Self {
        let handler = move |AxumState(state): AxumState<State>, request: Request| async move {
            serve(state, request, scope, run)
                .await
                .unwrap_or_else(|refusal| refused(&refusal))
        };
        Self {
            method,
            path,
            scope,
            guard: W::GUARD,
            asks: <W::Proof as Proof>::ASKS,
            handler: match method {
                Method::Get => get(handler),
                Method::Post => post(handler),
            },
        }
    }

    #[must_use]
    pub fn paths(&self) -> Vec<String> {
        match self.scope {
            Scope::Box => vec![self.path.to_owned()],
            Scope::Chain => vec![
                format!("/api{}", self.path),
                format!("/api/{{chain}}{}", self.path),
            ],
            Scope::Named => vec![format!("/api/{{chain}}{}", self.path)],
        }
    }

    /// Its lines in the surface listing: `METHOD path guard asks`, one per mounted path.
    #[must_use]
    pub fn listed(&self) -> Vec<String> {
        let method = match self.method {
            Method::Get => "GET",
            Method::Post => "POST",
        };
        self.paths()
            .into_iter()
            .map(|path| format!("{method} {path} {} {}", self.guard.name(), self.asks.name()))
            .collect()
    }

    pub fn mount(self, router: Router<State>) -> Router<State> {
        self.paths().into_iter().fold(router, |router, path| {
            router.route(&path, self.handler.clone())
        })
    }
}

/// The guard first: nothing about a request is read for a caller its guard refuses.
async fn serve<W: Guarded, P: Params, B: Body, O: Outcome>(
    state: State,
    request: Request,
    scope: Scope,
    run: Run<W, P, B, O>,
) -> Result<Response, Refusal> {
    let (mut parts, body) = request.into_parts();
    let admitted = who(&parts.extensions)?;
    W::GUARD.admits(&state, &admitted, &act_headers(&parts.headers))?;
    let segments = RawPathParams::from_request_parts(&mut parts, &state)
        .await
        .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
    let query = Query::try_from_uri(&parts.uri)
        .map_err(|failed| Refusal::bad_request(failed.body_text()))?
        .0;
    let asked = Asked::of(&segments, query);
    let chain = state.bound(asked.chain())?;
    let params = P::parse(&asked, &chain)?;
    // A chain's route writes that chain; a box route can reach every chain, so its device must
    // be trusted on each, read where the door reads it.
    let trusting = match scope {
        Scope::Box => state.chain_stores(),
        Scope::Chain | Scope::Named => vec![chain.caps.secretary.chain],
    };
    let mut traced = Traced::of(<W::Proof as Proof>::ASKS, &parts);
    let (body, proof) =
        super::asked::asked::<W::Proof>(&state, &admitted, trusting, &parts, body, B::CAP)
            .await
            .map_err(|refusal| traced.refused("door", refusal))?;
    traced.passed("door");
    // The witness is minted from the door's proof, so a route holds one only where it passed.
    let guard = W::check(&state, &admitted, &act_headers(&parts.headers), &proof)?;
    let performing = match proof.proven() {
        None => None,
        Some(proven) => {
            traced.named(proven.record().name.as_str());
            // The record is read only under the claim, and a performance keeps its record before
            // it releases the claim, so no two copies of a request can both find none.
            let claimed = super::asked::Performing::claim(&state, proven)
                .map_err(|refusal| traced.refused("claim", refusal))?;
            if let Some(answer) = super::asked::answered(&state, &claimed)
                .await
                .map_err(|refusal| traced.refused("record", refusal))?
            {
                traced.answered("replayed", answer.status().as_u16());
                return Ok(answer);
            }
            Some(claimed)
        }
    };
    let body = B::read(body, &state).await?;
    let call = Call {
        state,
        chain,
        admitted,
        guard,
        params,
        body,
        device: performing.as_ref().map(|asked| asked.device().clone()),
    };
    let (answer, wrote) =
        blocking(move || Ok(suzatary_core::store::recording(|| run(&call).outcome()))).await?;
    let response = answer.unwrap_or_else(|refusal| refused(&refusal));
    traced.passed("perform");
    let status = response.status().as_u16();
    let kept = match performing {
        Some(performing) => performing
            .kept(response, wrote)
            .await
            .map_err(|refusal| traced.refused("keep", refusal))?,
        None => response,
    };
    traced.answered("performed", status);
    Ok(kept)
}

/// A signed request's way through the door, logged at `debug` under `suzatary::requests`.
/// Never its nonce or body.
struct Traced {
    on: bool,
    target: String,
    at: String,
    name: String,
    started: std::time::Instant,
    stages: Vec<(&'static str, u128)>,
}

impl Traced {
    fn of(asks: Asks, parts: &axum::http::request::Parts) -> Self {
        let on = asks != Asks::Nothing
            && tracing::enabled!(target: "suzatary::requests", tracing::Level::DEBUG);
        Self {
            on,
            target: if on {
                parts.uri.path().to_owned()
            } else {
                String::new()
            },
            at: parts
                .headers
                .get(super::asked::AT_HEADER)
                .and_then(|value| value.to_str().ok())
                .filter(|_| on)
                .unwrap_or_default()
                .to_owned(),
            name: String::new(),
            started: std::time::Instant::now(),
            stages: Vec::new(),
        }
    }

    fn named(&mut self, name: &str) {
        if self.on {
            name.get(..12).unwrap_or(name).clone_into(&mut self.name);
        }
    }

    fn passed(&mut self, stage: &'static str) {
        if self.on {
            let since: u128 = self.stages.iter().map(|(_, ms)| ms).sum();
            self.stages.push((
                stage,
                self.started.elapsed().as_millis().saturating_sub(since),
            ));
        }
    }

    fn answered(&self, how: &str, status: u16) {
        if self.on {
            tracing::debug!(
                target: "suzatary::requests",
                "POST {} name={} at={} status={status} {how} total={}ms stages={:?}",
                self.target,
                self.name,
                self.at,
                self.started.elapsed().as_millis(),
                self.stages,
            );
        }
    }

    fn refused(&mut self, stage: &'static str, refusal: Refusal) -> Refusal {
        self.passed(stage);
        self.answered(refusal.message(), refusal.status().code());
        refusal
    }
}

/// Run blocking work off the async workers, turning a panic into a 500.
pub async fn blocking<T, F>(work: F) -> Result<T, Refusal>
where
    F: FnOnce() -> Result<T, Refusal> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|_| Err(Refusal::internal("the request's work did not finish")))
}
