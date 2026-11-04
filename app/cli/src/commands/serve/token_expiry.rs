//! Each GitHub token's expiry as a todo (`secretary::token_expiry`), every [`EVERY`] from the
//! start, on the `suzatary` chain, since a token is the system's, or the first chain where none
//! is named so. GitHub is asked about each token at most once a day (`GithubTokens`).

use std::sync::Arc;
use std::time::Duration;

use suzatary_io::github_tokens::GithubTokens;

use super::State;

/// How often the expiry is read.
const EVERY: Duration = Duration::from_hours(1);

/// The rule, forever, each token's expiry read through `tokens`.
pub async fn watching(state: State, tokens: Arc<GithubTokens>) {
    let mut every = tokio::time::interval(EVERY);
    loop {
        every.tick().await;
        once(&state, &tokens).await;
    }
}

/// One reading off the async threads; a failure is a log line, and the next tick goes on.
async fn once(state: &State, tokens: &Arc<GithubTokens>) {
    let chain = state.chains.get("suzatary").unwrap_or(&state.first).clone();
    let (deployment, settables) = (state.deployment.clone(), state.settables());
    let pushed = super::settings::pushed(state);
    let tokens = Arc::clone(tokens);
    let written = tokio::task::spawn_blocking(move || {
        let github: Vec<_> = settables
            .declarations()
            .filter(|(_, by)| by.iter().any(|by| by.github() && by.in_use(&pushed)))
            .map(|(name, _)| name)
            .collect();
        let now = prodrome_core::fpl::instant_of(chain.caps.secretary.clock.now());
        let expiring = tokens.expiries(&github, now);
        suzatary_core::secretary::token_expiry::write(
            &chain.caps.secretary,
            &chain.policy,
            &deployment,
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
