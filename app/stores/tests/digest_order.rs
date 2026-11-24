//! The laws of the digest log's order (`memorydigest`); law 2, that the order is a function
//! of the set of lines, is `digest::the_shape_is_a_function_of_the_set_of_lines`.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::digest::{Node, Tree};
use prodrome_core::event::Hash;
use prodrome_core::literal::Datetime;
use proptest::prelude::*;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::Entry;
use suzatary_digest::secretary::compaction::{Compaction, Reading};
use suzatary_digest::secretary::digest::Digest;
use suzatary_digest::secretary::digest_census::Census;
use suzatary_digest::secretary::digest_label::Label;
use suzatary_digest::secretary::digest_line::Line;
use suzatary_digest::secretary::digest_view;
use suzatary_store::store::Reads as _;

use crate::digest::{Box as Stores, at, model, named, write_line};

/// A log: each line's conversation (of four) and minute (of eight, so instants tie often), in
/// the order they are written.
fn log() -> impl Strategy<Value = Vec<(u8, usize)>> {
    proptest::collection::vec((0u8..4, 0usize..8), 1..40)
}

fn written(log: &[(u8, usize)]) -> (Stores, BTreeMap<u8, u32>) {
    let the = Stores::default();
    let mut held = BTreeMap::new();
    for &(conversation, minute) in log {
        say(&the, &mut held, conversation, minute);
    }
    (the, held)
}

