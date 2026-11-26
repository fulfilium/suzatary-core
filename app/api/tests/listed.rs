//! The conversations' list read by titles, each row from its skeleton and the few lines its words
//! are read from, is the list read from every line wherever both are held; its rows are the
//! skeleton's conversations whatever content the device holds; and a search over the
//! conversations held whole says exactly how many it read and finds in them what a search of the
//! whole store finds there.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

use prodrome_data::literal::Datetime;
use prodrome_store::store::frontier::{Frontiers, KEPT, Since};
use suzatary_api::routes::{self, Books, Cut};
use suzatary_conversations::secretary::search::{LIST_CONTEXT, Needle, search};
use suzatary_conversations::secretary::store::Stored;
use suzatary_inbox::secretary::inbox::Fold;
use suzatary_lines::secretary::held_replica::{HeldReplica, NotAnAnswer};
use suzatary_lines::secretary::outlines::{Nodes, Scope};
use suzatary_signatures::epoch::Epochs;
use suzatary_stores::stores::conversations::InMemory;
use suzatary_topics::secretary::readings::{self, SEARCH_LIMIT};
use suzatary_topics::wire::topics::{ConversationRow, ConversationsPayload, RowWords};

use suzatary_memory::secretary::key_ring::KeyRing;
use suzatary_memory::secretary::locked_text::{KeyName, opens};

use crate::held_lines::{Step, resealed, step, written};
use crate::pieces::fetched;

/// The count the suite affords; `PROPTEST_CASES` raises it for one run. A failing seed is kept
/// beside this file.
fn cases(n: u32) -> ProptestConfig {
    let n = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(n);
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource(
            "proptest-regressions",
        ))),
        ..ProptestConfig::with_cases(n)
    }
}

fn now() -> Datetime {
    Datetime::new(2026, 10, 1, 12, 0, 0, 0).expect("an instant")
}

/// The script's store, as `held_lines`' laws write it.
fn store(script: &[Step], stale: &[usize], swept: bool, after: &[Step]) -> InMemory {
    let held = written(script);
    resealed(&held, stale, swept, after);
    held
}

/// Every conversation of `held` as a tab that pulled it and holds every line reads it: under the
/// keys the box sends, which leave out every key an erasure anywhere names.
fn whole(held: &InMemory) -> Vec<std::sync::Arc<Stored>> {
    let mut tab = pulled(held);
    let every = tab.lacking(&Scope::Nodes(Nodes::Whole));
    given(&mut tab, held, &every);
    tab.lines.read().0.expect("the script writes conversations")
}

/// What a box remembers of the frontiers it named, so a tab's next pull asks from its own.
struct Boxed {
    epochs: Epochs,
    frontiers: Frontiers,
}

impl Boxed {
    fn new() -> Self {
        Boxed {
            epochs: Epochs::remembered(),
            frontiers: Frontiers::remembered(KEPT),
        }
    }

    fn books(&self) -> Books<'_> {
        Books {
            epochs: &self.epochs,
            frontiers: &self.frontiers,
        }
    }

    fn piece(
        &self,
        held: &InMemory,
        since: &Since,
    ) -> suzatary_lines::wire::lines::SkeletonPayload {
        routes::lines_skeleton(held.conversations(), since, self.books(), now(), Cut::Whole)
            .expect("the skeleton")
    }
}

/// A tab that pulled the skeleton of `held` whole and nothing of its content.
fn pulled(held: &InMemory) -> HeldReplica {
    let boxed = Boxed::new();
    HeldReplica::pulled(|_| -> Result<_, NotAnAnswer> { Ok(boxed.piece(held, &Since::Nothing)) })
        .expect("pulled")
}

/// `tab` once it pulled what `held` holds beyond what it holds, and let go of every key the box
/// no longer sends, as its purge does.
fn again(boxed: &Boxed, held: &InMemory, mut tab: HeldReplica) -> HeldReplica {
    let since = match tab.rows().holding().replica().frontier().digest() {
        Some(digest) => Since::Frontier(
            prodrome_store::store::frontier::Token::parse(digest.as_str()).expect("a digest"),
        ),
        None => Since::Nothing,
    };
    let piece = boxed.piece(held, &since);
    let given = piece
        .keys
        .iter()
        .map(|(name, key)| (KeyName::new(name.clone()).expect("a key name"), key.clone()))
        .collect();
    tab.piece(piece, KeyRing::of(&given), 1).expect("a piece");
    let _ = tab.land();
    let owed: BTreeSet<KeyName> = tab.owed(&BTreeSet::new()).into_keys().collect();
    let _ = tab.forget(owed);
    tab
}

