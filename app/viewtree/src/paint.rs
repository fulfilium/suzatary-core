//! The keyed diff: the tree last painted, a new view, and the patches between them.
//! Keyed rows already in relative order (a longest increasing subsequence) stay put; a lazy part
//! whose hash is the one drawn at its place last time is neither drawn nor compared.

use std::collections::{BTreeMap, BTreeSet};

use crate::node::{Element, Key, Kids, Lazy, Listener, Never, Node, On, Picked};
use crate::patch::{Patch, ROOT};

type Painted<M> = Node<M, u32, Never>;

/// Also the hash of each lazy part, by the id of the node it drew.
#[derive(Debug)]
pub struct Painter<M> {
    next: u32,
    painted: Painted<M>,
    handlers: BTreeMap<u32, Vec<Listener<M>>>,
    lazies: BTreeMap<u32, u64>,
}

impl<M: Clone> Painter<M> {
    #[must_use]
    pub fn start(view: Node<M>) -> (Self, Vec<Patch>) {
        let mut out = Vec::new();
        let mut painter = Self {
            next: ROOT + 1,
            painted: Node::Text(ROOT, String::new()),
            handlers: BTreeMap::new(),
            lazies: BTreeMap::new(),
        };
        painter.painted = painter.create(view, ROOT, None, &mut out);
        (painter, out)
    }

    /// The patches that turn what was painted into `view`.
    pub fn paint(&mut self, view: Node<M>) -> Vec<Patch> {
        let mut out = Vec::new();
        // The empty text stands in only while the old tree is patched into the new one.
        let old = std::mem::replace(&mut self.painted, Node::Text(ROOT, String::new()));
        self.painted = self.patch(old, view, ROOT, &mut out);
        out
    }

    /// `None` when the element is gone, does not listen for `on`, or its handler answers none.
    #[must_use]
    pub fn message(&self, id: u32, on: On, words: String) -> Option<M> {
        self.handlers
            .get(&id)?
            .iter()
            .find(|l| l.on() == on)
            .map(|l| match l {
                Listener::Click(m) | Listener::Submit(m) => Some(m.clone()),
                Listener::Input(f) | Listener::Intent(f) => Some(f.call(words)),
                Listener::Keydown(f) => Key::read(&words).and_then(|k| f.call(k)),
                Listener::Toggle(f) => Some(f.call(!words.is_empty())),
                Listener::Files(f) => Some(f.call(Picked::read(&words))),
            })?
    }

    fn id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn create(
        &mut self,
        view: Node<M>,
        parent: u32,
        before: Option<u32>,
        out: &mut Vec<Patch>,
    ) -> Painted<M> {
        match view {
            Node::Lazy(l) => {
                let node = self.create(l.draw(), parent, before, out);
                self.lazies.insert(id_of(&node), l.hash);
                node
            }
            Node::Text((), text) => {
                let id = self.id();
                out.push(Patch::Text {
                    id,
                    parent,
                    before,
                    text: text.clone(),
                });
                Node::Text(id, text)
            }
            Node::Foreign((), f) => {
                let id = self.id();
                out.push(Patch::Foreign {
                    id,
                    parent,
                    before,
                    kind: f.kind.to_owned(),
                    key: f.key.clone(),
                    props: f.props.clone(),
                });
                Node::Foreign(id, f)
            }
            Node::Block((), b) => {
                let id = self.id();
                out.push(Patch::Block {
                    id,
                    parent,
                    before,
                    key: b.key.clone(),
                    fallback: b.fallback.clone(),
                });
                Node::Block(id, b)
            }
            Node::Element(e) => {
                let Element {
                    tag,
                    attrs,
                    on,
                    kids,
                    ..
                } = *e;
                let id = self.id();
                out.push(Patch::Element {
                    id,
                    parent,
                    before,
                    tag,
                });
                for (a, v) in &attrs {
                    out.push(Patch::Set {
                        id,
                        name: a.name(),
                        value: v.clone(),
                    });
                }
                for l in &on {
                    out.push(Patch::Listen { id, on: l.on() });
                }
                if !on.is_empty() {
                    self.handlers.insert(id, on.clone());
                }
                let kids = match kids {
                    Kids::Plain(xs) => Kids::Plain(
                        xs.into_iter()
                            .map(|x| self.create(x, id, None, out))
                            .collect(),
                    ),
                    Kids::Keyed(xs) => Kids::Keyed(
                        xs.into_iter()
                            .map(|(k, x)| (k, self.create(x, id, None, out)))
                            .collect(),
                    ),
                };
                Node::Element(Box::new(Element {
                    id,
                    tag,
                    attrs,
                    on,
                    kids,
                }))
            }
        }
    }

