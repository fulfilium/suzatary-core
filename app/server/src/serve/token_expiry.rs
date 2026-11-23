//! Each GitHub token's expiry as a todo (`secretary::token_expiry`), every [`EVERY`] from the
//! start, on the upkeep chain, since a token is the system's; with no `[upkeep]`, on none.
//! GitHub is asked about each token at most once a day (`GithubTokens`).

use std::sync::Arc;
use std::time::Duration;

use suzatary_io_access::github_tokens::GithubTokens;

use super::State;

const EVERY: Duration = Duration::from_hours(1);

/// The rule, forever, each token's expiry read through `tokens`.
pub(crate) async fn watching(state: State, tokens: Arc<GithubTokens>) {
    let mut every = tokio::time::interval(EVERY);
    loop {
        every.tick().await;
        once(&state, &tokens).await;
    }
}

/// One reading off the async threads; a failure is a log line, and the next tick goes on.
async fn once(state: &State, tokens: &Arc<GithubTokens>) {
    let (state, settables) = (state.clone(), state.settables());
    let pushed = super::settings::pushed(&state);
    let tokens = Arc::clone(tokens);
    let written = tokio::task::spawn_blocking(move || {
        let github: Vec<_> = settables
            .declarations()
            .filter(|(_, by)| by.iter().any(|by| by.github() && by.in_use(&pushed)))
            .map(|(name, _)| name)
            .collect();
        let now = prodrome_core::fpl::instant_of(state.first.caps.secretary.clock.now());
        let expiring = tokens.expiries(&github, now);
        suzatary_secretary::secretary::token_expiry::write(
            &state.every_chain(),
            &state.deployment,
            &expiring,
        )
    })
    .await;
    match written {
        Ok(Ok(objects)) if !objects.is_empty() => {
            tracing::info!(written = objects.len(), "token expiry: the todos moved");
        }
        Ok(Ok(_)) => {}
        Ok(Err(refused)) => tracing::warn!("token expiry: {}", refused.message()),
        Err(panicked) => tracing::warn!("token expiry: {panicked}"),
    }
}
