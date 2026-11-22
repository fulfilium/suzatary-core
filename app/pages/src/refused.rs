//! A line of the person's the box refused for good, kept on the device until they act on it: a
//! question placed into an empty composer, whose draft every tab of the device reads, or else
//! shown with Copy and Dismiss. Placing the same words twice is one write, and acting is a join of
//! names, so any number of tabs may do either. Acting lets the device's record go.

use suzatary_topics::wire::topics::{Given, Ruling, Waiting};
use viewtree::Attr::Type;
use viewtree::kit::para;
use viewtree::{Node, view};

use crate::composer;

/// Each refusal the person has not acted on: its line, the box's sentence, and its words.
pub(crate) fn rows(waiting: &[Waiting]) -> Vec<(&str, &str, &Given)> {
    waiting
        .iter()
        .filter_map(|w| match &w.ruling {
            Ruling::Refused { why, words } => Some((w.line.as_str(), why.as_str(), words)),
            Ruling::Unsent | Ruling::Trust | Ruling::Later { .. } => None,
        })
        .collect()
}

/// The first refusal that is a question, its line and its words as the composer holds what was
/// sent: what a composer that takes words back places. A title is never placed: no place the
/// device keeps edits one, so it stays here until the person acts.
pub(crate) fn question(waiting: &[Waiting]) -> Option<(String, composer::Sent)> {
    rows(waiting)
        .into_iter()
        .find_map(|(line, _, words)| Some((line.to_owned(), sent(words)?)))
}

/// A question taken back, as the composer holds what was sent.
fn sent(words: &Given) -> Option<composer::Sent> {
    match words {
        Given::Question {
            text,
            references,
            files,
            attachments,
        } => Some(composer::Sent {
            text: text.clone(),
            references: references.clone(),
            files: files.clone(),
            attachments: attachments.clone(),
            mode: composer::Mode::Asking,
        }),
        Given::Edit {
            question,
            text,
            references,
            files,
            attachments,
        } => Some(composer::Sent {
            text: text.clone(),
            references: references.clone(),
            files: files.clone(),
            attachments: attachments.clone(),
            mode: composer::Mode::Editing {
                question: question.clone(),
            },
        }),
        Given::Title { .. } => None,
    }
}

/// The words as they are copied.
fn quoted(words: &Given) -> &str {
    match words {
        Given::Question { text, .. } | Given::Edit { text, .. } => text,
        Given::Title { title } => title,
    }
}

/// Each refusal the person has not acted on, with its words to copy or let go.
pub(crate) fn view<M: Clone + 'static>(
    waiting: &[Waiting],
    copy: impl Fn(String) -> M,
    dismiss: impl Fn(String) -> M,
) -> Node<M> {
    view! { Div "ownrefused" [rows(waiting).into_iter().map(|(line, why, words)| view! { Div "ownrefusal" [
        para("problem", format!("The box refused what you wrote: {why}")),
        view! { P "ownwords" [quoted(words)] },
        view! { Button "act" {Type => "button"} @click(copy(quoted(words).to_owned())) ["Copy"] },
        view! { Button "act" {Type => "button"} @click(dismiss(line.to_owned())) ["Dismiss"] },
    ] }).collect::<Vec<_>>()] }
}
