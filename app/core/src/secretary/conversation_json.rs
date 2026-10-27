//! How the conversations' schema crosses into the browser's module: each line under
//! `conversation`, its one register by name, and the register's value as the line's literal.

use prodrome_core::schema::Schema;
use prodrome_wasm_exports::Json;
use prodrome_wasm_exports::wire::json_literal;
use serde_json::Value;

use crate::secretary::line_event::{LineEvent, LineVocabulary};
use prodrome_core::fold::sole::Only;

impl Json for LineEvent {
    fn key_field(_: &LineVocabulary) -> &'static str {
        "conversation"
    }

    fn register(_: &LineVocabulary, _: Only) -> &'static str {
        "transcript"
    }

    /// A transcript's value is the line itself, its words locked (its down-set is read by ancestry, never from the
    /// value), so its JSON is the line's literal.
    fn value(&self, _: Only) -> Value {
        json_literal(&Schema::to_value(self))
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::Actor;
    use proptest::prelude::*;

    use super::*;
    use crate::capability::Turn;
    use crate::secretary::conversations::Counting;
    use crate::secretary::store::{ConversationId, Entry};

    fn said(text: &str, minute: u32) -> LineEvent {
        let at = prodrome_core::literal::Datetime::new(2026, 10, 1, 12, minute, 0, 0)
            .expect("an instant");
        let entry = Entry::Said {
            at,
            turn: Turn::Person {
                text: text.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        };
        LineEvent::write(
            ConversationId::new("about-the-deed").expect("an id"),
            Actor::new("ada").expect("an actor"),
            0,
            &entry,
            &Counting::default(),
        )
        .expect("a line")
        .0
    }

    proptest! {
        /// A line's JSON is a function of the line: equal lines answer equal JSON, and the
        /// literal reads back as the same line.
        #[test]
        fn a_value_is_its_line(text in "[a-z ]{0,24}", minute in 0u32..59) {
            let one = said(&text, minute);
            let two = said(&text, minute);
            prop_assert_eq!(one.value(Only), two.value(Only));
            let back = LineEvent::from_value(&LineVocabulary, &one.to_value()).expect("it reads back");
            prop_assert_eq!(back, one);
        }
    }
}
