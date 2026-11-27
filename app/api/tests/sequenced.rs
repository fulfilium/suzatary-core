//! The order every reading of a conversation takes ([`sequence::sequenced`]), on a store the size
//! of the box's own (`known_pull::store`), with a tab's renames among the box's lines: every
//! conversation reads its turns in the order it read them before the box's lines were the spine,
//! since the box wrote every turn. A rename concurrent with the box's lines is placed after the
//! box's last line, wherever its clock puts it. The time of a conversation's first read is printed
//! as a measurement, beside the order's own cost under each function.

use std::collections::BTreeMap;

use prodrome_data::change::mk_change;
use prodrome_data::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_data::literal::Datetime;
use prodrome_data::schema::Schema as _;
use suzatary_conversations::secretary::store::{Entry, Namer, Title};
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::line_shape::LineShape;
use suzatary_lines::secretary::sequence::{self, Node, Order};
use suzatary_lines::secretary::spoken::Spoken;
use suzatary_lines::secretary::transcript;
use suzatary_lines::wire::lines::LineWriter;
use suzatary_memory::secretary::words::Words;
use suzatary_store::store::{Doc as _, Prodrome as _, Reads as _};
use suzatary_stores::stores::conversations::InMemory;

/// How many conversations get a tab's rename.
const RENAMED: usize = 10;

/// `lines` with a rename by a tab of each of the first `RENAMED` conversations, resting on its
/// opening alone and dated before every box line: concurrent with all of them.
fn renamed(lines: &InMemory) {
    let objects = lines.lines.objects().expect("reads");
    let genesis = objects
        .iter()
        .find(|(_, object)| matches!(object, Envelope::Genesis(_)))
        .map(|(name, _)| name.clone())
        .expect("a genesis");
    let mut posted = Vec::new();
    for n in 0..RENAMED {
        let id = crate::known_pull::id(n);
        let opening = objects
            .iter()
            .find(|(_, object)| {
                object.event().is_some_and(|line| {
                    *line.conversation() == id && matches!(line.shape(), LineShape::Opened { .. })
                })
            })
            .map(|(name, _)| name.clone())
            .expect("an opening");
        let (line, keys) = LineEvent::write(
            id,
            Actor::new("ada").expect("an actor"),
            1,
            &Entry::Titled {
                at: Datetime::new(2026, 9, 29, 0, 0, 0, 0).expect("an instant"),
                title: Words::kept(Title::new("Renamed by a tab").expect("a title")),
                by: Namer::Person {
                    actor: "ada".to_owned(),
                },
            },
            &lines.entropy,
        )
        .expect("a line");
        for key in &keys {
            lines.keys.set(&key.name(), key).expect("kept");
        }
        let change =
            Envelope::Change(mk_change(genesis.clone(), vec![opening], line).expect("a change"));
        posted.push((seal_hash(&change), canonical_envelope(&change)));
    }
    lines.lines.accept(&posted).expect("taken");
}

/// A line held: its name, the line and what it rests on.
type Held<'a> = (&'a Hash, &'a LineEvent, &'a [Hash]);

