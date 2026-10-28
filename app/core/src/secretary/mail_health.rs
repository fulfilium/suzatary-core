//! The rule over the mail health register: an account failing for more than a day proposes a
//! todo on the project's chain, through the path every proposed changeset takes, and a tap
//! performs it.
//!
//! "Once" is identity, not a flag. The act is a function of the account and the instant it last
//! synced, never of the failure's words or of the day it is asked, so asking again is asking for
//! the same object, which the inbox already holds however it was answered; a new failure after a
//! success is a new act.

use crate::escape::escape_markup;
use crate::health::{Instant, Reading};
use crate::reference::Reference;
use crate::refusal::Refusal;
use crate::secretary::changes::{self, Act, Changeset};
use crate::secretary::store::{ConversationId, Proposal, Reasons};
use crate::secretary::{Bound, WRITES_AS};
use crate::source_key::SourceKey;
use crate::todo::{Price, TodoFields};

/// What the proposed todo is priced: an account that does not sync hides its mail from triage,
/// a real consequence within weeks.
pub const PERCENT: u32 = 45;

/// The todo an account's failure since `since` proposes: `mailsync` and a digest of the two,
/// since a todo id is letters and digits and an address is not.
#[must_use]
pub fn todo_of(account: &str, since: Instant) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(format!(
        "{}\n{}",
        account.to_lowercase(),
        since.format("%Y-%m-%dT%H:%M:%S")
    ));
    format!("mailsync{}", &hex::encode(digest)[..12])
}

/// An instant as the owner reads it, his zone named: `2026-10-01 05:00 America/New_York`.
fn on_the_owners_clock(zone: crate::zone::Zone, at: Instant) -> String {
    format!("{} {}", zone.wall(at).format("%Y-%m-%d %H:%M"), zone.name())
}

/// The act an account's failure since `since` proposes: its text is the box's own, the account
/// escaped as data, and nothing in it is the sync's words.
#[must_use]
pub fn act_of(zone: crate::zone::Zone, account: &str, since: Instant) -> Act {
    Act::Add {
        todo: todo_of(account, since),
        fields: TodoFields {
            body: escape_markup(&format!("Restore the mail sync of {account}"))
                .as_str()
                .to_owned(),
            rationale: "An account that does not sync hides its mail from triage until it is \
                        fixed, a real consequence within weeks."
                .to_owned(),
            kind: "suzateryTodo".to_owned(),
            category: String::new(),
            price: Price::Flat { percent: PERCENT },
            detail: escape_markup(&format!(
                "It has not synced since {}; the Settings page shows what its sync says now.",
                on_the_owners_clock(zone, since)
            ))
            .as_str()
            .to_owned(),
            waiting_on: String::new(),
        },
        from: changes::Provenance::default(),
    }
}

/// How long an account may fail, in days, before the box proposes a todo about it.
const PERSISTENT: i64 = 1;

