//! The tree's laws: a group of one is its todo; grouping twice by a key is grouping once; a
//! node's value is the fulfillment of its linked term; an unlinkable node carries the `LinkError`
//! instead of a value.

use std::collections::BTreeMap;

use prodrome_data::event::{Hash, TodoId, mk_sealed, seal_hash};
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Untrusted;
use prodrome_fpl::fpl::{self, LinkError};
use prodrome_term::fpl::PRIORITY_POWER;
use prodrome_todo::event::mk_completed;

use prodrome_term::term::Term;
use suzatary_record::record::Entry;

use suzatary_base::tree::{By, Classifier, Members, Monoid, Place, Segment, Tree};
use suzatary_record::record::{Envelope, TodoEvent, mk_authored};
use suzatary_signatures::believed::Believed;
use suzatary_todo::view::{self, Row};

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
}

fn flat(value: f64) -> Term {
    fpl::mk_flat(value).expect("a flat")
}

fn reference(todo: &str) -> Term {
    fpl::mk_ref(todo.to_owned()).expect("a ref")
}

fn with(own: f64, children: &[&str]) -> Term {
    let mut terms = vec![flat(own)];
    terms.extend(children.iter().map(|c| reference(c)));
    fpl::mk_conj(terms, PRIORITY_POWER).expect("a conj")
}

fn todo(id: &str, category: &str, spec: Term) -> TodoEvent {
    todo_at(id, category, spec, at(1))
}

fn todo_at(id: &str, category: &str, spec: Term, moment: Datetime) -> TodoEvent {
    mk_authored(
        id,
        moment,
        "ada",
        "todo",
        moment,
        suzatary_record::record::MarkupSource::escaped(&format!("Do {id}")),
        spec,
        vec!["why".to_owned()],
        category,
        "",
        suzatary_record::record::MarkupSource::empty(),
        None,
        vec![],
        vec![],
        "",
    )
    .expect("a record")
}

struct Folded {
    entries: Vec<Entry>,
    members: Members,
    now: Datetime,
    functions: BTreeMap<TodoId, Term>,
    env: fpl::Env,
}

fn folded(events: Vec<TodoEvent>) -> Folded {
    folded_at(events, at(20))
}

fn folded_at(events: Vec<TodoEvent>, now: Datetime) -> Folded {
    let mut objects: Vec<(Hash, Envelope)> = Vec::new();
    for event in events {
        let envelope = mk_sealed(objects.last().map(|(name, _)| name.clone()), event);
        objects.push((seal_hash(&envelope), envelope));
    }
    let nodes = prodrome_dag::dag::Dag::from_iter(objects.clone())
        .nodes()
        .expect("a closed set reads");
    let none = Untrusted::none();
    let entries = prodrome_reading::view::entries(&nodes, now, &none).expect("a fold");
    let members = view::members(&entries, &view::by_hash(&objects));
    let believed = Believed::of(&nodes, now, &none).expect("functions");
    Folded {
        functions: believed.functions,
        env: believed.env,
        now,
        entries,
        members,
    }
}

impl Folded {
    fn tree(&self, classifier: &Classifier) -> Tree<Row> {
        view::tree(
            &self.entries,
            &self.members,
            classifier,
            self.now,
            &self.env,
        )
    }
}

fn by(generators: &[By]) -> Classifier {
    generators
        .iter()
        .map(|g| Classifier::by(*g))
        .fold(Classifier::empty(), Classifier::combine)
}

fn todo_node<'a>(tree: &'a Tree<Row>, id: &str) -> &'a Row {
    tree.nodes()
        .into_iter()
        .find(|row| matches!(&row.place, Place::At(Segment::Todo(t)) if t.as_str() == id))
        .expect("the todo is placed")
}

/// Three categories, a parent with two subtodos (one completed), a
/// grandchild.
fn household() -> Folded {
    folded(vec![
        todo("permit", "work", with(0.4, &["forms", "photos"])),
        todo("forms", "work", with(0.6, &["scan"])),
        todo("photos", "work", flat(0.3)),
        todo("scan", "home", flat(0.5)),
        todo("plants", "home", flat(0.9)),
        todo("taxes", "money", flat(0.55)),
        mk_completed("photos", at(2), "ada", "").expect("a completion"),
    ])
}

#[test]
fn a_group_of_one_is_its_todo() {
    let one = folded(vec![todo("alone", "home", flat(0.42))]);
    let tree = one.tree(&by(&[By::Category, By::Ancestry]));
    let [home] = tree.children.as_slice() else {
        panic!("one group")
    };
    let [alone] = home.children.as_slice() else {
        panic!("one todo")
    };
    assert_eq!(tree.node.term, alone.node.term);
    assert_eq!(home.node.term, alone.node.term);
    assert_eq!(home.node.worth, Ok(Some(0.42)));
    assert_eq!(tree.node.worth, Ok(Some(0.42)));
}

#[test]
fn grouping_twice_by_a_key_is_grouping_once() {
    let house = household();
    for key in [By::Category, By::Urgency] {
        assert_eq!(
            house.tree(&by(&[key, key, By::Ancestry])),
            house.tree(&by(&[key, By::Ancestry])),
            "{key:?}"
        );
        assert_eq!(house.tree(&by(&[key, key])), house.tree(&by(&[key])));
    }
}

