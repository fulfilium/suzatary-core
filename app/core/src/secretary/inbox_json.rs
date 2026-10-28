//! How the inbox's schema crosses into the browser's module: each proposal under `proposal`, its
//! two registers by name, and each register's value as JSON, a function of the value alone.

use prodrome_core::schema::Schema;
use prodrome_wasm_exports::Json;
use prodrome_wasm_exports::wire::json_literal;
use serde_json::Value;

use crate::secretary::inbox_event::{InboxEvent, InboxVocabulary, Said};
use crate::secretary::inbox_registers::InboxRegister;

impl Json for InboxEvent {
    fn key_fields(_: &InboxVocabulary) -> Vec<&str> {
        vec!["proposal"]
    }

    fn key_values(key: &<Self as Schema>::Key) -> Vec<Value> {
        vec![Value::from(key.as_ref())]
    }

    fn register(_: &InboxVocabulary, register: InboxRegister) -> &'static str {
        match register {
            InboxRegister::Content => "content",
            InboxRegister::State => "state",
        }
    }

    /// The content register's value is the whole `Asked` event, so its JSON is the event's
    /// literal; the state register's is the response's
    /// [`Phase`](crate::secretary::phase::Phase), so two responses at one phase answer one JSON
    /// and the write's name says which response it was. An event writes one register, the one
    /// its kind names ([`InboxEvent`]'s `writes`), so the event alone decides.
    fn value(&self, _: InboxRegister) -> Value {
        match &self.said {
            Said::Responded(responded) => serde_json::to_value(responded.phase())
                .expect("a phase is a tag and two scalars, which always serialise"),
            Said::Asked(_) => json_literal(&Schema::to_value(self)),
        }
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::Actor;
    use prodrome_core::literal::Datetime;
    use proptest::prelude::*;

    use super::*;
    use crate::secretary::inbox::ProposalId;
    use crate::secretary::responded::{Responded, Tried};

    fn responded(said: Responded, minute: u32) -> InboxEvent {
        InboxEvent {
            proposal: ProposalId::new("p0").expect("an id"),
            at: Datetime::new(2026, 10, 1, 12, minute, 0, 0).expect("an instant"),
            actor: Actor::new("ada").expect("an actor"),
            said: Said::Responded(said),
        }
    }

    /// A phase's JSON is tagged on `phase`, its name in camel case, a try's two fields beside it.
    #[test]
    fn a_phase_is_tagged_on_phase() {
        use crate::secretary::phase::Phase;
        let json = |phase| serde_json::to_string(&phase).expect("prints");
        assert_eq!(
            json(Phase::Trying {
                attempt: 2,
                running: true
            }),
            r#"{"phase":"trying","attempt":2,"running":true}"#
        );
        assert_eq!(json(Phase::MayHaveGone), r#"{"phase":"mayHaveGone"}"#);
        assert_eq!(json(Phase::Dismissed), r#"{"phase":"dismissed"}"#);
    }

    proptest! {
        /// A state value's JSON is a function of its phase: two responses at one phase, whatever
        /// they say beside it and whenever, answer the same JSON.
        #[test]
        fn a_state_value_is_its_phase(attempt in 1u32..9, a in ".{0,12}", b in ".{0,12}", m in 0u32..59) {
            let one = responded(Responded::Tried { attempt, tried: Tried::NotSent { reason: a } }, 0);
            let two = responded(Responded::Tried { attempt, tried: Tried::NotSent { reason: b } }, m);
            prop_assert_eq!(one.value(InboxRegister::State), two.value(InboxRegister::State));
        }
    }
}
