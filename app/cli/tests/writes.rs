//! The write surface is exactly the named POSTs, and only `POST /api/objects` reaches the chain.
//!
//! Checked as this box's own node, which reads and never writes, since these are claims about the
//! router, not identity.

mod common;

use common::Fakes;

fn base() -> String {
    let fakes = Fakes::new("writes");
    let deployment = common::deployment([], []);
    common::start(
        std::sync::Arc::new(fakes),
        deployment,
        common::loopback_is_this_box(),
    )
}

/// A GET on any `/api/act/` path is a 405 with `Allow: POST`, never the frontend's index.
#[test]
fn a_get_on_an_act_route_is_405_with_allow_post() {
    let base = base();
    for path in [
        // The removed act verbs must fail loudly for an old client.
        "/api/act/complete",
        "/api/act/cancel",
        "/api/act/reopen",
        "/api/act/confirm",
        "/api/act/add",
        "/api/act/batch",
        "/api/act/device",
        "/api/act/key",
        "/api/act/pair",
        "/api/act/no-such-verb",
    ] {
        let response = common::get(&format!("{base}{path}"));
        assert_eq!(common::status(&response), 405, "{path}");
        assert_eq!(
            response
                .headers()
                .get("allow")
                .and_then(|v| v.to_str().ok()),
            Some("POST"),
            "{path}"
        );
    }
}

/// Every write route refuses a bare POST from loopback, which never writes.
#[test]
fn every_write_route_refuses_a_bare_post_from_loopback() {
    let base = base();
    for path in [
        "/api/objects",
        "/api/author",
        "/api/act/device",
        // Loopback asking to trust a key would be the box vouching for a device nobody holds.
        "/api/act/key",
        // Loopback typing a pairing code would be the box trusting a device nobody holds.
        "/api/act/pair",
        // Loopback reading a held value would hand it to the box's shell or the triage agent.
        "/api/held/reveal",
        // Loopback cancelling a watch would be the box's shell or the triage agent closing it.
        "/api/mail/watches/0123456789abcdef/cancel",
        // The error report's gate keeps the log to the logins that may write.
        "/api/client-errors",
        // Loopback writing a topic would be the box's shell or the triage agent writing one.
        "/api/stores/topics/objects",
        "/api/stores/topic-aliases/objects",
    ] {
        let response = common::agent()
            .post(format!("{base}{path}"))
            .send_json(serde_json::json!({}))
            .expect("the request completes");
        assert_eq!(common::status(&response), 403, "{path}");
    }
}

/// The router's whole mutating surface, enumerated so a new write is added on purpose.
///
/// A POST to a read route is 405, which tells "no handler" from "a handler refused".
#[test]
fn nothing_but_the_named_writes_answers_a_post() {
    let base = base();
    for path in [
        "/api/chain",
        "/api/fold",
        "/api/tip",
        "/api/tree",
        "/api/me",
        "/api/about",
        "/api/briefings",
        "/api/mail/inbox",
        "/api/mail/search",
        // The watches' read; its one write is `…/watches/<id>/cancel`.
        "/api/mail/watches",
        "/events",
        // A read whose sibling writes are separate paths.
        "/api/overview",
        // The memory's read; its one write is `…/memory/<id>/forget`.
        "/api/secretary/memory",
        // The record's read; its one write is `…/record/<node>/retract`.
        "/api/secretary/record",
        // A replica's pull of the lines, which the box alone writes.
        "/api/stores/lines/skeleton",
    ] {
        let mut response = common::agent()
            .post(format!("{base}{path}"))
            .send_json(serde_json::json!({}))
            .expect("the request completes");
        assert_eq!(
            common::status(&response),
            405,
            "{path} answered a POST — is it a write nobody decided to add?"
        );
        // Drained because the agent is pooled and a leftover body breaks the next request;
        // read as text because axum's own 405 has an empty body.
        let _ = response.body_mut().read_to_string();
    }
}

