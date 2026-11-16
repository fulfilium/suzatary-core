//! A tool turn's stored words, as the turn's split writes them: bounded at the cap.

use proptest::prelude::*;

use suzatary_memory::secretary::line_words::{LineWords, STORED_ANSWER_BYTES};
use suzatary_memory::secretary::salted_text::Words as _;

/// An answer of `len` bytes or a little more, of one-, two-, three- and four-byte characters,
/// so a cut can land inside a character.
fn answer_of(len: usize) -> String {
    "aé€😀"
        .chars()
        .cycle()
        .scan(0, |bytes, c| {
            (*bytes < len).then(|| {
                *bytes += c.len_utf8();
                c
            })
        })
        .collect()
}

proptest! {
    /// No stored tool turn exceeds the cap, whatever the verb answered; an answer within it
    /// is stored whole, and a longer one keeps its head and says how much it left out.
    #[test]
    fn a_stored_tool_answer_is_bounded(len in prop_oneof![
        0..STORED_ANSWER_BYTES + 1,
        STORED_ANSWER_BYTES - 8..STORED_ANSWER_BYTES + 8,
        STORED_ANSWER_BYTES..4 * STORED_ANSWER_BYTES,
    ]) {
        let answer = answer_of(len);
        let turn = suzatary_core::secretary::model::Turn::Tool {
            verb: suzatary_core::secretary::tool_name::ToolName::from("junk_digest"),
            answer: answer.clone(),
            outcome: suzatary_core::secretary::outcome::Outcome::Answered,
            files: Vec::new(),
        };
        let (_, words) = suzatary_core::secretary::turn_shape::split(&turn);
        let words = words.expect("a whole turn has words");
        let [LineWords::Tool { answer: stored, .. }] = words.as_slice() else {
            panic!("a tool turn is one tool slot: {words:?}");
        };
        prop_assert!(stored.as_str().len() <= STORED_ANSWER_BYTES);
        if answer.len() <= STORED_ANSWER_BYTES {
            prop_assert_eq!(stored.as_str(), answer.as_str());
        } else {
            let (head, note) = stored.as_str().rsplit_once('\n').expect("a note");
            prop_assert!(answer.starts_with(head));
            prop_assert_eq!(
                note,
                format!("[{} more bytes of this answer were not stored]", answer.len() - head.len())
            );
        }
        let opened = LineWords::read(&words[0].print()).expect("reads back");
        prop_assert_eq!(&opened, &words[0]);
    }
}