/// An order of one conversation's lines.
type Ordering = dyn Fn(&[Node<'_, Hash, Datetime, LineWriter>]) -> Order;

/// Each conversation's lines as the order reads them.
fn by_conversation(
    nodes: &[prodrome_dag::registers::Node<LineEvent>],
) -> BTreeMap<String, Vec<Held<'_>>> {
    let mut by: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for node in nodes {
        if let Some(line) = node.event() {
            by.entry(line.conversation().to_string())
                .or_default()
                .push((node.name(), line, node.parents()));
        }
    }
    by
}

fn view<'a>(
    lines: &[(&'a Hash, &'a LineEvent, &'a [Hash])],
) -> Vec<Node<'a, Hash, Datetime, LineWriter>> {
    lines
        .iter()
        .map(|&(name, line, parents)| Node {
            name,
            at: line.at(),
            parents,
            writer: LineWriter::of(line.actor()),
        })
        .collect()
}

/// The names of the turns, in `order`: the lines the fold makes a turn of.
fn turns<'a>(lines: &[(&'a Hash, &'a LineEvent, &[Hash])], order: &Order) -> Vec<&'a Hash> {
    order
        .iter()
        .map(|(at, _)| lines[*at])
        .filter(|(_, line, _)| {
            matches!(
                line.shape(),
                LineShape::Said { .. } | LineShape::Stopped { .. }
            )
        })
        .map(|(name, _, _)| name)
        .collect()
}

#[test]
fn every_conversation_reads_its_turns_in_the_order_it_read_them_before() {
    let lines = crate::known_pull::store();
    renamed(&lines);
    let nodes = lines.lines.nodes().expect("reads");
    let conversations = by_conversation(&nodes);
    assert_eq!(conversations.len(), 100);
    for (id, said) in &conversations {
        let before = sequence::causal(&view(said));
        let read = transcript::read(
            lines.conversations(),
            &suzatary_base::conversation_id::ConversationId::new(id.clone()).expect("an id"),
        )
        .expect("reads");
        let now: Vec<&Hash> = read
            .lines()
            .iter()
            .filter(|written| matches!(written.entry, Entry::Said { .. } | Entry::Stopped { .. }))
            .map(|written| &written.name)
            .collect();
        assert_eq!(now, turns(said, &before), "{id}");
        let last = read.lines().last().expect("a line");
        if read
            .lines()
            .iter()
            .any(|written| written.line.actor().as_str() == "ada")
        {
            assert_eq!(
                last.line.actor().as_str(),
                "ada",
                "{id}: the rename comes last"
            );
        }
    }
}

/// Printed as a measurement: the first read of one conversation (the whole store folded, from a
/// fresh cache), a round's start after it (the claim step), and the order alone over every
/// conversation, under [`sequence::causal`] and [`sequence::sequenced`].
#[test]
fn a_conversation_s_first_read_measured() {
    let median = |mut runs: Vec<std::time::Duration>| {
        runs.sort();
        runs[runs.len() / 2]
    };
    let first = median(
        (0..3)
            .map(|_| {
                let lines = crate::known_pull::store();
                renamed(&lines);
                let started = std::time::Instant::now();
                transcript::read(lines.conversations(), &crate::known_pull::id(0)).expect("reads");
                started.elapsed()
            })
            .collect(),
    );
    let start = median(
        (0..3)
            .map(|_| {
                use suzatary_lines::secretary::run::Runs as _;
                let lines = crate::known_pull::store();
                renamed(&lines);
                transcript::read(lines.conversations(), &crate::known_pull::id(0)).expect("reads");
                let run = lines.runs.begin().expect("a run");
                let started = std::time::Instant::now();
                let claimed = transcript::claim(
                    lines.conversations(),
                    &crate::known_pull::id(0),
                    &run.run,
                    Datetime::new(2026, 9, 30, 4, 0, 0, 0).expect("an instant"),
                    suzatary_lines::secretary::transcript::Retry::Allowed,
                    &|_| true,
                    |_| Vec::new(),
                )
                .expect("claims");
                let took = started.elapsed();
                assert_eq!(claimed, transcript::Claimed::Began);
                took
            })
            .collect(),
    );
    let lines = crate::known_pull::store();
    renamed(&lines);
    let nodes = lines.lines.nodes().expect("reads");
    let conversations = by_conversation(&nodes);
    let views: Vec<_> = conversations.values().map(|said| view(said)).collect();
    let timed = |order: &Ordering| {
        median(
            (0..5)
                .map(|_| {
                    let started = std::time::Instant::now();
                    for view in &views {
                        std::hint::black_box(order(view));
                    }
                    started.elapsed()
                })
                .collect(),
        )
    };
    let causal = timed(&|view| sequence::causal(view));
    let sequenced = timed(&|view| sequence::sequenced(view, LineWriter::spine));
    println!(
        "a conversation's first read over {} objects: median {first:?}; a round's start after it (the claim step: the read under the lock, the claim and its Began): median {start:?}; the order of all {} conversations: causal {causal:?}, sequenced {sequenced:?}",
        nodes.len(),
        views.len(),
    );
}

/// One step of a generated conversation: the box appends a person's turn over every line it
/// holds, or a tab posts one resting on some earlier lines, at its own clock.
#[derive(Debug, Clone)]
enum Step {
    Box,
    Tab {
        rests_on: Vec<proptest::sample::Index>,
        second: u32,
    },
}

fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
    use proptest::prelude::*;
    proptest::collection::vec(
        prop_oneof![
            Just(Step::Box),
            (
                proptest::collection::vec(any::<proptest::sample::Index>(), 1..3),
                0u32..60
            )
                .prop_map(|(rests_on, second)| Step::Tab { rests_on, second }),
        ],
        0..14,
    )
}

