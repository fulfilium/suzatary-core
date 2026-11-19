//! The box's opened lines' law, over the conversations held in core's memory.

use prodrome_core::literal::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Kind};
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::held_lines::HeldLines;
use suzatary_lines::secretary::opened::Opened;
use suzatary_lines::secretary::transcript;
use suzatary_memory::secretary::erasure::Erasure;
use suzatary_memory::secretary::words::Words;
use suzatary_stores::stores::conversations::InMemory;

const SAID: &str = "a word only the owner says";

fn at(second: u32) -> Datetime {
    Datetime::new(2026, 11, 9, 9, 0, second, 0).expect("an instant")
}

/// What every reader of the fold answers, printed.
fn read_by_all(conversations: Conversations<'_>, id: &ConversationId) -> String {
    let reads = conversations.reads();
    format!(
        "{:?} {:?} {:?}",
        reads.fold(HeldLines::gathered).expect("reads"),
        reads.fold(HeldLines::transcripts).expect("reads"),
        reads.fold(|lines| lines.transcript(id)).expect("reads"),
    )
}

/// Law (a privacy boundary): an erasure's sweep leaves its fold holding neither the key nor
/// the words, before any read, and no reader of the fold answers them after it. A fold the
/// sweep never told, as in another process, drops both at its next read.
#[test]
fn an_erasure_leaves_no_words_in_the_fold_or_its_readers() {
    let id = ConversationId::new("2026-11-09-1").expect("a slug");
    for told in [true, false] {
        let held = InMemory::default();
        let elsewhere = Opened::default();
        let fold = if told { &held.opened } else { &elsewhere };
        let reading = Conversations {
            opened: fold,
            ..held.conversations()
        };
        let opening = Entry::Opened {
            at: at(0),
            title: Words::kept("Notes".to_owned()),
            model: "a-cli/small".to_owned(),
            kind: Kind::Topic,
        };
        transcript::start(held.conversations(), &id, &opening).expect("started");
        let said = Entry::Said {
            at: at(1),
            turn: Turn::Person {
                text: SAID.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        };
        transcript::push(held.conversations(), &id, &said).expect("said");
        assert!(read_by_all(reading, &id).contains(SAID), "told: {told}");
        assert!(fold.says(SAID));
        let keys: std::collections::BTreeSet<_> =
            transcript::read(reading, &id).expect("reads").lines()[1]
                .line
                .keys()
                .cloned()
                .collect();
        let erasure = Erasure::Asked {
            by: transcript::writer(),
        };
        transcript::erase(held.conversations(), &id, keys.clone(), erasure, at(2)).expect("erased");
        let holds = || keys.iter().any(|key| fold.holds(key)) || fold.says(SAID);
        assert_eq!(holds(), !told, "before a read, told: {told}");
        assert!(!read_by_all(reading, &id).contains(SAID), "told: {told}");
        assert!(!holds(), "after a read, told: {told}");
    }
}
