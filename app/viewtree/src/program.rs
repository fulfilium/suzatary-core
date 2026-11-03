//! A program and the runtime that steps it: one model, one pure `update`, the view a function of
//! the model, and the effects `update` answers handed to whoever performs them.

use crate::node::{Node, On};
use crate::paint::Painter;
use crate::patch::{self, Patch};

/// A page: its model, its messages, the effects it asks for, and three pure functions.
pub trait Program {
    /// Everything the page knows.
    type Model;
    /// Everything that can happen to it: a person's act on the view, or an answer from outside.
    type Msg: Clone;
    /// What it asks the world to do; the runtime's handler performs it and answers a message.
    type Effect;
    /// What it starts from.
    type Flags;

    /// The model it starts with, and what it asks for first.
    fn init(flags: Self::Flags) -> (Self::Model, Vec<Self::Effect>);
    /// The next model, in place, and what it asks for. `&mut` is the same function as one that
    /// takes the model and answers the next: nothing else holds the model, so nobody can see
    /// the old one change, and no step copies it.
    fn update(model: &mut Self::Model, msg: Self::Msg) -> Vec<Self::Effect>;
    /// What it looks like.
    fn view(model: &Self::Model) -> Node<Self::Msg>;
}

/// What one step answers: the bytes of the patches that paint it, and the effects to perform.
#[derive(Debug)]
pub struct Frame<E> {
    /// The patches, encoded ([`patch::encode`]); empty when the view did not move.
    pub patches: Vec<u8>,
    /// The effects, in the order `update` asked.
    pub effects: Vec<E>,
}

/// A program running: its model and the tree last painted, and nothing else.
pub struct Runtime<P: Program> {
    model: P::Model,
    painter: Painter<P::Msg>,
}

impl<P: Program> Runtime<P> {
    /// The program started, and its first frame: the whole view, from nothing.
    pub fn start(flags: P::Flags) -> (Self, Frame<P::Effect>) {
        let (model, effects) = P::init(flags);
        let (painter, patches) = Painter::start(P::view(&model));
        (Self { model, painter }, frame(&patches, effects))
    }

    /// The model as it stands: what a law draws a fresh view of.
    pub fn model(&self) -> &P::Model {
        &self.model
    }

    /// One message stepped and painted.
    pub fn send(&mut self, msg: P::Msg) -> Frame<P::Effect> {
        let (patches, effects) = self.step(msg);
        frame(&patches, effects)
    }

    /// The same step, its patches as values: what a law compares.
    pub fn step(&mut self, msg: P::Msg) -> (Vec<Patch>, Vec<P::Effect>) {
        let effects = P::update(&mut self.model, msg);
        (self.painter.paint(P::view(&self.model)), effects)
    }

    /// An event the painter reported: the message its element carries, stepped; nothing when
    /// no handler answers it any more.
    pub fn event(&mut self, id: u32, on: On, words: String) -> Frame<P::Effect> {
        match self.painter.message(id, on, words) {
            Some(msg) => self.send(msg),
            None => frame(&[], Vec::new()),
        }
    }
}

fn frame<E>(patches: &[Patch], effects: Vec<E>) -> Frame<E> {
    Frame {
        patches: patch::encode(patches),
        effects,
    }
}
