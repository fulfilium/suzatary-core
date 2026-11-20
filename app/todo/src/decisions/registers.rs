use prodrome_core::fold::{Climb, Discrete, Frontier, GrowSet, Product, Register, RegisterType};
use prodrome_core::literal::ProdromeError;
use prodrome_core::registers::Stamp;
use prodrome_core::schema::Schema as _;

use super::{DecisionEvent, DecisionId, Life, Said, Statement};

/// A register a write supersedes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DecisionRegister {
    /// What it says.
    Content,
    /// Whether it still stands.
    Life,
}

/// The content register's type: the statement, discrete, so two revisions that say the same
/// thing are one candidate and two that differ are a conflict.
#[derive(Debug)]
pub struct ContentRegister;

/// The life register's type: a [`Life`], inflationary.
#[derive(Debug)]
pub struct LifeRegister;

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
        })
    }
}

/// One decision's registers, so folding its events in any order is one value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecisionRegisters<'a> {
    /// What it says.
    pub content: Frontier<'a, DecisionEvent>,
    /// Whether it still stands.
    pub life: Climb<'a, DecisionEvent>,
    /// Every decision any of its statements supersedes: the union, so a link once made stays.
    pub supersedes: GrowSet<&'a DecisionId>,
}

impl<'a> Product<'a> for DecisionRegisters<'a> {
    type Schema = DecisionEvent;

    fn registers(&self) -> Vec<DecisionRegister> {
        vec![DecisionRegister::Content, DecisionRegister::Life]
    }

    fn join(&mut self, stamp: &'a Stamp<DecisionEvent>) {
        for link in stamp.event.body().supersedes().into_iter().flatten() {
            self.supersedes.join(link);
        }
        for register in stamp.event.writes() {
            self.join_to(register, stamp);
        }
    }

    fn frontier(&self, register: DecisionRegister) -> &Frontier<'a, DecisionEvent> {
        match register {
            DecisionRegister::Content => &self.content,
            DecisionRegister::Life => self.life.frontier(),
        }
    }

    fn join_to(&mut self, register: DecisionRegister, stamp: &'a Stamp<DecisionEvent>) {
        match register {
            DecisionRegister::Content => self.content.join(stamp),
            DecisionRegister::Life => self.life.join::<LifeRegister>(stamp),
        }
    }

    fn reading(&self, register: DecisionRegister) -> Vec<&'a Stamp<DecisionEvent>> {
        match register {
            DecisionRegister::Content => self.content.reading::<ContentRegister>(),
            DecisionRegister::Life => self.life.reading::<LifeRegister>(),
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
