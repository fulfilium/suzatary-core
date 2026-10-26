//! The Drive's conformance suite over the real adapter, `ProtonDrive`, behind a fake CLI: a
//! shell script answering the four commands `ProtonDrive` builds over a directory tree, so what
//! the suite keeps is really written, listed and read back through the CLI's own grammar.

use std::path::{Path, PathBuf};

use suzatary_core::conformance::drive::{Case, check};
use suzatary_io::drive::ProtonDrive;

/// The Drive CLI's grammar over `tree`: `list -j` prints the sections at `/` and the SDK's node
/// shape elsewhere, `download` copies one file into an empty directory, `create-folder` makes
/// one, and `upload -f skip -t` leaves a name already there as it is. Anything else exits 99.
fn fake_cli(dir: &Path) -> PathBuf {
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("my-files")).expect("the tree");
    let t = tree.display();
    let script = format!(
        r#"#!/bin/sh
tree='{t}'
echo "$*" >> '{t}/../calls.log'
if [ "$1 $2 $3" = 'filesystem list -j' ] && [ $# -eq 4 ]; then
  if [ "$4" = / ]; then
    sep=''; printf '['
    for d in "$tree"/*; do printf '%s{{"path":"/%s"}}' "$sep" "$(basename "$d")"; sep=','; done
    printf ']\n'; exit 0
  fi
  [ -d "$tree$4" ] || {{ echo "not found: $4" >&2; exit 3; }}
  sep=''; printf '['
  for e in "$tree$4"/*; do
    [ -e "$e" ] || continue
    n=$(basename "$e")
    if [ -d "$e" ]; then
      printf '%s{{"name":{{"ok":true,"value":"%s"}},"type":"folder"}}' "$sep" "$n"
    else
      printf '%s{{"name":{{"ok":true,"value":"%s"}},"type":"file","activeRevision":{{"claimedSize":%s}}}}' "$sep" "$n" "$(wc -c < "$e" | tr -d ' ')"
    fi
    sep=','
  done
  printf ']\n'
elif [ "$1 $2" = 'filesystem download' ] && [ $# -eq 4 ]; then
  [ -f "$tree$3" ] || {{ echo "cannot download $3" >&2; exit 4; }}
  cp "$tree$3" "$4/$(basename "$3")"
elif [ "$1 $2" = 'filesystem create-folder' ] && [ $# -eq 4 ]; then
  [ -d "$tree$3" ] || {{ echo "no folder $3" >&2; exit 5; }}
  mkdir -p "$tree$3/$4"
elif [ "$1 $2 $3 $4 $5" = 'filesystem upload -f skip -t' ] && [ $# -eq 7 ]; then
  [ -d "$tree$7" ] || {{ echo "no folder $7" >&2; exit 6; }}
  [ -e "$tree$7/$(basename "$6")" ] || cp "$6" "$tree$7/"
else
  echo "refused: $*" >&2; exit 99
fi
"#
    );
    suzatary_testkit::fake_program(dir, "proton-drive", &script)
}

fn case(folder: &str, name: &str, bytes: &[u8]) -> Case {
    Case {
        folder: folder.parse().expect("a Drive path"),
        name: name.to_owned(),
        bytes: bytes.to_vec(),
    }
}

#[test]
fn the_proton_drive_adapter_keeps_every_law_behind_a_fake_cli() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    let drive = ProtonDrive::new(
        fake_cli(dir.path()),
        dir.path().join("scratch"),
        PathBuf::from("."),
    );
    let binary: Vec<u8> = (0..=255).collect();
    let cases = [
        case("/my-files/case-a", "f-letter.txt", b"Dear Ada,\n"),
        case("/my-files/case-b/deeper", "f-a scan.jpg", &binary),
        case("/my-files/case-c", "f-empty", b""),
    ];
    assert_eq!(check(&drive, &cases), Ok(()));
    assert!(
        std::fs::read_dir(dir.path().join("scratch"))
            .into_iter()
            .flatten()
            .next()
            .is_none(),
        "every download and upload left nothing behind"
    );
}

/// The private folder is the private folder whatever its case: a Drive that matches names
/// case-insensitively would read `/my-files/Suzatary/PRIVATE` as it, so the fence folds case
/// for it, and the CLI is never asked to list or download under any spelling of it.
#[test]
fn the_private_folder_is_fenced_whatever_its_case() {
    use suzatary_core::drive::{DrivePath, Fenced, Port as _};

    let dir = tempfile::tempdir().expect("a scratch dir");
    let drive = ProtonDrive::new(
        fake_cli(dir.path()),
        dir.path().join("scratch"),
        PathBuf::from("."),
    );
    let shouted = dir.path().join("tree/my-files/Suzatary/PRIVATE");
    std::fs::create_dir_all(&shouted).expect("a private folder, spelt otherwise");
    std::fs::write(shouted.join("w2.pdf"), b"%PDF w2").expect("a private file");
    let path = |text: &str| -> DrivePath { text.parse().expect("a Drive path") };
    let calls = || {
        std::fs::read_to_string(dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let fenced = Fenced::new(drive.read_only(), vec![path("/my-files")]);
    for spelling in ["/my-files/Suzatary/PRIVATE", "/My-Files/suzatary/private"] {
        assert!(fenced.list(&path(spelling)).is_err(), "{spelling} listed");
        assert!(
            fenced
                .download(&path(&format!("{spelling}/w2.pdf")))
                .is_err(),
            "{spelling} read"
        );
    }
    assert!(calls().is_empty(), "the CLI was asked: {:?}", calls());
    let above = fenced
        .list(&path("/my-files/Suzatary"))
        .expect("the folder above lists");
    assert!(
        above.is_empty(),
        "the private folder is not shown: {above:?}"
    );
    assert_eq!(calls(), ["filesystem list -j /my-files/Suzatary"]);
}
