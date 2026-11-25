use std::collections::BTreeSet;

use prodrome_dag::dag::Dag;
use prodrome_dag::sign::{Registrar, Secret};
use prodrome_data::event::{Actor, Envelope, Hash};
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Untrusted;
use prodrome_data::sign::{PublicKey, Signed};
use prodrome_todo::event::mk_completed;
use proptest::prelude::*;

use suzatary_record::record::TodoEvent;
use suzatary_signatures::signatures::*;
use suzatary_store::store::Reads as _;
use suzatary_store::store::{Prodrome, Signing, sign};
use suzatary_stores::stores::{Health, InMemory};

fn key(byte: char) -> Secret {
    Secret::new(&byte.to_string().repeat(64)).expect("a seed")
}

fn root() -> Secret {
    key('9')
}

/// The deployment's two keys: the root, and the agent's own.
fn box_keys() -> BoxKeys {
    BoxKeys::of(
        &root().public(),
        &suzatary_base::capability::Secret::new("9".repeat(64)),
        &suzatary_base::capability::Secret::new("8".repeat(64)),
    )
    .expect("the keys")
}

fn roots() -> Registrar {
    Registrar::Roots([root().public()].into())
}

fn actor(name: &str) -> Actor {
    Actor::new(name).expect("an actor")
}

fn done(todo: &str, by: &str) -> TodoEvent {
    let at = Datetime::new(2026, 10, 5, 9, 0, 0, 0).expect("an instant");
    mk_completed(todo, at, by, "").expect("an event")
}

fn dag<H: Health>(store: &InMemory<TodoEvent, H>) -> Dag<TodoEvent> {
    store.objects().expect("reads").into_iter().collect()
}

fn trust_of<H: Health>(store: &InMemory<TodoEvent, H>) -> Trust {
    Trust::of(&dag(store), &roots())
}

/// `event` appended to `store` and signed by `by`, as a `Signing` store does it.
fn signed<H: Health>(store: &InMemory<TodoEvent, H>, by: &Secret, event: TodoEvent) -> Hash {
    let name = store.append(event).expect("appends");
    sign(store, by.sign(&name)).expect("signed");
    name
}

/// The registers' key of the store's one prodrome.
fn home<H: Health>(store: &InMemory<TodoEvent, H>) -> prodrome_dag::registers::Genesis {
    let dag = dag(store);
    let genesis = dag.geneses().into_iter().next().expect("begun");
    dag.key(&genesis)
}

/// `store` begun by a write of an actor who never holds a key, so its genesis exists.
fn begun<H: Health>(store: InMemory<TodoEvent, H>) -> InMemory<TodoEvent, H> {
    store.append(done("first", "scene")).expect("begins");
    store
}

#[test]
fn a_key_the_root_registered_proves_what_it_signs() {
    let store = begun(InMemory::default());
    let phone = key('1');
    trust(&store, &root(), &actor("ada"), &phone.public()).expect("registered");
    let written = signed(&store, &phone, done("alpha", "ada"));
    let reading = trust_of(&store);
    assert!(reading.proves(&written));
    assert!(matches!(
        reading.provenance(&written),
        Some(Provenance::Proven { .. })
    ));
    assert_eq!(reading.why(&written), None);
}

#[test]
fn an_unsigned_object_is_unsigned_and_a_forged_one_forged() {
    let store = begun(InMemory::faulty());
    let phone = key('1');
    trust(&store, &root(), &actor("ada"), &phone.public()).expect("registered");
    let bare = store.append(done("alpha", "ada")).expect("appends");
    assert_eq!(trust_of(&store).why(&bare), Some(Unproven::Unsigned));
    let other = store.append(done("beta", "ada")).expect("appends");
    let forged = Signed {
        object: bare.clone(),
        ..phone.sign(&other)
    };
    store
        .accept(&[(
            prodrome_data::event::seal_hash(&Envelope::<TodoEvent>::Signed(forged.clone())),
            prodrome_data::event::canonical_envelope(&Envelope::<TodoEvent>::Signed(forged)),
        )])
        .expect("held");
    assert_eq!(trust_of(&store).why(&bare), Some(Unproven::Forged));
}

#[test]
fn an_untrusted_key_proves_nothing_until_the_root_registers_it() {
    let store = begun(InMemory::default());
    let phone = key('1');
    let written = signed(&store, &phone, done("alpha", "ada"));
    assert_eq!(
        trust_of(&store).why(&written),
        Some(Unproven::Untrusted {
            key: phone.public()
        })
    );
    // A registration nobody signed, or one only the key itself signed, does not count.
    let added = trust(&store, &phone, &actor("ada"), &phone.public()).expect("written");
    assert!(
        !trust_of(&store).proves(&written),
        "a key cannot register itself"
    );
    assert_eq!(
        trust_of(&store).standing(&home(&store), &actor("ada"), &phone.public()),
        KeyStanding::Untrusted
    );
    for name in &added {
        sign(&store, root().sign(name)).expect("signed");
    }
    assert!(
        trust_of(&store).proves(&written),
        "signed offline, registered later"
    );
}

