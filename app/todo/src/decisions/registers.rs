use prodrome_data::fold::{Climb, Discrete, Frontier, GrowSet, Product, Register, RegisterType};
use prodrome_data::literal::ProdromeError;
use prodrome_data::schema::Schema as _;
use prodrome_data::stamp::Stamp;

use super::{Area, DecisionEvent, DecisionId, Life, Said, Statement};

/// A register a write supersedes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DecisionRegister {
    Content,
    /// Whether it still stands.
    Life,
    /// The words of a digest node, for a key that names one.
    Summary,
}

/// The content register's type: the statement, discrete, so two revisions that say the same
/// thing are one candidate and two that differ are a conflict.
#[derive(Debug)]
pub struct ContentRegister;

/// The life register's type: a [`Life`], inflationary.
#[derive(Debug)]
pub struct LifeRegister;

/// The summary register's type: the words, discrete, so two writers that say the same are one
/// candidate and two that differ are a conflict.
#[derive(Debug)]
pub struct SummaryRegister;

impl RegisterType<DecisionEvent> for ContentRegister {
    type Value<'e> = Discrete<&'e Statement>;

    fn value(event: &DecisionEvent) -> Option<Discrete<&Statement>> {
        event.body().statement().map(Discrete)
    }
}

impl RegisterType<DecisionEvent> for LifeRegister {
    type Value<'e> = Life;

    fn value(event: &DecisionEvent) -> Option<Life> {
        Some(match event.body() {
            Said::Decided { .. } | Said::Revised { .. } => Life::Stated,
            Said::Withdrawn { .. } => Life::Withdrawn,
            Said::Governs { .. } | Said::Summarized { .. } => return None,
        })
    }
}

impl RegisterType<DecisionEvent> for SummaryRegister {
    type Value<'e> = Discrete<&'e str>;

    fn value(event: &DecisionEvent) -> Option<Discrete<&str>> {
        match event.body() {
            Said::Summarized { words } => Some(Discrete(words.as_str())),
            Said::Decided { .. }
            | Said::Revised { .. }
            | Said::Withdrawn { .. }
            | Said::Governs { .. } => None,
        }
    }
}

/// One decision's registers, so folding its events in any order is one value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecisionRegisters<'a> {
    pub content: Frontier<'a, DecisionEvent>,
    /// Whether it still stands.
    pub life: Climb<'a, DecisionEvent>,
    /// The words of a node, for a key that names one.
    pub summary: Frontier<'a, DecisionEvent>,
    /// Every area any event of it names: the union, so an area once named stays.
    pub areas: GrowSet<&'a Area>,
    /// Every decision any of its statements supersedes: the union, so a link once made stays.
    pub supersedes: GrowSet<&'a DecisionId>,
}

impl<'a> Product<'a> for DecisionRegisters<'a> {
    type Schema = DecisionEvent;

    fn registers(&self) -> Vec<DecisionRegister> {
        vec![
            DecisionRegister::Content,
            DecisionRegister::Life,
            DecisionRegister::Summary,
        ]
    }

    fn join(&mut self, stamp: &'a Stamp<DecisionEvent>) {
        for link in stamp.event.body().supersedes().into_iter().flatten() {
            self.supersedes.join(link);
        }
        if let Said::Governs { areas } = stamp.event.body() {
            for area in areas {
                self.areas.join(area);
            }
        }
        for register in stamp.event.writes() {
            self.join_to(register, stamp);
        }
    }

    fn frontier(&self, register: DecisionRegister) -> &Frontier<'a, DecisionEvent> {
        match register {
            DecisionRegister::Content => &self.content,
            DecisionRegister::Life => self.life.frontier(),
            DecisionRegister::Summary => &self.summary,
        }
    }

    fn join_to(&mut self, register: DecisionRegister, stamp: &'a Stamp<DecisionEvent>) {
        match register {
            DecisionRegister::Content => self.content.join(stamp),
            DecisionRegister::Life => self.life.join::<LifeRegister>(stamp),
            DecisionRegister::Summary => self.summary.join(stamp),
        }
    }

    fn reading(&self, register: DecisionRegister) -> Vec<&'a Stamp<DecisionEvent>> {
        match register {
            DecisionRegister::Content => self.content.reading::<ContentRegister>(),
            DecisionRegister::Life => self.life.reading::<LifeRegister>(),
            DecisionRegister::Summary => self.summary.reading::<SummaryRegister>(),
        }
    }

    fn grows(&self, event: &DecisionEvent) -> Result<(), ProdromeError> {
        self.life.grows::<LifeRegister>(event)
    }

    fn climbed(&self, earlier: &Self) -> Result<(), ProdromeError> {
        self.life.climbed::<LifeRegister>(&earlier.life)
    }

    fn climbing(&self) -> Vec<DecisionRegister> {
        vec![DecisionRegister::Life]
    }
}