    fn remove(&mut self, old: &Painted<M>, out: &mut Vec<Patch>) {
        out.push(Patch::Remove { id: id_of(old) });
        self.forget(old);
    }

    fn forget(&mut self, old: &Painted<M>) {
        self.lazies.remove(&id_of(old));
        if let Node::Element(e) = old {
            self.handlers.remove(&e.id);
            match &e.kids {
                Kids::Plain(xs) => xs.iter().for_each(|x| self.forget(x)),
                Kids::Keyed(xs) => xs.iter().for_each(|(_, x)| self.forget(x)),
            }
        }
    }

    fn patch(
        &mut self,
        old: Painted<M>,
        view: Node<M>,
        parent: u32,
        out: &mut Vec<Patch>,
    ) -> Painted<M> {
        let view = match view {
            Node::Lazy(l) => return self.lazy(old, &l, parent, out),
            Node::Text(..) | Node::Element(_) | Node::Foreign(..) | Node::Block(..) => view,
        };
        // What stands here now is drawn by no lazy part, whatever drew it before.
        self.lazies.remove(&id_of(&old));
        match (old, view) {
            (Node::Text(id, was), Node::Text((), text)) => {
                if was != text {
                    out.push(Patch::Retext {
                        id,
                        text: text.clone(),
                    });
                }
                Node::Text(id, text)
            }
            (Node::Foreign(id, was), Node::Foreign((), f))
                if was.kind == f.kind && was.key == f.key =>
            {
                if was.props != f.props {
                    out.push(Patch::Reprop {
                        id,
                        props: f.props.clone(),
                    });
                }
                Node::Foreign(id, f)
            }
            (Node::Block(id, was), Node::Block((), b)) => {
                if was != b {
                    out.push(Patch::Rekey {
                        id,
                        key: b.key.clone(),
                        fallback: b.fallback.clone(),
                    });
                }
                Node::Block(id, b)
            }
            (Node::Element(was), Node::Element(e)) if was.tag == e.tag => {
                self.element(*was, *e, out)
            }
            (old, view) => {
                let made = self.create(view, parent, Some(id_of(&old)), out);
                self.remove(&old, out);
                made
            }
        }
    }

    fn lazy(
        &mut self,
        old: Painted<M>,
        l: &Lazy<M>,
        parent: u32,
        out: &mut Vec<Patch>,
    ) -> Painted<M> {
        if self.lazies.get(&id_of(&old)) == Some(&l.hash) {
            return old;
        }
        let node = self.patch(old, l.draw(), parent, out);
        self.lazies.insert(id_of(&node), l.hash);
        node
    }

    fn element(
        &mut self,
        was: Element<M, u32, Never>,
        e: Element<M>,
        out: &mut Vec<Patch>,
    ) -> Painted<M> {
        let id = was.id;
        for (a, _) in &was.attrs {
            if !e.attrs.iter().any(|(b, _)| b == a) {
                out.push(Patch::Unset { id, name: a.name() });
            }
        }
        for (a, v) in &e.attrs {
            if !was.attrs.iter().any(|(b, w)| b == a && w == v) {
                out.push(Patch::Set {
                    id,
                    name: a.name(),
                    value: v.clone(),
                });
            }
        }
        let had: BTreeSet<On> = was.on.iter().map(Listener::on).collect();
        let has: BTreeSet<On> = e.on.iter().map(Listener::on).collect();
        out.extend(had.difference(&has).map(|&on| Patch::Unlisten { id, on }));
        out.extend(has.difference(&had).map(|&on| Patch::Listen { id, on }));
        if e.on.is_empty() {
            self.handlers.remove(&id);
        } else {
            self.handlers.insert(id, e.on.clone());
        }
        let kids = match (was.kids, e.kids) {
            (Kids::Plain(olds), Kids::Plain(news)) => Kids::Plain(self.plain(olds, news, id, out)),
            (Kids::Keyed(olds), Kids::Keyed(news)) => Kids::Keyed(self.keyed(olds, news, id, out)),
            (olds, news) => {
                match &olds {
                    Kids::Plain(xs) => xs.iter().for_each(|x| self.remove(x, out)),
                    Kids::Keyed(xs) => xs.iter().for_each(|(_, x)| self.remove(x, out)),
                }
                match news {
                    Kids::Plain(xs) => Kids::Plain(
                        xs.into_iter()
                            .map(|x| self.create(x, id, None, out))
                            .collect(),
                    ),
                    Kids::Keyed(xs) => Kids::Keyed(
                        xs.into_iter()
                            .map(|(k, x)| (k, self.create(x, id, None, out)))
                            .collect(),
                    ),
                }
            }
        };
        Node::Element(Box::new(Element {
            id,
            tag: e.tag,
            attrs: e.attrs,
            on: e.on,
            kids,
        }))
    }

