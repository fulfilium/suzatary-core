//! The guide in both its worlds: the closed world's HTML passes the tripwire, and typst makes its
//! PDF with every picture it names, the end-to-end suite's own (`docs/images`).

use super::{Scratch, repo};
use std::process::Command;

use suzatary_cli::commands::guide::{self, World};

fn text() -> String {
    std::fs::read_to_string(repo().join("docs/guide/guide.typ")).expect("the guide reads")
}

/// Every file a `#shot` names.
fn pictures(text: &str) -> Vec<String> {
    text.split("#shot(\"")
        .skip(1)
        .filter_map(|rest| rest.split_once('"').map(|(file, _)| file.to_owned()))
        .collect()
}

#[test]
fn the_guide_compiles_in_the_closed_world_and_shows_the_demo_init_writes() {
    let html = guide::html(&text()).expect("the closed world compiles the guide");
    for shown in [
        "instance.toml",
        "suzatary init",
        "[[chains]]",
        "her@example.invalid",
    ] {
        assert!(html.contains(shown), "the guide's HTML lacks {shown:?}");
    }
}

#[test]
fn the_guide_compiles_to_a_pdf_with_every_picture_it_names() {
    let text = text();
    let shown = pictures(&text);
    assert!(!shown.is_empty(), "the guide shows no picture");
    let out = Scratch::new("guide-pdf");
    for file in &shown {
        // Its bytes, not its mode: a store path's files are read-only.
        let bytes = std::fs::read(repo().join("docs/images").join(file))
            .unwrap_or_else(|e| panic!("docs/images/{file}: {e}"));
        std::fs::write(out.join(file), bytes).expect("placed");
    }
    std::fs::write(
        out.join("guide.typ"),
        guide::source(World::Pdf, &text).expect("the source"),
    )
    .expect("written");
    let compiled = Command::new("typst")
        .args(["compile", "--root"])
        .arg(&*out)
        .arg(out.join("guide.typ"))
        .arg(out.join("guide.pdf"))
        .env_remove("SOURCE_DATE_EPOCH")
        .output()
        .expect("typst runs (it is in every shell the gate runs in)");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(out.join("guide.pdf").is_file());
}