#[test]
fn a_key_proves_only_for_the_actor_it_was_registered_to() {
    let store = begun(InMemory::default());
    let phone = key('1');
    trust(&store, &root(), &actor("ada"), &phone.public()).expect("registered");
    let theirs = signed(&store, &phone, done("alpha", "triage"));
    assert!(matches!(
        trust_of(&store).why(&theirs),
        Some(Unproven::Untrusted { .. })
    ));
}

#[test]
fn a_revoked_key_proves_what_it_signed_before_and_nothing_after() {
    let store = begun(InMemory::faulty());
    let phone = key('1');
    trust(&store, &root(), &actor("ada"), &phone.public()).expect("registered");
    let before = signed(&store, &phone, done("alpha", "ada"));
    revoke(&store, &root(), &actor("ada"), &phone.public()).expect("revoked");
    let after = signed(&store, &phone, done("beta", "ada"));
    let reading = trust_of(&store);
    assert!(reading.proves(&before), "its earlier objects still read");
    assert_eq!(
        reading.why(&after),
        Some(Unproven::Revoked {
            key: phone.public()
        })
    );
    assert_eq!(
        reading.standing(&home(&store), &actor("ada"), &phone.public()),
        KeyStanding::Revoked
    );
    // Registering it again is the same object: revoked stays revoked.
    trust(&store, &root(), &actor("ada"), &phone.public()).expect("again");
    assert!(!trust_of(&store).proves(&after));
}

/// A revoke interrupted between chains is completed: a key one chain holds revoked is revoked on
/// every other that still trusts it, and nothing is written where it already is.
#[test]
fn a_revocation_reaches_every_chain_or_is_completed() {
    let (life, project) = (begun(InMemory::default()), begun(InMemory::default()));
    let phone = key('1');
    for chain in [&life, &project] {
        trust(chain, &root(), &actor("ada"), &phone.public()).expect("trusted");
    }
    revoke(&life, &root(), &actor("ada"), &phone.public()).expect("revoked on one");
    let standing = |chain: &InMemory<TodoEvent>| {
        trust_of(chain).standing(&home(chain), &actor("ada"), &phone.public())
    };
    assert_eq!(
        standing(&project),
        KeyStanding::Trusted,
        "the interrupted state"
    );
    let Keeping::Keyed(keyed) = Keeping::stores(&[&life, &project]) else {
        panic!("two keyed chains");
    };
    let stores = keyed.map(|chain| chain.store);
    assert!(
        !complete_revocations(&stores, &root())
            .expect("completed")
            .is_empty()
    );
    assert_eq!(standing(&project), KeyStanding::Revoked);
    assert_eq!(standing(&life), KeyStanding::Revoked);
    assert!(
        complete_revocations(&stores, &root())
            .expect("again")
            .is_empty(),
        "nothing left to complete"
    );
}

#[test]
fn history_beneath_the_epoch_predates_signatures_and_nothing_after_it_does() {
    let store = begun(InMemory::default());
    let earlier = store.append(done("alpha", "ada")).expect("appends");
    trust(&store, &root(), &actor("ada"), &root().public()).expect("registered");
    epoch(&store, &root()).expect("the epoch");
    let later = store.append(done("beta", "ada")).expect("appends");
    let reading = trust_of(&store);
    assert_eq!(reading.provenance(&earlier), Some(Provenance::Predates));
    assert_eq!(
        reading.provenance(&later),
        Some(Provenance::Unproven(Unproven::Unsigned))
    );
    // An epoch nobody the deployment trusts signed is no epoch.
    let unrooted = Trust::of(&dag(&store), &Registrar::Roots([key('5').public()].into()));
    assert_ne!(unrooted.provenance(&earlier), Some(Provenance::Predates));
    // Asked again with an unsigned write since, it refuses rather than absorb the write, and
    // names the repair; once that write is signed, it is the epoch already taken.
    let first = trust_of(&store).epochs().clone();
    let refused = epoch(&store, &root()).expect_err("an unsigned write since");
    assert!(
        refused.to_string().contains("device sign-unsigned"),
        "{refused}"
    );
    let signers = Signers::Held(box_keys());
    let listed = unsigned_writes(&store, &root().public()).expect("listed");
    assert_eq!(
        listed.iter().map(|write| &write.name).collect::<Vec<_>>(),
        vec![&later],
        "exactly the write since the epoch, and nothing beneath it"
    );
    let repaired = sign_unsigned(&store, &signers, &Untrusted::none(), &listed).expect("signed");
    assert_eq!(
        repaired.len(),
        1,
        "the one unsigned write, and nothing beneath the epoch"
    );
    let again: BTreeSet<Hash> = epoch(&store, &root())
        .expect("again")
        .iter()
        .map(|epoch| epoch.name().clone())
        .collect();
    assert_eq!(again, first, "the same epoch, none new");
    assert_eq!(
        trust_of(&store).provenance(&later),
        Some(Provenance::Proven {
            key: root().public()
        })
    );
}

/// How one generated write is signed.
#[derive(Debug, Clone, Copy)]
enum Signer {
    Nobody,
    Registered,
    Stranger,
    Forger,
    RevokedAfter,
}

fn signer() -> impl Strategy<Value = Signer> {
    prop_oneof![
        Just(Signer::Nobody),
        Just(Signer::Registered),
        Just(Signer::Stranger),
        Just(Signer::Forger),
        Just(Signer::RevokedAfter),
    ]
}

