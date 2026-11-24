//! Each membership's frontier as its presence and the actors who wrote that presence.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::registers::Prodrome;
use prodrome_data::event::Actor;
use prodrome_data::fold::Product as _;
use prodrome_data::fold::sole::{Bodies, One, Only};
use prodrome_data::policy::Everything;
use prodrome_data::schema::Schema as _;
use prodrome_tags::Tagging;
use prodrome_tags::membership::{Membership, MembershipEvent, Pair, Presence};

/// Every pair that a write reached, with the presence its frontier reads (the greatest) and the
/// writers of the frontier's writes of that presence.
#[must_use]
pub fn writers<S: Tagging>(
    memberships: &Prodrome<MembershipEvent<S>>,
) -> BTreeMap<Pair<S>, (Presence, BTreeSet<Actor>)> {
    memberships
        .iter()
        .filter_map(|(pair, stream)| {
            let registers: One<'_, Bodies<Membership<S>>> =
                prodrome_dag::fold::read(stream, None, &Everything);
            let stamps = registers.reading(Only);
            let presence = stamps
                .iter()
                .map(|stamp| stamp.event.body().presence())
                .max()?;
            let actors = stamps
                .iter()
                .filter(|stamp| stamp.event.body().presence() == presence)
                .map(|stamp| stamp.event.actor().clone())
                .collect();
            Some((pair.clone(), (presence, actors)))
        })
        .collect()
}
