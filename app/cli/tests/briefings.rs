//! `/api/briefings/{date}` over the real `BriefingFiles` in a temp directory, which pins the
//! on-disk shape.

use crate::common;

use common::Fakes;

fn start() -> (String, std::path::PathBuf) {
    let fakes = Fakes::new("briefings");
    let dir = fakes.root.join("briefings");
    let deployment = common::deployment([], []);
    let base = common::start(
        std::sync::Arc::new(fakes),
        deployment,
        common::loopback_is_this_box(),
    );
    (base, dir)
}

fn write(dir: &std::path::Path, day: &str, text: &str) {
    std::fs::create_dir_all(dir).expect("dir exists");
    std::fs::write(dir.join(format!("{day}.typ")), text).expect("writes");
}

#[test]
fn a_good_date_returns_its_source() {
    let (base, dir) = start();
    write(&dir, "2026-09-01", "= Morning\n\nHello there.");
    let mut response = common::get(&format!("{base}/api/briefings/2026-09-01"));
    assert_eq!(common::status(&response), 200);
    let body = common::body_json(&mut response);
    assert_eq!(body["date"], "2026-09-01");
    assert_eq!(body["day"]["kind"], "saved");
    assert_eq!(body["day"]["source"], "= Morning\n\nHello there.");
}

/// A day the secretary briefed reads as its brief, whatever the saved files say.
#[test]
fn a_day_the_secretary_briefed_is_its_brief_block_by_block() {
    use suzatary_base::conversation_id::DEFAULT;
    use suzatary_lines::secretary::conversations::read_or_open_default;
    use suzatary_secretary::secretary::briefs::BRIEF;
    use suzatary_secretary::secretary::converse;

    let fakes = Fakes::new("briefings-brief").model(common::FakeModel::saying([
        "{\"say\": \"Nothing urgent.\\n\\nThe permit filing moved.\"}",
    ]));
    let deployment = common::deployment([], []);
    read_or_open_default(
        fakes.conversations.conversations(),
        &fakes.model,
        &DEFAULT,
        common::now(),
    )
    .expect("opens");
    converse(
        &fakes.secretary(),
        &deployment,
        &suzatary_connect::settings::Settables::default(),
        &[],
        &DEFAULT,
        suzatary_secretary::secretary::Saying {
            text: BRIEF,
            references: &[],
            by: suzatary_secretary::secretary::Asker::Box(
                suzatary_inbox::secretary::own_round::OwnRound::Brief,
            ),
        },
        "system prompt",
    )
    .expect("the scripted model answers")
    .expect("a round ran");
    let day = common::now().isoformat();
    let day = &day[..10];
    write(
        &fakes.root.join("briefings"),
        day,
        "= Saved\n\nOlder words.",
    );
    let base = common::start(
        std::sync::Arc::new(fakes),
        deployment,
        common::loopback_is_this_box(),
    );
    let mut response = common::get(&format!("{base}/api/briefings/{day}"));
    assert_eq!(common::status(&response), 200);
    let body = common::body_json(&mut response);
    assert_eq!(body["day"]["kind"], "brief", "{body}");
    assert_eq!(body["day"]["conversation"], DEFAULT.as_str());
    let blocks = body["day"]["blocks"].as_array().expect("blocks");
    assert_eq!(blocks.len(), 2, "{body}");
    assert_eq!(blocks[0]["plain"], "Nothing urgent.");
}

/// A percent-encoded path traversal is refused as a bad date before it becomes a filename.
#[test]
fn a_path_traversal_attempt_is_400() {
    let (base, _dir) = start();
    let response = common::get(&format!("{base}/api/briefings/%2e%2e"));
    assert_eq!(common::status(&response), 400);
}

/// `today` is the owner's day by the box's clock, so a file saved for that day answers to it and
/// a day nobody wrote answers 404.
#[test]
fn today_is_the_day_the_clock_names() {
    let (base, dir) = start();
    let mut response = common::get(&format!("{base}/api/briefings/today"));
    assert_eq!(
        common::status(&response),
        404,
        "{}",
        common::body_json(&mut response)
    );
    let day = common::now().isoformat();
    write(&dir, &day[..10], "= Morning\n\nHello there.");
    let mut response = common::get(&format!("{base}/api/briefings/today"));
    assert_eq!(common::status(&response), 200);
    assert_eq!(common::body_json(&mut response)["date"], day[..10]);
}
