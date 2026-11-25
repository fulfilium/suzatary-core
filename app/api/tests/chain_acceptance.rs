//! The chain's door measured on a chain the size of a box's, adopted late: about 10,000 legacy
//! writes, each resting on the one before, then a device's key registered by the root, a merge
//! over it, and a write to each todo after the merge, so every change a device posts rests on the
//! whole chain. Printed as measurements; run with `--release --ignored`.

use std::collections::BTreeMap;

use prodrome_dag::dag::Dag;
use prodrome_dag::sign::Secret;
use prodrome_data::event::{Actor, Hash, canonical_envelope, mk_sealed, mk_woven, seal_hash};
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Untrusted;
use prodrome_data::sign::KeyAdded;
use prodrome_store::store::frontier::{Frontiers, KEPT, Since};
use prodrome_store::store::{MemoryStore, Replica as _};
use prodrome_todo::event::{mk_completed, mk_created};
use suzatary_api::routes;
use suzatary_base::capability::{Rejected, RenderError, Typeset};
use suzatary_base::escape::Escaped;
use suzatary_record::record::{Envelope, TodoEvent};
use suzatary_store::store::{Prodrome as _, Reads as _};
use suzatary_todo::render::{Html, Render};
use suzatary_todo::template::Called;

const TODOS: usize = 500;
const WRITES: usize = 10_000;

/// Renders nothing: the measured posts carry no record.
struct Unrendered;

impl Typeset for Unrendered {
    fn closed(&self, _source: &str) -> Result<String, Rejected> {
        Err(Rejected::whole("unrendered"))
    }
}

impl Render for Unrendered {
    fn html(&self, _markup: &Escaped) -> Result<Html, RenderError> {
        Err(RenderError::Refused("unrendered".into()))
    }
    fn cached(&self, _markup: &Escaped) -> Option<Html> {
        None
    }
    fn warm(&self, _markup: &[Escaped]) -> usize {
        0
    }
    fn page(&self, _page: &Called) -> Result<Html, RenderError> {
        Err(RenderError::Refused("unrendered".into()))
    }
}

fn at(n: usize) -> Datetime {
    let n = u32::try_from(n).expect("a count");
    Datetime::new(2026, 10, 1, (n / 3600) % 24, (n / 60) % 60, n % 60, 0).expect("an instant")
}

fn todo(n: usize) -> String {
    format!("todo{n}")
}

/// The adopted chain's objects, parents first.
fn adopted(phone: &Secret, root: &Secret) -> Vec<Envelope> {
    let mut all: Vec<Envelope> = Vec::new();
    let mut prev: Option<Hash> = None;
    for n in 0..WRITES {
        let event = if n < TODOS {
            mk_created(&todo(n), at(n), "ada", "", "").expect("valid")
        } else {
            mk_completed(&todo(n % TODOS), at(n), "ada", "").expect("valid")
        };
        let object = mk_sealed(prev.clone(), event);
        prev = Some(seal_hash(&object));
        all.push(object);
    }
    let added = Envelope::KeyAdded(KeyAdded {
        genesis: seal_hash(&all[0]),
        actor: Actor::new("ada").expect("an actor"),
        key: phone.public(),
    });
    let registered = Envelope::Signed(root.sign(&seal_hash(&added)));
    let mut tips = vec![prev.clone().expect("written"), seal_hash(&added)];
    tips.sort();
    let merge: Envelope = mk_woven(tips, None).expect("a merge");
    let mut after = Some(seal_hash(&merge));
    all.extend([added, registered, merge]);
    for n in 0..TODOS {
        let object = mk_sealed(
            after.clone(),
            mk_completed(&todo(n), at(WRITES + n), "triage", "").expect("valid"),
        );
        after = Some(seal_hash(&object));
        all.push(object);
    }
    all
}

fn chain(root: &Secret) -> suzatary_base::chains::ChainPolicy {
    let actor = |name: &str| Actor::new(name).expect("an actor");
    suzatary_base::chains::ChainPolicy {
        name: suzatary_base::chains::ChainName::new("life").expect("a slug"),
        untrusted: Untrusted::of([actor("triage")]),
        roster: [actor("ada"), actor("triage")].into_iter().collect(),
        root: root.public(),
        naming: suzatary_base::chains::Naming::AsActs,
    }
}

/// `count` posts of `per` changes each, each to its own todo over its heads, signed by `phone`,
/// handed to the chain's route: how long each post took, the lock held throughout.
fn posts(count: usize, per: usize) -> Vec<std::time::Duration> {
    let (root, phone) = (
        Secret::new(&"ab".repeat(32)).expect("a seed"),
        Secret::new(&"cd".repeat(32)).expect("a seed"),
    );
    let all = adopted(&phone, &root);
    let prints: BTreeMap<Hash, String> = all
        .iter()
        .map(|object| (seal_hash(object), canonical_envelope(object)))
        .collect();
    let dag: Dag<TodoEvent> = all.iter().map(|o| (seal_hash(o), o.clone())).collect();
    // Its legacy writes are ada's and unsigned, and her key rests on nothing, so the store's check
    // reports each: the store is built as one that holds findings.
    let store = suzatary_stores::stores::InMemory::<TodoEvent, _>::faulty();
    let ordered: Vec<(Hash, String)> = dag
        .linearise()
        .expect("linear")
        .iter()
        .map(|name| (name.clone(), prints[name].clone()))
        .collect();
    store.accept(&ordered).expect("held");
    let tab: MemoryStore<TodoEvent> = MemoryStore::default();
    tab.receive(
        dag.tips(),
        &|name| prints.get(name).map(|print| print.clone().into_bytes()),
        prodrome_data::husk::Sender::Stranger,
    )
    .expect("received");
    let policy = chain(&root);
    let frontiers = Frontiers::remembered(KEPT);
    let ada = Actor::new("ada").expect("an actor");
    let mut took = Vec::new();
    for post in 0..count {
        let mut objects = Vec::new();
        for n in 0..per {
            let at_ = at(2 * WRITES + post * per + n);
            let name = tab
                .append(
                    mk_completed(&todo((post * per + n) % TODOS), at_, "ada", "").expect("valid"),
                )
                .expect("appended");
            let print = String::from_utf8(tab.print(&name).expect("held")).expect("utf8");
            objects.push(print);
            objects.push(canonical_envelope(&Envelope::Signed(phone.sign(&name))));
        }
        let started = std::time::Instant::now();
        let (answer, parts) = suzatary_lines::parts::scoped(|| {
            routes::receive_objects(
                &store,
                &Unrendered,
                &policy,
                &routes::Posted {
                    objects: &objects,
                    since: &Since::Nothing,
                },
                &frontiers,
                &ada,
            )
        });
        let (receipt, _) = answer.expect("answered");
        took.push(started.elapsed());
        println!(
            "adopted chain: a post of {per}: its steps: {}",
            parts.line("")
        );
        assert_eq!(
            receipt.adopted.len(),
            2 * per,
            "every change taken: {:?}",
            receipt.refused
        );
    }
    took
}

