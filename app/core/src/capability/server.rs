use super::{Agent, Notify, Secretary, Whois};
use crate::secretary::answers::Answers;
use crate::secretary::model::Sessions;

/// What the serving process supplies: the secretary's effects, since it runs the
/// secretary, plus peers and phones. Nothing here sends mail; only a route holding
/// a `Tap` the gate mints reaches the transport, and that is not an effect a record holds.
#[derive(Clone, Copy)]
pub struct Server<'a> {
    /// The secretary's effects.
    pub secretary: Secretary<'a>,
    /// Who owns a peer address: the only source of a `Login`.
    pub whois: &'a dyn Whois,
    /// Push to the owner's phones.
    pub notify: &'a dyn Notify,
    /// Typst, for a model's answer blocks.
    pub answers: &'a dyn Answers,
    /// The phones that asked to be woken.
    pub devices: &'a dyn crate::store::Log<String, crate::devices::Device>,
    /// The session-holding model, when there is one.
    pub sessions: &'a dyn Sessions,
}

impl<'a> Server<'a> {
    /// The agent's share of these effects.
    #[must_use]
    pub fn agent(&self) -> Agent<'a> {
        self.secretary.agent()
    }
}
