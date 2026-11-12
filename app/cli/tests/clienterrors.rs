//! `POST /api/client-errors`, sent the way `web/src/app/beacon.ts` sends it: a writer's report
//! is 204, an oversized one is refused, and a login past its budget is 429
//! before its body is read.

mod common;

use std::sync::Arc;

use common::Fakes;

fn writer(tag: &str) -> String {
    let ada = common::login("ada@example.com");
    let whois = common::FakeWhois::new().naming(common::v4(), ada.clone());
    let fakes = Fakes::new(tag).whois(whois);
    let deployment = common::deployment([ada.clone()], [(ada, common::actor("ada"))]);
    common::start(Arc::new(fakes), deployment, Vec::new())
}

fn report() -> serde_json::Value {
    serde_json::json!({
        "kind": "core",
        "build": "abc123",
        "agent": "a phone",
        "page": "todos",
        "message": "the core could not be loaded: CompileError",
        "stack": "loadCore@http://localhost/app.js:1:1",
    })
}

#[test]
fn a_writers_report_is_taken_and_answered_with_nothing() {
    let base = writer("clienterrors-taken");
    let mut response = common::act(&format!("{base}/api/client-errors"), &report());
    assert_eq!(common::status(&response), 204);
    assert_eq!(response.body_mut().read_to_string().expect("a body"), "");
}

#[test]
fn a_report_over_the_cap_is_refused() {
    let base = writer("clienterrors-cap");
    let mut body = report();
    body["stack"] = serde_json::json!("x".repeat(suzatary_api::clienterror::MAX_BODY_BYTES));
    let response = common::act(&format!("{base}/api/client-errors"), &body);
    assert_eq!(common::status(&response), 400);
}

#[test]
fn a_login_past_its_budget_is_told_429() {
    let base = writer("clienterrors-budget");
    let most = suzatary_api::clienterror::BUDGET.most;
    for n in 0..most {
        let response = common::act(&format!("{base}/api/client-errors"), &report());
        assert_eq!(common::status(&response), 204, "report {n}");
    }
    let response = common::act(&format!("{base}/api/client-errors"), &report());
    assert_eq!(common::status(&response), 429);
}