    fn plain(
        &mut self,
        olds: Vec<Painted<M>>,
        news: Vec<Node<M>>,
        parent: u32,
        out: &mut Vec<Patch>,
    ) -> Vec<Painted<M>> {
        let mut olds = olds.into_iter();
        let mut made = Vec::with_capacity(news.len());
        for view in news {
            made.push(match olds.next() {
                Some(old) => self.patch(old, view, parent, out),
                None => self.create(view, parent, None, out),
            });
        }
        for old in olds {
            self.remove(&old, out);
        }
        made
    }

    fn keyed(
        &mut self,
        olds: Vec<(String, Painted<M>)>,
        news: Vec<(String, Node<M>)>,
        parent: u32,
        out: &mut Vec<Patch>,
    ) -> Vec<(String, Painted<M>)> {
        let wanted: BTreeSet<&str> = news.iter().map(|(k, _)| k.as_str()).collect();
        let mut kept: BTreeMap<String, (usize, Painted<M>)> = BTreeMap::new();
        for (at, (k, old)) in olds.into_iter().enumerate() {
            if wanted.contains(k.as_str()) {
                kept.insert(k, (at, old));
            } else {
                self.remove(&old, out);
            }
        }
        // Each new row, patched where it was kept, with its old place; a new key is made below.
        let rows: Vec<(String, Matched<M>)> = news
            .into_iter()
            .map(|(k, view)| match kept.remove(&k) {
                Some((at, old)) => {
                    let node = self.patch(old, view, parent, out);
                    (k, Matched::Kept { at, node })
                }
                None => (k, Matched::New(view)),
            })
            .collect();
        let places: Vec<usize> = rows
            .iter()
            .filter_map(|(_, m)| match m {
                Matched::Kept { at, .. } => Some(*at),
                Matched::New(_) => None,
            })
            .collect();
        let stays: BTreeSet<usize> = increasing(&places).into_iter().map(|i| places[i]).collect();
        // From the end backwards, so `before` is always a node already in its place.
        let mut before: Option<u32> = None;
        let mut made = Vec::with_capacity(rows.len());
        for (k, row) in rows.into_iter().rev() {
            let node = match row {
                Matched::Kept { at, node } => {
                    if !stays.contains(&at) {
                        out.push(Patch::Move {
                            id: id_of(&node),
                            parent,
                            before,
                        });
                    }
                    node
                }
                Matched::New(view) => self.create(view, parent, before, out),
            };
            before = Some(id_of(&node));
            made.push((k, node));
        }
        made.reverse();
        made
    }
}

enum Matched<M> {
    Kept { at: usize, node: Painted<M> },
    New(Node<M>),
}

fn id_of<M>(n: &Painted<M>) -> u32 {
    match n {
        Node::Text(id, _) | Node::Foreign(id, _) | Node::Block(id, _) => *id,
        Node::Element(e) => e.id,
        Node::Lazy(never) => match *never {},
    }
}

/// The indices of a longest strictly increasing subsequence of `xs` (patience sorting).
fn increasing(xs: &[usize]) -> Vec<usize> {
    let mut from: Vec<Option<usize>> = vec![None; xs.len()];
    let mut tails: Vec<usize> = Vec::new();
    for (i, &x) in xs.iter().enumerate() {
        let at = tails.partition_point(|&t| xs[t] < x);
        from[i] = at.checked_sub(1).map(|p| tails[p]);
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(i);
        at = from[i];
    }
    out.reverse();
    out
}