fn say(the: &Stores, held: &mut BTreeMap<u8, u32>, conversation: u8, minute: usize) {
    let count = held.entry(conversation).or_default();
    let entry = Entry::Said {
        at: at(minute),
        turn: Turn::Person {
            text: format!("{conversation} line {count}"),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some("ada".into()),
    };
    let id = ConversationId::new(format!("2026-10-06-{conversation}")).expect("a slug");
    write_line(&the.lines, id, *count, &entry);
    *count += 1;
}

/// Every line by name: its conversation and instant.
fn stamps(the: &Stores) -> BTreeMap<Hash, (ConversationId, Datetime)> {
    the.lines
        .lines
        .objects()
        .expect("objects")
        .into_iter()
        .filter_map(|(name, object)| {
            let line = object.event()?;
            Some((name.clone(), (line.conversation().clone(), line.at())))
        })
        .collect()
}

fn leaves(read: &Digest) -> Vec<Hash> {
    nodes(read)
        .into_iter()
        .filter(|node| node.is_leaf())
        .map(|node| node.name().clone())
        .collect()
}

fn nodes(read: &Digest) -> Vec<&Node<Label>> {
    let mut out = Vec::new();
    let mut stack: Vec<_> = read.tree().map(Tree::root).into_iter().collect();
    while let Some(node) = stack.pop() {
        out.push(node);
        stack.extend(node.children().iter().rev());
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    /// Law 1, append lands last. A new line rests on its conversation's heads, so on every held
    /// line of its conversation and on nothing else. It moves no held line, whatever its clock
    /// says; when its instant is after every held line's of another conversation, or equal with
    /// a greater name, it is last. One written with an older clock lands among the lines it does
    /// not rest on, which is where its instant puts it.
    #[test]
    fn append_lands_last(log in log(), conversation in 0u8..5, minute in 0usize..10) {
        let (the, mut held) = written(&log);
        let before = leaves(&the.read());
        say(&the, &mut held, conversation, minute);
        let after = leaves(&the.read());
        let stamps = stamps(&the);
        let new = after.iter().find(|line| !before.contains(line)).expect("one new line").clone();
        let kept: Vec<Hash> = after.iter().filter(|line| **line != new).cloned().collect();
        prop_assert_eq!(&kept, &before, "the held lines keep their order");
        let (own, instant) = &stamps[&new];
        let after_every_other = stamps
            .iter()
            .filter(|(_, (of, _))| of != own)
            .all(|(name, (_, at))| (at, name) < (instant, &new));
        if after_every_other {
            prop_assert_eq!(after.last(), Some(&new));
        }
    }

    /// Law 3, prefix stability. An appended line renames only the nodes that hold the old last
    /// line, at most one per level ([`prodrome_core::memo::balance`]); every node of the new
    /// tree that does not hold the new last line is a node of the old one, of the same span,
    /// summarised alike, and a reader opening its handle is shown the same parts.
    #[test]
    fn an_append_keeps_every_node_before_it(log in log(), conversation in 0u8..5) {
        let (the, mut held) = written(&log);
        the.summarise_all();
        let old = Reading::of(&the.digests().reads()).expect("reads");
        let newest = log.iter().map(|(_, minute)| *minute).max().unwrap_or(0);
        say(&the, &mut held, conversation, newest + 1);
        let new = Reading::of(&the.digests().reads()).expect("reads");
        let (old_tree, new_tree) = (
            old.digest().tree().expect("lines"),
            new.digest().tree().expect("lines"),
        );
        for node in nodes(new.digest()) {
            if node.span().end == new_tree.lines() {
                continue;
            }
            let was = old.digest().node(node.name()).expect("an old node keeps its name");
            prop_assert_eq!(was.span(), node.span());
            prop_assert_eq!(old.digest().summary(node.name()), new.digest().summary(node.name()));
            let handle = format!("{}+{}", node.span().start, node.span().len());
            let shown = |reading: &Reading| {
                digest_view::opened(reading.digest(), &reading.entries(), &handle, |_| true)
            };
            prop_assert_eq!(shown(&old), shown(&new));
        }
        let renamed: Vec<_> = nodes(old.digest())
            .into_iter()
            .filter(|node| new.digest().node(node.name()).is_none())
            .collect();
        for node in &renamed {
            prop_assert_eq!(node.span().end, old_tree.lines(), "only nodes over the last line");
        }
        let levels: BTreeSet<usize> = renamed.iter().map(|node| node.level()).collect();
        prop_assert_eq!(levels.len(), renamed.len(), "at most one per level");
    }
}

/// The one redo after the order changes: a node is named by its children, so a node whose lines
/// another conversation's now come between is gone, and its summary with it. Every such summary
/// stays in the digest's history, is shown to no reader, is counted in none of the census's
/// totals and is counted on a line of its own.
#[test]
fn a_summary_of_a_span_no_longer_in_the_tree_reads_as_nothing() {
    let the = Stores::default();
    let mut held = BTreeMap::new();
    for minute in 0..24 {
        say(&the, &mut held, 0, minute);
    }
    the.summarise_all();
    let before = the.read();
    // Another conversation's lines, at the same minutes, land between the held ones.
    for minute in 0..24 {
        say(&the, &mut held, 1, minute);
    }
    let read = the.read();
    let gone: Vec<Hash> = nodes(&before)
        .into_iter()
        .filter(|node| before.summary(node.name()).is_some() && read.node(node.name()).is_none())
        .map(|node| node.name().clone())
        .collect();
    assert!(
        !gone.is_empty(),
        "the lines between rename a summarised node"
    );
    assert_eq!(read.orphaned(), gone.len());
    let shown: Vec<Line> = gone
        .iter()
        .map(|node| before.summary(node).expect("summarised").clone())
        .collect();
    let view = read.view(u64::MAX).expect("lines");
    assert!(view.parts.iter().all(|part| !gone.contains(&part.node)));
    for node in nodes(&read) {
        assert!(
            read.summary(node.name())
                .is_none_or(|words| !shown.contains(words))
        );
    }
    let census = Census::of(
        &read,
        &Compaction {
            ladder: named(model()),
        },
    );
    let said = census.to_string();
    let line = format!("summaries of spans no longer in the tree: {}\n", gone.len());
    assert!(said.contains(&line), "{said}");
    let counted = census.count(|_, _, _| true);
    assert_eq!(
        counted,
        nodes(&read).iter().filter(|node| !node.is_leaf()).count()
    );
}