fn held<H: Health>(store: &InMemory<TodoEvent, H>, object: &Envelope<TodoEvent>) {
    store
        .accept(&[(
            prodrome_data::event::seal_hash(object),
            prodrome_data::event::canonical_envelope(object),
        )])
        .expect("held");
}

proptest! {
    // Each case verifies every signature twice in a debug build; a few dozen cover the cases.
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// The explanation never disagrees with the Prodrome's proof: an object carrying an event
    /// has no reason exactly when it is proven, and the reason is the one its signer earns.
    /// And the reading is a function of the object set: the same objects read in any order
    /// (here, the store's own and a reversed copy) answer alike.
    #[test]
    fn why_is_none_exactly_where_the_proof_proves(signers in proptest::collection::vec(signer(), 1..8)) {
        let store = begun(InMemory::faulty());
        let (registered, stranger, late) = (key('1'), key('2'), key('3'));
        trust(&store, &root(), &actor("ada"), &registered.public()).expect("registered");
        trust(&store, &root(), &actor("ada"), &late.public()).expect("registered");
        let mut written: Vec<(Hash, Signer)> = Vec::new();
        for (n, signer) in signers.iter().enumerate() {
            let name = store.append(done(&format!("todo{n}"), "ada")).expect("appends");
            match signer {
                // A late key signs after its revocation, below.
                Signer::Nobody | Signer::RevokedAfter => {}
                Signer::Registered => { sign(&store, registered.sign(&name)).expect("signed"); }
                Signer::Stranger => { sign(&store, stranger.sign(&name)).expect("signed"); }
                Signer::Forger => {
                    let other = Hash::new("a".repeat(64)).expect("a name");
                    held(&store, &Envelope::Signed(Signed { object: name.clone(), ..registered.sign(&other) }));
                }
            }
            written.push((name, *signer));
        }
        revoke(&store, &root(), &actor("ada"), &late.public()).expect("revoked");
        for (name, signer) in &written {
            if matches!(signer, Signer::RevokedAfter) {
                sign(&store, late.sign(name)).expect("signed");
            }
        }
        let reading = trust_of(&store);
        let reversed: Dag<TodoEvent> = store.objects().expect("reads").into_iter().rev().collect();
        let again = Trust::of(&reversed, &roots());
        for (name, signer) in &written {
            prop_assert_eq!(reading.why(name).is_none(), reading.proves(name));
            prop_assert_eq!(reading.provenance(name), again.provenance(name));
            let expected = match signer {
                Signer::Registered => None,
                Signer::Nobody => Some(Unproven::Unsigned),
                Signer::Stranger => Some(Unproven::Untrusted { key: stranger.public() }),
                Signer::Forger => Some(Unproven::Forged),
                Signer::RevokedAfter => Some(Unproven::Revoked { key: late.public() }),
            };
            prop_assert_eq!(reading.why(name), expected);
        }
    }
}

/// A `Signing` store signs every append, so once its key is registered nothing it writes is
/// unproven, and a replayed append writes nothing new, its signature included.
#[test]
fn a_signing_store_writes_nothing_unsigned() {
    let phone = key('1');
    let store = Signing::new(
        begun(InMemory::default()),
        suzatary_store::store::Signer::Key(key('1').into()),
    );
    trust(store.inner(), &root(), &actor("ada"), &phone.public()).expect("registered");
    let written = store.append(done("alpha", "ada")).expect("appends");
    let count = store.objects().expect("reads").len();
    assert_eq!(store.append(done("alpha", "ada")).expect("a twin"), written);
    assert_eq!(store.objects().expect("reads").len(), count);
    let reading = trust_of(store.inner());
    assert!(reading.proves(&written));
    assert_eq!(
        PublicKey::new(phone.public().as_str()).expect("a key"),
        phone.public()
    );
}

/// A chain that adopts signatures keeps `verify` quiet about its history and loud about an
/// unsigned write after the epoch.
#[test]
fn adopting_signatures_makes_history_earlier_and_a_later_unsigned_write_a_finding() {
    let store = begun(InMemory::default());
    let earlier = store.append(done("alpha", "ada")).expect("appends");
    let keys = BoxKeys::of(
        &root().public(),
        &suzatary_base::capability::Secret::new("9".repeat(64)),
        &suzatary_base::capability::Secret::new("8".repeat(64)),
    )
    .expect("the deployment's keys");
    let chain = suzatary_base::chains::ChainPolicy {
        name: suzatary_base::chains::ChainName::new("life").expect("a slug"),
        untrusted: prodrome_data::policy::Untrusted::of([actor("triage")]),
        roster: [actor("ada"), actor("triage")].into_iter().collect(),
        root: root().public(),
        naming: suzatary_base::chains::Naming::AsActs,
    };
    adopt(&store, &keys, &chain).expect("adopted");
    let findings = |store: &InMemory<TodoEvent>| findings_of(&dag(store), &chain.untrusted);
    assert!(findings(&store).is_empty(), "{:?}", findings(&store));
    let reading = trust_of(&store);
    assert_eq!(reading.provenance(&earlier), Some(Provenance::Predates));
    assert_eq!(
        reading.standing(&home(&store), &actor("triage"), &keys.agent().public()),
        KeyStanding::Trusted
    );
    assert_eq!(
        reading.standing(&home(&store), &actor("ada"), &keys.agent().public()),
        KeyStanding::Untrusted,
        "the agent's key speaks for the agent alone"
    );
    let later = store.append(done("beta", "ada")).expect("appends");
    assert!(matches!(
        findings(&store).as_slice(),
        [prodrome_dag::dag::Finding::Unsigned { object, .. }] if *object == later
    ));
    sign(&store, keys.root().sign(&later)).expect("signed");
    assert!(findings(&store).is_empty());
}

