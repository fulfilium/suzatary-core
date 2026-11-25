//! The crate names no page, device or program of the application it came from.

use std::path::Path;

/// Words of the application the layout was made for, or of the devices it was first drawn on.
const FOREIGN: [&str; 12] = [
    "route",
    "phone",
    "browser",
    "suzatary",
    "secretary",
    "todo",
    "todos",
    "bar",
    "desk",
    "inbox",
    "briefing",
    "mail",
];

fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the crate's directory lists") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn the_crate_names_none_of_its_first_applications_words() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut read = vec![root.join("README.md"), root.join("Cargo.toml")];
    files(&root.join("src"), &mut read);
    files(&root.join("examples"), &mut read);
    for path in read {
        let text = std::fs::read_to_string(&path).expect("a file of the crate reads");
        for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
            let word = word.to_ascii_lowercase();
            assert!(
                !FOREIGN.contains(&word.as_str()),
                "{} says {word}",
                path.display()
            );
        }
    }
}