/// The secretary's writes, the tap above all, refuse loopback; its reads deliberately do not.
#[test]
fn every_secretary_write_refuses_a_bare_post_from_loopback() {
    let base = base();
    for path in [
        "/api/secretary/conversations",
        "/api/secretary/conversations/anything/say",
        "/api/secretary/conversations/anything/title",
        "/api/secretary/conversations/anything/stop",
        "/api/secretary/conversations/anything/regenerate",
        "/api/secretary/conversations/anything/edit",
        "/api/secretary/attachments",
        "/api/secretary/conversations/anything/proposals/0/tap",
        "/api/secretary/conversations/anything/proposals/0/acknowledge",
        // Dropping a session's branch is the owner's answer.
        "/api/secretary/conversations/anything/proposals/0/drop",
        // So is asking for a todo to be built: its card spends the account once tapped.
        "/api/todo/anything/build",
        // Forgetting erases, so it is the owner's act.
        "/api/secretary/memory/1/forget",
        // So is saying a summary is wrong.
        "/api/secretary/record/0000000000000000000000000000000000000000000000000000000000000000/retract",
    ] {
        let response = common::agent()
            .post(format!("{base}{path}"))
            .send_json(serde_json::json!({}))
            .expect("the request completes");
        assert_eq!(
            common::status(&response),
            403,
            "{path} let this box's own shell through"
        );
    }
}

