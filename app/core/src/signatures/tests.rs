use std::collections::BTreeSet;

use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Envelope, Hash, mk_completed};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use prodrome_core::sign::{PublicKey, Registrar, Secret, Signed};
use proptest::prelude::*;

use super::*;
use crate::record::TodoEvent;
use crate::store::Reads as _;
use crate::store::{InMemory, Prodrome, Signing, sign};

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
        &crate::capability::Secret::new("9".repeat(64)),
        &crate::capability::Secret::new("8".repeat(64)),
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

fn dag(store: &InMemory<TodoEvent>) -> Dag<TodoEvent> {
    store.objects().expect("reads").into_iter().collect()
}

fn trust_of(store: &InMemory<TodoEvent>) -> Trust {
    Trust::of(&dag(store), &roots())
}

/// `event` appended to `store` and signed by `by`, as a `Signing` store does it.
fn signed(store: &InMemory<TodoEvent>, by: &Secret, event: TodoEvent) -> Hash {
    let name = store.append(event).expect("appends");
    sign(store, by, &name).expect("signed");
    name
}

/// The registers' key of the store's one prodrome.
fn home(store: &InMemory<TodoEvent>) -> prodrome_core::registers::Genesis {
    let dag = dag(store);
    let genesis = dag.geneses().into_iter().next().expect("begun");
    dag.key(&genesis)
}

/// A store begun, with one write in it so its genesis exists.
fn begun() -> InMemory<TodoEvent> {
    let store = InMemory::default();
    store.append(done("first", "ada")).expect("begins");
    store
}

#[test]
fn a_key_the_root_registered_proves_what_it_signs() {
    let store = begun();
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
    let store = begun();
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
            prodrome_core::event::seal_hash(&Envelope::<TodoEvent>::Signed(forged.clone())),
            prodrome_core::event::canonical_envelope(&Envelope::<TodoEvent>::Signed(forged)),
        )])
        .expect("held");
    assert_eq!(trust_of(&store).why(&bare), Some(Unproven::Forged));
}

#[test]
fn an_untrusted_key_proves_nothing_until_the_root_registers_it() {
    let store = begun();
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
        sign(&store, &root(), name).expect("signed");
    }
    assert!(
        trust_of(&store).proves(&written),
        "signed offline, registered later"
    );
}

