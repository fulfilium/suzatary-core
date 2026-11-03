//! A DOM kept as data, which applies patches the way `web/src/painter/painter.ts` does: what the
//! painter's laws (`tests/laws.rs`) and every page's laws compare a patched page with.

use std::collections::BTreeMap;

use crate::node::Node;
use crate::paint::Painter;
use crate::patch::{Patch, ROOT};
use crate::program::{Program, Runtime};

/// A DOM as data: each node by id, an element's attributes, listeners and children in order.
#[derive(Debug, Clone, Default)]
pub struct Dom {
    /// Each node by id.
    pub nodes: BTreeMap<u32, Shape>,
    /// Each element's children, in order.
    pub kids: BTreeMap<u32, Vec<u32>>,
}

/// One node as the DOM holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Element {
        tag: &'static str,
        attrs: BTreeMap<String, String>,
        on: Vec<u8>,
    },
    Text(String),
    Foreign {
        kind: String,
        key: String,
        props: String,
    },
}

/// A node as a reader of the page sees it: no ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    Element {
        tag: &'static str,
        attrs: BTreeMap<String, String>,
        on: Vec<u8>,
        kids: Vec<Seen>,
    },
    Text(String),
    Foreign {
        kind: String,
        key: String,
        props: String,
    },
}

impl Dom {
    fn insert(&mut self, id: u32, parent: u32, before: Option<u32>) {
        for list in self.kids.values_mut() {
            list.retain(|&k| k != id);
        }
        let list = self.kids.entry(parent).or_default();
        let at = before
            .and_then(|b| list.iter().position(|&k| k == b))
            .unwrap_or(list.len());
        assert!(
            before.is_none() || at < list.len(),
            "inserted before {before:?}, which is not in {parent}"
        );
        list.insert(at, id);
    }

    fn drop_tree(&mut self, id: u32) {
        self.nodes.remove(&id);
        for k in self.kids.remove(&id).unwrap_or_default() {
            self.drop_tree(k);
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per patch, as the painter's own table"
    )]
    /// The patches applied, as the painter applies them; a patch the DOM cannot take panics.
    pub fn apply(&mut self, patches: &[Patch]) {
        for p in patches {
            match p {
                Patch::Element {
                    id,
                    parent,
                    before,
                    tag,
                } => {
                    assert!(!self.nodes.contains_key(id), "id {id} made twice");
                    self.nodes.insert(
                        *id,
                        Shape::Element {
                            tag: tag.name(),
                            attrs: BTreeMap::new(),
                            on: Vec::new(),
                        },
                    );
                    self.insert(*id, *parent, *before);
                }
                Patch::Text {
                    id,
                    parent,
                    before,
                    text,
                } => {
                    self.nodes.insert(*id, Shape::Text(text.clone()));
                    self.insert(*id, *parent, *before);
                }
                Patch::Foreign {
                    id,
                    parent,
                    before,
                    kind,
                    key,
                    props,
                } => {
                    self.nodes.insert(
                        *id,
                        Shape::Foreign {
                            kind: kind.clone(),
                            key: key.clone(),
                            props: props.clone(),
                        },
                    );
                    self.insert(*id, *parent, *before);
                }
                Patch::Set { id, name, value } => match self.nodes.get_mut(id) {
                    Some(Shape::Element { attrs, .. }) => {
                        attrs.insert(name.clone(), value.clone());
                    }
                    other => panic!("set on {other:?}"),
                },
                Patch::Unset { id, name } => match self.nodes.get_mut(id) {
                    Some(Shape::Element { attrs, .. }) => {
                        assert!(
                            attrs.remove(name).is_some(),
                            "unset {name}, which was not set"
                        );
                    }
                    other => panic!("unset on {other:?}"),
                },
                Patch::Retext { id, text } => match self.nodes.get_mut(id) {
                    Some(Shape::Text(t)) => t.clone_from(text),
                    other => panic!("retext on {other:?}"),
                },
                Patch::Reprop { id, props } => match self.nodes.get_mut(id) {
                    Some(Shape::Foreign { props: p, .. }) => p.clone_from(props),
                    other => panic!("reprop on {other:?}"),
                },
                Patch::Move { id, parent, before } => {
                    assert!(
                        self.nodes.contains_key(id),
                        "moved {id}, which is not there"
                    );
                    self.insert(*id, *parent, *before);
                }
                Patch::Remove { id } => {
                    assert!(
                        self.nodes.contains_key(id),
                        "removed {id}, which is not there"
                    );
                    for list in self.kids.values_mut() {
                        list.retain(|k| k != id);
                    }
                    self.drop_tree(*id);
                }
                Patch::Listen { id, on } => match self.nodes.get_mut(id) {
                    Some(Shape::Element { on: ons, .. }) => {
                        assert!(!ons.contains(&on.code()), "listened twice");
                        ons.push(on.code());
                        ons.sort_unstable();
                    }
                    other => panic!("listen on {other:?}"),
                },
                Patch::Unlisten { id, on } => match self.nodes.get_mut(id) {
                    Some(Shape::Element { on: ons, .. }) => {
                        assert!(ons.contains(&on.code()), "unlistened what it did not hear");
                        ons.retain(|o| *o != on.code());
                    }
                    other => panic!("unlisten on {other:?}"),
                },
            }
        }
    }

