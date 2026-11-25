//! Every seed `scripts/web-e2e.sh` starts a fixture server with builds whole, the bulk stores at a
//! small size: none writes a line the box refuses.
//!
//! The plain line writer refuses a model's turn only when it runs (`NotALine::Uncarried`), since
//! `Entry` is both what a conversation reads and what is written; until a written-only sum splits
//! them, this is the guard that every seed writes a model's turn as an answer with its carry. The
//! seeds `--settings` adds (a calendar, a lapsed sign-in) write no conversation's line, and need
//! the secret store `web-e2e.sh` isolates per server, so they are not built here.

use crate::binary::keyed_fixture as fixture;

#[test]
fn every_seed_the_scenes_start_builds() {
    let root = crate::common::scratch("seeds");
    for (name, flags) in [
        ("read", vec!["--login", "e2e-reader@example.invalid"]),
        (
            "proposed",
            vec![
                "--login",
                "e2e-reader@example.invalid",
                "--propose",
                "--reviewed",
            ],
        ),
        ("write", vec!["--propose"]),
        ("bootstrap", vec!["--model", "unanswered"]),
        (
            "lines",
            vec![
                "--lines",
                "60",
                "--conversations",
                "6",
                "--reseals",
                "6",
                "--proposals",
                "4",
                "--topics",
                "9",
                "--todos",
                "6",
            ],
        ),
        (
            "rescrub",
            vec![
                "--lines",
                "30",
                "--conversations",
                "3",
                "--sealed",
                "unversioned",
            ],
        ),
        ("wide", vec!["--wide", "40"]),
        ("shell", Vec::new()),
    ] {
        let here = root.join(name);
        std::fs::create_dir_all(here.join("state")).expect("a directory");
        let built = fixture(&here)
            .args(["fixture-chain", "--port", "0", "--root"])
            .arg(here.join("box"))
            // No page is loaded, so the frontend the checkout would hold is not needed.
            .arg("--repo")
            .arg(&here)
            .args(&flags)
            .output()
            .expect("the fixture runs");
        assert!(
            built.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
    }
}
