//! A held replica's snapshot is a stored format: the lines a tab's core holds print, and read
//! back holding the same nodes and keys, whatever step each line takes; a print of the read-back
//! replica is the same bytes; and bytes of another form, cut short, or holding a key that is not
//! its name's read as no replica, never as another one.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use suzatary_core::secretary::held_replica::HeldReplica;
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::outlines::Parsed;
use suzatary_core::secretary::step::Step;
use suzatary_core::snapshot::{NotASnapshot, from_snapshot, snapshot};
use suzatary_core::wire::{NodeHeader, SkeletonNode};

fn name(seed: u8) -> String {
    hex::encode(Sha256::digest([seed]))
}

/// A key's bytes and the name they hash to.
fn key(seed: u8) -> (KeyName, [u8; 32]) {
    let bytes = [seed; 32];
    let named = KeyName::new(hex::encode(Sha256::digest(bytes))).expect("a name");
    (named, bytes)
}

fn line(seed: u8, parents: &[String], keys: &[&KeyName], step: Step) -> SkeletonNode {
    SkeletonNode {
        hash: name(seed),
        parents: parents.to_vec(),
        size: usize::from(seed) * 10,
        header: NodeHeader::Line {
            conversation: "first".to_owned(),
            at: format!("2026-10-31T10:00:{seed:02}"),
            kind: "Said".to_owned(),
            keys: keys.iter().map(|k| k.as_str().to_owned()).collect(),
            step,
        },
    }
}

/// A replica holding a genesis and a line of every step, and the keys of two slots.
fn held() -> HeldReplica {
    let ((a, a_bytes), (b, b_bytes)) = (key(1), key(2));
    let genesis = SkeletonNode {
        hash: name(0),
        parents: Vec::new(),
        size: 3,
        header: NodeHeader::Structure {
            kind: "Genesis".to_owned(),
        },
    };
    let root = vec![name(0)];
    let steps = [
        Step::Opened,
        Step::Asks,
        Step::Noted,
        Step::Answers,
        Step::Rewound { to: 2 },
        Step::Proposed,
        Step::Coding { proposal: 0 },
        Step::Titled,
        Step::Resealed {
            of: a.clone(),
            from: b.clone(),
        },
        Step::Erased {
            keys: [b.clone()].into(),
        },
        Step::Still,
    ];
    let mut nodes = vec![genesis];
    for (at, step) in (1u8..).zip(steps) {
        nodes.push(line(at, &root, &[&a, &b], step));
    }
    let mut replica = HeldReplica::default();
    replica.outlines.hold(
        nodes
            .into_iter()
            .map(|n| Parsed::of(n).expect("a node"))
            .collect(),
        &[],
    );
    let keys: BTreeMap<KeyName, String> =
        [(a, hex::encode(a_bytes)), (b, hex::encode(b_bytes))].into();
    replica.lines.keys(&keys, &[]);
    replica
}

fn given(replica: &HeldReplica) -> Vec<SkeletonNode> {
    replica.outlines.given().cloned().collect()
}

#[test]
fn a_snapshot_reads_back_holding_the_same_nodes_and_keys() {
    let replica = held();
    let bytes = snapshot(&replica).expect("prints");
    let back: HeldReplica = from_snapshot(&bytes).expect("reads back");
    assert_eq!(given(&back), given(&replica), "the same nodes, as given");
    assert_eq!(
        back.lines.key_ring(),
        replica.lines.key_ring(),
        "the same keys"
    );
    assert_eq!(
        back.outlines.outlines().collect::<Vec<_>>(),
        replica.outlines.outlines().collect::<Vec<_>>(),
        "the same outlines, which the stream chooses its rounds by"
    );
    assert_eq!(snapshot(&back), Ok(bytes), "and it prints the same bytes");
}

#[test]
fn bytes_of_another_form_cut_short_or_holding_a_wrong_key_read_as_no_replica() {
    let bytes = snapshot(&held()).expect("prints");
    let mut other = bytes.clone();
    other[0] ^= 1;
    assert!(matches!(
        from_snapshot::<HeldReplica>(&other),
        Err(NotASnapshot::Form { .. })
    ));
    assert!(matches!(
        from_snapshot::<HeldReplica>(&bytes[..2]),
        Err(NotASnapshot::Short(2))
    ));
    assert!(matches!(
        from_snapshot::<HeldReplica>(&bytes[..bytes.len() - 1]),
        Err(NotASnapshot::Unread(_))
    ));
    // The last key's bytes end the print, so the last byte is one of them.
    let mut tampered = bytes.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert!(
        matches!(
            from_snapshot::<HeldReplica>(&tampered),
            Err(NotASnapshot::Unread(_))
        ),
        "a key whose bytes are not its name's"
    );
}
