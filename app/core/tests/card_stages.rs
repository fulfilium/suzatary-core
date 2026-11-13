//! A repository card at each stage its publish can stop at: its own words, and only the answers
//! that stage admits.

use suzatary_core::repos::Visibility;
use suzatary_core::secretary::card::{at, of};
use suzatary_core::secretary::inbox_reading::{Answer, Reading};
use suzatary_core::secretary::responded::{Responded, Tried};
use suzatary_core::wire::inbox::{CardLink, ProposedAct};

#[test]
fn a_repository_card_says_each_stage_in_its_own_words() {
    let card = of(&ProposedAct::Patch {
        repo: "owner/cv".to_owned(),
        visibility: Visibility::Public,
        base: "a".repeat(40),
        message: "Retitle".to_owned(),
        files: vec!["cv.typ".to_owned()],
        patch: "--- a/cv.typ\n+++ b/cv.typ\n@@ -1 +1 @@\n-a\n+b\n".to_owned(),
        sentence: "open a pull request on owner/cv (1 file): Retitle".to_owned(),
    });
    assert_eq!(card.check, "I checked owner/cv's pull requests");
    assert_eq!(card.failed, "Not published");
    let after = |tried| {
        at(
            card.clone(),
            &Reading::One(Answer {
                name: prodrome_core::event::Hash::new("a".repeat(64)).expect("hex"),
                at: prodrome_core::literal::Datetime::new(2026, 9, 28, 9, 0, 0, 0)
                    .expect("an instant"),
                response: Responded::Tried { attempt: 1, tried },
            }),
            &suzatary_core::secretary::inbox::ProposalId::new(format!("p{:032x}", 1))
                .expect("an id"),
        )
    };
    let reason = || "why".to_owned();
    assert!(
        after(Tried::Refused { reason: reason() })
            .answers
            .is_empty()
    );
    assert_eq!(
        after(Tried::NotSent { reason: reason() }).answers[0].label,
        "Try again"
    );
    let link = "https://github.com/owner/cv/pulls".to_owned();
    let unfinished = after(Tried::Unfinished {
        reason: reason(),
        link: link.clone(),
    });
    assert_eq!(unfinished.answers[0].label, "Try again");
    assert_eq!(
        unfinished.link,
        Some(CardLink::Page {
            label: "See where it stands".to_owned(),
            href: link,
        })
    );
}