/// Every line the titles lack asked for until they lack none: what `linesync.over` does with
/// `Titles`. No name is asked for twice.
fn titled(tab: &mut HeldReplica, held: &InMemory) -> Result<(), TestCaseError> {
    let mut asked = BTreeSet::new();
    loop {
        let need = tab.lacking(&Scope::Titles);
        if need.is_empty() {
            return Ok(());
        }
        for node in &need {
            prop_assert!(
                asked.insert(node.hash.clone()),
                "{} asked for twice",
                node.hash
            );
        }
        given(tab, held, &need);
    }
}

/// `tab` given the content of `nodes`, as the box answers it by name.
fn given(
    tab: &mut HeldReplica,
    held: &InMemory,
    nodes: &[suzatary_lines::wire::lines::SkeletonNode],
) {
    let sizes: BTreeMap<String, usize> = nodes
        .iter()
        .map(|node| (node.hash.clone(), node.size))
        .collect();
    tab.take(
        fetched(held.conversations(), &sizes)
            .into_values()
            .collect(),
    )
    .expect("taken");
}

/// The list as the device's core reads it.
fn listed(tab: &mut HeldReplica, fold: &Fold) -> ConversationsPayload {
    readings::listed(tab.listed(|id, read| {
        read.map(|(rounds, row)| readings::listed_row(id, rounds, row, fold))
            .map_err(Clone::clone)
    }))
    .expect("lists")
}

/// The conversation a node is a line of; none for a structure node.
fn conversation_of(node: &suzatary_lines::wire::lines::SkeletonNode) -> Option<&str> {
    match &node.header {
        suzatary_lines::wire::lines::NodeHeader::Line { conversation, .. } => Some(conversation),
        suzatary_lines::wire::lines::NodeHeader::Structure { .. } => None,
    }
}

/// Whether the newest live turn of `stored` that the skeleton does not say says no words (a
/// person's turn, or a model's with no calls) says words a list reads: where it does, the
/// skeleton decides the row's lines, and one fetch of what the row says it lacks reads it.
fn newest_says(stored: &Stored) -> bool {
    use suzatary_conversations::secretary::model::Turn;
    stored
        .live()
        .into_iter()
        .rev()
        .find(|(_, said)| match &said.turn {
            // An erased turn's shape stays, so the skeleton cannot tell it from the turn it was.
            Turn::Person { .. } | Turn::Erased { .. } => true,
            Turn::Model { calls, .. } => calls.is_empty(),
            Turn::Tool { .. } | Turn::Act { .. } => false,
        })
        .is_some_and(|(_, said)| {
            suzatary_conversations::secretary::search::words(&said.turn).is_some()
        })
}

/// What the skeleton says of a row, read before any of its words.
fn place(
    row: &ConversationRow,
) -> (
    &str,
    suzatary_conversations::secretary::store::Kind,
    &str,
    u32,
    &str,
) {
    (&row.id, row.kind, &row.opened, row.turns, &row.last_at)
}

