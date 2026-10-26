//! The owner's Drive through the Proton Drive CLI, as a [`Port`]: every invocation is built
//! from `Command`, a closed enum of the port's two reads and two writes. The writes serve
//! `Port::keep` alone, which only the owner's confirm on a private upload reaches
//! (`suzatary_core::vault`).
//!
//! A download lands in a private scratch directory removed before returning; a read extracts
//! its text as a mail attachment's is, and redacts it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use suzatary_core::capability::DriveFailure;
use suzatary_core::drive::{
    DriveEntry, DrivePath, EntryKind, MAX_FILE, Port, WritePort, extension_of, mime_of,
};
use suzatary_core::file_read::FileRead;

use crate::run::{Cmd, RunError, succeed};
use crate::scratch::Scratch;

/// The most of a listing read from the CLI.
const MAX_LISTING: usize = 4 * 1024 * 1024;

/// How long a listing may take.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a download may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(2);

/// The only commands this program runs: the port's two reads and two writes.
#[derive(Debug, Clone, Copy)]
enum Command<'a> {
    /// `filesystem list -j <folder>`.
    List(&'a DrivePath),
    /// `filesystem download <file> <into>`, into an empty local directory.
    Download { file: &'a DrivePath, into: &'a Path },
    /// `filesystem create-folder <parent> <name>`.
    CreateFolder {
        parent: &'a DrivePath,
        name: &'a str,
    },
    /// `filesystem upload -f skip -t <file> <parent>`: a file already there with the same
    /// name is left as it is, since the name carries the content's digest.
    Upload {
        file: &'a Path,
        parent: &'a DrivePath,
    },
}

impl Command<'_> {
    fn argv(&self) -> Vec<OsString> {
        match self {
            Command::List(folder) => vec![
                "filesystem".into(),
                "list".into(),
                "-j".into(),
                folder.to_string().into(),
            ],
            Command::Download { file, into } => vec![
                "filesystem".into(),
                "download".into(),
                file.to_string().into(),
                into.as_os_str().to_owned(),
            ],
            Command::CreateFolder { parent, name } => vec![
                "filesystem".into(),
                "create-folder".into(),
                parent.to_string().into(),
                (*name).into(),
            ],
            Command::Upload { file, parent } => vec![
                "filesystem".into(),
                "upload".into(),
                "-f".into(),
                "skip".into(),
                "-t".into(),
                file.as_os_str().to_owned(),
                parent.to_string().into(),
            ],
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            Command::List(_) | Command::CreateFolder { .. } => LIST_TIMEOUT,
            Command::Download { .. } | Command::Upload { .. } => DOWNLOAD_TIMEOUT,
        }
    }
}

/// The Drive CLI at one path, with downloads landing under one root.
#[derive(Debug, Clone)]
pub struct ProtonDrive {
    cli: PathBuf,
    scratch: PathBuf,
    pdftotext: crate::pdftotext::Pdftotext,
    timeout: Option<Duration>,
}

impl ProtonDrive {
    /// The CLI at `cli`, downloading under `scratch` (a directory per read,
    /// private to this user, removed when the read ends). `repo` is
    /// `pdftotext`'s nix fallback, as for mail attachments.
    #[must_use]
    pub fn new(cli: PathBuf, scratch: PathBuf, repo: PathBuf) -> Self {
        Self {
            cli,
            scratch,
            pdftotext: crate::pdftotext::Pdftotext::new(repo),
            timeout: None,
        }
    }

    /// This Drive, for reading alone: what a holder that must never write is given.
    #[must_use]
    pub fn read_only(self) -> ReadOnly {
        ReadOnly(self)
    }

