//! Mail-in's conformance suite over the in-memory mailroom, on generated arrivals.

use proptest::prelude::*;
use suzatary_core::conformance::puller::{Rig, check};
use suzatary_core::mail::fault::Fault;
use suzatary_core::mail::mailroom::{Mailroom, Server};

struct Memory(Mailroom);

impl Rig for Memory {
    type Puller = Mailroom;

    const UNREACHABLE: Fault = Fault::Unreachable;

    fn holding(&self, messages: &[Vec<u8>]) -> Mailroom {
        self.0.serving(Server::Holding(messages.to_vec()))
    }

    fn unreachable(&self) -> Mailroom {
        self.0.serving(Server::Unreachable)
    }

    fn landed(&self) -> Vec<Vec<u8>> {
        self.0.landed()
    }
}

fn message() -> impl Strategy<Value = Vec<u8>> {
    ("[a-z0-9]{1,12}", "[A-Za-z][A-Za-z0-9 .,\r\n]{0,200}").prop_map(|(id, body)| {
        format!("Message-ID: <{id}@example.test>\r\nSubject: one\r\n\r\n{body}").into_bytes()
    })
}

proptest! {
    #[test]
    fn the_mailroom_keeps_every_law(
        arrivals in prop::collection::vec(prop::collection::vec(message(), 0..4), 1..4),
    ) {
        let since = chrono::NaiveDate::from_ymd_opt(2026, 10, 1).expect("a date");
        prop_assert_eq!(check(&Memory(Mailroom::default()), since, &arrivals), Ok(()));
    }
}