#[test]
fn a_nodes_value_is_the_fulfillment_of_its_linked_term() {
    let house = household();
    let specs = prodrome_reading::fold::link_specs(&house.functions, house.functions.keys());
    let now = fpl::instant_of(at(20));
    for classifier in [
        by(&[By::Category, By::Ancestry]),
        by(&[By::Urgency, By::Ancestry]),
        by(&[By::Ancestry]),
        by(&[]),
    ] {
        let tree = house.tree(&classifier);
        for row in tree.nodes() {
            let linked = fpl::link(&row.term, &specs).expect("the household links");
            assert_eq!(
                row.worth,
                Ok(fpl::fulfillment(&linked, now, &house.env)),
                "{:?}",
                row.place
            );
        }
        for (todo, member) in &house.members {
            assert_eq!(todo_node(&tree, todo.as_str()).worth, member.worth);
        }
    }
}

#[test]
fn a_subtodo_sits_under_its_parent_and_a_group_holds_the_root() {
    let house = household();
    let tree = house.tree(&by(&[By::Category, By::Ancestry]));
    let groups: Vec<&Place> = tree.children.iter().map(|c| &c.node.place).collect();
    assert_eq!(groups.len(), 3, "work, home, money: {groups:?}");
    let work = tree
        .children
        .iter()
        .find(|c| c.node.place == Place::At(Segment::Category("work".into())))
        .expect("work");
    let [permit] = work.children.as_slice() else {
        panic!("permit alone at work's top: {:?}", work.children)
    };
    let [forms] = permit.children.as_slice() else {
        panic!("photos is done, so forms alone under permit")
    };
    assert_eq!(forms.children.len(), 1, "scan under forms, not under home");
    assert_eq!(
        work.node.term,
        reference("permit"),
        "a group of one root is that root"
    );
    let home = tree
        .children
        .iter()
        .find(|c| c.node.place == Place::At(Segment::Category("home".into())))
        .expect("home");
    assert_eq!(home.children.len(), 1, "plants only; scan sits under forms");
}

#[test]
fn an_unknown_reference_carries_its_link_error() {
    let house = folded(vec![
        todo("parent", "home", with(0.5, &["ghost"])),
        todo("fine", "home", flat(0.9)),
    ]);
    let tree = house.tree(&by(&[By::Category, By::Ancestry]));
    let unknown = Err(LinkError::Unknown("ghost".into()));
    assert_eq!(todo_node(&tree, "parent").worth, unknown);
    assert_eq!(tree.children[0].node.worth, unknown, "the group");
    assert_eq!(tree.node.worth, unknown, "the root");
    assert_eq!(todo_node(&tree, "fine").worth, Ok(Some(0.9)));
}

#[test]
fn a_cycle_carries_its_link_error() {
    let house = folded(vec![
        todo("hen", "home", with(0.5, &["egg"])),
        todo("egg", "home", with(0.5, &["hen"])),
    ]);
    let tree = house.tree(&by(&[By::Ancestry]));
    for row in tree.nodes() {
        assert!(
            matches!(row.worth, Err(LinkError::Cycle(_))),
            "{:?}: {:?}",
            row.place,
            row.worth
        );
    }
    assert_eq!(tree.children.len(), 2, "each under the other, both placed");
}

/// Under every grouping, the children of every node ascend in value: low is urgent, so the
/// most urgent line comes first at each level.
#[test]
fn children_ascend_in_value_at_every_level() {
    fn ascends(tree: &Tree<Row>) {
        let values: Vec<f64> = tree
            .children
            .iter()
            .map(|c| c.node.worth.clone().expect("links").expect("a value"))
            .collect();
        assert!(values.is_sorted_by(|a, b| a <= b), "{values:?}");
        tree.children.iter().for_each(ascends);
    }
    let house = household();
    for generators in [
        &[][..],
        &[By::Ancestry],
        &[By::Category, By::Ancestry],
        &[By::Urgency, By::Ancestry],
    ] {
        ascends(&house.tree(&by(generators)));
    }
}

/// A shift of the grammar's longest span, either way, and a window of it, read at an instant beside
/// each end of the axis: every reading is a number, where the evaluator once panicked on a sum that
/// left the calendar.
#[test]
fn a_shift_of_the_longest_span_prices_beside_both_ends_of_the_axis() {
    let longest = chrono::Duration::microseconds(i64::MAX);
    let ends = [
        Datetime::new(1, 1, 2, 12, 0, 0, 0).expect("the start of the axis"),
        Datetime::new(9999, 12, 30, 12, 0, 0, 0).expect("the end of the axis"),
    ];
    let terms = [
        fpl::mk_shift(longest, flat(0.5)).expect("a shift forward"),
        fpl::mk_shift(-longest, flat(0.5)).expect("a shift back"),
        fpl::mk_within(longest, PRIORITY_POWER, flat(0.5)).expect("a window"),
    ];
    for term in terms {
        for end in &ends {
            let house = folded_at(vec![todo_at("far", "home", term.clone(), *end)], *end);
            let tree = house.tree(&by(&[By::Ancestry]));
            assert_eq!(
                todo_node(&tree, "far").worth,
                Ok(Some(0.5)),
                "{term:?} at {end:?}"
            );
        }
    }
}