/// A conversation written by `steps`, each turn the box writes dated after every earlier line.
fn written(steps: &[Step]) -> InMemory {
    let lines = InMemory::default();
    let id = crate::known_pull::id(0);
    let at = |second: u32| {
        Datetime::new(2026, 11, 18, 9, second / 60, second % 60, 0).expect("an instant")
    };
    let said = |second: u32, text: &str| Entry::Said {
        at: at(second),
        turn: Spoken::person(text.to_owned(), Vec::new(), Vec::new()),
        by: None,
    };
    transcript::start(
        lines.conversations(),
        &id,
        &Entry::Opened {
            at: at(0),
            title: Words::kept("Permits".to_owned()),
            model: "a-cli/small".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        },
    )
    .expect("opened");
    for (n, step) in steps.iter().enumerate() {
        let n = u32::try_from(n).expect("small");
        match step {
            Step::Box => {
                transcript::push(lines.conversations(), &id, &said(60 + n, "From the box."))
                    .expect("pushed");
            }
            Step::Tab { rests_on, second } => {
                let objects = lines.lines.objects().expect("reads");
                let held: Vec<&Hash> = objects
                    .iter()
                    .filter(|(_, object)| object.event().is_some())
                    .map(|(name, _)| name)
                    .collect();
                let genesis = objects
                    .iter()
                    .find(|(_, object)| matches!(object, Envelope::Genesis(_)))
                    .map(|(name, _)| name.clone())
                    .expect("a genesis");
                let mut parents: Vec<Hash> = rests_on
                    .iter()
                    .map(|i| held[i.index(held.len())].clone())
                    .collect();
                // A tab writes over lines it holds as heads: none of them is beneath another.
                let dag: prodrome_dag::dag::Dag<LineEvent> = objects.iter().cloned().collect();
                let beneath =
                    dag.closure(parents.iter().flat_map(|parent| {
                        prodrome_data::event::parents_of(&dag.objects()[parent])
                    }));
                parents.retain(|parent| !beneath.contains(parent));
                parents.sort();
                parents.dedup();
                let (line, keys) = LineEvent::write(
                    id.clone(),
                    Actor::new("ada").expect("an actor"),
                    n,
                    &said(*second, "From a tab."),
                    &lines.entropy,
                )
                .expect("a line");
                for key in &keys {
                    lines.keys.set(&key.name(), key).expect("kept");
                }
                let change = Envelope::Change(mk_change(genesis, parents, line).expect("a change"));
                lines
                    .lines
                    .accept(&[(seal_hash(&change), canonical_envelope(&change))])
                    .expect("taken");
            }
        }
    }
    lines
}

