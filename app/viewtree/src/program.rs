//! A program (one model, one pure `update`, a view of the model, the sources it listens to) and the
//! runtime that steps it.

use crate::node::{Node, On};
use crate::paint::Painter;
use crate::patch::{self, Patch};
use crate::subpatch::SubPatch;
use crate::subs::Subs;

pub trait Program {
    type Model;
    type Msg: Clone;
    type Effect;
    type Flags;
    /// The name a program gives a source it listens to.
    type Slot: Ord + Clone;
    /// A listener, as data: equal sources are one listener. It holds no counter or clock, which
    /// would make every step a new source.
    type Sub: Clone + PartialEq;

    fn init(flags: Self::Flags) -> (Self::Model, Vec<Self::Effect>);
    /// `&mut` is a pure step here: nothing else holds the model.
    fn update(model: &mut Self::Model, msg: Self::Msg) -> Vec<Self::Effect>;
    fn view(model: &Self::Model) -> Node<Self::Msg>;
    /// What the program listens to when the model is `model`; a program that listens to nothing
    /// leaves this as it is.
    fn subscriptions(_model: &Self::Model) -> Subs<Self::Slot, Self::Sub> {
        Subs::new()
    }
}

#[derive(Debug)]
pub struct Frame<E, K, V> {
    /// Encoded; empty when the view did not move.
    pub patches: Vec<u8>,
    /// What takes the host's listeners to the model's subscriptions, which it applies before it
    /// performs an effect, so an effect may name a listener that starts in the same frame.
    pub subs: Vec<SubPatch<K, V>>,
    /// In the order `update` asked.
    pub effects: Vec<E>,
}

/// A step before its patches are printed.
pub struct Stepped<P: Program> {
    pub patches: Vec<Patch>,
    pub subs: Vec<SubPatch<P::Slot, P::Sub>>,
    pub effects: Vec<P::Effect>,
}

pub struct Runtime<P: Program> {
    model: P::Model,
    painter: Painter<P::Msg>,
    /// What the host listens to: the subscriptions of the model as of the last frame.
    active: Subs<P::Slot, P::Sub>,
}

type Sent<P> = Frame<<P as Program>::Effect, <P as Program>::Slot, <P as Program>::Sub>;

impl<P: Program> Runtime<P> {
    /// The first frame starts every source of the first model.
    pub fn start(flags: P::Flags) -> (Self, Sent<P>) {
        let (model, effects) = P::init(flags);
        let (painter, patches) = Painter::start(P::view(&model));
        let next = P::subscriptions(&model);
        let subs = Subs::new().diff(&next);
        let frame = Frame {
            patches: patch::encode(&patches),
            subs,
            effects,
        };
        (
            Self {
                model,
                painter,
                active: next,
            },
            frame,
        )
    }

    pub fn model(&self) -> &P::Model {
        &self.model
    }

    pub fn active(&self) -> &Subs<P::Slot, P::Sub> {
        &self.active
    }

    pub fn send(&mut self, msg: P::Msg) -> Sent<P> {
        let Stepped {
            patches,
            subs,
            effects,
        } = self.advance(msg);
        Frame {
            patches: patch::encode(&patches),
            subs,
            effects,
        }
    }

    /// The patches and effects of a step, for a reader that does not look at the subscriptions.
    pub fn step(&mut self, msg: P::Msg) -> (Vec<Patch>, Vec<P::Effect>) {
        let Stepped {
            patches, effects, ..
        } = self.advance(msg);
        (patches, effects)
    }

    pub fn advance(&mut self, msg: P::Msg) -> Stepped<P> {
        let effects = P::update(&mut self.model, msg);
        let patches = self.painter.paint(P::view(&self.model));
        let next = P::subscriptions(&self.model);
        let subs = self.active.diff(&next);
        self.active = next;
        Stepped {
            patches,
            subs,
            effects,
        }
    }

    /// The view drawn from nothing, which a painted DOM must equal.
    #[must_use]
    pub fn fresh(&self) -> Vec<u8> {
        patch::encode(&Painter::start(P::view(&self.model)).1)
    }

    /// `None` when no handler answers, so the painter leaves the event to the browser.
    pub fn event(&mut self, id: u32, on: On, words: String) -> Option<Sent<P>> {
        self.painter
            .message(id, on, words)
            .map(|msg| self.send(msg))
    }
}
