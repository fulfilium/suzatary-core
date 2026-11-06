//! Each membership's frontier as its presence and the actors who wrote that presence. It names no
//! Suzatary type, so it can join `prodrome_core::tags::set`.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Actor;
use prodrome_core::fold::Product as _;
use prodrome_core::fold::sole::{Bodies, One, Only};
use prodrome_core::policy::Everything;
use prodrome_core::registers::Prodrome;
use prodrome_core::schema::Schema as _;
use prodrome_core::tags::Tagging;
use prodrome_core::tags::membership::{Membership, MembershipEvent, Pair, Presence};

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
                prodrome_core::fold::read(stream, None, &Everything);
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
