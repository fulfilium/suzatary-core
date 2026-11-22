/// Who a verb surface writes as.
/// Never a call parameter: each surface holds a constant, and no variant is the owner's, so an
/// agent cannot forge a trusted origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    /// The unattended triage agent: untrusted, so its writes stay provisional until confirmed.
    Triage,
    /// The main assistant in a read conversation: trusted, because the conversation is the review.
    Assistant,
}

impl Writer {
    /// The name that lands in the event's `actor` field.
    #[must_use]
    pub fn actor(self) -> &'static str {
        match self {
            Writer::Triage => "triage",
            Writer::Assistant => "assistant",
        }
    }
}