proptest! {
    #![proptest_config(cases(24))]

    /// THE ROW LAW: over a store whatever was renamed, stopped, rewound, proposed, erased, sealed
    /// again or swept, and any part of its content held, the list's rows are the skeleton's
    /// conversations, newest first, each standing where the list read from every line puts it
    /// (its id, kind, opening, turns and last instant) and each either with the words that list
    /// reads or saying how many of its lines it lacks. Asking for what the titles lack until they
    /// lack nothing asks for no name twice and reads the list from every line exactly.
    #[test]
    fn the_list_by_titles_is_the_list_read_whole_where_held(
        script in proptest::collection::vec(step(), 1..60),
        stale in proptest::collection::vec(0usize..3, 0..3),
        swept in any::<bool>(),
        after in proptest::collection::vec(
            (0usize..3, proptest::collection::vec(any::<prop::sample::Index>(), 1..3))
                .prop_map(|(conversation, lines)| Step::Erase { conversation, lines }),
            0..3,
        ),
        holds in proptest::collection::vec(any::<bool>(), 1..32),
    ) {
        let held = store(&script, &stale, swept, &after);
        let fold = Fold::default();
        let all = whole(&held);
        let read_whole = readings::conversations(&all, &fold);

        let mut tab = pulled(&held);
        let nodes = tab.lacking(&Scope::Nodes(Nodes::Whole));
        let some: Vec<_> = nodes.iter().enumerate().filter(|(at, _)| holds[at % holds.len()]).map(|(_, node)| node.clone()).collect();
        given(&mut tab, &held, &some);
        let part = listed(&mut tab, &fold);
        prop_assert_eq!(part.conversations.iter().map(place).collect::<Vec<_>>(), read_whole.conversations.iter().map(place).collect::<Vec<_>>());
        let lacking = tab.lacking(&Scope::Titles);
        for (row, read) in part.conversations.iter().zip(&read_whole.conversations) {
            match &row.words {
                RowWords::Read { .. } => prop_assert_eq!(row, read),
                RowWords::NotHere { lines } => {
                    let asked = lacking.iter().filter(|node| conversation_of(node) == Some(row.id.as_str())).count();
                    prop_assert_eq!(*lines, asked, "a row says the lines a fetch asks for: {}", row.id);
                }
                RowWords::Unread {} => prop_assert!(false, "no print here is forged: {:?}", row),
            }
        }
        // Where a row's newest live turn says words, the count was exact: one fetch reads it.
        given(&mut tab, &held, &lacking);
        let once = listed(&mut tab, &fold);
        for ((row, read), stored) in once.conversations.iter().zip(&read_whole.conversations).zip(&all) {
            if newest_says(stored) && !matches!(part.conversations.iter().find(|r| r.id == row.id).map(|r| &r.words), Some(RowWords::Read { .. })) {
                prop_assert_eq!(row, read, "one fetch of what it said it lacked reads the row");
            }
        }

        titled(&mut tab, &held)?;
        prop_assert_eq!(listed(&mut tab, &fold), read_whole.clone());
        // A list read again over the same lines opens no sealed word: every step is a part of
        // its own, found in the memo.
        let before = opens();
        prop_assert_eq!(listed(&mut tab, &fold), read_whole);
        prop_assert_eq!(opens(), before, "a second list opens nothing");
    }

    /// THE FORGETTING LAW: an erasure, and the loss of the keys it names, that arrive after the
    /// rows were read are read by the next list: a row's memoised parts are forgotten as a
    /// round's are, and the list is the one a new device reads.
    #[test]
    fn rows_read_before_an_erasure_read_it_after(
        script in proptest::collection::vec(step(), 1..50),
        after in proptest::collection::vec(
            (0usize..7, proptest::collection::vec(any::<prop::sample::Index>(), 1..3))
                .prop_map(|(conversation, lines)| Step::Erase { conversation, lines }),
            1..3,
        ),
    ) {
        let held = store(&script, &[], false, &[]);
        let fold = Fold::default();
        let boxed = Boxed::new();
        let mut tab = HeldReplica::pulled(|_| -> Result<_, NotAnAnswer> { Ok(boxed.piece(&held, &Since::Nothing)) })
            .expect("pulled");
        titled(&mut tab, &held)?;
        prop_assert_eq!(listed(&mut tab, &fold), readings::conversations(&whole(&held), &fold));
        resealed(&held, &[], false, &after);
        let mut tab = again(&boxed, &held, tab);
        titled(&mut tab, &held)?;
        prop_assert_eq!(listed(&mut tab, &fold), readings::conversations(&whole(&held), &fold));
    }

    /// THE COVERAGE LAW: over any part of the content held, a search reads exactly the
    /// conversations whose every line is held, says so with the skeleton's count, and finds in
    /// them what a search of the whole store finds there, newest first.
    #[test]
    fn a_search_over_part_finds_the_whole_s_hits_in_what_it_read(
        script in proptest::collection::vec(step(), 1..60),
        holds in proptest::collection::vec(any::<bool>(), 1..32),
        q in prop_oneof![Just("answer"), Just("question"), Just("more of answer 1")],
    ) {
        let held = store(&script, &[], false, &[]);
        let all = whole(&held);
        let mut tab = pulled(&held);
        let nodes = tab.lacking(&Scope::Nodes(Nodes::Whole));
        let some: Vec<_> = nodes.iter().enumerate().filter(|(at, _)| holds[at % holds.len()]).map(|(_, node)| node.clone()).collect();
        given(&mut tab, &held, &some);

        let whole_ids: Vec<_> = all.iter().map(|stored| stored.id.clone()).collect();
        let read: BTreeSet<_> = whole_ids
            .iter()
            .filter(|id| tab.lacking(&Scope::Nodes(Nodes::Conversations(BTreeSet::from([(*id).clone()])))).is_empty())
            .cloned()
            .collect();
        let (held_whole, of) = tab.held_whole();
        let transcripts: Vec<_> = held_whole.into_iter().collect::<Result<_, _>>().expect("reads");
        let stored: Vec<&Stored> = transcripts.iter().map(|read| read.stored()).collect();
        let found = readings::search(&stored, of, suzatary_topics::wire::topics::Counted::Every, q).expect("a search");

        let mut expected = search(&all, &Needle::new(q).expect("a needle"), LIST_CONTEXT, usize::MAX);
        expected.retain(|hit| read.contains(&hit.conversation));
        expected.truncate(SEARCH_LIMIT);
        prop_assert_eq!(found.of, all.len());
        prop_assert_eq!(found.searched, read.len());
        prop_assert_eq!(found.hits, expected);
    }
}