/// A chain as the instance sample writes it, `untrusted` named or empty: the agent's actor is
/// the agent key's either way.
fn chain_trusting(untrusted: Untrusted) -> suzatary_base::chains::ChainPolicy {
    suzatary_base::chains::ChainPolicy {
        name: suzatary_base::chains::ChainName::new("life").expect("a slug"),
        untrusted,
        roster: [actor("ada"), actor("assistant"), actor("triage")]
            .into_iter()
            .collect(),
        root: root().public(),
        naming: suzatary_base::chains::Naming::AsActs,
    }
}

/// ADOPTION REGISTERS EACH ACTOR WITH THE KEY ITS WRITES ARE SIGNED BY, under either policy the
/// instance file may name: after it, the agent's write as its actor is proven and the box's key
/// speaking as that actor is not.
#[test]
fn adoption_registers_each_actor_with_the_key_that_signs_its_writes() {
    for untrusted in [Untrusted::none(), Untrusted::of([actor("triage")])] {
        let store = begun(InMemory::faulty());
        let keys = box_keys();
        let chain = chain_trusting(untrusted);
        adopt(&store, &keys, &chain).expect("adopted");
        let reading = trust_of(&store);
        let standing =
            |who: &str, key: &Secret| reading.standing(&home(&store), &actor(who), &key.public());
        assert_eq!(standing("triage", keys.agent()), KeyStanding::Trusted);
        assert_eq!(
            standing("triage", keys.root()),
            KeyStanding::Untrusted,
            "the box's key never speaks for the agent's actor"
        );
        for trusted in ["ada", "assistant"] {
            assert_eq!(standing(trusted, keys.root()), KeyStanding::Trusted);
            assert_eq!(standing(trusted, keys.agent()), KeyStanding::Untrusted);
        }

        let agents = signed(&store, keys.agent(), done("alpha", "triage"));
        let boxs = signed(&store, keys.root(), done("beta", "triage"));
        let reading = trust_of(&store);
        assert!(reading.proves(&agents), "{:?}", reading.why(&agents));
        assert_eq!(
            reading.why(&boxs),
            Some(Unproven::Untrusted {
                key: keys.root().public()
            })
        );
        let findings = findings_of(&dag(&store), &chain.untrusted);
        assert!(
            matches!(
                findings.as_slice(),
                [prodrome_dag::dag::Finding::Unsigned { object, .. }] if *object == boxs
            ),
            "{findings:?}"
        );
    }
}

/// The signer a box writer holds for an actor is the key adoption registered for it.
#[test]
fn a_writer_signs_as_each_actor_with_the_key_adoption_registered() {
    let keys = box_keys();
    let signers = Signers::Held(keys.clone());
    let key = |who: &str| match signers.for_actor(&actor(who), &Untrusted::none()) {
        suzatary_store::store::Signer::Key(key) => key.public(),
        suzatary_store::store::Signer::Refusing(_) | suzatary_store::store::Signer::Unsigned => {
            panic!("no key for {who}")
        }
    };
    assert_eq!(key("triage"), keys.agent().public());
    assert_eq!(key("ada"), keys.root().public());
}

/// A REVOCATION IS PER (ACTOR, KEY): the box's key revoked for the agent's actor leaves it
/// speaking for every other actor, before and after, and what it signed as the agent's actor
/// before stays proven. Revoked twice, it is revoked once.
#[test]
fn revoking_a_key_for_one_actor_leaves_its_other_actors_trusted() {
    let store = begun(InMemory::faulty());
    let keys = box_keys();
    for who in ["ada", "assistant", "triage"] {
        trust(&store, keys.root(), &actor(who), &keys.root().public()).expect("registered");
    }
    let earlier = signed(&store, keys.root(), done("alpha", "triage"));
    let owners = signed(&store, keys.root(), done("beta", "ada"));
    // Read, and so kept by the store's heads, before the revocation moves them.
    assert!(
        trusted_for(&store, &keys.root().public(), &keys.root().public())
            .expect("reads")
            .contains(&actor("triage"))
    );
    let revoked =
        revoke(&store, keys.root(), &actor("triage"), &keys.root().public()).expect("revoked");
    assert_eq!(revoked.len(), 1, "one prodrome, one revocation");
    let later = signed(&store, keys.root(), done("gamma", "triage"));
    let owners_later = signed(&store, keys.root(), done("delta", "ada"));
    let assistants_later = signed(&store, keys.root(), done("epsilon", "assistant"));

    let reading = trust_of(&store);
    let standing = |who: &str| reading.standing(&home(&store), &actor(who), &keys.root().public());
    assert_eq!(standing("triage"), KeyStanding::Revoked);
    assert_eq!(standing("ada"), KeyStanding::Trusted);
    assert_eq!(standing("assistant"), KeyStanding::Trusted);
    for proven in [&earlier, &owners, &owners_later, &assistants_later] {
        assert!(reading.proves(proven), "{:?}", reading.why(proven));
    }
    assert_eq!(
        reading.why(&later),
        Some(Unproven::Revoked {
            key: keys.root().public()
        })
    );
    assert_eq!(
        trusted_for(&store, &keys.root().public(), &keys.root().public()).expect("reads"),
        [actor("ada"), actor("assistant")].into_iter().collect()
    );
    let count = store.objects().expect("reads").len();
    assert!(
        revoke(&store, keys.root(), &actor("triage"), &keys.root().public())
            .expect("again")
            .is_empty(),
        "revoked twice is revoked once"
    );
    assert_eq!(store.objects().expect("reads").len(), count);
}

