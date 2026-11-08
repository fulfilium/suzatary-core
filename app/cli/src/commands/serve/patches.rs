//! Hourly, a repository card whose change is already in its default branch closes as done
//! another way (`secretary::merged_patches`). Nothing is fetched while no such card waits.

use std::sync::Arc;
use std::time::Duration;

use super::State;

const EVERY: Duration = Duration::from_hours(1);

/// The sweep, forever.
pub async fn sweeping(state: State, mirrors: Arc<suzatary_io::repos::Mirrors>) {
    let mut every = tokio::time::interval(EVERY);
    loop {
        every.tick().await;
        once(&state, &mirrors).await;
    }
}

/// A failure is a log line; the next tick goes on.
async fn once(state: &State, mirrors: &Arc<suzatary_io::repos::Mirrors>) {
    let (caps, mirrors) = (state.first.caps, Arc::clone(mirrors));
    let swept = tokio::task::spawn_blocking(move || {
        suzatary_core::secretary::merged_patches::sweep(
            caps.secretary.inbox,
            mirrors.as_ref(),
            caps.secretary.clock.now(),
        )
    })
    .await;
    match swept {
        Ok(Ok(swept)) => {
            for (card, why) in &swept.unread {
                tracing::warn!("repository cards: {card} was not checked: {why}");
            }
            if !swept.closed.is_empty() {
                tracing::info!(
                    closed = swept.closed.len(),
                    "repository cards: done another way"
                );
                state.heard(&suzatary_core::conversation_id::DEFAULT);
            }
        }
        Ok(Err(failed)) => tracing::warn!("repository cards: {}", failed.message()),
        Err(panicked) => tracing::warn!("repository cards: {panicked}"),
    }
}
