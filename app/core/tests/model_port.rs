//! The model's conformance suite over the free model, `Answering`, on generated conversations,
//! and the suite catching a model that adds a call of its own.

use std::sync::{Arc, Mutex, PoisonError};

use proptest::prelude::*;
use suzatary_core::secretary::answering::Answering;
use suzatary_core::secretary::model::laws::{Case, Law, Rig, check};
use suzatary_core::secretary::model::{Conversation, Model, ModelError, Reply, ToolCall, Turn};

type Answer = Box<dyn Fn(&Conversation) -> Result<String, ModelError> + Sync>;

/// The free model, its far end a function that keeps every conversation it was asked.
#[derive(Default)]
struct Free(Arc<Mutex<Vec<Conversation>>>);

impl Rig for Free {
    type Model = Answering<Answer>;

    fn answering(&self, text: &str) -> Self::Model {
        let (heard, text) = (Arc::clone(&self.0), text.to_owned());
        Answering::new(
            "free/test",
            Box::new(move |conversation: &Conversation| {
                heard
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(conversation.clone());
                Ok(text.clone())
            }) as Answer,
        )
    }

    fn unreachable(&self) -> Self::Model {
        Answering::new(
            "free/test",
            Box::new(|_: &Conversation| Err(ModelError::Unreachable("no far end".to_owned())))
                as Answer,
        )
    }

    fn heard(&self, _model: &Self::Model) -> String {
        let heard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let last = heard.last().map_or_else(String::new, |conversation| {
            std::iter::once(conversation.system.clone())
                .chain(conversation.turns.iter().map(|turn| format!("{turn:?}")))
                .collect::<Vec<_>>()
                .join("\n")
        });
        drop(heard);
        last
    }
}

fn turn() -> impl Strategy<Value = Turn> {
    prop_oneof![
        "[A-Za-z0-9 ?.]{1,40}".prop_map(|text| Turn::Person {
            text,
            references: Vec::new(),
            attachments: Vec::new(),
        }),
        "[A-Za-z0-9 .]{1,40}".prop_map(|text| Turn::Model {
            text,
            calls: Vec::new(),
        }),
        "[A-Za-z0-9 .]{1,40}".prop_map(|text| Turn::Act {
            text,
            references: Vec::new(),
        }),
    ]
}

fn answer() -> impl Strategy<Value = String> {
    prop_oneof![
        "[A-Za-z0-9 .]{0,40}",
        ("[A-Za-z0-9 .]{0,20}", prop::collection::vec("[a-z_]{1,12}", 0..3)).prop_map(
            |(say, verbs)| {
                serde_json::json!({
                    "say": say,
                    "calls": verbs.iter().map(|verb| serde_json::json!({"verb": verb, "args": {}})).collect::<Vec<_>>(),
                })
                .to_string()
            }
        ),
    ]
}

fn cases() -> impl Strategy<Value = Vec<Case>> {
    prop::collection::vec(
        (
            "[A-Za-z .]{1,40}",
            prop::collection::vec(turn(), 1..5),
            answer(),
        )
            .prop_map(|(system, turns, answer)| Case {
                conversation: Conversation { system, turns },
                answer,
            }),
        1..4,
    )
}

proptest! {
    #[test]
    fn the_free_model_keeps_every_law(cases in cases()) {
        prop_assert_eq!(check(&Free::default(), &serde_json::json!([]), &cases), Ok(()));
    }
}

/// A model that asks for a verb its far end never named.
struct Adds(Free);

impl Rig for Adds {
    type Model = Eager;

    fn answering(&self, text: &str) -> Eager {
        Eager(self.0.answering(text))
    }

    fn unreachable(&self) -> Eager {
        Eager(self.0.unreachable())
    }

    fn heard(&self, model: &Eager) -> String {
        self.0.heard(&model.0)
    }
}

struct Eager(Answering<Answer>);

impl Model for Eager {
    fn name(&self) -> String {
        self.0.name()
    }

    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        self.0.turn(conversation, tools).map(|mut reply| {
            reply.calls.push(ToolCall {
                verb: "context".into(),
                args: std::collections::BTreeMap::new(),
            });
            reply
        })
    }
}

#[test]
fn a_model_that_adds_a_call_is_not_faithful() {
    let case = Case {
        conversation: Conversation {
            system: "Be brief.".to_owned(),
            turns: vec![Turn::Person {
                text: "hello".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            }],
        },
        answer: r#"{"say": "hi"}"#.to_owned(),
    };
    let broken =
        check(&Adds(Free::default()), &serde_json::json!([]), &[case]).expect_err("a law breaks");
    assert_eq!(broken.law, Law::Faithful);
}
