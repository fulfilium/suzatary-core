//! A calendar card lapses once its event has ended (`secretary::lapsed`).
use chrono::NaiveDate;
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::ProposalRef;
use suzatary_base::zone::Zone;
use suzatary_core::calendar::When;
use suzatary_core::secretary::acts::Act;
use suzatary_core::secretary::inbox::{self, Asked, Price};
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::inbox_reading::Reading;
use suzatary_core::secretary::lapsed::sweep;
use suzatary_core::secretary::proposal::Proposal;
use suzatary_core::secretary::proposal::Reasons;
use suzatary_core::secretary::proposer::Proposer;
use suzatary_core::secretary::responded::Responded;
use suzatary_core::secretary::responded::Withdrawal;
use suzatary_core::stores::InMemory;

fn at(day: u32, hour: u32) -> Datetime {
    Datetime::new(2026, 11, day, hour, 0, 0, 0).expect("an instant")
}

fn card(inbox: &InMemory<InboxEvent>, index: usize, when: When) -> inbox::ProposalId {
    let event = suzatary_core::google::NewEvent::new("Dentist", when, "").expect("an event");
    inbox::ask(
        inbox,
        Asked {
            proposer: Proposer::Secretary,
            proposal: Proposal::Act(Act::CalendarEvent {
                account: suzatary_core::mail::address::EmailAddress::new("ada@example.test")
                    .expect("an address"),
                event,
            }),
            reasons: Reasons::new(
                "the owner asked",
                vec!["briefing:2026-11-12".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: ProposalRef {
                conversation: ConversationId::new("default").expect("a slug"),
                index,
            },
            price: Price::unpriced(),
        },
        &suzatary_core::secretary::transcript::writer(),
        at(1, 9),
    )
    .expect("filed")
}

fn withdrawn(inbox: &InMemory<InboxEvent>, id: &inbox::ProposalId) -> bool {
    let fold = inbox::fold(inbox).expect("reads");
    matches!(
        &fold.get(id).expect("held").reading,
        Reading::One(answer)
            if matches!(answer.response, Responded::Withdrawn(Withdrawal::NoLongerNeeded { .. }))
    )
}

/// A calendar card closes as no longer needed once its event has ended, and not before:
/// a timed event at its end, an all-day one when the owner's day after its last begins.
#[test]
fn a_calendar_card_lapses_once_its_event_has_ended() {
    let inbox = InMemory::<InboxEvent>::default();
    let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).expect("a day");
    let timed = card(
        &inbox,
        0,
        When::timed(
            day(12).and_hms_opt(9, 0, 0).expect("a time"),
            day(12).and_hms_opt(10, 0, 0).expect("a time"),
        )
        .expect("an interval"),
    );
    let all_day = card(&inbox, 1, When::all_day(day(12), day(13)).expect("a day"));
    let zone = Zone::UTC;

    let early = sweep(&inbox, zone, at(12, 9)).expect("swept");
    assert!(early.closed.is_empty(), "{early:?}");
    let after_timed = sweep(&inbox, zone, at(12, 10)).expect("swept");
    assert_eq!(after_timed.closed, vec![timed.clone()]);
    assert!(withdrawn(&inbox, &timed));
    assert!(!withdrawn(&inbox, &all_day));
    let next_day = sweep(&inbox, zone, at(13, 0)).expect("swept");
    assert_eq!(next_day.closed, vec![all_day.clone()]);
    assert!(withdrawn(&inbox, &all_day));
    assert!(
        sweep(&inbox, zone, at(14, 0))
            .expect("swept")
            .closed
            .is_empty()
    );
}
