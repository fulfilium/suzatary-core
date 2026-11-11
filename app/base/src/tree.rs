//! Todos, subtodos and groups as one tree of terms.
//!
//! A [`Classifier`] sends a todo to [`Paths`]; the paths build a [`Trie`]; the
//! trie's catamorphism builds a term per node ([`term`]): a todo is
//! `Ref(todo)`, a group the `Conj` of its children at p = −4. Nothing here
//! computes a number: `crate::view::tree` links each term and asks the core.
//!
//! A subtodo is a `Ref` in its parent's spec, so ancestry is derived from the
//! specs, never stored. A key (category, urgency) classifies a subtree by its
//! root: a subtodo sits wherever its parent does.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::TodoId;
use prodrome_core::fpl::{LinkError, PRIORITY_POWER};
use prodrome_core::term::{Term, TermF};

/// An associative combine with a unit.
pub trait Monoid {
    /// The unit.
    fn empty() -> Self;
    /// The combine.
    #[must_use]
    fn combine(self, other: Self) -> Self;
}

/// A rose tree: `map` is its functor, `cata` its fold.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree<A> {
    /// This node's value.
    pub node: A,
    /// Its subtrees, in order.
    pub children: Vec<Tree<A>>,
}

impl<A> Tree<A> {
    /// The functor.
    pub fn map<B, F: FnMut(A) -> B>(self, f: &mut F) -> Tree<B> {
        Tree {
            node: f(self.node),
            children: self.children.into_iter().map(|c| c.map(f)).collect(),
        }
    }

    /// The catamorphism.
    pub fn cata<B, F: FnMut(A, Vec<B>) -> B>(self, alg: &mut F) -> B {
        let children = self.children.into_iter().map(|c| c.cata(alg)).collect();
        alg(self.node, children)
    }

    /// Every node beside what `alg` folds its subtree to: the cata that keeps
    /// its intermediate results.
    pub fn scan<B, F: FnMut(&A, &[&B]) -> B>(self, alg: &mut F) -> Tree<(A, B)> {
        self.cata(&mut |node, children: Vec<Tree<(A, B)>>| {
            let below: Vec<&B> = children.iter().map(|c| &c.node.1).collect();
            let folded = alg(&node, &below);
            Tree {
                node: (node, folded),
                children,
            }
        })
    }

    /// Every node, parents before children.
    pub fn nodes(&self) -> Vec<&A> {
        let mut out = vec![&self.node];
        for child in &self.children {
            out.extend(child.nodes());
        }
        out
    }
}

/// Where a value falls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    /// Below 0.5, or no value at all: a reference that does not link is a
    /// problem to fix.
    Problem,
    /// 0.5 to below 0.7.
    Watch,
    /// 0.7 and up.
    Fine,
    /// No number: `∅`, the value of a todo with no function. Not a problem and
    /// not fine, and after every priced band, as the core's list order puts it.
    Unpriced,
}

impl Band {
    /// The band of a worth.
    #[must_use]
    pub fn of(worth: &Result<Option<f64>, LinkError>) -> Band {
        match worth {
            Ok(None) => Band::Unpriced,
            Ok(Some(value)) if *value >= 0.7 => Band::Fine,
            Ok(Some(value)) if *value >= 0.5 => Band::Watch,
            Ok(Some(_)) | Err(_) => Band::Problem,
        }
    }

    /// The section's name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Band::Problem => "Problem",
            Band::Watch => "Watch",
            Band::Fine => "Fine",
            Band::Unpriced => "Unpriced",
        }
    }
}

/// One step of a path: a group a key names, or the todo its subtodos sit
/// under.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Segment {
    /// A category group.
    Category(String),
    /// An urgency section.
    Band(Band),
    /// A todo, whose subtodos are its children.
    Todo(TodoId),
}

/// A word over segments with no segment twice in a row: the free monoid on
/// segments quotiented by `s·s = s`, so grouping by one key twice is grouping
/// by it once.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Path(Vec<Segment>);

impl Path {
    /// The path of one segment.
    #[must_use]
    pub fn one(segment: Segment) -> Path {
        Path(vec![segment])
    }

    /// Its segments, root first.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.0
    }
}

impl Monoid for Path {
    fn empty() -> Path {
        Path::default()
    }

    /// Both sides are reduced, so only the seam can repeat.
    fn combine(mut self, other: Path) -> Path {
        let mut rest = other.0.into_iter().peekable();
        if rest.peek().is_some() && rest.peek() == self.0.last() {
            rest.next();
        }
        self.0.extend(rest);
        self
    }
}

/// Every place one todo sits, under the pointwise product: `{[]}` is the
/// unit and the combine concatenates every pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths(BTreeSet<Path>);

impl Paths {
    /// A single path.
    #[must_use]
    pub fn one(path: Path) -> Paths {
        Paths(BTreeSet::from([path]))
    }

    /// The paths.
    pub fn iter(&self) -> impl Iterator<Item = &Path> {
        self.0.iter()
    }
}

impl Monoid for Paths {
    fn empty() -> Paths {
        Paths::one(Path::empty())
    }

