//! The settings page's overview over an inbox held in core's memory.

use proptest::prelude::*;
use suzatary_cards::settings::overview::world::{World, account};
use suzatary_cards::settings::overview::*;
use suzatary_connect::connect::Group;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::proposal::Proposal;

proptest! {
    /// A draft waiting to be sent from an account never hides its sign-in: whichever was
    /// asked first, the account's line draws the sign-in's card, and the draft's nowhere.
    #[test]
    fn a_waiting_draft_never_hides_the_accounts_sign_in(draft_first in any::<bool>(), kind in 0u8..4) {
        use suzatary_inbox::secretary::inbox;
        use suzatary_base::conversation_id::ConversationId;
        use suzatary_inbox::secretary::proposal::{Draft, Reasons};
        let email = "ada@example.test";
        let store = suzatary_secretary::stores::InMemory::<suzatary_inbox::secretary::inbox_event::InboxEvent>::default();
        let ask = |index: usize, proposal: Proposal, minute: u32| {
            let asked = inbox::Asked {
                proposer: suzatary_inbox::secretary::proposer::Proposer::Secretary,
                proposal,
                reasons: Reasons::new("asked", vec!["briefing:2026-10-07".parse().expect("a reference")]).expect("reasons"),
                asked: suzatary_base::reference::ProposalRef {
                    conversation: ConversationId::new("setup").expect("a slug"),
                    index,
                },
                price: inbox::Price::unpriced(),
            };
            let at = suzatary_base::capability::Datetime::new(2026, 10, 7, 9, minute, 0, 0).expect("an instant");
            inbox::ask(&store, asked, &suzatary_lines::secretary::transcript::writer(), at).expect("filed");
        };
        let (draft, sign_in) = if draft_first { (0, 1) } else { (1, 0) };
        ask(0, Proposal::Email(Draft { account: email.to_owned(), ..Draft::default() }), draft);
        ask(1, Proposal::Act(Act::Auth {
            account: suzatary_mail::mail::address::EmailAddress::new(email).expect("an address"),
        }), sign_in);
        let mut world = World {
            accounts: vec![account(email, kind)],
            ..World::empty()
        };
        world.asking = Asking::open(&inbox::fold(&store).expect("reads"));
        let (page, _) = world.wire();
        let mail = page.groups.iter().find(|group| group.group == Group::Mail).expect("a mail group");
        prop_assert_eq!(&mail.lines[0].cards, &vec!["proposal:setup/1".to_owned()]);
    }
}