/// The SMTP transport is named only by `commands::mail`, whose `deliver` every send uses.
///
/// It is a text grep, so `tests/` is skipped and prose elsewhere must not spell the call.
#[test]
fn the_smtp_transport_has_exactly_one_calling_site() {
    let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/cli has a parent");
    let mut naming: Vec<String> = Vec::new();
    let mut walk = vec![app.to_path_buf()];
    while let Some(dir) = walk.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            // `tests/` names the transport only to assert about it.
            if path
                .file_name()
                .is_some_and(|name| name == "target" || name == "tests")
            {
                continue;
            }
            if path.is_dir() {
                walk.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && std::fs::read_to_string(&path)
                    .is_ok_and(|text| text.contains("suzatary_io::smtp::"))
            {
                naming.push(
                    path.strip_prefix(app)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }
    naming.sort();
    assert_eq!(
        naming,
        vec!["cli/src/commands/mail.rs".to_owned()],
        "the set of files that call the SMTP transport changed. `mail.rs` holds \
         `deliver`, which the human's `send` verb calls directly and the \
         secretary's tap reaches through its postman. Anything else is a second \
         place that decides which account sends how, and a new way to send mail."
    );
}

/// No CORS or cookie header rides with any answer.
#[test]
fn it_sends_no_cors_or_cookie_headers() {
    let base = base();
    let response = common::get(&format!("{base}/api/chain"));
    assert_eq!(common::status(&response), 200);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
    assert!(response.headers().get("set-cookie").is_none());
}

/// Every answer carries the page policy, and every API answer says `no-store`, so a phone's HTTP
/// cache keeps no mail and no todo; the shell (here, the page that says it is not built) keeps
/// its own caching.
#[test]
fn every_answer_carries_the_policy_and_no_api_answer_is_stored() {
    let base = base();
    let header = |response: &ureq::http::Response<ureq::Body>, name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    for path in [
        "/api/chain",
        "/api/tip",
        "/api/me",
        "/api/about",
        "/api/mail/inbox",
        "/api/act/device",
    ] {
        let response = common::get(&format!("{base}{path}"));
        assert_eq!(
            header(&response, "content-security-policy").as_deref(),
            Some(suzatary_cli::serve::POLICY),
            "{path}"
        );
        assert_eq!(
            header(&response, "cross-origin-opener-policy").as_deref(),
            Some("same-origin"),
            "{path}"
        );
        assert_eq!(
            header(&response, "cache-control").as_deref(),
            Some("no-store"),
            "{path}"
        );
    }
    let shell = common::get(&format!("{base}/"));
    assert_eq!(
        header(&shell, "content-security-policy").as_deref(),
        Some(suzatary_cli::serve::POLICY)
    );
    assert_eq!(
        header(&shell, "cross-origin-opener-policy").as_deref(),
        Some("same-origin")
    );
    assert_ne!(header(&shell, "cache-control").as_deref(), Some("no-store"));
    for directive in [
        "default-src 'self'",
        "script-src 'self' 'wasm-unsafe-eval';",
        "style-src-attr 'unsafe-inline';",
        "object-src 'none'",
        "base-uri 'none'",
        "form-action 'none'",
        "frame-ancestors 'none'",
    ] {
        assert!(
            suzatary_cli::serve::POLICY.contains(directive),
            "{directive}"
        );
    }
    // Typst's style attributes are the one inline thing a page may carry; no script, no eval.
    assert_eq!(
        suzatary_cli::serve::POLICY
            .matches("'unsafe-inline'")
            .count(),
        1
    );
    assert!(!suzatary_cli::serve::POLICY.contains("'unsafe-eval'"));
}

/// The app authors no HTML; the only HTML on the wire is typst's.
#[test]
fn the_app_authors_no_html() {
    let serve = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/serve");
    let files: Vec<_> = std::fs::read_dir(&serve)
        .unwrap_or_else(|e| panic!("reading {}: {e}", serve.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    assert!(
        files.iter().any(|path| path.ends_with("route.rs")),
        "the walk found the serve module"
    );
    for path in files {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        assert!(!source.contains("HTMLResponse"), "{}", path.display());
        assert!(!source.contains("<html"), "{}", path.display());
    }
}

/// The device flow's `Consent` constructors each have exactly their named callers.
#[test]
fn the_device_flow_consent_is_claimed_in_exactly_the_named_places() {
    let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/cli has a parent")
        .to_path_buf();
    let repo = app.parent().expect("app has a parent").to_path_buf();
    let mut terminal = Vec::new();
    let mut by_tap = Vec::new();
    // `app/io/src/oauth.rs` may claim `given_at_a_terminal` only from its test module.
    let mut oauth_offsets: Vec<usize> = Vec::new();
    let mut oauth_cfg_test_offset: Option<usize> = None;
    let mut walk = vec![app];
    while let Some(dir) = walk.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                walk.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "rs")
                && !path.ends_with("app/core/src/mail/oauth.rs")
            {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let shown = path
                    .strip_prefix(&repo)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                // Assembled at runtime so this file's prose does not count itself.
                let at_terminal = ["Consent::", "given_at_a_terminal("].concat();
                let by_a_tap = ["Consent::", "given_by_tap("].concat();
                if shown == "app/io/src/oauth.rs" {
                    oauth_offsets = text.match_indices(&at_terminal).map(|(i, _)| i).collect();
                    oauth_cfg_test_offset = text.find("#[cfg(test)]");
                }
                if text.contains(&at_terminal) {
                    terminal.push(shown.clone());
                }
                if text.contains(&by_a_tap) {
                    by_tap.push(shown);
                }
            }
        }
    }
    terminal.sort();
    by_tap.sort();
    // Exactly these two, since an `.all()` over an empty list would pass silently.
    assert_eq!(
        terminal,
        vec![
            "app/cli/src/commands/mail.rs".to_owned(),
            "app/io/src/oauth.rs".to_owned(),
        ],
        "given_at_a_terminal callers"
    );
    let cfg_test_offset = oauth_cfg_test_offset
        .expect("app/io/src/oauth.rs has a #[cfg(test)] module — it is where this call must live");
    assert!(
        !oauth_offsets.is_empty(),
        "app/io/src/oauth.rs was counted as a caller but no occurrence was found"
    );
    assert!(
        oauth_offsets.iter().all(|&offset| offset > cfg_test_offset),
        "every given_at_a_terminal( in app/io/src/oauth.rs must sit after its #[cfg(test)] line \
         (offset {cfg_test_offset}), so the only NON-TEST caller stays suzatary auth alone: \
         {oauth_offsets:?}"
    );
    assert!(
        by_tap.iter().all(
            |p| p == "app/core/src/secretary/tap.rs" || p == "app/core/src/secretary/routes.rs"
        ),
        "given_by_tap callers: {by_tap:?}"
    );
}

/// A `Tap` is minted in one production place, the route guard, and only from the door's
/// `Trusted` proof (its one constructor takes it): a route cannot hold one without a device
/// signature the history trusts.
#[test]
fn the_tap_is_minted_only_by_the_guard_from_a_proof() {
    let constructor = ["Tap::", "of("].concat();
    assert_eq!(
        production_callers(&constructor),
        vec!["cli/src/commands/serve/guard.rs"],
        "{constructor} callers outside test code"
    );
}

/// What edits the instance file's connections, and what deletes an account's mail or its
/// sign-ins, is called from the tap's route, the server's wrappers and the fixture alone, so no
/// verb, read or other route reaches any of them.
#[test]
fn the_connection_edits_and_the_mail_delete_are_called_from_the_tap_alone() {
    let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/cli has a parent")
        .to_path_buf();
    // Assembled at runtime so this file's prose does not count itself; a method call, so the
    // trait's own declarations and the instance file's pure functions are not callers.
    let edits = [
        [".add_", "connection("].concat(),
        [".remove_", "connection("].concat(),
        [".add_", "origin("].concat(),
    ];
    // An account is what the mail delete is handed; the rendered blocks' forget is not this.
    let forget = [".forget", "(&account"].concat();
    let revoke = [".revoke", "("].concat();
    let mut editing: Vec<String> = Vec::new();
    let mut forgetting: Vec<String> = Vec::new();
    let mut revoking: Vec<String> = Vec::new();
    for crate_dir in ["core", "io", "cli", "plugin", "wasm"] {
        let mut walk = vec![app.join(crate_dir).join("src")];
        while let Some(dir) = walk.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let shown = path
                    .strip_prefix(&app)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                if edits.iter().any(|needle| text.contains(needle.as_str())) {
                    editing.push(shown.clone());
                }
                if text.contains(&forget) {
                    forgetting.push(shown.clone());
                }
                if text.contains(&revoke) {
                    revoking.push(shown);
                }
            }
        }
    }
    editing.sort();
    forgetting.sort();
    revoking.sort();
    assert_eq!(
        editing,
        [
            "cli/src/commands/fixture.rs",
            "cli/src/commands/serve/proposals.rs",
            "core/src/secretary/routes.rs",
        ],
        "callers of the connection edits"
    );
    // The mail cache's own tests call it on a scratch root.
    assert_eq!(
        forgetting,
        [
            "cli/src/commands/fixture.rs",
            "cli/src/commands/serve/mail_cache.rs",
            "core/src/secretary/routes.rs",
        ],
        "callers of the mail delete"
    );
    // The token store's own deletion is called by the mail cache alone, under the sync's hold,
    // and by the token store's own tests.
    assert_eq!(
        revoking,
        ["cli/src/commands/serve/mail_cache.rs", "io/src/oauth.rs"],
        "callers of the sign-ins' delete"
    );
}