    fn combine(self, other: Paths) -> Paths {
        Paths(
            self.0
                .iter()
                .flat_map(|left| {
                    other
                        .0
                        .iter()
                        .map(move |right| left.clone().combine(right.clone()))
                })
                .collect(),
        )
    }
}

/// One way to group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// The root's category.
    Category,
    /// The root's band.
    Urgency,
    /// The todos above it, through the references.
    Ancestry,
}

/// A classifier: the free monoid on [`By`], its unit the flat list.
/// [`Classifier::paths`] interprets it, a homomorphism into [`Paths`] per
/// route.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Classifier(Vec<By>);

impl Classifier {
    /// The classifier of one generator.
    #[must_use]
    pub fn by(by: By) -> Classifier {
        Classifier(vec![by])
    }

    /// Where `todo` sits: one path per route to it. Without `Ancestry` there
    /// is one route, the todo alone, so keys read the todo itself.
    #[must_use]
    pub fn paths(&self, todo: &TodoId, members: &Members) -> Paths {
        let routes = if self.0.contains(&By::Ancestry) {
            routes(members, todo, &mut vec![todo.clone()])
        } else {
            vec![Vec::new()]
        };
        Paths(
            routes
                .into_iter()
                .map(|route| {
                    let root = &members[route.first().unwrap_or(todo)];
                    self.0
                        .iter()
                        .map(|by| match by {
                            By::Category => Path::one(Segment::Category(root.category.clone())),
                            By::Urgency => Path::one(Segment::Band(Band::of(&root.worth))),
                            By::Ancestry => {
                                Path(route.iter().cloned().map(Segment::Todo).collect())
                            }
                        })
                        .fold(Path::empty(), Path::combine)
                })
                .collect(),
        )
    }
}

impl Monoid for Classifier {
    fn empty() -> Classifier {
        Classifier::default()
    }

    fn combine(mut self, other: Classifier) -> Classifier {
        self.0.extend(other.0);
        self
    }
}

/// Every route from a root down to `todo`, excluding it, root first. A parent
/// already on the walk is cut, so a cycle is placed rather than unrolled.
fn routes(members: &Members, todo: &TodoId, walk: &mut Vec<TodoId>) -> Vec<Vec<TodoId>> {
    let parents: Vec<&TodoId> = members[todo]
        .parents
        .iter()
        .filter(|parent| !walk.contains(parent))
        .collect();
    if parents.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for parent in parents {
        walk.push(parent.clone());
        for mut route in routes(members, parent, walk) {
            route.push(parent.clone());
            out.push(route);
        }
        walk.pop();
    }
    out
}

/// A todo the tree places: what the classifiers read and what a row shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    /// The action line, as plain text.
    pub title: String,
    /// Its category, or its kind's.
    pub category: String,
    /// Its own value (`None` is `∅`), or why its function does not link.
    pub worth: Result<Option<f64>, LinkError>,
    /// The members whose spec holds a `Ref` to it.
    pub parents: BTreeSet<TodoId>,
}

/// The todos a tree places, by id.
pub type Members = BTreeMap<TodoId, Member>;

/// Each member's parents: the members whose spec holds a `Ref` to it.
#[must_use]
pub fn parents<'a>(
    specs: impl IntoIterator<Item = (&'a TodoId, Option<&'a Term>)>,
) -> BTreeMap<TodoId, BTreeSet<TodoId>> {
    let specs: Vec<_> = specs.into_iter().collect();
    let listed: BTreeSet<&TodoId> = specs.iter().map(|(todo, _)| *todo).collect();
    let mut out: BTreeMap<TodoId, BTreeSet<TodoId>> = BTreeMap::new();
    for (parent, spec) in &specs {
        for child in spec.map(refs).unwrap_or_default() {
            if let Some(child) = TodoId::new(child).ok().filter(|c| listed.contains(c)) {
                out.entry(child).or_default().insert((*parent).clone());
            }
        }
    }
    out
}

/// Every todo a term references.
#[must_use]
pub fn refs(term: &Term) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    if let TermF::Ref {
        store: None,
        entity,
    } = term.out()
    {
        out.insert(entity.as_str());
    }
    for child in term.out().children() {
        out.extend(refs(child));
    }
    out
}

/// Paths, shared by prefix: the free structure on them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trie(BTreeMap<Segment, Trie>);

impl Trie {
    /// Every member placed by `classifier`, each at its paths then itself.
    #[must_use]
    pub fn of(members: &Members, classifier: &Classifier) -> Trie {
        let mut trie = Trie::default();
        for todo in members.keys() {
            let own = Paths::one(Path::one(Segment::Todo(todo.clone())));
            for path in classifier.paths(todo, members).combine(own).iter() {
                trie.insert(path.segments());
            }
        }
        trie
    }

    fn insert(&mut self, segments: &[Segment]) {
        if let Some((first, rest)) = segments.split_first() {
            self.0.entry(first.clone()).or_default().insert(rest);
        }
    }

