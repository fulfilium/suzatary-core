//! An account whose sync has stalled (failed for longer than `stalled_after`) proposes a todo on the upkeep chain, which a tap
//! performs; the act is a function of the account and the instant it last synced, so asking again
//! asks for the same object, and a new failure after a success is a new act.

use crate::secretary::{Bound, WRITES_AS};
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::escape::escape_markup;
use suzatary_base::reference::Reference;
use suzatary_base::refusal::Refusal;
use suzatary_base::zone::Zone;
use suzatary_connect::deployment::Deployment;
use suzatary_health::health::SourceKey;
use suzatary_health::health::{Instant, Reading};
use suzatary_inbox::secretary::changes;
use suzatary_inbox::secretary::changes::{Act, Changeset};
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_record::todo::{Price, TodoFields};

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
fn on_the_owners_clock(zone: Zone, at: Instant) -> String {
    format!("{} {}", zone.wall(at).format("%Y-%m-%d %H:%M"), zone.name())
}

/// The act an account's failure since `since` proposes: its text is the box's own, the account
/// escaped as data, and nothing in it is the sync's words.
#[must_use]
pub fn act_of(zone: Zone, account: &str, since: Instant) -> Act {
    Act::Add {
        todo: todo_of(account, since),
        fields: TodoFields {
            body: escape_markup(&format!("Restore the mail sync of {account}"))
                .as_str()
                .to_owned(),
            rationale: "An account that does not sync hides its mail from triage until it is \
                        fixed, a real consequence within weeks."
                .to_owned(),
            kind: "todo".to_owned(),
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

/// Propose the todo for every account whose sync has stalled at `now`, on the upkeep chain,
/// in `conversation`; answers each proposal's index. A deployment with no `[upkeep]` proposes
/// nothing, and neither does an account whose todo the chain already holds.
pub fn propose_persistent(
    chains: &[Bound<'_>],
    deployment: &Deployment,
    conversation: &ConversationId,
) -> Result<Vec<usize>, Refusal> {
    let Some(bound) = super::upkeep(chains, deployment)? else {
        return Ok(Vec::new());
    };
    let zone = deployment.zone;
    let categories: Vec<&str> = deployment.categories.keys().map(String::as_str).collect();
    let caps = &bound.caps;
    let now = prodrome_data::instant::instant_of(caps.clock.now());
    let healths = caps
        .health
        .healths()
        .map_err(|failed| Refusal::internal(format!("the sources' health register: {failed}")))?;
    let nodes = caps.chain.nodes()?;
    let held = suzatary_todo::view::events_of(&nodes);
    let mut proposed = Vec::new();
    for (source, reading) in healths.readings() {
        let (SourceKey::Mail(account), Reading::Failing { words, since, .. }) = (source, &reading)
        else {
            continue;
        };
        let account = account.as_str();
        if reading.stalled_since(now).is_none() {
            continue;
        }
        let todo = todo_of(account, *since);
        if held.iter().any(|event| event.todo().as_str() == todo) {
            continue;
        }
        let changeset = Changeset::new(bound.name, vec![act_of(zone, account, *since)])
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        let preview = suzatary_signatures::believed::Preview::of(caps.chain.objects()?)
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        changes::events_of(
            &changeset,
            &preview,
            &changes::Building {
                now: suzatary_base::zone::Moment {
                    at: caps.clock.now(),
                    zone,
                },
                by: WRITES_AS,
                categories: &categories,
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

    /// A register holding one account that last synced `minutes` before the clock and has failed
    /// ever since.
    struct FailingFor(i64);

    impl suzatary_health::health::Register<SourceKey> for FailingFor {
        fn healths(
            &self,
        ) -> Result<suzatary_health::health::Healths<SourceKey>, suzatary_store::store::StoreError>
        {
            use suzatary_base::capability::Clock as _;
            let now = prodrome_data::instant::instant_of(suzatary_base::capability::Nothing.now());
            let row = |ok| suzatary_mail::mail::sync_report::SyncReport {
                unheld: None,
                synced: vec![suzatary_mail::mail::sync_report::AccountSync {
                    account: suzatary_mail::mail::account_email::AccountEmail::new(
                        "ada@hotmail.test",
                    )
                    .expect("an address"),
                    outcome: ok,
                    tail: "IMAP: not connected".to_owned(),
                }],
                notmuch: String::new(),
            };
            let at = |minutes| now - chrono::Duration::minutes(minutes);
            let failed = suzatary_mail::outcome::Outcome::Failed(
                suzatary_mail::mail::fault::Fault::Unclassified,
            );
            let healths = suzatary_health::health::Healths::of_sync;
            Ok(
                healths(&row(suzatary_mail::outcome::Outcome::Succeeded), at(self.0))
                    .join(&healths(&row(failed), at(0))),
            )
        }
    }

    /// What the rule proposes over two bound chains, `life` and `ops`, when `[upkeep]` names
    /// `upkeep`, and the chain each card in the inbox then writes to.
    fn proposed(
        health: &FailingFor,
        stores: &Stores,
        upkeep: Option<&str>,
    ) -> (Vec<usize>, Vec<String>) {
        use suzatary_base::chains::{ChainName, ChainPolicy, Chains};
        let policy = |name: &str| ChainPolicy {
            name: ChainName::new(name).expect("a slug"),
            untrusted: prodrome_data::policy::Untrusted::of([]),
            roster: std::collections::BTreeSet::new(),
            root: suzatary_connect::cast::keys::root(),
            naming: suzatary_base::chains::Naming::AsActs,
        };
        let (life, ops) = (policy("life"), policy("ops"));
        let deployment = Deployment {
            viewers: std::collections::BTreeSet::new(),
            actors: std::collections::BTreeMap::new(),
            chains: Chains::of(vec![life.clone(), ops.clone()]).expect("two chains"),
            categories: std::collections::BTreeMap::new(),
            app_url: "https://example.test".to_owned(),
            coding: None,
            upkeep: upkeep.map(|name| ChainName::new(name).expect("a slug")),
            zone: suzatary_base::zone::Zone::UTC,
        };
        let caps = crate::secretary::Secretary {
            health,
            conversations: stores.lines.conversations(),
            inbox: &stores.inbox,
            ..crate::secretary::Secretary::nothing()
        };
        let chains = [&life, &ops].map(|policy| Bound {
            name: policy.name.as_str(),
            caps,
            policy,
        });
        let indices = propose_persistent(&chains, &deployment, &stores.id).expect("an answer");
        let inbox = suzatary_inbox::secretary::inbox::fold(&stores.inbox).expect("reads");
        let written = inbox.objects().map(|object| match &object.asked.proposal {
            Proposal::Changes(changeset) => changeset.chain().to_owned(),
            other => panic!("the rule proposes chain acts, not {other:?}"),
        });
        (indices, written.collect())
    }

    struct Stores {
        lines: suzatary_stores::stores::conversations::InMemory,
        inbox:
            suzatary_stores::stores::InMemory<suzatary_inbox::secretary::inbox_event::InboxEvent>,
        id: ConversationId,
    }

    fn stores() -> Stores {
        let stores = Stores {
            lines: suzatary_stores::stores::conversations::InMemory::default(),
            inbox: suzatary_stores::stores::InMemory::default(),
            id: ConversationId::new("default").expect("a slug"),
        };
        suzatary_lines::secretary::transcript::start(
            stores.lines.conversations(),
            &stores.id,
            &suzatary_conversations::secretary::store::Entry::Opened {
                at: suzatary_base::capability::Datetime::new(2026, 10, 1, 8, 0, 0, 0)
                    .expect("an instant"),
                title: suzatary_memory::secretary::words::Words::kept("Default".to_owned()),
                model: "a fake model".to_owned(),
                kind: suzatary_conversations::secretary::store::Kind::Topic,
            },
        )
        .expect("opens");
        stores
    }

    /// An hour's failure proposes one todo on the upkeep chain alone, whatever it is called (the
    /// routing boundary), and asking again asks for the same one; a younger failure, or no
    /// `[upkeep]`, proposes nothing.
    #[test]
    fn a_failure_past_an_hour_is_proposed_once_on_the_upkeep_chain() {
        for upkeep in ["ops", "life"] {
            let (held, once) = (stores(), (vec![0], vec![upkeep.to_owned()]));
            assert_eq!(proposed(&FailingFor(90), &held, Some(upkeep)), once);
            assert_eq!(
                proposed(&FailingFor(90), &held, Some(upkeep)),
                once,
                "the same object"
            );
        }
        let nothing = (Vec::new(), Vec::new());
        assert_eq!(proposed(&FailingFor(30), &stores(), Some("ops")), nothing);
        assert_eq!(proposed(&FailingFor(90), &stores(), None), nothing);
    }
}
