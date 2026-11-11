//! A model given as a function: the free model, and the reference the model's suite is read
//! against.

use crate::secretary::model::{Conversation, Model, ModelError, Reply};

/// A model whose far end is `answer`: each turn is the text `answer` gives for the conversation,
/// read by [`Reply::parse`] exactly as a real model's text is, or the failure it gives. A
/// [`Model`] means no more than this, so every other interpreter is this with a wire between.
pub struct Answering<F> {
    name: String,
    answer: F,
}

impl<F> Answering<F>
where
    F: Fn(&Conversation) -> Result<String, ModelError> + Sync,
{
    /// The model `name`, answering by `answer`.
    pub fn new(name: impl Into<String>, answer: F) -> Self {
        Self {
            name: name.into(),
            answer,
        }
    }
}

impl<F> Model for Answering<F>
where
    F: Fn(&Conversation) -> Result<String, ModelError> + Sync,
{
    fn name(&self) -> String {
        self.name.clone()
    }

    fn turn(
        &self,
        conversation: &Conversation,
        _tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        (self.answer)(conversation).map(|text| Reply::parse(&text))
    }
}
