use suzatary_base::capability::Whois;
use suzatary_core::agent::Agent;
use suzatary_core::notify::Notify;
use suzatary_core::secretary::Secretary;
use suzatary_core::secretary::answers::Answers;
use suzatary_core::secretary::model::Sessions;

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
    pub devices: &'a dyn suzatary_store::store::Log<String, suzatary_signatures::devices::Device>,
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

#[cfg(test)]
mod tests {
    use super::Server;
    use suzatary_core::agent::Agent;
    use suzatary_core::secretary::Secretary;

    /// Each record is exactly these effects, so widening a role's authority is an edit here too.
    #[test]
    fn each_role_holds_exactly_its_effects() {
        let Agent {
            chain: _,
            corpus: _,
            mail: _,
            repository: _,
            manuals: _,
            render: _,
            clock: _,
            memory: _,
        } = Agent::nothing();
        let Secretary {
            chain: _,
            corpus: _,
            mail: _,
            repository: _,
            manuals: _,
            render: _,
            clock: _,
            model: _,
            conversations: _,
            inbox: _,
            memories: _,
            decisions: _,
            topics: _,
            briefings: _,
            weather: _,
            calendar: _,
            secrets: _,
            web: _,
            repositories: _,
            drive: _,
            junk: _,
            sync: _,
            health: _,
            older: _,
            uploads: _,
            upload_index: _,
            kept: _,
            pdf: _,
            tapped: _,
            configuration: _,
            watches: _,
        } = Secretary::nothing();
        let Server {
            secretary: _,
            whois: _,
            notify: _,
            answers: _,
            devices: _,
            sessions: _,
        } = Server::nothing();
    }
}
