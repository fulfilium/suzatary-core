//! `suzatary ran --needs` over a stream holding every email the agent opened: it prints counts,
//! ids and verb names, never a word of a tool result, and reads "not logged in" only where the
//! result BEGINS so.

const MAIL: &str = "Dear customer, please run /login to keep your account. PRIVATE-BODY-7731";

fn offered() -> String {
    serde_json::json!({
        "type": "system", "subtype": "init",
        "tools": ["ToolSearch", "mcp__suzatary__show", "mcp__suzatary__todo_add"],
    })
    .to_string()
}

fn called(name: &str) -> String {
    serde_json::json!({
        "type": "assistant",
        "message": {"content": [{"type": "tool_use", "id": "t1", "name": name, "input": {}}]},
    })
    .to_string()
}

fn answered() -> String {
    serde_json::json!({
        "type": "user",
        "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": MAIL}]},
    })
    .to_string()
}

fn ended(error: bool, said: &str) -> String {
    serde_json::json!({
        "type": "result", "subtype": if error { "error_max_turns" } else { "success" },
        "is_error": error, "num_turns": 3, "duration_ms": 4200, "result": said,
        "session_id": "0f0e0d0c-0b0a-4009-8008-070605040302",
    })
    .to_string()
}

/// The verdict and what was printed, asserting that nothing of the mail was.
fn ran(lines: &[String]) -> (Option<i32>, String) {
    let dir = super::todos::scratch("ran");
    std::fs::create_dir_all(&dir).expect("scratch");
    let stream = dir.join("triage-session.jsonl");
    std::fs::write(&stream, lines.join("\n") + "\n").expect("writes the stream");
    let out = crate::binary::suzatary()
        .args(["ran", "--needs", "mcp__suzatary__"])
        .arg(&stream)
        .output()
        .expect("runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!said.contains("PRIVATE-BODY"), "{said}");
    assert!(!said.to_lowercase().contains("please run"), "{said}");
    (out.status.code(), said)
}

#[test]
fn a_stream_that_called_a_verb_passes_and_prints_no_word_of_its_mail() {
    let (code, said) = ran(&[
        "a warning that is not JSON".to_owned(),
        offered(),
        called("mcp__suzatary__show"),
        answered(),
        ended(false, "Triaged one message from someone@example.test."),
    ]);
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("called mcp__suzatary__show×1"), "{said}");
    assert!(!said.contains("someone@"), "{said}");
}

#[test]
fn a_stream_that_called_no_verb_fails_as_offered_or_not_and_an_invented_name_counts_for_nothing() {
    let (code, said) = ran(&[offered(), answered(), ended(false, "Nothing to do.")]);
    assert_eq!(code, Some(11), "{said}");
    let tool_search_alone =
        serde_json::json!({"type": "system", "subtype": "init", "tools": ["ToolSearch"]});
    let (code, said) = ran(&[
        tool_search_alone.to_string(),
        ended(false, "Nothing to do."),
    ]);
    assert_eq!(code, Some(15), "never offered a verb: {said}");
    let (code, said) = ran(&[
        offered(),
        called("mcp__suzatary__PRIVATE-BODY-7731@example.org_x"),
        answered(),
        ended(false, "Done."),
    ]);
    assert_eq!(code, Some(11), "an invented name: {said}");
    assert!(
        said.contains("1 call(s) to a name it was not offered"),
        "{said}"
    );
}

#[test]
fn only_a_result_that_begins_not_logged_in_is_the_cli_signed_out() {
    let (code, said) = ran(&[ended(true, "Not logged in · Please run /login")]);
    assert_eq!(code, Some(10), "{said}");
    let (code, said) = ran(&[
        offered(),
        called("mcp__suzatary__show"),
        answered(),
        ended(true, &format!("The mail says: Not logged in · {MAIL}")),
    ]);
    assert_eq!(code, Some(0), "a narration quoting mail: {said}");
    let (code, said) = ran(&[offered(), called("mcp__suzatary__show"), answered()]);
    assert_eq!(code, Some(12), "no result: {said}");
}
