//! The inert answers: test support, never production.

use suzatary_base::capability::Nothing;
use suzatary_todo::render::Html;

use crate::secretary::answers::{AnswerError, Answers};

impl Answers for Nothing {
    fn block(&self, block: &suzatary_base::answer::Renderable) -> Result<Html, AnswerError> {
        Err(AnswerError(format!("nothing renders {block:?}")))
    }
}