#[test]
fn a_key_proves_only_for_the_actor_it_was_registered_to() {
    let store = begun();
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
    let store = begun();
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
    let (life, project) = (begun(), begun());
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
    let stores: [&dyn Prodrome<TodoEvent>; 2] = [&life, &project];
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
    let store = begun();
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

fn held(store: &InMemory<TodoEvent>, object: &Envelope<TodoEvent>) {
    store
        .accept(&[(
            prodrome_core::event::seal_hash(object),
            prodrome_core::event::canonical_envelope(object),
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
        let store = begun();
        let (registered, stranger, late) = (key('1'), key('2'), key('3'));
        trust(&store, &root(), &actor("ada"), &registered.public()).expect("registered");
        trust(&store, &root(), &actor("ada"), &late.public()).expect("registered");
        let mut written: Vec<(Hash, Signer)> = Vec::new();
        for (n, signer) in signers.iter().enumerate() {
            let name = store.append(done(&format!("todo{n}"), "ada")).expect("appends");
            match signer {
                // A late key signs after its revocation, below.
                Signer::Nobody | Signer::RevokedAfter => {}
                Signer::Registered => { sign(&store, &registered, &name).expect("signed"); }
                Signer::Stranger => { sign(&store, &stranger, &name).expect("signed"); }
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
                sign(&store, &late, name).expect("signed");
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
    let store = Signing::new(begun(), crate::store::Signer::Key(key('1').into()));
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
    let store = begun();
    let earlier = store.append(done("alpha", "ada")).expect("appends");
    let keys = BoxKeys::of(
        &root().public(),
        &crate::capability::Secret::new("9".repeat(64)),
        &crate::capability::Secret::new("8".repeat(64)),
    )
    .expect("the deployment's keys");
    let chain = crate::chains::ChainPolicy {
        name: crate::chains::ChainName::new("life").expect("a slug"),
        untrusted: prodrome_core::policy::Untrusted::of([actor("triage")]),
        roster: [actor("ada"), actor("triage")].into_iter().collect(),
        root: root().public(),
    };
    adopt(&store, &keys, &chain).expect("adopted");
    let findings = |store: &InMemory<TodoEvent>| {
        let dag = dag(store);
        since_epoch(&dag, dag.verify(&chain.untrusted))
    };
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
        [prodrome_core::dag::Finding::Unsigned { object, .. }] if *object == later
    ));
    sign(&store, keys.root(), &later).expect("signed");
    assert!(findings(&store).is_empty());
}

/// A chain as the instance sample writes it, `untrusted` named or empty: the agent's actor is
/// the agent key's either way.
fn chain_trusting(untrusted: Untrusted) -> crate::chains::ChainPolicy {
    crate::chains::ChainPolicy {
        name: crate::chains::ChainName::new("life").expect("a slug"),
        untrusted,
        roster: [actor("ada"), actor("assistant"), actor("triage")]
            .into_iter()
            .collect(),
        root: root().public(),
    }
}

/// ADOPTION REGISTERS EACH ACTOR WITH THE KEY ITS WRITES ARE SIGNED BY, under either policy the
/// instance file may name: after it, the agent's write as its actor is proven and the box's key
/// speaking as that actor is not.
#[test]
fn adoption_registers_each_actor_with_the_key_that_signs_its_writes() {
    for untrusted in [Untrusted::none(), Untrusted::of([actor("triage")])] {
        let store = begun();
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
        let findings = since_epoch(&dag(&store), dag(&store).verify(&chain.untrusted));
        assert!(
            matches!(
                findings.as_slice(),
                [prodrome_core::dag::Finding::Unsigned { object, .. }] if *object == boxs
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
        crate::store::Signer::Key(key) => key.public(),
        crate::store::Signer::Refusing(_) => panic!("no key for {who}"),
    };
    assert_eq!(key("triage"), keys.agent().public());
    assert_eq!(key("ada"), keys.root().public());
}

/// A REVOCATION IS PER (ACTOR, KEY): the box's key revoked for the agent's actor leaves it
/// speaking for every other actor, before and after, and what it signed as the agent's actor
/// before stays proven. Revoked twice, it is revoked once.
#[test]
fn revoking_a_key_for_one_actor_leaves_its_other_actors_trusted() {
    let store = begun();
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
    let store = begun();
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

/// A signature of `object` by `by`, honest or carried over from another object.
fn signature_of(by: &Secret, object: &Hash, honest: bool) -> Signed {
    if honest {
        by.sign(object)
    } else {
        Signed {
            object: object.clone(),
            ..by.sign(&prodrome_core::memo::name(b"another object", []))
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// `memo f = f`: a check read through the table is the check itself, whatever the table
    /// kept or dropped (a capacity of one to three drops nearly everything), and asked again
    /// it answers the same.
    #[test]
    fn a_kept_check_is_the_check(
        asks in proptest::collection::vec((0..6_usize, any::<bool>()), 1..32),
        capacity in 1..4_usize,
    ) {
        let table = Verified::new(capacity);
        let signer = key('1');
        for (object, honest) in asks {
            let signed = signature_of(&signer, &prodrome_core::memo::name(format!("object {object}").as_bytes(), []), honest);
            let name = prodrome_core::event::seal_hash(&Envelope::<TodoEvent>::Signed(signed.clone()));
            prop_assert_eq!(table.verifies(&name, &signed), signed.verifies());
            prop_assert_eq!(table.verifies(&name, &signed), honest);
        }
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
fn written_by(store: &InMemory<TodoEvent>, by: &Secret, object: &Envelope<TodoEvent>) {
    held(store, object);
    sign(store, by, &prodrome_core::event::seal_hash(object)).expect("signed");
}

fn taken(store: &InMemory<TodoEvent>, step: Step, n: usize) {
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
                    sign(store, &numbered(by), &name).expect("signed");
                }
            }
        }
        Step::Register(by, true) => {
            trust(store, &root(), &actor("ada"), &numbered(by).public()).expect("registered");
        }
        Step::Register(by, false) => written_by(
            store,
            &key('5'),
            &Envelope::KeyAdded(prodrome_core::sign::KeyAdded {
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
                prodrome_core::sign::mk_key_revoked(
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
        let store = begun();
        let warm = Verified::new(capacity);
        for (n, step) in steps.iter().enumerate() {
            taken(&store, *step, n);
            if n == steps.len() / 2 {
                let _ = Trust::through(&dag(&store), &roots(), &warm);
            }
        }
        let history = dag(&store);
        let reversed: Dag<TodoEvent> = store.objects().expect("reads").into_iter().rev().collect();
        for registrar in [roots(), Registrar::Anyone] {
            let cold = Trust::through(&history, &registrar, &Verified::new(KEPT_CHECKS));
            let proof = prodrome_core::sign::Proof::of(&history, &registrar);
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
    let store = begun();
    let phone = key('1');
    trust(&store, &root(), &actor("ada"), &phone.public()).expect("registered");
    let request = phone.sign(&prodrome_core::memo::name(b"a request", []));
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
