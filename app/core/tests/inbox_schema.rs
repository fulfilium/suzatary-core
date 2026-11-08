//! The inbox schema's laws (`docs/design-register-types.md` §7 in the Prodrome): its state's
//! order is a partial order (Prodrome law 30), every transition the code makes climbs it, the
//! store's real append refuses a response below or beside the reading it supersedes and the
//! reading of a union is the join of the readings (law 32), and every event prints and parses
//! back.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome_core::event::{Actor, Hash, canonical, parse_event};
use prodrome_core::fold::{Order, maximal, read};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Everything;
use prodrome_core::store::EventStore;
use proptest::prelude::*;

use suzatary_core::health::plugin::PluginName;
use suzatary_core::secretary::inbox::{self, Asked, ProposalId};
use suzatary_core::secretary::inbox_event::{InboxEvent, InboxVocabulary, Said};
use suzatary_core::secretary::phase::Phase;
use suzatary_core::secretary::proposer::Proposer;
use suzatary_core::secretary::responded::{Done, Responded, Tried, Withdrawal};
use suzatary_core::secretary::store::{Draft, Proposal, Reasons};

type Store = EventStore<InboxEvent, Everything>;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A store at the inbox schema, removed when dropped.
struct Scratch(Store);

impl Scratch {
    fn new() -> Scratch {
        let root: PathBuf = std::env::temp_dir().join(format!(
            "suzatary-inbox-schema-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let store = Store::new(root, Everything);
        store.init("inbox").expect("begins");
        Scratch(store)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.root());
    }
}

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 1, 9 + minute / 60, minute % 60, 0, 0).expect("an instant")
}

fn asked() -> Asked {
    inbox::Asked {
        proposer: Proposer::Secretary,
        proposal: Proposal::Email(Draft {
            account: "ada@example.com".to_owned(),
            to: vec!["mira@example.com".to_owned()],
            cc: Vec::new(),
            subject: "Thursday".to_owned(),
            body: "Thursday works.".to_owned(),
            attachments: Vec::new(),
        }),
        reasons: Reasons::new(
            "she asked",
            vec!["mail:abcdef012345".parse().expect("a ref")],
        )
        .expect("reasons"),
        asked: "proposal:default/0".parse().expect("a ref"),
        price: inbox::Price::Serves("todo:life/reply".parse().expect("a ref")),
    }
}

fn id() -> ProposalId {
    ProposalId::of(&asked(), at(0))
}

fn event(minute: u32, said: Said) -> InboxEvent {
    InboxEvent {
        proposal: id(),
        at: at(minute),
        actor: Actor::new("ada").expect("an actor"),
        said,
    }
}

fn responded(minute: u32, responded: Responded) -> InboxEvent {
    event(minute, Said::Responded(responded))
}

/// Every phase a proposal reaches within three tries.
fn every_phase() -> Vec<Phase> {
    (1..=3)
        .flat_map(|attempt| [false, true].map(|running| Phase::Trying { attempt, running }))
        .chain([
            Phase::Sent,
            Phase::Applied,
            Phase::Acted,
            Phase::MayHaveGone,
            Phase::Checked,
            Phase::Dismissed,
            Phase::Withdrawn,
        ])
        .collect()
}

