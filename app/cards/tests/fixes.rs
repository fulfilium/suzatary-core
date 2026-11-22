//! The fixes' rule, over the stores held in core's memory.

use std::collections::BTreeSet;
use suzatary_base::capability::Datetime;
use suzatary_connect::settings::{Settable, Settables};
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::proposal::Proposal;
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded::{Done, Responded};

use suzatary_cards::secretary::fixes::*;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::responded::Performed;
use suzatary_store::store::Reads as _;
use suzatary_stores::stores::InMemory;

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 7, 9, minute, 0, 0).expect("an instant")
}

use suzatary_connect::connect::Channel;

fn declared(channels: &[Channel]) -> Settables {
    Settables::derive(
        suzatary_connect::settings::MODEL_TOKEN,
        channels.iter().copied(),
        None,
        &[],
    )
}

struct World {
    conversations: suzatary_stores::stores::conversations::InMemory,
    inbox: InMemory<InboxEvent>,
    stored: BTreeSet<String>,
}

impl World {
    fn new() -> Self {
        Self {
            conversations: suzatary_stores::stores::conversations::InMemory::default(),
            inbox: InMemory::default(),
            stored: BTreeSet::new(),
        }
    }

    fn fold(&self) -> inbox::Fold {
        inbox::fold(&self.inbox).expect("reads")
    }

    fn file(
        &self,
        settables: &Settables,
        minute: u32,
    ) -> suzatary_cards::secretary::noticed::Filed {
        owed(
            settables,
            |name| !self.stored.contains(name.name()),
            &self.fold(),
        )
        .file(self.conversations.conversations(), &self.inbox, at(minute))
        .expect("filed")
    }

    fn open(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .fold()
            .objects()
            .filter(|object| object.asked.proposer == Proposer::Box && object.reading.open())
            .filter_map(|object| match &object.asked.proposal {
                Proposal::Act(Act::Secret { name, .. }) => Some(name.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names
    }

    fn card_for(&self, name: &str) -> Option<inbox::ProposalId> {
        self.fold()
            .objects()
            .find(|object| {
                object.reading.open()
                    && matches!(&object.asked.proposal,
                        Proposal::Act(Act::Secret { name: asked, .. }) if asked == name)
            })
            .map(|object| object.id.clone())
    }

    /// The owner taps the card for `name`, and the value is stored.
    fn tap(&mut self, name: &str, minute: u32) {
        let id = self.card_for(name).expect("a card");
        let owner = prodrome_core::event::Actor::new("ada").expect("an actor");
        inbox::submit(&self.inbox, &id, &owner, at(minute), |_| {
            Ok(Performed::Done(Done::Acted {
                what: format!("stored {name}"),
            }))
        })
        .expect("taps");
        self.stored.insert(name.to_owned());
    }

    fn dismiss(&self, name: &str, minute: u32) {
        let id = self.card_for(name).expect("a card");
        let owner = prodrome_core::event::Actor::new("ada").expect("an actor");
        inbox::dismiss(&self.inbox, &id, &owner, at(minute), "not now").expect("dismissed");
    }
}

#[derive(Debug, Clone)]
enum Step {
    Declare(Vec<Channel>),
    StoreElsewhere(&'static str),
    Lose(&'static str),
    Tap(&'static str),
    Dismiss(&'static str),
    File,
}

const NAMES: [&str; 2] = ["NTFY_TOPIC", "FCM_SERVICE_ACCOUNT_B64"];

fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
    use proptest::prelude::*;
    let name = proptest::sample::select(NAMES.to_vec());
    proptest::collection::vec(
        prop_oneof![
            proptest::sample::subsequence(Channel::ALL.to_vec(), 0..=2).prop_map(Step::Declare),
            name.clone().prop_map(Step::StoreElsewhere),
            name.clone().prop_map(Step::Lose),
            name.clone().prop_map(Step::Tap),
            name.prop_map(Step::Dismiss),
            Just(Step::File),
        ],
        0..24,
    )
}

proptest::proptest! {
    /// After any history, a filing leaves exactly one open card for each declared name that
    /// does not resolve and was not answered no in this episode, none for any other, and
    /// filing again writes nothing.
    #[test]
    fn a_filing_leaves_one_open_card_per_unresolved_name(script in steps()) {
        let mut held = World::new();
        let mut settables = declared(&[]);
        let mut minute = 0;
        for step in script {
            minute += 1;
            match step {
                Step::Declare(channels) => settables = declared(&channels),
                Step::StoreElsewhere(name) => { held.stored.insert(name.to_owned()); }
                Step::Lose(name) => { held.stored.remove(name); }
                Step::Tap(name) => if held.card_for(name).is_some() { held.tap(name, minute) },
                Step::Dismiss(name) => if held.card_for(name).is_some() { held.dismiss(name, minute) },
                Step::File => { held.file(&settables, minute); }
            }
        }
        held.file(&settables, minute + 1);
        let answered_no = |name: &str| {
            let fold = held.fold();
            let fix = fixes(&settables, |_| true, &fold)
                .into_iter()
                .find(|fix| fix.name.name() == name);
            fix.is_some_and(|fix| fold.objects().any(|object| {
                object.asked.proposal == Proposal::Act(fix.act())
                    && matches!(object.reading.shown().map(|answer| &answer.response),
                        Some(Responded::Dismissed { .. }))
            }))
        };
        let mut expected: Vec<String> = settables
            .names()
            .into_iter()
            .filter(|name| NAMES.contains(name) && !held.stored.contains(*name))
            .filter(|name| !answered_no(name))
            .map(str::to_owned)
            .collect();
        expected.sort();
        proptest::prop_assert_eq!(held.open(), expected);
        let objects = held.inbox.objects().expect("reads").len();
        held.file(&settables, minute + 2);
        proptest::prop_assert_eq!(held.inbox.objects().expect("reads").len(), objects);
    }
}

/// A Replace stands until it is answered, whatever the box files meanwhile, and a stored
/// secret replaced once can be replaced again.
#[test]
fn a_replace_card_stands_until_it_is_answered() {
    let settables = declared(&[Channel::Ntfy]);
    let mut world = World::new();
    world.stored.insert("NTFY_TOPIC".to_owned());
    let name = Settable::mint(&settables, "NTFY_TOPIC").expect("declared");
    let replace_at = |world: &World, minute| {
        replace(&name, at(minute))
            .holding()
            .file(
                world.conversations.conversations(),
                &world.inbox,
                at(minute),
            )
            .expect("filed")
    };
    let filing = |world: &World, minute| {
        owed(
            &settables,
            |name| !world.stored.contains(name.name()),
            &world.fold(),
        )
        .join(replacing(&settables, &world.fold()))
        .file(
            world.conversations.conversations(),
            &world.inbox,
            at(minute),
        )
        .expect("filed")
    };
    assert!(world.open().is_empty());
    replace_at(&world, 1);
    assert_eq!(world.open(), ["NTFY_TOPIC"]);
    replace_at(&world, 2);
    filing(&world, 3);
    assert_eq!(
        world.open(),
        ["NTFY_TOPIC"],
        "one card, kept by the box's filing"
    );
    world.tap("NTFY_TOPIC", 4);
    filing(&world, 5);
    assert!(world.open().is_empty());
    replace_at(&world, 6);
    assert_eq!(world.open(), ["NTFY_TOPIC"], "replaced again, asked again");
}
