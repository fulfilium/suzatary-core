//! What the lines' husked store costs against the Prodrome's (`suzatary_io::husked`). The prune's
//! laws are the CLI's (`app/cli/tests/husked.rs`), which reads the routes.

use suzatary_core::secretary::conversations::Conversations;
use suzatary_core::secretary::transcript;

use crate::stores::{conversations_at_scale, lines_at, said};

/// What a look and one line's append cost over a store of 100 conversations of 300 lines, kept
/// by the Prodrome's store and by the husked one, each twice in turn. Run by name, in release:
/// `cargo test --release -p suzatary-io --test all -- --ignored an_append_at_scale --nocapture`.
#[test]
#[ignore = "a measurement, run by name"]
fn an_append_at_scale() {
    static OPENED: std::sync::LazyLock<suzatary_core::secretary::opened::Opened> =
        std::sync::LazyLock::new(Default::default);
    let dir = tempfile::tempdir().expect("a scratch dir");
    let (lines, keys) = lines_at(dir.path());
    let (ids, _) = conversations_at_scale(&lines, &keys, 100, 300);
    let conversations = Conversations {
        lines: &lines,
        keys: &keys,
        entropy: &suzatary_io::texts::SystemEntropy,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened: &OPENED,
    };
    let prodrome: suzatary_io::objects::Objects<suzatary_core::secretary::line_event::LineEvent> =
        suzatary_io::objects::Objects::open(dir.path().join(crate::stores::LINES));
    for (what, store) in [
        (
            "prodrome",
            &prodrome as &dyn suzatary_core::store::Prodrome<_>,
        ),
        ("husked", &lines),
        ("prodrome", &prodrome),
        ("husked", &lines),
    ] {
        let conversations = Conversations {
            lines: store,
            ..conversations
        };
        store.tips().expect("warm");
        let started = std::time::Instant::now();
        for _ in 0..10 {
            store.tips().expect("reads");
        }
        println!("{what}: {:?} a look", started.elapsed() / 10);
        transcript::push(conversations, &ids[0], &said(1)).expect("warm");
        let started = std::time::Instant::now();
        for k in 0..10 {
            transcript::push(conversations, &ids[k % ids.len()], &said(k + 3)).expect("said");
        }
        println!("{what}: {:?} an append", started.elapsed() / 10);
    }
}