/// A response of each kind, at a small attempt.
fn a_response() -> impl Strategy<Value = Responded> {
    let reason = "[a-z ]{0,8}";
    prop_oneof![
        (1..4_u32, reason).prop_map(|(attempt, reason)| Responded::Tried {
            attempt,
            tried: Tried::NotSent { reason }
        }),
        (1..4_u32).prop_map(|attempt| Responded::Tried {
            attempt,
            tried: Tried::Lacks(suzatary_core::mail::lacking::Lacking::GoogleClient),
        }),
        (1..4_u32).prop_map(|attempt| Responded::Tried {
            attempt,
            tried: Tried::Running {
                began: "2026-10-01T09:00:00".to_owned(),
                expires_at: "2026-10-01T09:15:00".to_owned(),
            }
        }),
        "[a-z]{1,4}".prop_map(|message| Responded::Done(Done::Sent {
            recipients: vec!["mira@example.com".to_owned()],
            message_id: format!("<{message}@box>"),
        })),
        Just(Responded::Done(Done::Applied {
            objects: vec!["a".repeat(64)],
            chain: "life".to_owned(),
        })),
        "[a-z ]{0,8}".prop_map(|what| Responded::Done(Done::Acted { what })),
        reason.prop_map(|reason| Responded::Done(Done::MayHaveGone { reason })),
        Just(Responded::Checked(())),
        reason.prop_map(|reason| Responded::Dismissed { reason }),
        Just(Responded::Withdrawn(Withdrawal::Superseded)),
        reason.prop_map(|why| Responded::Withdrawn(Withdrawal::NoLongerNeeded { why })),
        (1..4_u32, reason).prop_map(|(attempt, reason)| Responded::Tried {
            attempt,
            tried: Tried::Refused { reason }
        }),
        (1..4_u32, reason).prop_map(|(attempt, reason)| Responded::Tried {
            attempt,
            tried: Tried::Unfinished {
                reason,
                link: "https://github.com/owner/name/pulls".to_owned(),
            }
        }),
        reason.prop_map(|what| Responded::Withdrawn(Withdrawal::DoneElsewhere { what })),
    ]
}