    /// Every command bounded by `timeout` instead of its own default.
    #[must_use]
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            ..self
        }
    }

    /// One file read for the model, its text redacted; the download is gone when this returns.
    /// A type no model reads is refused by its extension before anything is downloaded.
    ///
    /// # Errors
    /// As `Port::entry`, then the CLI's or `pdftotext`'s failures.
    pub fn read(&self, file: &DrivePath) -> Result<FileRead, DriveFailure> {
        self.entry(file)?;
        let name = file.name().unwrap_or_default();
        let Some(mime) = mime_of(name) else {
            return Ok(FileRead::Unsupported {
                what: format!("a .{} file", extension_of(name)),
            });
        };
        let bytes = self.download(file)?;
        crate::mailbox::read_of(mime, bytes, &self.pdftotext).map_err(DriveFailure::Unreachable)
    }

    /// Run one command, bounded, and answer its standard output.
    fn run(&self, command: &Command<'_>) -> Result<Vec<u8>, DriveFailure> {
        let timeout = self.timeout.unwrap_or_else(|| command.timeout());
        succeed(
            &Cmd::new(&self.cli)
                .args(command.argv())
                .pass(crate::run::NETWORK)
                // The Drive CLI keeps its session in the secret store.
                .pass(crate::secrets::STORE)
                .timeout(timeout)
                .max_stdout(MAX_LISTING),
        )
        .map(|ran| ran.stdout)
        .map_err(|failed| match failed {
            RunError::Spawn { source, .. } => DriveFailure::Unreachable(format!(
                "the Drive CLI at {} could not be run: {source}",
                self.cli.display()
            )),
            RunError::TimedOut { .. } => DriveFailure::Unreachable(format!(
                "the Drive CLI did not finish within {timeout:?}"
            )),
            RunError::Exited { ref stderr, .. } if asks_for_sign_in(stderr.as_str()) => {
                DriveFailure::SignedOut(failed.to_string())
            }
            other => DriveFailure::Unreachable(other.to_string()),
        })
    }
}

/// Whether the CLI's own words say its session expired or was never begun, which its sign-in
/// at a terminal mends. Read once, where the CLI's exit is observed.
fn asks_for_sign_in(stderr: &str) -> bool {
    let said = stderr.to_ascii_lowercase();
    [
        "session expired",
        "session has expired",
        "not logged in",
        "not signed in",
        "not authenticated",
        "unauthenticated",
        "auth login",
        "to sign in again",
        "log in again",
    ]
    .iter()
    .any(|asks| said.contains(asks))
}

impl Port for ProtonDrive {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        let out = self.run(&Command::List(folder))?;
        entries(folder, &out).map_err(DriveFailure::Unreachable)
    }

    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        let scratch = scratch(&self.scratch)?;
        self.run(&Command::Download {
            file,
            into: scratch.path(),
        })?;
        only_file(scratch.path())
    }
}

/// The two writes a private upload needs, which only the vault holds.
impl WritePort for ProtonDrive {
    fn create_folder(&self, parent: &DrivePath, name: &str) -> Result<(), DriveFailure> {
        parent
            .join(name)
            .map_err(|failed| DriveFailure::Refused(failed.to_string()))?;
        self.run(&Command::CreateFolder { parent, name }).map(drop)
    }

    fn upload(&self, parent: &DrivePath, name: &str, bytes: &[u8]) -> Result<(), DriveFailure> {
        parent
            .join(name)
            .map_err(|failed| DriveFailure::Refused(failed.to_string()))?;
        let scratch = scratch(&self.scratch)?;
        let local = scratch.path().join(name);
        std::fs::write(&local, bytes)
            .map_err(|failed| DriveFailure::Unreachable(format!("no scratch space: {failed}")))?;
        self.run(&Command::Upload {
            file: &local,
            parent,
        })
        .map(drop)
    }
}

/// The Drive CLI with its writes out of reach: a [`Port`] and the model's read, and no
/// [`WritePort`], so whatever holds one (the tap's performer, the secretary's record) cannot
/// write, and the vault is the one holder of a `ProtonDrive` that can.
#[derive(Debug, Clone)]
pub struct ReadOnly(ProtonDrive);

