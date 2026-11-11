//! The box's proposals (`app/core/tests/web/proposals.json`) read by the card: an open one has
//! something to press, a closed one nothing.

use suzatary_core::wire::topics::{ProposalRow, ProposalState};
use suzatary_pages::card::answerable;

#[derive(serde::Deserialize)]
struct Open {
    open: bool,
}

fn read<T: serde::de::DeserializeOwned>(file: &str) -> T {
    let path = format!("{}/../core/tests/web/{file}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("the vector is in the tree"))
        .expect("the vector reads as its type")
}

#[test]
fn the_page_and_the_box_agree_which_proposals_are_open() {
    let rows: Vec<ProposalRow> = read("proposals.json");
    let wants: Vec<Open> = read("proposals.answer.json");
    assert_eq!(rows.len(), wants.len());
    for (row, want) in rows.iter().zip(&wants) {
        let open = answerable(row.state)
            || matches!(row.state, ProposalState::Started | ProposalState::Unknown);
        assert_eq!(open, want.open, "{:?}", row.state);
    }
}