    fn seen(&self, id: u32) -> Seen {
        let kids = || {
            self.kids
                .get(&id)
                .map(|ks| ks.iter().map(|&k| self.seen(k)).collect())
                .unwrap_or_default()
        };
        match &self.nodes[&id] {
            Shape::Element { tag, attrs, on } => Seen::Element {
                tag,
                attrs: attrs.clone(),
                on: on.clone(),
                kids: kids(),
            },
            Shape::Text(t) => Seen::Text(t.clone()),
            Shape::Foreign { kind, key, props } => Seen::Foreign {
                kind: kind.clone(),
                key: key.clone(),
                props: props.clone(),
            },
        }
    }

    /// The page under the root, as a reader sees it.
    #[must_use]
    pub fn page(&self) -> Vec<Seen> {
        self.kids
            .get(&ROOT)
            .map(|ks| ks.iter().map(|&k| self.seen(k)).collect())
            .unwrap_or_default()
    }

    /// The ids of every node in the page, in order.
    pub fn ids(&self, id: u32, out: &mut Vec<u32>) {
        for &k in self.kids.get(&id).map(Vec::as_slice).unwrap_or_default() {
            out.push(k);
            self.ids(k, out);
        }
    }
}

/// What a fresh painter draws of `view`, as a reader sees it.
#[must_use]
pub fn fresh<M: Clone>(view: &Node<M>) -> Vec<Seen> {
    let mut dom = Dom::default();
    dom.apply(&Painter::start(view.clone()).1);
    dom.page()
}

/// A page's two laws, over one run of messages from `flags`: the view is a pure function of the
/// model (a second run of the same messages paints the same bytes, and the view drawn twice is
/// the same tree), and a patch applied equals a fresh draw (after every step the patched DOM is a
/// fresh draw of the model's view). Panics where one fails.
pub fn page_laws<P: Program<Msg: std::fmt::Debug>>(flags: impl Fn() -> P::Flags, msgs: &[P::Msg]) {
    let (mut one, first) = Runtime::<P>::start(flags());
    let (mut two, again) = Runtime::<P>::start(flags());
    assert_eq!(
        first.patches, again.patches,
        "the same flags paint the same first frame"
    );
    let mut dom = Dom::default();
    dom.apply(&crate::patch::decode(&first.patches).expect("the first frame reads back"));
    for msg in msgs {
        let (patches, _) = one.step(msg.clone());
        let (replayed, _) = two.step(msg.clone());
        assert_eq!(
            patches, replayed,
            "the same message from the same model paints the same patches"
        );
        dom.apply(&patches);
        let view = P::view(one.model());
        let drawn = fresh(&view);
        assert_eq!(
            drawn,
            fresh(&P::view(one.model())),
            "the view drawn twice is one tree"
        );
        assert_eq!(
            dom.page(),
            drawn,
            "after {msg:?}, the patched page is a fresh draw"
        );
    }
}
