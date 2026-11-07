use crate::common::*;

/// Each page's rehearsal (`app/pages/tests/rehearsal/`) is flags and inputs its page takes, so the
/// worker that runs it never meets a refused input, and a wire change that would refuse one fails
/// here.
#[test]
fn every_rehearsal_runs_as_its_page_would() {
    use viewtree::host::Host;
    Host::<secretary::Secretary>::rehearse().expect("the stream's");
    Host::<conversation::Conversation>::rehearse().expect("a conversation's");
    Host::<inbox::Inbox>::rehearse().expect("the Inbox's");
    Host::<needsyou::NeedsYou>::rehearse().expect("the popup's");
    Host::<settings::Settings>::rehearse().expect("Settings'");
}
