//! The vault (`scripts/vault-backup.sh`), run as the real script against a fake Drive: a
//! directory a stub `proton-drive` lists, uploads into, trashes into and deletes from, logging
//! every call, with a stub `gpg` that copies the archive through, so the snapshot uploaded is the
//! tar itself and its list can be read back.

use super::{Scratch, repo};
use std::process::{Command, Output};

/// The Drive as a directory under `$FAKE_DRIVE`: `/my-files/...` and `/trash` are its paths.
/// `$FAKE_DRIVE/skip-uploads` makes every upload of a snapshot a silent no-op, as the real CLI
/// is with a same-named file.
const DRIVE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
d="$FAKE_DRIVE"
echo "$*" >>"$d/log"
[ "$1" = filesystem ] || exit 64
shift
case "$1" in
  create-folder) mkdir "$d/$2/$3" ;;
  upload)
    shift
    [ "$1" = -f ] && shift 2
    case "$1" in *.gpg) [ -f "$d/skip-uploads" ] && exit 0 ;; esac
    cp "$1" "$d/$2/" ;;
  list) ls "$d/$2" ;;
  trash) mv "$d/$2" "$d/trash/" ;;
  delete) case "$2" in /trash/*) rm "$d$2" ;; *) exit 65 ;; esac ;;
  *) exit 66 ;;
esac
"#;

/// `gpg -o <out> --encrypt <in>`, as a copy.
const GPG: &str = r#"#!/usr/bin/env bash
while [ $# -gt 1 ]; do [ "$1" = -o ] && out="$2"; shift; done
cp "$1" "$out"
"#;

const BOX: &str = "box";
const REMOTE: &str = "my-files/Backups/box-vault";
/// The box-local stores of words and keys, which only the vault backs up.
const WORDS: [&str; 5] = [
    "secretary-memory-texts",
    "secretary-digest-texts",
    "secretary-lines-keys",
    "secretary-topics",
    "secretary-topic-aliases",
];
/// Which the owner trashed himself: no vault run may delete it.
const THEIRS: &str = "an-owners-file.pdf";

struct Vault {
    scratch: Scratch,
}

impl Vault {
    /// A box with the state the tar list names, and a Drive holding `snapshots` of the vault
    /// beside the recipe, a file of the owner's in the folder, and one in the trash.
    fn new(name: &str, snapshots: &[&str]) -> Self {
        let scratch = Scratch::new(name);
        let (home, data, drive) = (
            scratch.join("home"),
            scratch.join("data"),
            scratch.join("drive"),
        );
        let memory = format!(
            ".claude/projects/{}/memory",
            home.display().to_string().replace('/', "-")
        );
        for dir in [".password-store", ".gnupg", &memory] {
            std::fs::create_dir_all(home.join(dir)).expect("home state");
        }
        for dir in WORDS {
            std::fs::create_dir_all(data.join(dir)).expect("data state");
            std::fs::write(data.join(dir).join("a".repeat(64)), "words").expect("a text");
        }
        std::fs::write(data.join("instance.toml"), "").expect("an instance");
        std::fs::write(data.join("FETCHER"), format!("{BOX}\n")).expect("the fetcher");
        std::fs::write(data.join("VAULT_RECIPIENT"), "0xVAULT\n").expect("the recipient");
        std::fs::create_dir_all(drive.join(REMOTE)).expect("the vault folder");
        std::fs::create_dir_all(drive.join("trash")).expect("the trash");
        for snapshot in snapshots {
            std::fs::write(drive.join(REMOTE).join(snapshot), "old").expect("a snapshot");
        }
        std::fs::write(drive.join(REMOTE).join("notes.txt"), "his").expect("his file");
        std::fs::write(drive.join("trash").join(THEIRS), "his").expect("his trash");
        let bin = scratch.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        for (stub, text) in [("proton-drive", DRIVE), ("gpg", GPG)] {
            let _ = suzatary_testkit::fake_program(&bin, stub, text);
        }
        Vault { scratch }
    }

    fn run(&self) -> Output {
        let path = format!(
            "{}:{}",
            self.scratch.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(repo().join("scripts").join("vault-backup.sh"))
            .env("PATH", path)
            .env("HOME", self.scratch.join("home"))
            .env("SUZATARY_INSTANCE", self.scratch.join("data/instance.toml"))
            .env("PROTON_DRIVE_BIN", self.scratch.join("bin/proton-drive"))
            .env("FAKE_DRIVE", self.scratch.join("drive"))
            .output()
            .expect("the vault runs")
    }

    fn listed(&self, dir: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.scratch.join("drive").join(dir))
            .expect("a Drive folder")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .into_string()
                    .expect("utf8")
            })
            .collect();
        names.sort();
        names
    }

    fn snapshots(&self) -> Vec<String> {
        self.listed(REMOTE)
            .into_iter()
            .filter(|name| name.ends_with(".tar.gz.gpg"))
            .collect()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.scratch.join("drive/log")).unwrap_or_default()
    }
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// Nine older snapshots, a legacy name among them: after a run the Drive holds the newest six
/// and the new one, the rest gone from the folder and from the trash, the owner's own files in
/// both untouched, and the trash never emptied whole.
#[test]
fn vault_keeps_the_newest_seven_and_deletes_the_rest_from_the_trash() {
    let mut old: Vec<String> = (1..=8)
        .map(|day| format!("box-vault-202501{day:02}-041700.tar.gz.gpg"))
        .collect();
    old.push("box-vault-20241231.tar.gz.gpg".to_owned());
    let vault = Vault::new(
        "vault-keeps-seven",
        &old.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let out = vault.run();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let kept = vault.snapshots();
    assert_eq!(kept.len(), 7, "{kept:?}");
    let new = kept
        .iter()
        .find(|name| !old.contains(name))
        .expect("the new snapshot is kept");
    assert!(new.starts_with("box-vault-"), "{new}");
    for day in 3..=8 {
        let name = format!("box-vault-202501{day:02}-041700.tar.gz.gpg");
        assert!(kept.contains(&name), "{name} is pruned: {kept:?}");
    }
    assert!(vault.listed(REMOTE).contains(&"notes.txt".to_owned()));
    assert!(
        vault
            .listed(REMOTE)
            .contains(&"bootstrap-box.sh".to_owned())
    );
    assert_eq!(vault.listed("trash"), vec![THEIRS.to_owned()]);
    let calls = vault.calls();
    assert!(!calls.contains("empty-trash"), "{calls}");
    for gone in [
        "box-vault-20241231.tar.gz.gpg",
        "box-vault-20250101-041700.tar.gz.gpg",
    ] {
        assert!(
            calls.contains(&format!("filesystem delete /trash/{gone}")),
            "{calls}"
        );
    }
    assert!(
        text(&out.stdout).contains("kept 7, deleted 3"),
        "{}",
        text(&out.stdout)
    );
}

/// The snapshot holds what nothing else holds: the memory's facts, the digest's summaries, the
/// keys of locked words and the topics.
#[test]
fn vault_snapshot_holds_the_words_and_the_keys_nothing_else_holds() {
    let vault = Vault::new("vault-words", &[]);
    let out = vault.run();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let snapshot = vault
        .scratch
        .join("drive")
        .join(REMOTE)
        .join(&vault.snapshots()[0]);
    let listed = Command::new("tar")
        .arg("tzf")
        .arg(&snapshot)
        .output()
        .expect("tar lists the snapshot");
    let listed = text(&listed.stdout);
    for words in WORDS {
        assert!(
            listed.contains(&format!("{words}/{}", "a".repeat(64))),
            "{words} is not in the snapshot: {listed}"
        );
    }
}

/// A Drive holding fewer than seven loses none.
#[test]
fn vault_with_fewer_than_seven_deletes_nothing() {
    let vault = Vault::new(
        "vault-fewer",
        &[
            "box-vault-20250101-041700.tar.gz.gpg",
            "box-vault-20250102-041700.tar.gz.gpg",
        ],
    );
    let out = vault.run();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(vault.snapshots().len(), 3);
    assert!(!vault.calls().contains("trash"), "{}", vault.calls());
}

/// An upload the listing does not show fails the run before anything is pruned: a failed
/// snapshot never costs an old one.
#[test]
fn vault_whose_upload_is_not_read_back_prunes_nothing() {
    let old: Vec<String> = (1..=9)
        .map(|day| format!("box-vault-202501{day:02}-041700.tar.gz.gpg"))
        .collect();
    let vault = Vault::new(
        "vault-unread",
        &old.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    std::fs::write(vault.scratch.join("drive/skip-uploads"), "").expect("a skipping Drive");
    let out = vault.run();
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("NOT found"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(vault.snapshots(), old);
    assert!(!vault.calls().contains("trash"), "{}", vault.calls());
}
