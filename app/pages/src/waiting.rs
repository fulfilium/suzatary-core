//! What waits on the owner, most urgent first, each row resolved to its card: the inbox's
//! proposals, each chain's open decisions, and what was settled today.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::Deserialize;
use suzatary_core::reference::Reference;
use suzatary_core::secretary::store::Unheld;
use suzatary_core::wire::{
    Block, DrawnPayload, Entry, EntryState, FoldPayload, InboxProposal, InboxProposalsPayload,
    InboxState, ProposalRow, UnplacedProposal, WaitingPayload,
};
use ts_rs::TS;
use viewtree::kit::Reading;

use crate::card::{asker_name, is_setup};
use crate::runtime::{Answer, Local, read};
use crate::zone::{self, Offset};

/// Named, not discovered: the login lands after the first paint, and the Inbox answers quickly.
pub(crate) const CHAINS: [&str; 2] = ["life", "suzatary"];

/// The Inbox's proposals priced through every chain the tab holds (`Local::Proposals`).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct InboxRead {
    pub(crate) payload: InboxProposalsPayload,
    pub(crate) held: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Waiting<'a> {
    /// With the chain its price waits on where the reading did not hold the todo it serves: a price
    /// not yet known, which is not `∅`.
    Proposal(&'a InboxProposal, Option<&'a str>),
    Decision(&'a str, &'a Entry),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Where {
    Proposal(String, u32),
    Batch(String, String),
    Unresolved(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Standing<'a> {
    Open,
    Settled(&'static str, &'a str),
    Unreadable(&'a str),
}

#[must_use]
pub(crate) fn title(body: &str) -> String {
    body.lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !matches!(c, '#' | '*' | '_' | '\\' | '[' | ']'))
        .collect::<String>()
        .trim()
        .to_owned()
}

impl<'a> Waiting<'a> {
    #[must_use]
    pub(crate) fn key(self) -> &'a str {
        match self {
            Waiting::Proposal(p, _) => &p.id,
            Waiting::Decision(_, e) => &e.todo,
        }
    }

    #[must_use]
    pub(crate) fn standing(self) -> Standing<'a> {
        match self {
            Waiting::Proposal(p, _) => match &p.state {
                InboxState::Open {} => Standing::Open,
                InboxState::Completed { at } => Standing::Settled("completed", at),
                InboxState::Cancelled { at } => Standing::Settled("cancelled", at),
                InboxState::Unreadable { reason } => Standing::Unreadable(reason),
            },
            Waiting::Decision(_, e) => match e.state {
                EntryState::Open => Standing::Open,
                EntryState::Completed => Standing::Settled("completed", &e.at),
                EntryState::Cancelled => Standing::Settled("cancelled", &e.at),
            },
        }
    }

    #[must_use]
    pub(crate) fn source(self) -> String {
        match self {
            Waiting::Proposal(p, _) => asker_name(&p.by),
            Waiting::Decision(chain, _) => chain.to_owned(),
        }
    }

    #[must_use]
    pub(crate) fn asked(self) -> String {
        match self {
            Waiting::Proposal(p, _) => p.headline.clone(),
            Waiting::Decision(_, e) => {
                let t = title(e.authored.as_ref().map_or("", |a| a.body.as_str()));
                if t.is_empty() { e.todo.clone() } else { t }
            }
        }
    }

    #[must_use]
    pub(crate) fn at(self) -> Where {
        let named = |text: &str, key: &str| match text.parse::<Reference>() {
            Ok(Reference::Proposal(p)) => Where::Proposal(
                p.conversation.as_str().to_owned(),
                u32::try_from(p.index).unwrap_or(u32::MAX),
            ),
            Ok(Reference::Batch { chain, head }) => {
                Where::Batch(chain.to_string(), head.as_str().to_owned())
            }
            Ok(
                r @ (Reference::Todo(_)
                | Reference::Mail { .. }
                | Reference::Thread { .. }
                | Reference::Briefing { .. }
                | Reference::Target(_)),
            ) => Where::Unresolved(format!(
                "a decision about {}:{key} has no card here",
                r.scheme()
            )),
            Err(_) => {
                Where::Unresolved("this decision names nothing this app recognises".to_owned())
            }
        };
        match self {
            Waiting::Proposal(p, _) => named(&p.asked.to_string(), &p.id),
            Waiting::Decision(_, e) => named(
                e.authored
                    .as_ref()
                    .map_or("", |a| a.detail.lines().next().unwrap_or("")),
                &e.todo,
            ),
        }
    }

    #[must_use]
    pub(crate) const fn unpriced(self) -> Option<&'a str> {
        match self {
            Waiting::Proposal(_, unheld) => unheld,
            Waiting::Decision(..) => None,
        }
    }

    fn rank(self) -> (u8, f64) {
        let value = match self {
            Waiting::Proposal(p, _) => p.value,
            Waiting::Decision(_, e) => e.value,
        };
        match (
            self.unpriced(),
            value.and_then(suzatary_core::wire::Fulfillment::number),
        ) {
            (Some(_), _) => (2, 0.0),
            (None, None) => (1, 0.0),
            (None, Some(v)) => (0, v),
        }
    }
}