/// A store adopted before custody (the box's key for every actor) adopted again registers the
/// agent's key for its actor, and its repair revokes the box's key for that actor alone; both
/// asked again do nothing.
#[test]
fn adopting_again_and_repairing_converge_on_custody_and_then_do_nothing() {
    let store = begun(InMemory::default());
    let keys = box_keys();
    let chain = chain_trusting(Untrusted::none());
    for who in &chain.roster {
        trust(&store, keys.root(), who, &keys.root().public()).expect("registered as before");
    }
    epoch(&store, keys.root()).expect("the epoch");

    let adoption = adopt(&store, &keys, &chain).expect("adopted again");
    assert_eq!(
        adoption.registered,
        vec![(actor("triage"), keys.agent().public())]
    );
    assert_eq!(
        revoke_misplaced(&store, &keys, &chain).expect("repaired"),
        vec![(actor("triage"), keys.root().public())]
    );
    let count = store.objects().expect("reads").len();
    assert!(
        adopt(&store, &keys, &chain)
            .expect("again")
            .registered
            .is_empty()
    );
    assert!(
        revoke_misplaced(&store, &keys, &chain)
            .expect("again")
            .is_empty()
    );
    assert_eq!(
        store.objects().expect("reads").len(),
        count,
        "nothing written"
    );
    let reading = trust_of(&store);
    for who in &chain.roster {
        let custody = Custody::of(who, &chain.untrusted);
        let standing =
            |custody| reading.standing(&home(&store), who, &keys.of_custody(custody).public());
        assert_eq!(standing(custody), KeyStanding::Trusted, "{who}");
        assert_ne!(standing(custody.other()), KeyStanding::Trusted, "{who}");
    }
}

/// One step of a generated history.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// A write, signed by key 1 to 3, by nobody (0) or by key 1's signature of another object (4).
    Write(u8),
    /// Key 1 to 3 registered by the root, or (`false`) by a key that is no root.
    Register(u8, bool),
    /// Key 1 to 3 revoked by the root, standing behind every signature it made so far.
    Revoke(u8),
    /// Key 1 to 3 revoked by the root, standing behind nothing.
    RevokeAll(u8),
    /// The root's epoch, where one may be taken.
    Epoch,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (0..5_u8).prop_map(Step::Write),
        2 => (1..4_u8, any::<bool>()).prop_map(|(key, rooted)| Step::Register(key, rooted)),
        1 => (1..4_u8).prop_map(Step::Revoke),
        1 => (1..4_u8).prop_map(Step::RevokeAll),
        1 => Just(Step::Epoch),
    ]
}

fn numbered(n: u8) -> Secret {
    key(char::from(b'0' + n))
}

/// `object` held in `store` and signed by `by`.
fn written_by<H: Health>(
    store: &InMemory<TodoEvent, H>,
    by: &Secret,
    object: &Envelope<TodoEvent>,
) {
    held(store, object);
    sign(store, by.sign(&prodrome_data::event::seal_hash(object))).expect("signed");
}

