//! A program and the runtime that steps it: one model, one pure `update`, the view a function of
//! the model, and the effects `update` answers handed to whoever performs them.

use crate::node::{Node, On};
use crate::paint::Painter;
use crate::patch::{self, Patch};

pub trait Program {
    type Model;
    type Msg: Clone;
    type Effect;
    type Flags;

    fn init(flags: Self::Flags) -> (Self::Model, Vec<Self::Effect>);
    /// `&mut` is the same function as one answering the next model: nothing else holds the model,
    /// so nobody sees the old one change, and no step copies it.
    fn update(model: &mut Self::Model, msg: Self::Msg) -> Vec<Self::Effect>;
    fn view(model: &Self::Model) -> Node<Self::Msg>;
}

#[derive(Debug)]
pub struct Frame<E> {
    /// Encoded; empty when the view did not move.
    pub patches: Vec<u8>,
    /// In the order `update` asked.
    pub effects: Vec<E>,
}

pub struct Runtime<P: Program> {
    model: P::Model,
    painter: Painter<P::Msg>,
}

impl<P: Program> Runtime<P> {
    pub fn start(flags: P::Flags) -> (Self, Frame<P::Effect>) {
        let (model, effects) = P::init(flags);
        let (painter, patches) = Painter::start(P::view(&model));
        (Self { model, painter }, frame(&patches, effects))
    }

    pub fn model(&self) -> &P::Model {
        &self.model
    }

    pub fn send(&mut self, msg: P::Msg) -> Frame<P::Effect> {
        let (patches, effects) = self.step(msg);
        frame(&patches, effects)
    }

    pub fn step(&mut self, msg: P::Msg) -> (Vec<Patch>, Vec<P::Effect>) {
        let effects = P::update(&mut self.model, msg);
        (self.painter.paint(P::view(&self.model)), effects)
    }

    /// The view drawn from nothing: what a painted DOM must equal, whatever patches brought it
    /// there.
    #[must_use]
    pub fn fresh(&self) -> Vec<u8> {
        patch::encode(&Painter::start(P::view(&self.model)).1)
    }

    /// `None` when no handler answers, so the painter leaves the event to the browser.
    pub fn event(&mut self, id: u32, on: On, words: String) -> Option<Frame<P::Effect>> {
        self.painter
            .message(id, on, words)
            .map(|msg| self.send(msg))
    }
}

fn frame<E>(patches: &[Patch], effects: Vec<E>) -> Frame<E> {
    Frame {
        patches: patch::encode(patches),
        effects,
    }
}
