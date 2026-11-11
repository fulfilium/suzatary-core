//! No identity number reaches a model through a verb, whatever the verb read: a mail reading
//! with one, a Junk message read from its server, and a search row are answered to the secretary
//! and to the triage agent with placeholders, and nothing else of them moves.

use prodrome_core::event::Actor;
use prodrome_core::policy::Untrusted;
use serde_json::{Map, Value, json};
use suzatary_base::capability::Nothing;
use suzatary_base::chains::{ChainName, ChainPolicy, Chains};
use suzatary_core::agent::Agent;
use suzatary_core::deployment::Deployment;
use suzatary_core::mail::corpus::{Corpus, CorpusError};
use suzatary_core::mail::junk::{ChurnList, JunkHeader, JunkMessage, Uid};
use suzatary_core::mail::junk_folder::{Junk, JunkFailure};
use suzatary_core::mail::mailbox::Query;
use suzatary_core::mail::state::Decision;
use suzatary_core::mail::sync_report::SyncReport;
use suzatary_core::mail::verbs::{Marked, Message, Show, Summary};
use suzatary_core::secretary::{Bound, Secretary};

/// Synthetic numbers no person holds: SSN area 000, ITIN area 912.
const SSN: &str = "000-12-3456";
const ITIN: &str = "912-70-1234";

const ACCOUNT: &str = "ada@example.test";

/// A corpus and a Junk folder whose every reading names a number, as a reader that masked
/// nothing would answer.
struct Raw;

impl Corpus for Raw {
    fn sync(&self) -> Result<SyncReport, CorpusError> {
        Nothing.sync()
    }
    fn queue(&self, limit: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
        Nothing.queue(limit)
    }
    fn search(&self, _: &Query, _: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
        Ok(vec![Summary {
            hash: "ab".repeat(32),
            id: "form-1@example.test".to_owned(),
            sender: ACCOUNT.to_owned(),
            title: format!("SSN {SSN}"),
            timestamp: None,
            container: "sent".to_owned(),
            sources: 1,
            held: suzatary_core::held_value::Kinds::one(suzatary_core::redact::Kind::Ssn),
        }])
    }
    fn show(&self, id: &str) -> Result<Show, CorpusError> {
        Ok(Show::Shown(Message {
            id: id.to_owned(),
            tags: vec!["sent".to_owned()],
            headers: [("Subject".to_owned(), format!("ITIN {ITIN}"))]
                .into_iter()
                .collect(),
            body: format!("My SSN is {SSN}, for the zebra form."),
            links: Vec::new(),
        }))
    }
    fn mark(
        &self,
        hash: &str,
        decision: Decision,
        note: Option<&str>,
        native: Option<&str>,
    ) -> Result<Marked, CorpusError> {
        Nothing.mark(hash, decision, note, native)
    }
    fn last_sync(&self) -> Option<SyncReport> {
        None
    }
    fn last_triage_pass(&self) -> Option<String> {
        None
    }
}

impl Junk for Raw {
    fn junk_accounts(&self) -> Vec<String> {
        vec![ACCOUNT.to_owned()]
    }
    fn churn(&self) -> ChurnList {
        ChurnList::default()
    }
    fn junk_headers(&self, _: &str, _: chrono::NaiveDate) -> Result<Vec<JunkHeader>, JunkFailure> {
        Ok(Vec::new())
    }
    fn junk_message(&self, account: &str, uid: Uid) -> Result<JunkMessage, JunkFailure> {
        Ok(JunkMessage {
            account: account.to_owned(),
            header: JunkHeader::new(uid, None, Some(ACCOUNT), "your ITIN", "Mon, 6 Mar 2023"),
            body: format!("Your ITIN {ITIN} is filed."),
            truncated: false,
        })
    }
}

fn chain() -> ChainPolicy {
    let actor = |name: &str| Actor::new(name).expect("a roster name");
    ChainPolicy {
        name: ChainName::new("life").expect("a slug"),
        untrusted: Untrusted::of([actor("triage")]),
        roster: [actor("owner"), actor("triage")].into_iter().collect(),
        root: suzatary_core::cast::keys::root(),
    }
}

fn deployment() -> Deployment {
    Deployment {
        viewers: std::collections::BTreeSet::new(),
        actors: std::collections::BTreeMap::new(),
        chains: Chains::of(vec![chain()]).expect("one chain"),
        categories: [("home".to_owned(), 1)].into_iter().collect(),
        app_url: "https://example.test".to_owned(),
        coding: None,
        zone: suzatary_base::zone::Zone::UTC,
    }
}

fn args(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("arguments are an object, not {other}"),
    }
}

fn agent(verb: &str, call: Value) -> String {
    let caps = suzatary_core::agent::Agent {
        corpus: &Raw,
        ..Agent::nothing()
    };
    let records = [("life".to_owned(), caps)].into_iter().collect();
    let conversations = suzatary_core::secretary::conversations::InMemory::default();
    let inbox = suzatary_core::store::InMemory::<suzatary_core::secretary::inbox_event::InboxEvent>::default();
    let cards =
        suzatary_core::secretary::filed::TriageCards::new(conversations.conversations(), &inbox);
    suzatary_core::agent::call(&records, &cards, &deployment(), verb, &args(call))
        .expect("answered")
        .to_string()
}

fn secretary(verb: &str, call: Value) -> String {
    let policy = chain();
    let chains = [Bound {
        name: "life",
        caps: suzatary_core::secretary::Secretary {
            corpus: &Raw,
            junk: &Raw,
            ..Secretary::nothing()
        },
        policy: &policy,
    }];
    suzatary_core::secretary::call(
        &deployment(),
        &suzatary_core::settings::Settables::default(),
        &chains,
        &suzatary_base::conversation_id::DEFAULT.clone(),
        verb,
        &args(call),
    )
    .expect("answered")
    .value
    .to_string()
}

#[test]
fn no_verb_answer_names_a_number_whatever_it_read() {
    let answers = [
        agent("show", json!({"id": "form-1@example.test"})),
        agent("search", json!({"query": "folder:Sent"})),
        secretary("show", json!({"id": "form-1@example.test"})),
        secretary("search", json!({"query": "folder:Sent"})),
        secretary("junk_show", json!({"account": ACCOUNT, "uid": 7})),
    ];
    for answer in &answers {
        for number in [SSN, ITIN] {
            assert!(!answer.contains(number), "{number} in {answer}");
        }
    }
    assert!(answers[0].contains("My SSN is [SSN ending 3456 · withheld], for the zebra form."));
    assert!(answers[0].contains("ITIN [ITIN ending 1234 · withheld]"));
    assert!(answers[1].contains("SSN [SSN ending 3456 · withheld]"));
    assert!(answers[4].contains("Your ITIN [ITIN ending 1234 · withheld] is filed."));
    assert_eq!(answers[0], answers[2], "both surfaces answer alike");
}
