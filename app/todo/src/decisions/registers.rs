use prodrome_core::fold::{Discrete, Frontier, GrowSet, Product, Register, RegisterType};
use prodrome_core::literal::ProdromeError;
use prodrome_core::registers::Stamp;

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

/// One decision's registers: a product of two frontiers and a grow-only set, so folding its
/// events in any order is one value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecisionRegisters<'a> {
    /// What it says.
    pub content: Frontier<'a, DecisionEvent>,
    /// Whether it still stands.
    pub life: Frontier<'a, DecisionEvent>,
    /// Every decision any of its statements supersedes: the union, so a link once made stays.
    pub supersedes: GrowSet<&'a DecisionId>,
}

impl<'a> Product<'a> for DecisionRegisters<'a> {
    type Schema = DecisionEvent;

    fn registers(&self) -> Vec<DecisionRegister> {
        vec![DecisionRegister::Content, DecisionRegister::Life]
    }

    fn join(&mut self, stamp: &'a Stamp<DecisionEvent>) {
        if let Some(links) = stamp.event.body().supersedes() {
            self.content.join(stamp);
            for link in links {
                self.supersedes.join(link);
            }
        }
        self.life.join(stamp);
    }

    fn frontier(&self, register: DecisionRegister) -> &Frontier<'a, DecisionEvent> {
        match register {
            DecisionRegister::Content => &self.content,
            DecisionRegister::Life => &self.life,
        }
    }

    fn frontier_mut(&mut self, register: DecisionRegister) -> &mut Frontier<'a, DecisionEvent> {
        match register {
            DecisionRegister::Content => &mut self.content,
            DecisionRegister::Life => &mut self.life,
        }
    }

    fn reading(&self, register: DecisionRegister) -> Vec<&'a Stamp<DecisionEvent>> {
        match register {
            DecisionRegister::Content => self.content.reading::<ContentRegister>(),
            DecisionRegister::Life => self.life.reading::<LifeRegister>(),
        }
    }

    /// The life is inflationary; the content is discrete and grows nothing.
    fn grows(&self, event: &DecisionEvent) -> Result<(), ProdromeError> {
        self.life.grows::<LifeRegister>(event)
    }
}
