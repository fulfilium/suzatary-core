//! Who a verb surface writes as.

/// Who a verb surface writes as.
///
/// Never a parameter of a call: each surface holds a constant of this type, so "the agent
/// cannot forge a trusted origin" is a fact about which values exist where. The owner's actor
/// is deliberately absent, so no verb surface can make an agent's guess read as his word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    /// The unattended triage agent: untrusted, so what it writes is provisional until the
    /// owner confirms it.
    Triage,
    /// The main assistant, in a conversation somebody is reading: trusted, since the
    /// conversation is the review.
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
