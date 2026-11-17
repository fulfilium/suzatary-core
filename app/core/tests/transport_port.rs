//! Mail-out's conformance suite over the in-memory transport, on generated messages, and the
//! suite catching a transport that adds a recipient.

use proptest::prelude::*;
use suzatary_core::conformance::transport::{Case, Law, check};
use suzatary_mail::mail::outbox::Outbox;
use suzatary_mail::mail::send::{Composed, Outgoing, SendError, Sent};
use suzatary_mail::mail::transport::Transport;

fn address() -> impl Strategy<Value = String> {
    "[a-z]{1,8}@[a-z]{1,8}\\.test"
}

fn cases() -> impl Strategy<Value = Vec<Case>> {
    prop::collection::vec(
        (
            address(),
            prop::collection::vec(address(), 1..3),
            prop::collection::vec(address(), 0..3),
            "[A-Za-z][A-Za-z0-9 .,]{0,30}",
            "[A-Za-z][A-Za-z0-9 .,\n]{0,200}",
            "[a-z0-9]{1,12}@[a-z]{1,8}\\.test",
            0_i64..2_000_000_000,
        )
            .prop_map(|(sender, to, cc, subject, body, message_id, at)| Case {
                out: Outgoing::new(Composed {
                    sender,
                    to,
                    cc,
                    subject,
                    body,
                    ..Composed::default()
                })
                .expect("the generator writes sendable messages"),
                message_id,
                at,
            }),
        1..4,
    )
}

proptest! {
    #[test]
    fn the_in_memory_transport_keeps_every_law(cases in cases()) {
        let outbox = Outbox::default();
        prop_assert_eq!(check(&outbox, || outbox.delivered(), &cases), Ok(()));
    }
}

/// A transport that copies someone the message does not name.
struct Copies(Outbox);

impl Transport for Copies {
    fn send(&self, out: &Outgoing, message_id: &str, at: i64) -> Result<Sent, SendError> {
        let mut composed = out.composed();
        composed.cc.push("watcher@elsewhere.test".to_owned());
        self.0
            .send(&Outgoing::new(composed)?, message_id, at)
            .map(|sent| Sent {
                recipients: sent.recipients[..sent.recipients.len() - 1].to_vec(),
                ..sent
            })
    }
}

#[test]
fn a_transport_that_copies_someone_breaks_delivered() {
    let copies = Copies(Outbox::default());
    let case = Case {
        out: Outgoing::new(Composed {
            sender: "ada@example.test".to_owned(),
            to: vec!["him@example.test".to_owned()],
            subject: "Hello".to_owned(),
            body: "One line.".to_owned(),
            ..Composed::default()
        })
        .expect("a sendable message"),
        message_id: "one@example.test".to_owned(),
        at: 0,
    };
    let broken = check(&copies, || copies.0.delivered(), &[case]).expect_err("a law breaks");
    assert_eq!(broken.law, Law::Delivered);
}

/// A wire that loses the blank lines, the one between the headers and the body among them.
struct Squeezes(Outbox);

impl Transport for Squeezes {
    fn send(&self, out: &Outgoing, message_id: &str, at: i64) -> Result<Sent, SendError> {
        self.0.send(out, message_id, at)
    }
}

impl Squeezes {
    fn delivered(&self) -> Vec<suzatary_mail::mail::transport::Delivered> {
        self.0
            .delivered()
            .into_iter()
            .map(|mut delivered| {
                let text = String::from_utf8_lossy(&delivered.message).into_owned();
                delivered.message = text
                    .split("\r\n")
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join("\r\n")
                    .into_bytes();
                delivered
            })
            .collect()
    }
}

fn one() -> Case {
    Case {
        out: Outgoing::new(Composed {
            sender: "ada@example.test".to_owned(),
            to: vec!["him@example.test".to_owned()],
            subject: "Hello".to_owned(),
            body: "One line.\n\nAnd another after a blank.".to_owned(),
            ..Composed::default()
        })
        .expect("a sendable message"),
        message_id: "one@example.test".to_owned(),
        at: 0,
    }
}

#[test]
fn a_wire_that_loses_blank_lines_breaks_intact() {
    let squeezes = Squeezes(Outbox::default());
    let broken = check(&squeezes, || squeezes.delivered(), &[one()]).expect_err("a law breaks");
    assert_eq!(broken.law, Law::Intact);
}