fn taken<H: Health>(store: &InMemory<TodoEvent, H>, step: Step, n: usize) {
    let genesis = home(store).expect("a prodrome begun with a genesis");
    match step {
        Step::Write(by) => {
            let name = store
                .append(done(&format!("todo{n}"), "ada"))
                .expect("appends");
            match by {
                0 => {}
                4 => held(
                    store,
                    &Envelope::Signed(Signed {
                        object: name,
                        ..numbered(1).sign(&genesis)
                    }),
                ),
                by => {
                    sign(store, numbered(by).sign(&name)).expect("signed");
                }
            }
        }
        Step::Register(by, true) => {
            trust(store, &root(), &actor("ada"), &numbered(by).public()).expect("registered");
        }
        Step::Register(by, false) => written_by(
            store,
            &key('5'),
            &Envelope::KeyAdded(prodrome_data::sign::KeyAdded {
                genesis,
                actor: actor("ada"),
                key: numbered(by).public(),
            }),
        ),
        Step::Revoke(by) => {
            revoke(store, &root(), &actor("ada"), &numbered(by).public()).expect("revoked");
        }
        Step::RevokeAll(by) => written_by(
            store,
            &root(),
            &Envelope::KeyRevoked(
                prodrome_data::sign::mk_key_revoked(
                    genesis,
                    vec![],
                    actor("ada"),
                    numbered(by).public(),
                )
                .expect("a revocation"),
            ),
        ),
        // Refused once an unsigned write follows the epoch, which is the history still.
        Step::Epoch => {
            let _ = epoch(store, &root());
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// THE READING IS THE PRODROME'S PROOF, AND THE TABLE IS INVISIBLE. Over generated histories
    /// of writes signed honestly, by strangers or forged, keys registered by the root or by
    /// nobody, revoked keeping what they signed or keeping nothing, and epochs, under the root
    /// and under anyone: what the reading proves is exactly what `Proof::of` proves; a reading
    /// through a warm table (one that read half the history before, and keeps one to three checks
    /// or many) is the cold reading, whole; and the reading is a function of the object set (the
    /// objects in reverse read alike).
    #[test]
    fn trust_through_any_table_is_the_cold_proof(
        steps in proptest::collection::vec(step(), 1..24),
        capacity in prop_oneof![1..4_usize, Just(KEPT_CHECKS)],
    ) {
        let store = begun(InMemory::faulty());
        let warm = prodrome_dag::sign::Checks::new(capacity);
        for (n, step) in steps.iter().enumerate() {
            taken(&store, *step, n);
            if n == steps.len() / 2 {
                let _ = Trust::through(&dag(&store), &roots(), &warm);
            }
        }
        let history = dag(&store);
        let reversed: Dag<TodoEvent> = store.objects().expect("reads").into_iter().rev().collect();
        for registrar in [roots(), Registrar::Anyone] {
            let cold = Trust::through(&history, &registrar, &prodrome_dag::sign::Checks::new(KEPT_CHECKS));
            let proof = prodrome_dag::sign::Proof::of(&history, &registrar);
            for name in history.objects().keys() {
                prop_assert_eq!(cold.proves(name), proof.proves(name), "{}", name);
            }
            prop_assert_eq!(&Trust::through(&history, &registrar, &warm), &cold);
            prop_assert_eq!(&Trust::through(&reversed, &registrar, &warm), &cold);
            prop_assert_eq!(&Trust::of(&history, &registrar), &cold);
        }
    }
}

/// A REVOCATION IS READ AT ONCE, and an unmoved chain is read once: the door's reading of a
/// chain is kept by its heads, so asked twice it is the same reading, and a key revoked between
/// two asks is refused on the second, since the revocation is an object and moves the heads.
#[test]
fn a_key_revoked_between_two_asks_is_refused_on_the_second() {
    let store = begun(InMemory::default());
    let phone = key('1');
    trust(&store, &root(), &actor("ada"), &phone.public()).expect("registered");
    let request = phone.sign(&prodrome_memo::name(b"a request", []));
    let first = Trust::held(&store, &roots()).expect("reads");
    assert_eq!(first.admits(first.home(), &actor("ada"), &request), Ok(()));
    let again = Trust::held(&store, &roots()).expect("reads");
    assert!(
        std::sync::Arc::ptr_eq(&first, &again),
        "an unmoved chain is read once"
    );
    revoke(&store, &root(), &actor("ada"), &phone.public()).expect("revoked");
    let after = Trust::held(&store, &roots()).expect("reads");
    assert_eq!(
        after.admits(after.home(), &actor("ada"), &request),
        Err(Unproven::Revoked {
            key: phone.public()
        })
    );
}

/// A SIGNATURE VOUCHES FOR A HUSK AND PROVES NONE (the Prodrome's law 77): a line signed by a key
/// registered to its actor, then held as its husk, reads as vouched for by that key, as the
/// Prodrome's `Proof` reads it; it is never proven, and the sentence a page shows of it does not
/// say signed or proven, while the same line held whole is proven.
#[test]
fn a_signed_line_held_as_its_husk_is_vouched_for_and_not_proven() {
    use prodrome_dag::sign::{Proof, Proved};
    use prodrome_data::event::{canonical_envelope, seal_hash};
    use prodrome_data::husk::{HuskObject, Sender};
    use prodrome_data::sign::KeyAdded;
    use suzatary_lines::secretary::line_event::{LineEvent, LineVocabulary};

    struct Ones;
    impl suzatary_base::capability::Entropy for Ones {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            bytes.fill(1);
            Ok(())
        }
    }
    let ada = actor("ada");
    let phone = key('1');
    let genesis: Envelope<LineEvent> = Envelope::Genesis(
        prodrome_data::genesis::mk_genesis("lines", &"0".repeat(32)).expect("a genesis"),
    );
    let registered: Envelope<LineEvent> = Envelope::KeyAdded(KeyAdded {
        genesis: seal_hash(&genesis),
        actor: ada.clone(),
        key: phone.public(),
    });
    let opened = suzatary_conversations::secretary::store::Entry::Opened {
        at: Datetime::new(2026, 10, 5, 9, 0, 0, 0).expect("an instant"),
        title: suzatary_memory::secretary::words::Words::kept("Vouched".to_owned()),
        model: "scripted".to_owned(),
        kind: suzatary_conversations::secretary::store::Kind::Topic,
    };
    let id =
        suzatary_base::conversation_id::ConversationId::new("2026-10-05-vouched").expect("a slug");
    let (line, _) = LineEvent::write(id, ada, 0, &opened, &Ones).expect("a line");
    let change =
        prodrome_data::change::mk_change(seal_hash(&genesis), Vec::new(), line).expect("a change");
    let husk = HuskObject::of(&change).expect("a line with words has a husk");
    let whole = Envelope::Change(change);
    let name = seal_hash(&whole);
    let signature: Envelope<LineEvent> = Envelope::Signed(phone.sign(&name));
    let held = |line: String| {
        let prints = [
            (seal_hash(&genesis), canonical_envelope(&genesis)),
            (seal_hash(&registered), canonical_envelope(&registered)),
            (name.clone(), line),
            (seal_hash(&signature), canonical_envelope(&signature)),
        ];
        Dag::<LineEvent>::from_prints(
            &LineVocabulary,
            prints
                .into_iter()
                .map(|(name, print)| (name, Ok(print.into_bytes()))),
            Sender::Keeper,
        )
    };
    let husked = held(husk.print());
    assert!(husked.husks().contains(&name), "held as its husk");
    let trust = Trust::of(&husked, &Registrar::Anyone);
    assert_eq!(
        Proof::of(&husked, &Registrar::Anyone).standing(&name),
        Proved::Vouched
    );
    assert!(!trust.proves(&name), "a signature proves no husk");
    assert_eq!(trust.why(&name), None, "and it is not unproven either");
    let provenance = trust.provenance(&name).expect("it carries an event");
    assert_eq!(
        provenance,
        Provenance::Vouched {
            key: phone.public()
        }
    );
    let said = provenance.said();
    assert!(
        !said.contains("proven") && !said.starts_with("signed"),
        "a page says no husk is proven or signed: {said}"
    );
    let whole = Trust::of(&held(canonical_envelope(&whole)), &Registrar::Anyone);
    assert_eq!(
        whole.provenance(&name),
        Some(Provenance::Proven {
            key: phone.public()
        })
    );
}

/// What one chain of a generated deployment holds of the phone's key.
#[derive(Debug, Clone, Copy)]
enum Chained {
    /// A chain that keeps keys, the key standing so on it.
    Private(Standing),
    /// A public chain, holding these key objects as no real public store could.
    Public(Held),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    Unregistered,
    Trusted,
    Revoked,
    /// Trusted for another actor.
    Elsewhere,
    /// The store does not read.
    Unread,
}

#[derive(Debug, Clone, Copy)]
enum Held {
    Nothing,
    KeyAdded,
    KeyRevoked,
}

/// The door's answer, comparable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    Admitted,
    Keyless,
    Unread(String),
    Revoked,
    Untrusted,
}

