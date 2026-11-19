//! The undo of a withdrawal, over the stores held in core's memory.

use suzatary_base::capability::Datetime;
use suzatary_cards::secretary::undo::*;
use suzatary_inbox::secretary::changes::Act as Change;
use suzatary_inbox::secretary::changes::Changeset;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::Reasons;
use suzatary_inbox::secretary::responded::Withdrawal;
use suzatary_stores::stores::conversations::InMemory;
use suzatary_topics::secretary::filed::TriageCards;

/// A tapped restore applies exactly the changes of the card the secretary withdrew, and of no
/// card still standing.
#[test]
fn a_restore_applies_the_withdrawn_card_s_changes_alone() {
    let (lines, inbox) = (
        InMemory::default(),
        suzatary_stores::stores::InMemory::<InboxEvent>::default(),
    );
    let at = Datetime::new(2026, 11, 8, 9, 0, 0, 0).expect("an instant");
    let file = |todo: &str| {
        let changes = Changeset::new(
            "life",
            vec![Change::Complete {
                todo: todo.to_owned(),
                note: "it looked done".to_owned(),
            }],
        )
        .expect("one act");
        let reasons = Reasons::new(
            "a message says so",
            vec!["briefing:2026-11-08".parse().expect("a reference")],
        )
        .expect("reasons");
        let card = TriageCards::new(lines.conversations(), &inbox)
            .file(at, changes.clone(), reasons, None, &|_| Ok(()))
            .expect("filed")
            .card;
        (card, changes)
    };
    let (withdrawn_card, changes) = file("rent");
    let (standing_card, _) = file("lease");
    let id = inbox::fold(&inbox)
        .expect("reads")
        .objects()
        .find(|object| object.asked.asked == withdrawn_card)
        .expect("filed")
        .id
        .clone();
    inbox::withdraw_unanswered(
        &inbox,
        &id,
        &suzatary_lines::secretary::transcript::writer(),
        at,
        Withdrawal::NoLongerNeeded {
            why: "a duplicate".to_owned(),
        },
    )
    .expect("withdrawn");
    assert_eq!(withdrawn(&inbox, &withdrawn_card).expect("reads"), changes);
    assert!(
        withdrawn(&inbox, &standing_card).is_err(),
        "a standing card is no restore's"
    );
}