/// The phases the store reads for the one proposal.
fn phases(store: &Store) -> HashSet<Phase> {
    let nodes = store.dag().expect("reads").nodes().expect("orders");
    let state = prodrome_core::registers::fold(&nodes);
    state
        .entities()
        .find(|(key, _)| **key == id())
        .map(|(_, stream)| {
            read(stream, None, &Everything)
                .state
                .reading::<suzatary_core::secretary::inbox_registers::StateRegister>()
                .into_iter()
                .filter_map(|stamp| match &stamp.event.said {
                    Said::Responded(responded) => Some(responded.phase()),
                    Said::Asked(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Each response appended in turn, a refused one skipped, from minute `from`.
fn try_all(store: &Store, from: u32, responses: &[Responded]) {
    for (i, response) in responses.iter().enumerate() {
        let minute = from + u32::try_from(i).expect("a few");
        let _ = store.append(responded(minute, response.clone()));
    }
}

/// The transitions the code performs, each from where it is allowed to where it can go: a submit
/// from standing, a try that sent nothing, and a stale running flow, to whatever a performer
/// answers; a flow's end from a running one; a dismissal from standing or a try that sent nothing;
/// a check from a send that may have gone.
fn transitions() -> Vec<(Option<Phase>, Phase)> {
    let trying = |attempt, running| Phase::Trying { attempt, running };
    let performed = |next| {
        vec![
            trying(next, false),
            trying(next, true),
            Phase::Sent,
            Phase::Applied,
            Phase::Acted,
            Phase::MayHaveGone,
        ]
    };
    let mut out = Vec::new();
    for to in performed(1) {
        out.push((None, to));
    }
    for n in 1..4 {
        for from in [trying(n, false), trying(n, true)] {
            for to in performed(n + 1) {
                out.push((Some(from), to));
            }
        }
        for to in [Phase::Acted, Phase::MayHaveGone, trying(n + 1, false)] {
            out.push((Some(trying(n, true)), to));
        }
        out.push((Some(trying(n, false)), Phase::Dismissed));
    }
    out.push((None, Phase::Dismissed));
    out.push((Some(Phase::MayHaveGone), Phase::Checked));
    out
}

#[test]
fn every_transition_the_code_makes_climbs() {
    for (from, to) in transitions() {
        if let Some(from) = from {
            assert!(
                from.le(&to) && from != to,
                "{from:?} to {to:?} does not climb"
            );
        }
    }
}

/// The real conflicts: the settled phases two devices can reach from one standing proposal,
/// pairwise incomparable, and nothing else is. The two chains are a check over the send it
/// checks, and the owner's no over a withdrawal.
#[test]
fn the_conflicts_are_the_settled_pairs() {
    let settled = [
        Phase::Sent,
        Phase::Applied,
        Phase::Acted,
        Phase::MayHaveGone,
        Phase::Checked,
        Phase::Dismissed,
        Phase::Withdrawn,
    ];
    for one in settled {
        for two in settled {
            let comparable = one.le(&two) || two.le(&one);
            let chained = matches!(
                (one, two),
                (Phase::MayHaveGone, Phase::Checked)
                    | (Phase::Checked, Phase::MayHaveGone)
                    | (Phase::Withdrawn, Phase::Dismissed)
                    | (Phase::Dismissed, Phase::Withdrawn)
            );
            assert_eq!(comparable, one == two || chained, "{one:?} and {two:?}");
        }
    }
}

/// Law 30 over every phase within three tries, and a floor is below what stands on it.
#[test]
fn the_state_order_is_a_partial_order() {
    crate::laws::partial_order(&every_phase()).expect("a partial order");
    for phase in every_phase() {
        if let Some(floor) = phase.floor() {
            assert!(
                floor.le(&phase) && floor != phase,
                "{phase:?} over {floor:?}"
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Every event prints and parses back, through the schema's own vocabulary, whoever asked.
    #[test]
    fn every_event_prints_and_parses_back(response in a_response(), plugin in "[a-z][a-z0-9]{0,31}") {
        let offered = Asked {
            proposer: Proposer::Plugin(PluginName::parse(&plugin).expect("a name")),
            ..asked()
        };
        for one in [
            event(0, Said::Asked(Box::new(asked()))),
            event(0, Said::Asked(Box::new(offered))),
            responded(1, response),
        ] {
            let printed = canonical(&one);
            prop_assert_eq!(parse_event::<InboxEvent>(&InboxVocabulary, &printed).expect("parses"), one);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Law 32 through the real append: a response is written exactly when its phase stands at or
    /// above every phase the reading holds and on its floor, and a refusal writes nothing.
    #[test]
    fn the_append_refuses_a_response_that_does_not_climb(responses in prop::collection::vec(a_response(), 1..6)) {
        let scratch = Scratch::new();
        let store = &scratch.0;
        store.append(event(0, Said::Asked(Box::new(asked())))).expect("asked");
        for (i, response) in responses.iter().enumerate() {
            let before = phases(store);
            let held = store.dag().expect("reads").objects().len();
            let phase = response.phase();
            let reading: Vec<Phase> = before.iter().copied().collect();
            let climbs = reading.iter().all(|held| held.le(&phase)) && phase.stands_on(&reading);
            let written = store.append(responded(u32::try_from(i).expect("few") + 1, response.clone()));
            prop_assert_eq!(written.is_ok(), climbs, "{:?} over {:?}", response, before);
            if !climbs {
                prop_assert_eq!(store.dag().expect("reads").objects().len(), held);
            }
        }
        prop_assert!(store.verify().is_empty());
    }

    /// Law 32, the homomorphism: two replicas that each only climbed, merged by putting their
    /// objects in one directory, read the join of their readings.
    #[test]
    fn the_reading_of_a_union_is_the_join_of_the_readings(
        shared in prop::collection::vec(a_response(), 0..3),
        mine in prop::collection::vec(a_response(), 0..4),
        theirs in prop::collection::vec(a_response(), 0..4),
    ) {
        let (here, there) = (Scratch::new(), Scratch::new());
        let _ = std::fs::remove_dir_all(there.0.root());
        here.0.append(event(0, Said::Asked(Box::new(asked())))).expect("asked");
        try_all(&here.0, 1, &shared);
        let tips: Vec<Hash> = here.0.tips().expect("derive").into_iter().collect();
        for tip in &tips {
            there.0.adopt(&here.0, tip).expect("adopts");
        }
        try_all(&here.0, 10, &mine);
        try_all(&there.0, 20, &theirs);
        let (a, b) = (phases(&here.0), phases(&there.0));
        for entry in std::fs::read_dir(there.0.root().join("objects")).expect("lists") {
            let path = entry.expect("an entry").path();
            std::fs::copy(&path, here.0.root().join("objects").join(path.file_name().expect("a name")))
                .expect("copies");
        }
        let both: Vec<Phase> = a.iter().chain(&b).copied().collect();
        let joined: HashSet<Phase> = maximal(&both, |phase| phase).into_iter().collect();
        prop_assert_eq!(phases(&here.0), joined);
        prop_assert!(here.0.verify().is_empty());
    }
}

/// A check stands only on a send that may have gone. Asked, then checked at once, then tapped:
/// the store refuses the check, and an inbox handed one anyway (a replica's objects, taken in
/// whole) neither panics when read nor lets a tap perform what it cannot then record.
#[test]
fn a_check_over_nothing_never_lets_a_tap_perform_unrecorded() {
    use prodrome_core::event::{canonical_envelope, mk_sealed, seal_hash};
    use suzatary_core::store::{InMemory, Prodrome as _};

    let checked = responded(1, Responded::Checked(()));

    let door = InMemory::<InboxEvent>::default();
    door.append(event(0, Said::Asked(Box::new(asked()))))
        .expect("asked");
    assert!(
        door.append(checked.clone()).is_err(),
        "a check over a standing proposal is refused"
    );

    let first = mk_sealed(None, event(0, Said::Asked(Box::new(asked()))));
    let second = mk_sealed(Some(seal_hash(&first)), checked);
    let held = InMemory::<InboxEvent>::default();
    held.accept(&[first, second].map(|object| (seal_hash(&object), canonical_envelope(&object))))
        .expect("taken in whole");

    let fold = inbox::fold(&held).expect("the inbox reads");
    for object in fold.objects() {
        let _ = (object.reading.to_string(), object.reading.open());
    }
    let mut performed = 0;
    let owner = Actor::new("ada").expect("an actor");
    let tapped = inbox::submit(&held, &id(), &owner, at(2), |_| {
        performed += 1;
        Ok(Done::Sent {
            recipients: vec!["mira@example.com".to_owned()],
            message_id: "<1@box>".to_owned(),
        }
        .into())
    });
    assert!(
        performed == 0 || tapped.is_ok(),
        "the tap performed and recorded nothing: {tapped:?}"
    );
}

/// One unreadable proposal costs only itself: an inbox holding a check over nothing beside a
/// proposal that stands reads the one that stands, which a tap performs and records, and leaves
/// the other cannot be read, which a tap is refused and `verify` reports.
#[test]
fn one_unreadable_proposal_leaves_every_other_tappable() {
    use prodrome_core::event::{canonical_envelope, mk_sealed, seal_hash};
    use suzatary_core::secretary::inbox::Unchecked;
    use suzatary_core::store::{InMemory, Prodrome as _};

    let other = Asked {
        proposal: Proposal::Email(Draft {
            account: "ada@example.com".to_owned(),
            to: vec!["mira@example.com".to_owned()],
            cc: Vec::new(),
            subject: "Friday".to_owned(),
            body: "Friday works.".to_owned(),
            attachments: Vec::new(),
        }),
        ..asked()
    };
    let standing = ProposalId::of(&other, at(0));
    let events = [
        event(0, Said::Asked(Box::new(asked()))),
        responded(1, Responded::Checked(())),
        InboxEvent {
            proposal: standing.clone(),
            ..event(2, Said::Asked(Box::new(other)))
        },
    ];
    let mut objects = Vec::new();
    for event in events {
        let object = mk_sealed(
            objects
                .last()
                .map(|(name, _): &(Hash, String)| name.clone()),
            event,
        );
        objects.push((seal_hash(&object), canonical_envelope(&object)));
    }
    let held = InMemory::<InboxEvent>::default();
    held.accept(&objects).expect("taken in whole");

    let fold = inbox::fold(&held).expect("the inbox reads");
    assert_eq!(
        fold.get(&id()).map(|object| &object.reading),
        Some(&suzatary_core::secretary::inbox_reading::Reading::Unreadable(Unchecked))
    );
    assert!(fold.get(&standing).is_some(), "the other proposal reads");
    let owner = Actor::new("ada").expect("an actor");
    let sent = || -> Result<suzatary_core::secretary::responded::Performed, _> {
        Ok(Done::Sent {
            recipients: vec!["mira@example.com".to_owned()],
            message_id: "<2@box>".to_owned(),
        }
        .into())
    };
    let refused = inbox::submit(&held, &id(), &owner, at(3), |_| sent()).expect_err("unreadable");
    assert!(
        refused.message().contains("cannot be read"),
        "{}",
        refused.message()
    );
    assert!(matches!(
        inbox::submit(&held, &standing, &owner, at(4), |_| sent()),
        Ok(inbox::Response::Ran(_))
    ));
    let findings = inbox::fold(&held).expect("the inbox reads").findings();
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains(&format!("proposal {} cannot be read", id()))),
        "verify names the unreadable proposal: {findings:?}"
    );
}