/// No capability file names [`suzatary_core::settings::Writes`], so no verb can reach it.
#[test]
fn no_capability_file_ever_names_the_settings_writer() {
    let capability_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/cli has a parent")
        .join("core/src/capability");
    let entries = std::fs::read_dir(&capability_dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", capability_dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        assert!(
            !text.contains("Writes"),
            "{} names `Writes` — an effect record must never carry the settings write, \
             which would make it reachable from a verb",
            path.display()
        );
    }
}

/// Every `.rs` file under `app/` naming `needle` outside test code: a file under a `tests/`
/// directory, a `tests.rs` module, or past the file's first `#[cfg(test)]` is test code.
fn production_callers(needle: &str) -> Vec<String> {
    let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/cli has a parent")
        .to_path_buf();
    let mut callers = Vec::new();
    let mut walk = vec![app.clone()];
    while let Some(dir) = walk.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if name != "target" && name != "tests" {
                    walk.push(path);
                }
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || name == "tests.rs" {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let production = text
                .find("#[cfg(test)]")
                .map_or(text.as_str(), |at| &text[..at]);
            if production.contains(needle) {
                callers.push(
                    path.strip_prefix(&app)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }
    callers.sort();
    callers
}

/// A writer's markup has one door, and authoring is its one caller: the routes, the agent's
/// verbs, the secretary's acts and the CLI all reach it through `authoring`.
#[test]
fn the_markup_door_is_opened_by_authoring_alone() {
    let needle = ["MarkupSource::", "checked("].concat();
    assert_eq!(production_callers(&needle), vec!["core/src/authoring.rs"]);
}

/// Stored bytes are markup without a door only where they are READ: a record's literal, a
/// decision's, and the dev-only fixture's own data.
#[test]
fn stored_markup_is_minted_only_where_it_is_read() {
    let needle = ["MarkupSource::", "from_chain("].concat();
    assert_eq!(
        production_callers(&needle),
        vec![
            "cli/src/commands/fixture.rs",
            "core/src/decisions/event.rs",
            "core/src/record.rs",
        ]
    );
}

/// The closed world is the Prodrome's restricted project, and this application builds no typst
/// world or library of its own, nor the Prodrome's unrestricted project: a second world would be
/// a second containment to keep in step with the tab's.
#[test]
fn the_closed_world_is_the_one_world_and_library_built_here() {
    for needle in [
        ["impl World", " for "].concat(),
        ["impl typst::World", " for "].concat(),
        ["Library::", "builder("].concat(),
        ["Library::", "default("].concat(),
        ["Project::", "new("].concat(),
    ] {
        assert_eq!(
            production_callers(&needle),
            Vec::<String>::new(),
            "{needle}"
        );
    }
    assert_eq!(
        production_callers(&["Project::", "restricted("].concat()),
        vec!["io/src/typeset/mod.rs"]
    );
}

/// The inbox's and the memory's places are read in one file, the instance's, so each store is
/// opened where its place is named.
#[test]
fn the_inbox_and_the_memory_are_opened_by_the_instance_alone() {
    for store in ["inbox", "memory"] {
        let needle = ["secretary.", store, ".clone()"].concat();
        assert_eq!(
            production_callers(&needle),
            vec!["cli/src/instance.rs"],
            "{needle}"
        );
    }
}
