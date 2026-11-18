//! A proposal's registers: what it asks (discrete) and where its machine stands (a [`Phase`],
//! inflationary). A product of two frontiers, so folding a proposal's events in any order is one
//! value.

use prodrome_core::fold::{Discrete, Frontier, Product, Register, RegisterType};
use prodrome_core::literal::ProdromeError;
use prodrome_core::registers::Stamp;

use crate::secretary::inbox_event::{InboxEvent, Said};
use crate::secretary::phase::Phase;

/// A register a write supersedes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InboxRegister {
    /// The proposal as asked.
    Content,
    /// Where its machine stands.
    State,
}

/// The content register's type: the whole `Asked` event, discrete.
#[derive(Debug)]
pub struct ContentRegister;

/// The state register's type: a response's [`Phase`].
#[derive(Debug)]
pub struct StateRegister;

impl RegisterType<InboxEvent> for ContentRegister {
    type Value<'e> = Discrete<&'e InboxEvent>;

    fn value(event: &InboxEvent) -> Option<Discrete<&InboxEvent>> {
        matches!(event.said, Said::Asked(_)).then_some(Discrete(event))
    }
}

impl RegisterType<InboxEvent> for StateRegister {
    type Value<'e> = Phase;

    fn value(event: &InboxEvent) -> Option<Phase> {
        match &event.said {
            Said::Responded(responded) => Some(responded.phase()),
            Said::Asked(_) => None,
        }
    }
}

/// One proposal's registers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InboxRegisters<'a> {
    /// What was asked.
    pub content: Frontier<'a, InboxEvent>,
    /// What became of it.
    pub state: Frontier<'a, InboxEvent>,
}

impl<'a> Product<'a> for InboxRegisters<'a> {
    type Schema = InboxEvent;

    fn registers(&self) -> Vec<InboxRegister> {
        vec![InboxRegister::Content, InboxRegister::State]
    }

    fn join(&mut self, stamp: &'a Stamp<InboxEvent>) {
        match stamp.event.said {
            Said::Asked(_) => self.content.join(stamp),
            Said::Responded(_) => self.state.join(stamp),
        }
    }

    fn frontier(&self, register: InboxRegister) -> &Frontier<'a, InboxEvent> {
        match register {
            InboxRegister::Content => &self.content,
            InboxRegister::State => &self.state,
        }
    }

    fn frontier_mut(&mut self, register: InboxRegister) -> &mut Frontier<'a, InboxEvent> {
        match register {
            InboxRegister::Content => &mut self.content,
            InboxRegister::State => &mut self.state,
        }
    }

    fn reading(&self, register: InboxRegister) -> Vec<&'a Stamp<InboxEvent>> {
        match register {
            InboxRegister::Content => self.content.reading::<ContentRegister>(),
            InboxRegister::State => self.state.reading::<StateRegister>(),
        }
    }

    /// The state is inflationary, and a response with a floor ([`Phase::floor`]) stands on it;
    /// the content is discrete and grows nothing.
    fn grows(&self, event: &InboxEvent) -> Result<(), ProdromeError> {
        self.state.grows::<StateRegister>(event)?;
        let reading: Vec<Phase> = self
            .state
            .reading::<StateRegister>()
            .into_iter()
            .filter_map(|stamp| StateRegister::value(&stamp.event))
            .collect();
        match StateRegister::value(event) {
            Some(written) if !written.stands_on(&reading) => Err(ProdromeError::invalid(format!(
                "a response at {written:?} stands only on its floor, and the reading is {reading:?}"
            ))),
            _ => Ok(()),
        }
    }
}