impl ReadOnly {
    /// One file read for the model, as [`ProtonDrive::read`].
    ///
    /// # Errors
    /// As [`ProtonDrive::read`].
    pub fn read(&self, file: &DrivePath) -> Result<FileRead, DriveFailure> {
        self.0.read(file)
    }
}

impl Port for ReadOnly {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        self.0.list(folder)
    }

    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        self.0.download(file)
    }
}

/// A listing's entries, from the CLI's `-j` output: one JSON object per
/// entry. The top level answers `{"path": …}` per section; a folder answers
/// the node itself, whose `name` is `{"ok": true, "value": …}`, `type` is
/// `file` or `folder`, and size is its active revision's `claimedSize`. An
/// entry whose name cannot be read, or is not one path segment, is left out.
///
/// # Errors
/// A sentence when the output is not a JSON array.
pub fn entries(folder: &DrivePath, json: &[u8]) -> Result<Vec<DriveEntry>, String> {
    let value: serde_json::Value = serde_json::from_slice(json)
        .map_err(|failed| format!("the Drive CLI's listing is not JSON: {failed}"))?;
    let rows = value
        .as_array()
        .ok_or_else(|| "the Drive CLI's listing is not a list".to_owned())?;
    Ok(rows.iter().filter_map(|row| entry(folder, row)).collect())
}

