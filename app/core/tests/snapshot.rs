//! The lines' rows' snapshot is a stored format: rows print, and read back holding the same
//! nodes, keys, words and frontiers, whatever step each line takes; a print of the read-back
//! rows is the same bytes; every name prints as its 32 bytes; and bytes of another form, cut
//! short, or holding a key that is not its name's read as no rows, never as other ones.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use prodrome_core::event::Hash;
use suzatary_core::replica::held::Said;
use suzatary_core::secretary::key_ring::KeyRing;
use suzatary_core::secretary::line_form::{LinePulling, LineReplica};
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::outlines::Parsed;
use suzatary_core::secretary::step::Step;
use suzatary_core::snapshot::{NotASnapshot, from_snapshot, snapshot};
use suzatary_core::wire::lines::{NodeHeader, SkeletonNode};
use suzatary_core::wire::store::SinceRead;

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

/// Rows whose landed row holds a genesis, a line of every step and the keys of two slots, and
/// whose pending piece holds the genesis again.
fn held() -> LineReplica {
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
    let nodes: Vec<Parsed> = nodes
        .into_iter()
        .map(|n| Parsed::of(n).expect("a node"))
        .collect();
    let hash = |seed: u8| Hash::new(name(seed)).expect("a name");
    let said = |nodes: Vec<Parsed>, heads: BTreeSet<Hash>| Said {
        heads,
        geneses: [hash(0)].into(),
        epoch: 7,
        nodes,
        since: SinceRead::Known,
        digest: hash(200),
        count: 12,
    };
    let keys = KeyRing::of(&[(a, hex::encode(a_bytes)), (b, hex::encode(b_bytes))].into());
    let mut rows = LinePulling::default();
    let heads = (1..12).map(hash).collect();
    rows.piece(said(nodes.clone(), heads), keys, 1_000);
    rows.land();
    rows.piece(
        said(nodes[..1].to_vec(), [hash(0)].into()),
        KeyRing::default(),
        2_000,
    );
    LineReplica {
        rows,
        ..LineReplica::default()
    }
}

#[test]
fn a_snapshot_reads_back_holding_the_same_rows() {
    let rows = held();
    let bytes = snapshot(&rows).expect("prints");
    let back: LineReplica = from_snapshot(&bytes).expect("reads back");
    assert_eq!(back, rows, "the same nodes, keys, words and frontiers");
    assert_eq!(
        snapshot(&back),
        Ok(bytes.clone()),
        "and it prints the same bytes"
    );
    let hex = name(1);
    assert!(
        !bytes.windows(hex.len()).any(|w| w == hex.as_bytes()),
        "no name is printed as its hex"
    );
}

#[test]
fn bytes_of_another_form_cut_short_or_holding_a_wrong_key_read_as_no_rows() {
    let bytes = snapshot(&held()).expect("prints");
    let mut other = bytes.clone();
    other[0] ^= 1;
    assert!(matches!(
        from_snapshot::<LineReplica>(&other),
        Err(NotASnapshot::Form { .. })
    ));
    assert!(matches!(
        from_snapshot::<LineReplica>(&bytes[..2]),
        Err(NotASnapshot::Short(2))
    ));
    assert!(matches!(
        from_snapshot::<LineReplica>(&bytes[..bytes.len() - 1]),
        Err(NotASnapshot::Unread(_))
    ));
    let (_, key_bytes) = key(1);
    let at = bytes
        .windows(32)
        .position(|w| w == key_bytes)
        .expect("the key is printed");
    let mut tampered = bytes.clone();
    tampered[at] ^= 1;
    assert!(
        matches!(
            from_snapshot::<LineReplica>(&tampered),
            Err(NotASnapshot::Refused(_))
        ),
        "a key whose bytes are not its name's"
    );
}
