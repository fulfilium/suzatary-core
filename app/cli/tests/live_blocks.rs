//! Every record on the deployment's chains renders as a verified block, because a write to a todo
//! answers with its block (`todos::render_current`) and a changeset is gated on it: a live record
//! outside the block grammar would refuse every later write to that todo. The core's proptest pins
//! the law on generated rows; this asks it of the prose that exists. It skips with its reason
//! printed where this checkout has no chain.

use suzatary_core::capability::Clock;
use suzatary_core::record::EventStore;
use suzatary_core::store::Reads as _;
use suzatary_io::clock::SystemClock;
use suzatary_io::objects::Objects;

#[test]
fn every_live_record_renders_verified() {
    let Some(instance) = crate::this_box::instance() else {
        eprintln!("skipped: this checkout has no instance.toml");
        return;
    };
    let now = SystemClock.now();
    let mut seen = 0;
    for chain in &instance.chains {
        if !chain.events.is_dir() {
            continue;
        }
        let store = Objects::chain(EventStore::new(
            chain.events.clone(),
            chain.policy.untrusted.clone(),
        ));
        let nodes = store.nodes().expect("the live chain reads");
        let mut refused = Vec::new();
        for record in suzatary_core::believed::records(&nodes, now)
            .into_values()
            .flatten()
        {
            seen += 1;
            if let Err(refusal) =
                suzatary_core::todos::render_current(&nodes, now, &chain.policy.untrusted, &record)
            {
                refused.push(format!("{}: {}", record.todo.as_str(), refusal.message()));
            }
        }
        assert!(
            refused.is_empty(),
            "{}'s records outside the block grammar: {refused:#?}",
            chain.policy.name.as_str()
        );
    }
    if seen == 0 {
        eprintln!("skipped: this deployment's chains hold no records");
    }
}
