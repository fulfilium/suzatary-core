//! The watcher: scheduling and markup warming around the pure `suzatary_core::notify::decide`.

use std::collections::BTreeSet;

use suzatary_core::chains::ChainPolicy;
use suzatary_core::deployment::Deployment;
use suzatary_core::server::Server;
use suzatary_core::{notify, routes, view};

use super::watched::{Pace, Watched};

/// The longest a burst of writes that never falls quiet waits for its push.
const CAP: std::time::Duration = std::time::Duration::from_mins(10);

/// Compile every trusted record's markup on the blocking pool; untrusted ones are never served.
pub fn warm_now(caps: &'static Server<'static>, chain: &ChainPolicy) {
    let untrusted = chain.untrusted.clone();
    tokio::task::spawn_blocking(move || {
        let now = caps.secretary.clock.now();
        match routes::warmable(caps.secretary.chain, &untrusted, now) {
            Err(refusal) => tracing::warn!("warm: the chain could not be read: {refusal}"),
            Ok(sources) => {
                let began = std::time::Instant::now();
                let rendered = caps.secretary.render.warm(&sources);
                tracing::info!(
                    "warmed {rendered}/{} markup fragments in {:.2}s",
                    sources.len(),
                    began.elapsed().as_secs_f64()
                );
                for finding in caps.secretary.render.findings() {
                    tracing::warn!(
                        "the rendered table holds two values for {} at {}: never chosen, compiled again",
                        finding.key.function.as_str(),
                        finding.key.argument.as_str()
                    );
                }
            }
        }
    });
}

/// Warm and never page, for a chain that is not the first one.
///
/// The notify call is absent rather than suppressed, so a secondary chain cannot wake the phone.
pub async fn warming(caps: &'static Server<'static>, chain: ChainPolicy) {
    let mut last = read("the chain's heads", heads(caps));
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(notify::POLL_SECONDS)).await;
        if let Some(now) = read("the chain's heads", heads(caps))
            && last.as_ref() != Some(&now)
        {
            last = Some(now);
            warm_now(caps, &chain);
        }
    }
}

/// Page the phone about the agent's work, never the person's own taps.
pub async fn watcher(
    caps: &'static Server<'static>,
    deployment: Deployment,
    chain: ChainPolicy,
    presence: super::Presence,
) {
    let own: BTreeSet<String> = deployment
        .actors
        .values()
        .map(|actor| actor.as_str().to_owned())
        .collect();
    let mut watched = Watched::starting(
        Pace {
            quiet: std::time::Duration::from_secs(notify::QUIET_SECONDS),
            cap: CAP,
        },
        read("the chain's heads", heads(caps)),
        read("the chain", events(caps)).map(|events| events.len()),
        // A proposal moves the inbox, not the chain.
        read("the inbox", caps.secretary.inbox.events()).map(|inbox| inbox.len()),
    );

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(notify::POLL_SECONDS)).await;
        let inbox = read("the inbox", caps.secretary.inbox.events());
        let now = std::time::Instant::now();
        if watched.polled(
            read("the chain's heads", heads(caps)),
            inbox.as_ref().map(Vec::len),
            now,
        ) {
            warm_now(caps, &chain);
        }
        if !watched.due(now) {
            continue;
        }
        let Some(events) = read("the chain", events(caps)) else {
            continue;
        };
        let fresh = watched.announce(&events, inbox.as_deref());
        let agent_work = notify::decide(&fresh.events, &own, caps.secretary.clock.now());
        // An open page draws its own popup, so the phone rings only when nobody is looking.
        let needs_you =
            notify::needs_you(&fresh.events, &fresh.inbox).filter(|_| !presence.anyone());
        for push in [agent_work, needs_you].into_iter().flatten() {
            send(caps, push);
        }
    }
}

/// A read's answer, or nothing this round and a warning: an unreadable store is not an empty one.
fn read<T>(what: &str, answer: Result<T, suzatary_core::store::StoreError>) -> Option<T> {
    answer
        .inspect_err(|failed| tracing::warn!("watch: {what} could not be read: {failed}"))
        .ok()
}

/// One push on the blocking pool; a failure is a log line, never fatal.
fn send(caps: &'static Server<'static>, push: suzatary_core::notify::Push) {
    tokio::task::spawn_blocking(move || match caps.notify.push(&push) {
        Ok(delivered) => tracing::info!("notify: {push:?} ({} device(s))", delivered.devices()),
        Err(error) => tracing::warn!("notify: nothing delivered: {error}"),
    });
}

/// The chain's events, in the order the notifier counts them.
fn events(
    caps: &'static Server<'static>,
) -> Result<Vec<suzatary_core::record::TodoEvent>, suzatary_core::store::StoreError> {
    Ok(view::events_of(&caps.secretary.chain.nodes()?))
}

fn heads(caps: &'static Server<'static>) -> Result<Vec<String>, suzatary_core::store::StoreError> {
    Ok(caps
        .secretary
        .chain
        .tips()?
        .iter()
        .map(|hash| hash.as_str().to_owned())
        .collect())
}
