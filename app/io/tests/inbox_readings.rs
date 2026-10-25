//! Every proposal's reading, pinned: what the frozen proposal store reads as, proposal by
//! proposal, so a change to how the inbox is stored or folded shows here first.
//!
//! `fixtures/inbox-proposals/` holds objects written once, taking one proposal through each
//! response, and never regenerated, and `readings.txt` beside them (`pinned`, below):
//! per proposal, its reading and what follows from it, its value, its responses and what a tap
//! would do, all at [`now`], priced through [`life`].

use std::path::{Path, PathBuf};

use prodrome_core::event::{mk_sealed, mk_spec_revised, seal_hash};
use prodrome_core::fpl::mk_flat;
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use suzatary_core::chain_set::ChainSet;
use suzatary_core::chain_set::Read;
use suzatary_core::chains::ChainName;
use suzatary_core::rule::Rule as _;
use suzatary_core::secretary::inbox::{Fold, Object};
use suzatary_core::secretary::inbox_event::Said;
use suzatary_core::secretary::store::stamp;
use suzatary_core::secretary::tapped::Tapped;
use suzatary_core::store::Reads as _;
use suzatary_io::objects::Objects;

/// The instant every reading is taken at: after every response in the frozen store, before the
/// live device code's expiry.
fn now() -> Datetime {
    Datetime::new(2026, 10, 1, 12, 0, 0, 0).expect("an instant")
}

fn store() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inbox-proposals")
}

/// The chains the readings are priced through: `life`, whose `reply` (which one frozen proposal
/// serves) is priced 0.3, so that proposal reads its todo's 0.3.
fn life() -> Read {
    let root = mk_sealed(
        None,
        mk_spec_revised(
            "reply",
            Datetime::new(2026, 9, 27, 9, 0, 0, 0).expect("an instant"),
            "ada",
            mk_flat(0.3).expect("a flat"),
            "",
        )
        .expect("an event"),
    );
    Read::of_objects([(
        ChainName::new("life").expect("a slug"),
        Untrusted::none(),
        vec![(seal_hash(&root), root)],
    )])
}

fn printed(fold: &Fold) -> String {
    pinned(fold, &life().set(now()))
}

/// Every proposal of `fold` at the set's instant, three lines each, the frozen inbox's
/// `readings.txt`: when it was asked, its reading and what follows from it, its value priced
/// through the chains' `set`; every response; and what a tap would do. Then a line for each
/// proposal answered and never asked.
fn pinned(fold: &Fold, set: &ChainSet<'_>) -> String {
    let unasked = fold
        .unasked()
        .iter()
        .map(|id| format!("{id} answered and never asked\n"));
    fold.objects()
        .map(|object| pin(fold, object, set))
        .chain(unasked)
        .collect()
}

fn pin(fold: &Fold, object: &Object, set: &ChainSet<'_>) -> String {
    let responses: Vec<String> = object.responses.iter().map(ToString::to_string).collect();
    let tapped = Tapped { now: set.at() };
    let decide = match fold.one(&object.id) {
        Ok(one) if tapped.holds(one) => "perform".to_owned(),
        Ok(_) => "answer".to_owned(),
        Err(refused) => format!("refused: {}", refused.message()),
    };
    format!(
        "{id} at={at} reading={reading} open={open} performs={performs} tries={tries} \
         value={value:?}\n  responses=[{responses}]\n  decide={decide}\n",
        id = object.id,
        at = stamp(object.at),
        reading = object.reading,
        open = object.reading.open(),
        performs = object.reading.performs(),
        tries = object.tries(),
        value = suzatary_core::secretary::inbox::value(object, set),
        responses = responses.join(", "),
    )
}

/// `inbox-proposals` reads as `readings.txt` beside it, and `registers.json` is each proposal's
/// registers as this program reads them, what the browser module's `Proposals` must agree with
/// (`web/test/proposals.test.ts`).
#[test]
fn the_frozen_proposal_store_reads_as_pinned() {
    let dir = store();
    let store = Objects::open(&dir);
    let fold = suzatary_core::secretary::inbox::fold(&store).expect("the proposal store folds");
    let pinned = std::fs::read_to_string(dir.join("readings.txt")).expect("the pinned readings");
    assert_eq!(printed(&fold), pinned);

    let asked: std::collections::BTreeMap<String, String> = store
        .nodes()
        .expect("the proposal store reads")
        .into_iter()
        .filter_map(|node| {
            let event = node.event()?;
            matches!(event.said, Said::Asked(_)).then(|| {
                (
                    event.proposal.as_str().to_owned(),
                    node.name().as_str().to_owned(),
                )
            })
        })
        .collect();
    let registers: serde_json::Map<String, serde_json::Value> = fold
        .objects()
        .map(|object| {
            let id = object.id.as_str();
            let state: Vec<&str> = object
                .reading
                .answers()
                .into_iter()
                .map(|answer| answer.name.as_str())
                .collect();
            (
                id.to_owned(),
                serde_json::json!({ "content": [asked[id]], "state": state }),
            )
        })
        .collect();
    let printed = format!(
        "{}\n",
        serde_json::to_string_pretty(&registers).expect("a vector serialises")
    );
    let path = dir.join("registers.json");
    if std::env::var_os("SUZATARY_WRITE_VECTORS").is_some() {
        std::fs::write(&path, &printed).expect("the fixture is writable");
    }
    assert_eq!(
        std::fs::read_to_string(&path).expect("registers.json is committed"),
        printed,
        "the proposal store's registers moved: a finding, never a file to refresh"
    );
}