/// A chain's store that does not read.
struct Unreadable;

impl suzatary_store::store::Reads<TodoEvent> for Unreadable {
    fn objects(
        &self,
    ) -> Result<Vec<(Hash, Envelope<TodoEvent>)>, suzatary_store::store::StoreError> {
        Err(suzatary_store::store::StoreError::unreadable(
            "chain",
            "the disk is gone",
        ))
    }

    fn excluded(&self) -> Result<BTreeSet<Hash>, suzatary_store::store::StoreError> {
        Err(suzatary_store::store::StoreError::unreadable(
            "chain",
            "the disk is gone",
        ))
    }

    fn tips(&self) -> Result<Vec<Hash>, suzatary_store::store::StoreError> {
        Err(suzatary_store::store::StoreError::unreadable(
            "chain",
            "the disk is gone",
        ))
    }

    fn verify(&self) -> Vec<String> {
        vec!["the disk is gone".to_owned()]
    }
}

impl Prodrome<TodoEvent> for Unreadable {
    fn append(&self, _: TodoEvent) -> Result<Hash, suzatary_store::store::StoreError> {
        Err(suzatary_store::store::StoreError::unwritable(
            "chain",
            "the disk is gone",
        ))
    }

    fn accept(&self, _: &[(Hash, String)]) -> Result<Vec<Hash>, suzatary_store::store::StoreError> {
        Err(suzatary_store::store::StoreError::unwritable(
            "chain",
            "the disk is gone",
        ))
    }

    fn locked(
        &self,
    ) -> Result<Box<dyn suzatary_store::store::WriteLock + '_>, suzatary_store::store::StoreError>
    {
        Err(suzatary_store::store::StoreError::unwritable(
            "chain",
            "the disk is gone",
        ))
    }
}

fn chained() -> impl Strategy<Value = Chained> {
    prop_oneof![
        prop_oneof![
            Just(Standing::Unregistered),
            Just(Standing::Trusted),
            Just(Standing::Revoked),
            Just(Standing::Elsewhere),
            Just(Standing::Unread),
        ]
        .prop_map(Chained::Private),
        prop_oneof![
            Just(Held::Nothing),
            Just(Held::KeyAdded),
            Just(Held::KeyRevoked)
        ]
        .prop_map(Chained::Public),
    ]
}

