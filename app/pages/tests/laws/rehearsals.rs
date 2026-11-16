use crate::common::*;
use suzatary_pages::runtime::UNREAD;
use viewtree::host::{Host, Hosted};

/// `input` as the host hands it over.
fn deliver<P: Hosted>(host: &mut Host<P>, input: &Value) -> Vec<u8> {
    match delivered(input) {
        Delivered::Whole(v) => host.input(&v.to_string()).expect("an input it takes"),
        Delivered::Reply(r, text) => host.reply(&r.to_string(), text).expect("a reply it takes"),
    }
}

/// Drives `P` through its rehearsal as the rehearsal worker does, and answers each step whose
/// patches say a reading did not read.
fn unread<P: Hosted>() -> Vec<String> {
    let Some(text) = P::rehearsal() else {
        return vec!["no rehearsal".to_owned()];
    };
    let rehearsal: Value = serde_json::from_str(text).expect("a rehearsal is JSON");
    let (mut host, first) = Host::<P>::start(&rehearsal["flags"].to_string()).expect("its flags");
    let mut steps = vec![first];
    for input in rehearsal["inputs"].as_array().expect("its inputs") {
        steps.push(deliver(&mut host, input));
    }
    steps
        .iter()
        .enumerate()
        .filter(|(_, patches)| String::from_utf8_lossy(patches).contains(UNREAD))
        .map(|(n, _)| format!("step {n}"))
        .collect()
}

/// Each page's rehearsal (`app/pages/tests/rehearsal/`) is flags and inputs its page takes and
/// readings it reads, so the worker that runs it reaches the code a real reading runs, and a wire
/// change that would refuse one fails here.
#[test]
fn every_rehearsal_is_read_as_its_page_would() {
    assert_eq!(
        unread::<secretary::Secretary>(),
        Vec::<String>::new(),
        "the stream's"
    );
    assert_eq!(
        unread::<todos::Todos>(),
        Vec::<String>::new(),
        "the todo list's"
    );
    assert_eq!(
        unread::<conversation::Conversation>(),
        Vec::<String>::new(),
        "a conversation's"
    );
    assert_eq!(
        unread::<inbox::Inbox>(),
        Vec::<String>::new(),
        "the Inbox's"
    );
    assert_eq!(
        unread::<needsyou::NeedsYou>(),
        Vec::<String>::new(),
        "the popup's"
    );
    assert_eq!(
        unread::<settings::Settings>(),
        Vec::<String>::new(),
        "Settings'"
    );
}