fn entry(folder: &DrivePath, row: &serde_json::Value) -> Option<DriveEntry> {
    if let Some(path) = row.get("path").and_then(serde_json::Value::as_str) {
        let path: DrivePath = path.parse().ok()?;
        return path.is_under(folder).then_some(DriveEntry {
            path,
            kind: EntryKind::Folder,
            size: None,
            modified: None,
        });
    }
    let name = match row.get("name")? {
        serde_json::Value::String(name) => name.as_str(),
        named => named
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .filter(|ok| *ok)
            .and_then(|_| named.get("value"))
            .and_then(serde_json::Value::as_str)?,
    };
    let path = folder.join(name).ok()?;
    let kind = match row.get("type").and_then(serde_json::Value::as_str) {
        Some("file") => EntryKind::File,
        Some("folder") => EntryKind::Folder,
        Some(other) => EntryKind::Other(other.to_owned()),
        None => EntryKind::Other("entry of unknown type".to_owned()),
    };
    let revision = row.get("activeRevision");
    let size = revision
        .and_then(|revision| revision.get("claimedSize"))
        .or_else(|| row.get("totalStorageSize"))
        .or_else(|| row.get("size"))
        .and_then(serde_json::Value::as_u64);
    let modified = row
        .get("modificationTime")
        .or_else(|| revision.and_then(|revision| revision.get("claimedModificationTime")))
        .or_else(|| row.get("creationTime"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Some(DriveEntry {
        path,
        kind,
        size,
        modified,
    })
}

/// A private directory for one download, removed with everything in it when
/// dropped: a document is on disk only for the length of the call.
fn scratch(root: &Path) -> Result<Scratch, DriveFailure> {
    Scratch::under(root, "suzatary-drive-")
        .map_err(|failed| DriveFailure::Unreachable(format!("no scratch space: {failed}")))
}

/// The one regular file a download left in `dir`, read whole. A link, a
/// folder, nothing, or more than one entry is refused.
fn only_file(dir: &Path) -> Result<Vec<u8>, DriveFailure> {
    let unreadable =
        |failed: std::io::Error| DriveFailure::Unreachable(format!("the download: {failed}"));
    let found: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(unreadable)?
        .collect::<Result<_, _>>()
        .map_err(unreadable)?;
    let [one] = found.as_slice() else {
        return Err(DriveFailure::Refused(format!(
            "the download left {} entries where one file was expected (a Proton Doc or \
                 Sheet downloads as nothing)",
            found.len()
        )));
    };
    let meta = std::fs::symlink_metadata(one.path()).map_err(unreadable)?;
    if !meta.file_type().is_file() {
        return Err(DriveFailure::Refused(
            "the download is not a plain file".to_owned(),
        ));
    }
    if meta.len() > MAX_FILE {
        return Err(DriveFailure::Refused(format!(
            "the download is larger than {} MB, and is not read",
            MAX_FILE / 1024 / 1024
        )));
    }
    std::fs::read(one.path()).map_err(unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> DrivePath {
        text.parse().expect("a Drive path")
    }

    /// The shape `filesystem list -j` printed for a folder, one node per
    /// line, as the SDK serialises it.
    const LISTING: &str = r#"[
{"uid":"u1","name":{"ok":true,"value":"letter.txt"},"type":"file","mediaType":"text/plain","creationTime":"2026-09-01T10:00:00.000Z","activeRevision":{"uid":"r1","claimedSize":120,"claimedModificationTime":"2026-09-01T09:00:00.000Z"}},
{"uid":"u2","name":{"ok":true,"value":"Tax"},"type":"folder","creationTime":"2026-08-01T10:00:00.000Z"},
{"uid":"u3","name":{"ok":true,"value":"lease.jpg"},"type":"file","activeRevision":{"claimedSize":2048}},
{"uid":"u4","name":{"ok":true,"value":"huge.pdf"},"type":"file","activeRevision":{"claimedSize":999999999}},
{"uid":"u5","name":{"ok":false,"error":{"name":"undecryptable"}},"type":"file"},
{"uid":"u6","name":{"ok":true,"value":"../escape"},"type":"file"},
{"uid":"u7","name":{"ok":true,"value":"lease.docx"},"type":"file","activeRevision":{"claimedSize":4096}}
]"#;

    /// A fake CLI that logs every call and fails on anything but the two reads.
    struct Fake {
        dir: PathBuf,
        cli: PathBuf,
    }

    impl Fake {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("suzatary-io-drive-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("files")).expect("a fixture dir");
            std::fs::write(dir.join("listing.json"), LISTING).expect("the listing");
            std::fs::write(
                dir.join("root.json"),
                "[\n{\"path\":\"/my-files\"},\n{\"path\":\"/trash\"}\n]\n",
            )
            .expect("the root");
            std::fs::write(
                dir.join("files/letter.txt"),
                "Dear Ada,\nSSN: 123-45-6789\nPhone 617-555-1234\n",
            )
            .expect("a letter");
            std::fs::write(dir.join("files/lease.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 1, 2])
                .expect("a scan");
            let d = dir.display();
            let script = format!(
                "#!/bin/sh\n\
                 echo \"$*\" >> '{d}/calls.log'\n\
                 if [ \"$1 $2 $3\" = 'filesystem list -j' ] && [ $# -eq 4 ]; then\n\
                   case \"$4\" in\n\
                     /) cat '{d}/root.json' ;;\n\
                     /my-files/Docs) cat '{d}/listing.json' ;;\n\
                     /my-files/Slow) exec sleep 5 ;;\n\
                     /my-files/Expired) echo \"session expired. Run 'proton-drive auth login' to sign in again.\" >&2; exit 1 ;;\n\
                     /my-files|/my-files/Suzatary) echo '[]' ;;\n\
                     *) echo \"not found: $4\" >&2; exit 3 ;;\n\
                   esac\n\
                 elif [ \"$1 $2\" = 'filesystem download' ] && [ $# -eq 4 ]; then\n\
                   case \"$3\" in\n\
                     /my-files/Docs/letter.txt) cp '{d}/files/letter.txt' \"$4/letter.txt\" ;;\n\
                     /my-files/Docs/lease.jpg) cp '{d}/files/lease.jpg' \"$4/lease.jpg\" ;;\n\
                     *) echo \"cannot download $3\" >&2; exit 4 ;;\n\
                   esac\n\
                 elif [ \"$1 $2\" = 'filesystem create-folder' ] && [ $# -eq 4 ]; then\n\
                   :\n\
                 elif [ \"$1 $2 $3 $4 $5\" = 'filesystem upload -f skip -t' ] && [ $# -eq 7 ]; then\n\
                   mkdir -p '{d}/uploaded' && cp \"$6\" '{d}/uploaded/'\n\
                 else\n\
                   echo \"refused: $*\" >&2; exit 99\n\
                 fi\n"
            );
            let cli = suzatary_testkit::fake_program(&dir, "proton-drive", &script);
            Self { dir, cli }
        }

        fn drive(&self) -> ProtonDrive {
            ProtonDrive::new(
                self.cli.clone(),
                self.dir.join("scratch"),
                PathBuf::from("."),
            )
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.join("calls.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        /// Every call the fake saw was one of the two reads.
        fn only_reads_ran(&self) {
            let calls = self.calls();
            assert!(!calls.is_empty(), "the CLI was called");
            for call in &calls {
                assert!(
                    call.starts_with("filesystem list -j /")
                        || call.starts_with("filesystem download /"),
                    "a command other than a read ran: {call}"
                );
            }
        }

        fn scratch_is_empty(&self) -> bool {
            std::fs::read_dir(self.dir.join("scratch"))
                .into_iter()
                .flatten()
                .next()
                .is_none()
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn the_only_commands_are_two_reads_and_the_two_writes_a_private_upload_needs() {
        let folder = path("/my-files");
        let into = Path::new("/tmp/x");
        for command in [
            Command::List(&folder),
            Command::Download {
                file: &folder,
                into,
            },
            Command::CreateFolder {
                parent: &folder,
                name: "Suzatary",
            },
            Command::Upload {
                file: into,
                parent: &folder,
            },
        ] {
            let argv: Vec<String> = command
                .argv()
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            assert!(
                ["list", "download", "create-folder", "upload"].contains(&argv[1].as_str())
                    && argv[0] == "filesystem",
                "{argv:?}"
            );
        }
    }

    /// A private file's folder is made from the section down, the upload never replaces a file
    /// already there, and nothing is left on the box.
    #[test]
    fn keep_makes_the_folder_uploads_the_file_and_leaves_no_copy() {
        let fake = Fake::new("keep");
        let kept = fake
            .drive()
            .keep(
                &suzatary_core::vault::folder(),
                "1a2b3c4d w2.pdf",
                b"%PDF-1.7 w2",
            )
            .expect("kept");
        assert_eq!(kept, path("/my-files/Suzatary/Private/1a2b3c4d w2.pdf"));
        let calls = fake.calls();
        assert_eq!(
            calls,
            [
                "filesystem list -j /",
                "filesystem list -j /my-files",
                "filesystem create-folder /my-files Suzatary",
                "filesystem list -j /my-files/Suzatary",
                "filesystem create-folder /my-files/Suzatary Private",
            ]
            .iter()
            .map(|call| (*call).to_owned())
            .chain(calls.last().cloned())
            .collect::<Vec<_>>()
        );
        let upload = calls.last().expect("the upload");
        assert!(
            upload.starts_with("filesystem upload -f skip -t ")
                && upload.ends_with("/1a2b3c4d w2.pdf /my-files/Suzatary/Private"),
            "{upload}"
        );
        assert_eq!(
            std::fs::read(fake.dir.join("uploaded/1a2b3c4d w2.pdf")).expect("uploaded"),
            b"%PDF-1.7 w2"
        );
        assert!(fake.scratch_is_empty(), "the local copy was removed");
        assert!(
            fake.drive()
                .keep(&suzatary_core::vault::folder(), "a/b.pdf", b"%PDF")
                .is_err(),
            "a name is one path segment"
        );
    }

    #[test]
    fn a_listing_reads_names_kinds_sizes_and_dates() {
        let listed = entries(&path("/my-files/Docs"), LISTING.as_bytes()).expect("a listing");
        let names: Vec<String> = listed.iter().map(|entry| entry.path.to_string()).collect();
        assert_eq!(
            names,
            [
                "/my-files/Docs/letter.txt",
                "/my-files/Docs/Tax",
                "/my-files/Docs/lease.jpg",
                "/my-files/Docs/huge.pdf",
                "/my-files/Docs/lease.docx",
            ],
            "an unreadable name and a path-like name are left out"
        );
        assert_eq!(listed[0].kind, EntryKind::File);
        assert_eq!(listed[0].size, Some(120));
        assert_eq!(
            listed[0].modified.as_deref(),
            Some("2026-09-01T09:00:00.000Z")
        );
        assert_eq!(listed[1].kind, EntryKind::Folder);
        assert_eq!(listed[1].size, None);
    }

    #[test]
    fn a_file_is_read_redacted_and_its_download_removed() {
        let fake = Fake::new("read");
        let read = fake
            .drive()
            .read(&path("/my-files/Docs/letter.txt"))
            .expect("read");
        let FileRead::Text {
            text,
            truncated,
            withheld: None,
        } = read
        else {
            panic!("text expected: {read:?}");
        };
        assert!(!truncated);
        assert!(text.as_str().contains("SSN: [redacted]"), "{text:?}");
        assert!(!text.as_str().contains("6789"), "{text:?}");
        assert!(text.as_str().contains("617-555-1234"), "a phone stays");
        assert!(fake.scratch_is_empty(), "the download was removed");
        fake.only_reads_ran();
    }

    /// A fetch is the file's bytes unredacted, for a message the owner sees and taps.
    #[test]
    fn a_fetch_is_the_file_s_bytes_and_its_download_removed() {
        let fake = Fake::new("fetch");
        let bytes = fake
            .drive()
            .fetch(&path("/my-files/Docs/letter.txt"))
            .expect("fetched");
        assert_eq!(bytes, b"Dear Ada,\nSSN: 123-45-6789\nPhone 617-555-1234\n");
        assert!(fake.scratch_is_empty(), "the download was removed");
        assert!(
            fake.drive().fetch(&path("/my-files/Docs")).is_err(),
            "a folder is no file"
        );
        fake.only_reads_ran();
    }

    /// An image is handed over as itself, and its download removed.
    #[test]
    fn an_image_is_read_as_the_file_itself() {
        let fake = Fake::new("image");
        let read = fake
            .drive()
            .read(&path("/my-files/Docs/lease.jpg"))
            .expect("read");
        let FileRead::File(readable) = read else {
            panic!("the file itself: {read:?}");
        };
        assert_eq!(readable.bytes(), [0xFF, 0xD8, 0xFF, 0xE0, 1, 2]);
        assert!(fake.scratch_is_empty(), "the download was removed");
        fake.only_reads_ran();
    }

    #[test]
    fn a_folder_an_unread_type_a_huge_file_and_a_missing_one_are_refused_without_a_download() {
        let fake = Fake::new("refused");
        let drive = fake.drive();
        assert!(matches!(
            drive.read(&path("/my-files/Docs/Tax")),
            Err(DriveFailure::Refused(_))
        ));
        assert_eq!(
            drive.read(&path("/my-files/Docs/lease.docx")),
            Ok(FileRead::Unsupported {
                what: "a .docx file".to_owned()
            })
        );
        assert!(matches!(
            drive.read(&path("/my-files/Docs/huge.pdf")),
            Err(DriveFailure::Refused(_))
        ));
        assert!(matches!(
            drive.read(&path("/my-files/Docs/absent.txt")),
            Err(DriveFailure::Missing(_))
        ));
        assert!(matches!(
            drive.read(&DrivePath::root()),
            Err(DriveFailure::Refused(_))
        ));
        assert!(
            !fake.calls().iter().any(|call| call.contains("download")),
            "nothing was downloaded: {:?}",
            fake.calls()
        );
        fake.only_reads_ran();
    }
}