/// THE DOOR'S RULE: no keyed chain proves nothing; a keyed chain that does not read is said as
/// such; a revocation on any keyed chain wins; else trusted exactly where every keyed chain
/// trusts. Public chains are not in it.
fn ruled(chains: &[Chained]) -> Answer {
    let keyed: Vec<(usize, Standing)> = chains
        .iter()
        .enumerate()
        .filter_map(|(n, chain)| match chain {
            Chained::Private(standing) => Some((n, *standing)),
            Chained::Public(_) => None,
        })
        .collect();
    if keyed.is_empty() {
        return Answer::Keyless;
    }
    if let Some((n, _)) = keyed
        .iter()
        .find(|(_, standing)| *standing == Standing::Unread)
    {
        return Answer::Unread(format!("c{n}"));
    }
    if keyed
        .iter()
        .any(|(_, standing)| *standing == Standing::Revoked)
    {
        return Answer::Revoked;
    }
    if keyed
        .iter()
        .all(|(_, standing)| *standing == Standing::Trusted)
    {
        Answer::Admitted
    } else {
        Answer::Untrusted
    }
}

/// The door's answer to a request the phone signed as ada, over a deployment of `chains`.
fn door(chains: &[Chained]) -> Answer {
    let phone = key('1');
    let stores: Vec<Box<dyn Prodrome<TodoEvent>>> = chains
        .iter()
        .map(|chain| -> Box<dyn Prodrome<TodoEvent>> {
            let (standing, held) = match chain {
                Chained::Private(Standing::Unread) => return Box::new(Unreadable),
                Chained::Private(standing) => (*standing, Held::Nothing),
                Chained::Public(held) => (Standing::Unregistered, *held),
            };
            let store = begun(InMemory::default());
            let ada = actor("ada");
            match (standing, held) {
                (Standing::Trusted, _) | (_, Held::KeyAdded) => {
                    trust(&store, &root(), &ada, &phone.public()).expect("trusted");
                }
                (Standing::Revoked, _) | (_, Held::KeyRevoked) => {
                    trust(&store, &root(), &ada, &phone.public()).expect("trusted");
                    revoke(&store, &root(), &ada, &phone.public()).expect("revoked");
                }
                (Standing::Elsewhere, _) => {
                    trust(&store, &root(), &actor("bob"), &phone.public()).expect("trusted");
                }
                (Standing::Unregistered | Standing::Unread, Held::Nothing) => {}
            }
            Box::new(store)
        })
        .collect();
    let policies: Vec<suzatary_base::chains::ChainPolicy> = chains
        .iter()
        .enumerate()
        .map(|(n, chain)| suzatary_base::chains::ChainPolicy {
            name: suzatary_base::chains::ChainName::new(format!("c{n}")).expect("a slug"),
            naming: match chain {
                Chained::Private(_) => suzatary_base::chains::Naming::AsActs,
                Chained::Public(_) => {
                    suzatary_base::chains::Naming::Published(std::collections::BTreeMap::new())
                }
            },
            ..chain_trusting(Untrusted::none())
        })
        .collect();
    let paired = Keeping::of(policies.iter().zip(&stores), |pair| pair.0);
    let keeping = paired.map(|(policy, store)| Keeper {
        name: policy.name.clone(),
        store: store.as_ref(),
    });
    let (at, nonce, body) = (
        "2026-09-02T12:00:00.000Z",
        "0123456789abcdef0123456789abcdef",
        b"{}",
    );
    let name = request_name(
        "POST",
        "/api/act",
        at,
        nonce,
        &body_digest(body),
        phone.public().as_str(),
    );
    let signed = phone.sign(&name);
    let now = chrono::DateTime::parse_from_rfc3339(at)
        .expect("an instant")
        .with_timezone(&chrono::Utc);
    let asking = Asking::of(
        &Request {
            method: "POST",
            target: "/api/act",
            at: Some(at),
            nonce: Some(nonce),
            body,
            key: Some(signed.key.as_str()),
            signature: Some(signed.signature.as_str()),
        },
        actor("ada"),
        now,
    )
    .expect("a well-formed request");
    match asking.trusted(&keeping, &root().public()) {
        Ok(_) => Answer::Admitted,
        Err(Unadmitted::Keyless) => Answer::Keyless,
        Err(Unadmitted::Unread(unread)) => Answer::Unread(unread.chain.to_string()),
        Err(Unadmitted::Unproven(Unproven::Revoked { key })) => {
            assert_eq!(key, phone.public());
            Answer::Revoked
        }
        Err(Unadmitted::Unproven(Unproven::Untrusted { key })) => {
            assert_eq!(key, phone.public());
            Answer::Untrusted
        }
        Err(Unadmitted::Unproven(other @ (Unproven::Unsigned | Unproven::Forged))) => {
            panic!("a verifying signature refused as {other:?}")
        }
    }
}

proptest! {
    /// THE DOOR'S LAW, over generated deployments: any mix of private and public chains, the
    /// phone's key in any standing on each keyed chain, and the public chains holding key objects
    /// no real public store takes. The door answers exactly what the rule says, and emptying
    /// every public chain moves no answer.
    #[test]
    fn the_door_admits_exactly_what_the_keyed_chains_prove(
        chains in proptest::collection::vec(chained(), 0..6)
    ) {
        let answer = door(&chains);
        prop_assert_eq!(&answer, &ruled(&chains));
        let emptied: Vec<Chained> = chains
            .iter()
            .map(|chain| match chain {
                Chained::Public(_) => Chained::Public(Held::Nothing),
                Chained::Private(standing) => Chained::Private(*standing),
            })
            .collect();
        prop_assert_eq!(answer, door(&emptied));
    }
}