#[test]
#[ignore = "a measurement, run by hand"]
fn the_chain_s_acceptance_measured() {
    let mut one = posts(5, 1);
    one.sort();
    println!(
        "adopted chain: one posted todo, the lock held throughout: median {:?}, slowest {:?}",
        one[one.len() / 2],
        one[one.len() - 1]
    );
    let full = posts(1, 256);
    println!(
        "adopted chain: a post of 256 todos and their signatures: {:?}",
        full[0]
    );
}

/// Records, at each warm, whether the store already held `record`.
struct Watching<'s> {
    store: &'s suzatary_stores::stores::InMemory<TodoEvent, suzatary_stores::stores::Faulty>,
    record: Hash,
    held: std::sync::Mutex<Vec<bool>>,
}

impl Typeset for Watching<'_> {
    fn closed(&self, _source: &str) -> Result<String, Rejected> {
        Err(Rejected::whole("unrendered"))
    }
}

impl Render for Watching<'_> {
    fn html(&self, _markup: &Escaped) -> Result<Html, RenderError> {
        Err(RenderError::Refused("unrendered".into()))
    }
    fn cached(&self, _markup: &Escaped) -> Option<Html> {
        None
    }
    fn warm(&self, markup: &[Escaped]) -> usize {
        let held = holds(self.store, &self.record);
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(held);
        markup.len()
    }
    fn page(&self, _page: &Called) -> Result<Html, RenderError> {
        Err(RenderError::Refused("unrendered".into()))
    }
}

/// A record a device posts is rendered before the chain holds it: a reader that sees the record
/// finds its rendering, since no page is told to ask again when a rendering lands later.
#[test]
fn a_posted_record_is_rendered_before_the_chain_holds_it() {
    use suzatary_record::record::{MarkupSource, mk_authored};
    let (root, phone) = (
        Secret::new(&"ab".repeat(32)).expect("a seed"),
        Secret::new(&"cd".repeat(32)).expect("a seed"),
    );
    let genesis: Envelope = Envelope::Genesis(
        prodrome_data::genesis::mk_genesis("life", &"ef".repeat(16)).expect("a genesis"),
    );
    let added = Envelope::KeyAdded(KeyAdded {
        genesis: seal_hash(&genesis),
        actor: Actor::new("ada").expect("an actor"),
        key: phone.public(),
    });
    let registered = Envelope::Signed(root.sign(&seal_hash(&added)));
    let store = suzatary_stores::stores::InMemory::<TodoEvent, _>::faulty();
    store
        .accept(&[&genesis, &added, &registered].map(|o| (seal_hash(o), canonical_envelope(o))))
        .expect("held");
    let record = Envelope::Change(
        prodrome_data::change::mk_change(
            seal_hash(&genesis),
            Vec::new(),
            mk_authored(
                "renewthepassport",
                at(0),
                "ada",
                "todo",
                at(0),
                MarkupSource::escaped("Renew the passport"),
                prodrome_term::fpl::mk_absent(),
                Vec::new(),
                "",
                "",
                MarkupSource::escaped(""),
                None,
                Vec::new(),
                Vec::new(),
                "",
            )
            .expect("a record"),
        )
        .expect("a change"),
    );
    let watching = Watching {
        store: &store,
        record: seal_hash(&record),
        held: std::sync::Mutex::new(Vec::new()),
    };
    let posted = [
        canonical_envelope(&record),
        canonical_envelope(&Envelope::Signed(phone.sign(&seal_hash(&record)))),
    ];
    let (receipt, _) = routes::receive_objects(
        &store,
        &watching,
        &chain(&root),
        &routes::Posted {
            objects: &posted,
            since: &Since::Nothing,
        },
        &Frontiers::remembered(KEPT),
        &Actor::new("ada").expect("an actor"),
    )
    .expect("answered");
    assert!(
        receipt
            .adopted
            .contains(&seal_hash(&record).as_str().to_owned()),
        "the record is taken: {:?}",
        receipt.refused
    );
    assert_eq!(
        *watching
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        vec![false],
        "warmed once, before the chain held the record"
    );
    assert!(holds(&store, &seal_hash(&record)), "and then held");
}

/// Whether `store` holds the object `name`.
fn holds(
    store: &suzatary_stores::stores::InMemory<TodoEvent, suzatary_stores::stores::Faulty>,
    name: &Hash,
) -> bool {
    store
        .objects()
        .expect("objects")
        .iter()
        .any(|(held, _)| held == name)
}