proptest::proptest! {
    /// THE WINDOW LAW: a tab orders a conversation from the skeleton, which it holds whole, so the
    /// turns its outline folds are the box's turns, each at the box's index, and any lines of them
    /// it holds the content of read in the box's order, restricted to them.
    #[test]
    fn the_tab_orders_the_lines_it_holds_as_the_box_does(
        steps in steps(),
        subset in proptest::collection::vec(proptest::prelude::any::<bool>(), 14),
    ) {
        use suzatary_lines::secretary::outlines::{Outlines, Parsed};
        let lines = written(&steps);
        let id = crate::known_pull::id(0);
        let read = transcript::read(lines.conversations(), &id).expect("reads");
        let the_box: Vec<(usize, Hash)> = (0..read.stored().turns.len())
            .map(|turn| (turn, read.lines()[read.line_of(turn).expect("a line")].name.clone()))
            .collect();
        let nodes: Vec<Parsed> = lines
            .lines
            .objects()
            .expect("reads")
            .iter()
            .map(|(name, object)| {
                let sent = serde_json::to_string(&suzatary_api::routes::skeleton_node(name, object)).expect("prints");
                Parsed::of(serde_json::from_str(&sent).expect("reads back")).expect("a node")
            })
            .collect();
        let mut held = Outlines::default();
        held.hold(nodes, &[]);
        let (_, outline) = held.outlines().next().expect("one conversation");
        let the_tab: Vec<(usize, Hash)> = outline
            .as_ref()
            .expect("an outline")
            .rounds()
            .iter()
            .flat_map(|round| round.turns.iter().cloned())
            .collect();
        proptest::prop_assert_eq!(&the_tab, &the_box);
        let window = |turns: &[(usize, Hash)]| -> Vec<Hash> {
            turns
                .iter()
                .zip(subset.iter().cycle())
                .filter(|(_, fetched)| **fetched)
                .map(|((_, name), _)| name.clone())
                .collect()
        };
        proptest::prop_assert_eq!(window(&the_tab), window(&the_box));
    }
}

/// THE COMPARE LAW, on the big-store fixture: `secretary owed --compare`'s reading finds no
/// conversation whose turns' order moves; the claims' reading owes exactly what the reading before
/// it owed, on a store no claim was written to; and THE LAW OF THE FIRST START: the first claim
/// step claims no turn a start before the claim would not have sent, and reading no key, it
/// resumes exactly the conversations the box's own reading with its keys resumes.
#[test]
fn the_compare_finds_no_reorder_and_no_new_claim_on_the_big_store() {
    use suzatary_lines::secretary::compare::{self, Order};
    let lines = crate::known_pull::store();
    renamed(&lines);
    let now = Datetime::new(2026, 9, 30, 4, 0, 0, 0).expect("an instant");
    let compared = compare::compare(&lines.lines, now).expect("reads");
    assert_eq!(compared.len(), 100);
    let claiming = compared
        .iter()
        .filter(|(_, read)| !read.as_ref().expect("folds").claimed.is_empty())
        .count();
    assert!(claiming > 0, "the last hour's questions are claimed");
    let resumed = transcript::resumed(
        lines.conversations(),
        now,
        suzatary_lines::secretary::transcript::Retry::Allowed,
    )
    .expect("reads");
    for (id, read) in compared {
        let read = read.expect("folds");
        assert_eq!(read.order, Order::Kept, "{id}");
        assert_eq!(read.owed_by_claims, read.owed, "{id}");
        assert!(
            read.claimed.iter().all(|turn| read.resumed.contains(turn)),
            "{id}: {:?} claimed, {:?} resumed",
            read.claimed,
            read.resumed
        );
        assert_eq!(
            !read.claimed.is_empty(),
            resumed.contains(&id),
            "{id}: the keyless reading resumes what the box's own does"
        );
        let with_keys = transcript::read(lines.conversations(), &id).expect("reads");
        assert_eq!(
            read.owed,
            with_keys
                .owed()
                .into_iter()
                .map(|(turn, _)| turn)
                .collect::<Vec<_>>(),
            "{id}"
        );
    }
}