/// Propose the todo for every account failing longer than a day at `now`, on
/// the project's chain (second in the instance file's order of meaning, or the only one), in
/// `conversation`; answers each proposal's index. An account whose todo the chain already
/// holds proposes nothing.
///
/// # Errors
/// The register's, a chain's or the inbox's failure, and an act the chain would refuse.
pub fn propose_persistent(
    chains: &[Bound<'_>],
    project: &str,
    categories: &[&str],
    zone: crate::zone::Zone,
    conversation: &ConversationId,
) -> Result<Vec<usize>, Refusal> {
    let Some(bound) = chains.iter().find(|bound| bound.name == project) else {
        return Err(Refusal::internal(format!(
            "no chain named {project} is bound"
        )));
    };
    let caps = &bound.caps;
    let now = prodrome_core::fpl::instant_of(caps.clock.now());
    let healths = caps
        .health
        .healths()
        .map_err(|failed| Refusal::internal(format!("the sources' health register: {failed}")))?;
    let nodes = caps.chain.nodes()?;
    let held = crate::view::events_of(&nodes);
    let mut proposed = Vec::new();
    for (source, reading) in healths.readings() {
        let (SourceKey::Mail(account), Reading::Failing { words, since, .. }) = (source, &reading)
        else {
            continue;
        };
        let account = account.as_str();
        if reading.failing_for(now) <= Some(chrono::Duration::days(PERSISTENT)) {
            continue;
        }
        let todo = todo_of(account, *since);
        if held.iter().any(|event| event.todo().as_str() == todo) {
            continue;
        }
        let changeset = Changeset::new(project, vec![act_of(zone, account, *since)])
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        let preview = crate::believed::Preview::of(caps.chain.objects()?)
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        changes::events_of(
            &changeset,
            &preview,
            &changes::Building {
                now: crate::zone::Moment {
                    at: caps.clock.now(),
                    zone,
                },
                by: WRITES_AS,
                categories,
                untrusted: &bound.policy.untrusted,
                typeset: caps.render,
            },
        )?;
        let reasons = Reasons::new(
            format!(
                "{account} has not synced since {}, and its sync says: {words}",
                on_the_owners_clock(zone, *since)
            ),
            vec![Reference::Briefing {
                date: zone.day(now),
            }],
        )
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
        proposed.push(super::verbs::propose(
            caps,
            &[],
            conversation,
            Proposal::Changes(changeset),
            &reasons,
            None,
            None,
        )?);
    }
    Ok(proposed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A register holding one account that last synced `hours` before the clock.
    struct FailingFor(i64);

    impl crate::health::Register<SourceKey> for FailingFor {
        fn healths(&self) -> Result<crate::health::Healths<SourceKey>, crate::store::StoreError> {
            use crate::capability::Clock as _;
            let now = prodrome_core::fpl::instant_of(crate::capability::Nothing.now());
            let row = |ok| crate::mail::verbs::SyncReport {
                unheld: None,
                synced: vec![crate::mail::verbs::AccountSync {
                    account: crate::mail::account_email::AccountEmail::new("ada@hotmail.test")
                        .expect("an address"),
                    outcome: ok,
                    tail: "IMAP: not connected".to_owned(),
                }],
                notmuch: String::new(),
            };
            let at = |hours| now - chrono::Duration::hours(hours);
            let failed = crate::health::Outcome::Failed(crate::connect::Failure::Unclassified);
            Ok(row(crate::health::Outcome::Succeeded)
                .healths(at(self.0))
                .join(&row(failed).healths(at(1))))
        }
    }

    fn proposed(health: &FailingFor, stores: &Stores) -> Vec<usize> {
        let policy = crate::chains::ChainPolicy {
            name: crate::chains::ChainName::new("suzatary").expect("a slug"),
            untrusted: prodrome_core::policy::Untrusted::of([]),
            roster: std::collections::BTreeSet::new(),
            root: crate::cast::keys::root(),
        };
        let caps = crate::capability::Secretary {
            health,
            conversations: stores.lines.conversations(),
            inbox: &stores.inbox,
            ..crate::capability::Nothing::secretary()
        };
        propose_persistent(
            &[Bound {
                name: "suzatary",
                caps,
                policy: &policy,
            }],
            "suzatary",
            &[],
            crate::zone::Zone::UTC,
            &stores.id,
        )
        .expect("an answer")
    }

    struct Stores {
        lines: crate::secretary::conversations::InMemory,
        inbox: crate::store::InMemory<crate::secretary::inbox_event::InboxEvent>,
        id: ConversationId,
    }

    fn stores() -> Stores {
        let stores = Stores {
            lines: crate::secretary::conversations::InMemory::default(),
            inbox: crate::store::InMemory::default(),
            id: ConversationId::new("default").expect("a slug"),
        };
        crate::secretary::transcript::start(
            stores.lines.conversations(),
            &stores.id,
            &crate::secretary::store::Entry::Opened {
                at: crate::capability::Datetime::new(2026, 10, 1, 8, 0, 0, 0).expect("an instant"),
                title: crate::secretary::words::Words::kept("Default".to_owned()),
                model: "a fake model".to_owned(),
                kind: crate::secretary::store::Kind::Topic,
            },
        )
        .expect("opens");
        stores
    }

    /// A day's failure proposes one todo, asking again asks for the same one, and a failure
    /// younger than a day proposes nothing.
    #[test]
    fn a_failure_past_a_day_is_proposed_once_and_a_younger_one_not_at_all() {
        let held = stores();
        assert_eq!(proposed(&FailingFor(30), &held), vec![0]);
        assert_eq!(proposed(&FailingFor(30), &held), vec![0], "the same object");
        let inbox = crate::secretary::inbox::fold(&held.inbox).expect("reads");
        assert_eq!(inbox.objects().count(), 1);

        assert!(proposed(&FailingFor(20), &stores()).is_empty());
    }
}
