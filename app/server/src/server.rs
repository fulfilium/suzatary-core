use suzatary_base::capability::Whois;
use suzatary_cards::notify::Notify;
use suzatary_conversations::secretary::model::Picker;
use suzatary_secretary::agent::Agent;
use suzatary_secretary::secretary::Secretary;
use suzatary_topics::secretary::answers::Answers;

/// What the serving process supplies: the secretary's effects, since it runs the
/// secretary, plus peers and phones. Nothing here sends mail; only a route holding
/// a `Tap` the gate mints reaches the transport, and that is not an effect a record holds.
#[derive(Clone, Copy)]
pub struct Server<'a> {
    pub secretary: Secretary<'a>,
    /// Who owns a peer address: the only source of a `Login`.
    pub whois: &'a dyn Whois,
    pub notify: &'a dyn Notify,
    /// Typst, for a model's answer blocks.
    pub answers: &'a dyn Answers,
    /// The phones that asked to be woken.
    pub devices: &'a dyn suzatary_store::store::Log<String, suzatary_signatures::devices::Device>,
    /// Which model each round asks, picked as the round starts.
    pub models: &'a dyn Picker,
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
    use suzatary_secretary::agent::Agent;
    use suzatary_secretary::secretary::Secretary;

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
            journal: _,
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
            models: _,
        } = Server::nothing();
    }
}
