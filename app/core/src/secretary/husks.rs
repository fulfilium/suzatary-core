//! The husk of a line, what the box serves of it once the key that opens its words is not sent:
//! the lines' own reading, which the routes and the pruning both ask.

use std::collections::{BTreeMap, BTreeSet};

use suzatary_base::refusal::Answered;

/// What the lines of `graph` retire of their keys, kept in the graph.
pub fn retired(
    graph: &suzatary_store::store::Graph<crate::secretary::line_event::LineEvent>,
) -> std::sync::Arc<crate::secretary::transcript::Retired> {
    graph.reading("retired", |graph| {
        crate::secretary::transcript::Retired::of(graph.objects())
    })
}

/// The key of every slot the lines named `sent` name, by name, but a key `retired` says is dead,
/// erased or superseded by a reseal the box opens.
pub fn keys_of(
    conversations: crate::secretary::conversations::Conversations<'_>,
    graph: &suzatary_store::store::Graph<crate::secretary::line_event::LineEvent>,
    retired: &crate::secretary::transcript::Retired,
    sent: &[prodrome_core::event::Hash],
) -> Answered<BTreeMap<String, String>> {
    let reads = conversations.reads();
    let slots = |name: &crate::secretary::locked_text::KeyName| reads.key(name);
    let line = |name: &prodrome_core::event::Hash| graph.dag().get(name)?.event();
    let mut keys = BTreeMap::new();
    for name in sent
        .iter()
        .filter_map(line)
        .flat_map(|sent| sent.keys().map(move |key| (sent.conversation(), key)))
        .filter(|(id, name)| !retired.retires(id, name, line, &slots))
        .map(|(_, name)| name)
    {
        if let Some(key) = conversations.keys.get(name)? {
            keys.insert(
                name.as_str().to_owned(),
                crate::secretary::key_ring::spelled(&key),
            );
        }
    }
    Ok(keys)
}

/// The printed husk of each line of `names` with words no key the box sends opens.
pub fn husks(
    conversations: crate::secretary::conversations::Conversations<'_>,
    graph: &suzatary_store::store::Graph<crate::secretary::line_event::LineEvent>,
    names: &[prodrome_core::event::Hash],
) -> Answered<BTreeMap<prodrome_core::event::Hash, String>> {
    let sent = keys_of(conversations, graph, &retired(graph), names)?;
    Ok(husked(graph, &sent, names))
}

/// [`husks`] of `names`, given `sent`, the keys [`keys_of`] sends of them.
pub fn husked(
    graph: &suzatary_store::store::Graph<crate::secretary::line_event::LineEvent>,
    sent: &BTreeMap<String, String>,
    names: &[prodrome_core::event::Hash],
) -> BTreeMap<prodrome_core::event::Hash, String> {
    names
        .iter()
        .filter_map(|name| {
            let line = graph.dag().get(name)?.event()?;
            if line.keys().any(|key| sent.contains_key(key.as_str())) {
                return None;
            }
            Some((name.clone(), suzatary_store::store::Husk::of(line)?.print()))
        })
        .collect()
}

/// Each line of `names` with words that no key opens now or later: every key of them is retired,
/// erased or superseded by a reseal the box opens, and destroyed. Every one is among [`husks`].
/// A key store that cannot be read refuses, and one that holds no keys at all leaves a line no
/// erasure or reseal retires out.
pub fn dead(
    conversations: crate::secretary::conversations::Conversations<'_>,
    graph: &suzatary_store::store::Graph<crate::secretary::line_event::LineEvent>,
    names: &[prodrome_core::event::Hash],
) -> Answered<BTreeSet<prodrome_core::event::Hash>> {
    let retired = retired(graph);
    let reads = conversations.reads();
    let slots = |name: &crate::secretary::locked_text::KeyName| reads.key(name);
    let line = |name: &prodrome_core::event::Hash| graph.dag().get(name)?.event();
    let mut dead = BTreeSet::new();
    'lines: for name in names {
        let Some(held) = line(name) else { continue };
        if held.keys().next().is_none() {
            continue;
        }
        for key in held.keys() {
            if !retired.retires(held.conversation(), key, line, &slots)
                || conversations.keys.get(key)?.is_some()
            {
                continue 'lines;
            }
        }
        dead.insert(name.clone());
    }
    Ok(dead)
}