#[must_use]
pub(crate) fn all<'a>(
    read: Option<&'a InboxRead>,
    folds: &'a BTreeMap<String, Reading<FoldPayload>>,
) -> Vec<Waiting<'a>> {
    let unheld = |serves: Option<&'a suzatary_core::reference::TodoRef>| {
        let (read, serves) = (read?, serves?);
        let chain = serves.chain.as_str();
        (!read.held.iter().any(|h| h == chain)).then_some(chain)
    };
    let proposals = read
        .into_iter()
        .flat_map(|r| r.payload.proposals.iter())
        .map(|p| Waiting::Proposal(p, unheld(p.serves.as_ref())));
    let decisions = folds.iter().flat_map(|(chain, f)| match f {
        Reading::Val { v } => v
            .entries
            .iter()
            .filter(|e| {
                e.authored
                    .as_ref()
                    .is_some_and(|a| a.block == Block::Decision)
            })
            .map(|e| Waiting::Decision(chain, e))
            .collect::<Vec<_>>(),
        Reading::Loading | Reading::Error { .. } => Vec::new(),
    });
    proposals.chain(decisions).collect()
}

#[must_use]
pub(crate) fn open<'a>(ws: &[Waiting<'a>]) -> Vec<Waiting<'a>> {
    let mut open: Vec<Waiting<'a>> = ws
        .iter()
        .copied()
        .filter(|w| w.standing() == Standing::Open)
        .collect();
    open.sort_by(|a, b| {
        let ((ga, va), (gb, vb)) = (a.rank(), b.rank());
        ga.cmp(&gb)
            .then(va.total_cmp(&vb))
            .then_with(|| a.key().cmp(b.key()))
    });
    open
}

