//! A proposal's registers: what it asks (discrete) and where its machine stands (a [`Phase`],
//! inflationary), so folding a proposal's events in any order is one value.

use prodrome_data::fold::{Climb, Discrete, Frontier, Product, Register, RegisterType};
use prodrome_data::literal::ProdromeError;
use prodrome_data::stamp::Stamp;

use crate::secretary::inbox_event::{InboxEvent, Said};
use crate::secretary::phase::Phase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InboxRegister {
    Content,
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InboxRegisters<'a> {
    pub content: Frontier<'a, InboxEvent>,
    pub state: Climb<'a, InboxEvent>,
}

impl<'a> Product<'a> for InboxRegisters<'a> {
    type Schema = InboxEvent;

    fn registers(&self) -> Vec<InboxRegister> {
        vec![InboxRegister::Content, InboxRegister::State]
    }

    fn frontier(&self, register: InboxRegister) -> &Frontier<'a, InboxEvent> {
        match register {
            InboxRegister::Content => &self.content,
            InboxRegister::State => self.state.frontier(),
        }
    }

    fn join_to(&mut self, register: InboxRegister, stamp: &'a Stamp<InboxEvent>) {
        match register {
            InboxRegister::Content => self.content.join(stamp),
            InboxRegister::State => self.state.join::<StateRegister>(stamp),
        }
    }

    fn reading(&self, register: InboxRegister) -> Vec<&'a Stamp<InboxEvent>> {
        match register {
            InboxRegister::Content => self.content.reading::<ContentRegister>(),
            InboxRegister::State => self.state.reading::<StateRegister>(),
        }
    }

    /// A response with a floor ([`Phase::floor`]) also stands on it.
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

    fn climbed(&self, earlier: &Self) -> Result<(), ProdromeError> {
        self.state.climbed::<StateRegister>(&earlier.state)
    }

    fn climbing(&self) -> Vec<InboxRegister> {
        vec![InboxRegister::State]
    }
}