    /// The trie as a tree, its root the whole list.
    #[must_use]
    pub fn tree(self) -> Tree<Place> {
        fn under(trie: Trie) -> Vec<Tree<Place>> {
            trie.0
                .into_iter()
                .map(|(segment, below)| Tree {
                    node: Place::At(segment),
                    children: under(below),
                })
                .collect()
        }
        Tree {
            node: Place::All,
            children: under(self),
        }
    }
}

/// A node of the tree: the whole list, or a segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// The root.
    All,
    /// A group or a todo.
    At(Segment),
}

/// The term algebra: a todo is `Ref(todo)` — its subtodos are in its own
/// function — and a group is the `Conj` of its children at p = −4, a group of
/// one being that one.
#[must_use]
pub fn term(place: &Place, children: &[&Term]) -> Term {
    match (place, children) {
        (Place::At(Segment::Todo(todo)), _) => Term::new(TermF::Ref {
            store: None,
            entity: todo.as_str().to_owned(),
        }),
        (_, [one]) => (*one).clone(),
        (_, many) => Term::new(TermF::Conj {
            terms: many.iter().map(|t| (*t).clone()).collect(),
            p: PRIORITY_POWER,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn id(name: &str) -> TodoId {
        TodoId::new(name).expect("an id")
    }

    fn segment() -> impl Strategy<Value = Segment> {
        prop_oneof![
            "[ab]".prop_map(Segment::Category),
            prop_oneof![Just(Band::Problem), Just(Band::Fine)].prop_map(Segment::Band),
            "[xy]".prop_map(|s| Segment::Todo(id(&format!("{s}{s}{s}")))),
        ]
    }

    fn path() -> impl Strategy<Value = Path> {
        proptest::collection::vec(segment(), 0..5).prop_map(|s| {
            s.into_iter()
                .map(Path::one)
                .fold(Path::empty(), Path::combine)
        })
    }

    proptest! {
        #[test]
        fn paths_form_a_monoid(a in path(), b in path(), c in path()) {
            prop_assert_eq!(a.clone().combine(Path::empty()), a.clone());
            prop_assert_eq!(Path::empty().combine(a.clone()), a.clone());
            prop_assert_eq!(
                a.clone().combine(b.clone()).combine(c.clone()),
                a.combine(b.combine(c))
            );
        }

        #[test]
        fn a_segment_twice_is_once(a in path(), s in segment()) {
            let once = a.clone().combine(Path::one(s.clone()));
            prop_assert_eq!(once.clone().combine(Path::one(s)), once);
        }

        #[test]
        fn path_sets_form_a_monoid(a in path(), b in path(), c in path(), d in path()) {
            let x = Paths(BTreeSet::from([a, b]));
            let y = Paths::one(c);
            let z = Paths(BTreeSet::from([d, Path::empty()]));
            prop_assert_eq!(x.clone().combine(Paths::empty()), x.clone());
            prop_assert_eq!(Paths::empty().combine(x.clone()), x.clone());
            prop_assert_eq!(
                x.clone().combine(y.clone()).combine(z.clone()),
                x.combine(y.combine(z))
            );
        }
    }

    fn member(category: &str, worth: f64, parents: &[&str]) -> Member {
        Member {
            title: String::new(),
            category: category.to_owned(),
            worth: Ok(Some(worth)),
            parents: parents.iter().map(|p| id(p)).collect(),
        }
    }

    /// `parent` (home, Fine) has `child` (work, Problem); `loner` is work.
    fn family() -> Members {
        BTreeMap::from([
            (id("parent"), member("home", 0.8, &[])),
            (id("child"), member("work", 0.2, &["parent"])),
            (id("loner"), member("work", 0.6, &[])),
        ])
    }

    fn placed(members: &Members, classifier: &Classifier, todo: &str) -> Vec<Vec<Segment>> {
        classifier
            .paths(&id(todo), members)
            .iter()
            .map(|p| p.segments().to_vec())
            .collect()
    }

    #[test]
    fn classifiers_compose_by_concatenation() {
        let members = family();
        let (u, c, a) = (
            Classifier::by(By::Urgency),
            Classifier::by(By::Category),
            Classifier::by(By::Ancestry),
        );
        for todo in ["parent", "child", "loner"] {
            assert_eq!(
                placed(
                    &members,
                    &u.clone().combine(c.clone()).combine(a.clone()),
                    todo
                ),
                placed(
                    &members,
                    &u.clone().combine(c.clone().combine(a.clone())),
                    todo
                )
            );
            assert_eq!(
                placed(&members, &Classifier::empty(), todo),
                vec![Vec::<Segment>::new()]
            );
        }
    }

    #[test]
    fn a_cycle_is_placed_not_unrolled() {
        let members = BTreeMap::from([
            (id("aaa"), member("home", 0.5, &["bbb"])),
            (id("bbb"), member("home", 0.5, &["aaa"])),
        ]);
        let tree = Trie::of(&members, &Classifier::by(By::Ancestry)).tree();
        assert_eq!(tree.children.len(), 2);
        assert!(tree.children.iter().all(|c| c.children.len() == 1));
    }
}