#[must_use]
pub(crate) fn settled_on<'a>(
    ws: &[Waiting<'a>],
    today: NaiveDate,
    offsets: &[Offset],
) -> Vec<(Waiting<'a>, &'static str, &'a str)> {
    let mut settled: Vec<(i64, (Waiting<'a>, &'static str, &'a str))> = ws
        .iter()
        .filter_map(|&w| match w.standing() {
            Standing::Settled(how, at) => {
                let ms = zone::ms(at)?;
                (zone::day_ms(offsets, ms) == today).then_some((ms, (w, how, at)))
            }
            Standing::Open | Standing::Unreadable(_) => None,
        })
        .collect();
    settled.sort_by_key(|s| std::cmp::Reverse(s.0));
    settled.into_iter().map(|(_, s)| s).collect()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Conversation<'a> {
    pub(crate) id: &'a str,
    pub(crate) title: &'a str,
}

/// An unresolved row must not offer to perform anything.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Row<'a> {
    Card(Waiting<'a>, Conversation<'a>, &'a ProposalRow),
    Unresolved(Waiting<'a>),
}

impl<'a> Row<'a> {
    #[must_use]
    pub(crate) const fn waiting(self) -> Waiting<'a> {
        match self {
            Row::Card(w, ..) | Row::Unresolved(w) => w,
        }
    }

    #[must_use]
    pub(crate) fn setup(self) -> bool {
        match self {
            Row::Card(_, _, row) => is_setup(&row.card.by),
            Row::Unresolved(_) => false,
        }
    }
}

#[must_use]
pub(crate) const fn unheld(why: Unheld) -> &'static str {
    match why {
        Unheld::NotHeld => "the inbox holds no record of it",
        Unheld::Unasked => "it is answered and was never asked",
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Readings {
    pub(crate) proposals: Reading<InboxRead>,
    pub(crate) decisions: BTreeMap<String, Reading<FoldPayload>>,
    pub(crate) waiting: Reading<Option<WaitingPayload>>,
    pub(crate) cards: BTreeMap<String, Reading<DrawnPayload>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
pub enum Watched {
    Proposals,
    Decisions(String),
    Waiting,
    Cards(String),
}

impl Watched {
    #[must_use]
    pub(crate) fn local(&self) -> Local {
        match self {
            Watched::Proposals => Local::Proposals,
            Watched::Decisions(chain) => Local::Decisions {
                chain: chain.clone(),
            },
            Watched::Waiting => Local::Waiting,
            Watched::Cards(conversation) => Local::Cards {
                conversation: conversation.clone(),
            },
        }
    }
}

impl Readings {
    #[must_use]
    pub(crate) fn of(chains: &[&str]) -> (Readings, Vec<Watched>) {
        let decisions: BTreeMap<String, Reading<FoldPayload>> = chains
            .iter()
            .map(|c| ((*c).to_owned(), Reading::Loading))
            .collect();
        let mut watched = vec![Watched::Proposals, Watched::Waiting];
        watched.extend(decisions.keys().cloned().map(Watched::Decisions));
        (
            Readings {
                proposals: Reading::Loading,
                decisions,
                waiting: Reading::Loading,
                cards: BTreeMap::new(),
            },
            watched,
        )
    }

    /// Returns the conversations' cards to watch that no row named before.
    pub(crate) fn held(&mut self, w: Watched, r: Reading<Answer>) -> Vec<Watched> {
        match w {
            Watched::Proposals => self.proposals = read(r),
            Watched::Decisions(chain) => {
                self.decisions.insert(chain, read(r));
            }
            Watched::Waiting => self.waiting = read(r),
            Watched::Cards(conversation) => {
                self.cards.insert(conversation, read(r));
            }
        }
        let named: BTreeSet<String> = open(&self.all())
            .into_iter()
            .filter_map(|w| match w.at() {
                Where::Proposal(c, _) => Some(c),
                Where::Batch(..) | Where::Unresolved(_) => None,
            })
            .collect();
        let new: Vec<String> = named
            .into_iter()
            .filter(|c| !self.cards.contains_key(c))
            .collect();
        for c in &new {
            self.cards.insert(c.clone(), Reading::Loading);
        }
        new.into_iter().map(Watched::Cards).collect()
    }

    #[must_use]
    pub(crate) fn all(&self) -> Vec<Waiting<'_>> {
        let read = match &self.proposals {
            Reading::Val { v } => Some(v),
            Reading::Loading | Reading::Error { .. } => None,
        };
        all(read, &self.decisions)
    }

    /// Waiting or not: what a tap's answer is read against once the box has settled it.
    #[must_use]
    pub(crate) fn card(&self, key: &crate::card::Key) -> Option<&ProposalRow> {
        match self.cards.get(&key.conversation)? {
            Reading::Val { v } => v.proposals.iter().find(|p| p.index == key.index as usize),
            Reading::Loading | Reading::Error { .. } => None,
        }
    }

    /// A row whose conversation's cards are on their way is not drawn until they are.
    #[must_use]
    pub(crate) fn rows(&self) -> Vec<Row<'_>> {
        open(&self.all())
            .into_iter()
            .filter_map(|w| match w.at() {
                Where::Unresolved(_) | Where::Batch(..) => Some(Row::Unresolved(w)),
                Where::Proposal(c, index) => match self.cards.get_key_value(&c) {
                    Some((id, Reading::Val { v })) => Some(
                        match v.proposals.iter().find(|p| p.index == index as usize) {
                            Some(row) => Row::Card(
                                w,
                                Conversation {
                                    id,
                                    title: &v.title,
                                },
                                row,
                            ),
                            None => Row::Unresolved(w),
                        },
                    ),
                    Some((_, Reading::Error { .. })) => Some(Row::Unresolved(w)),
                    Some((_, Reading::Loading)) | None => None,
                },
            })
            .collect()
    }

    #[must_use]
    pub(crate) fn problem(&self, w: Waiting<'_>) -> String {
        match w.at() {
            Where::Unresolved(problem) => problem,
            Where::Batch(chain, head) => format!("batch {head} on {chain} is not a card yet"),
            Where::Proposal(c, index) => match self.cards.get(&c) {
                Some(Reading::Error { e }) => e.clone(),
                Some(Reading::Val { .. } | Reading::Loading) | None => {
                    format!("proposal {index} of conversation {c} could not be found")
                }
            },
        }
    }

    /// A row whose conversation is on its way is not yet unresolved.
    #[must_use]
    pub(crate) fn resolved(&self) -> bool {
        self.cards.values().all(|r| !matches!(r, Reading::Loading))
    }

    #[must_use]
    pub(crate) fn unplaced(&self) -> Vec<(&str, &UnplacedProposal)> {
        match &self.waiting {
            Reading::Val { v: Some(v) } => v
                .unplaced
                .iter()
                .flat_map(|(conversation, lines)| lines.iter().map(|u| (conversation.as_str(), u)))
                .collect(),
            Reading::Val { v: None } | Reading::Loading | Reading::Error { .. } => Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn problems(&self) -> Vec<String> {
        let mut said = Vec::new();
        if let Reading::Error { e } = &self.proposals {
            said.push(format!("The Inbox did not read: {e}"));
        }
        if let Reading::Error { e } = &self.waiting {
            said.push(format!("What waits did not read: {e}"));
        }
        for (chain, f) in &self.decisions {
            if let Reading::Error { e } = f {
                said.push(format!("The {chain} chain did not answer: {e}"));
            }
        }
        said
    }

    /// What waits is known, whether or not each card is drawn yet.
    #[must_use]
    pub(crate) fn read(&self) -> bool {
        !matches!(self.proposals, Reading::Loading)
            && self
                .decisions
                .values()
                .all(|f| !matches!(f, Reading::Loading))
    }

    /// Until every reading the rows are drawn from has answered, an empty list, or one row fewer, is
    /// no answer.
    #[must_use]
    pub(crate) fn answered(&self) -> bool {
        self.read() && self.resolved()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    fn proposal(n: usize, value: Option<f64>, chain: &str) -> InboxProposal {
        serde_json::from_value(json!({"id": format!("p{n}"), "asked": format!("proposal:c{n}/0"),
            "serves": format!("todo:{chain}/t{n}"), "headline": "", "by": {"kind": "secretary"}, "why": "",
            "state": {"kind": "open"}, "value": value, "at": ""}))
        .expect("a proposal reads")
    }

    proptest! {
        /// Ascending in urgency, a total order, whatever order the reading lists them in.
        #[test]
        fn what_waits_is_ordered_by_urgency_then_key(rows in proptest::collection::vec((prop::option::of(0.0f64..1.0), any::<bool>()), 0..12)) {
            let ps: Vec<InboxProposal> = rows.iter().enumerate().map(|(n, (v, held))| proposal(n, *v, if *held { "life" } else { "elsewhere" })).collect();
            let read = InboxRead {
                payload: InboxProposalsPayload { at: String::new(), proposals: ps, failed: Vec::new() },
                held: vec!["life".to_owned()],
            };
            let folds = BTreeMap::new();
            let ranked = open(&all(Some(&read), &folds));
            prop_assert_eq!(ranked.len(), rows.len());
            for pair in ranked.windows(2) {
                let (a, b) = (pair[0].rank(), pair[1].rank());
                prop_assert!(a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(pair[0].key().cmp(pair[1].key())).is_lt());
            }
        }
    }
}
