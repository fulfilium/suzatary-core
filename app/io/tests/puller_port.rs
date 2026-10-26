//! Mail-in's conformance suite over the four real pullers, each behind a fake far end: a
//! scripted IMAP server under the `imap` crate's own client for Microsoft, and shell scripts
//! answering mbsync's and lieer's command lines over a directory that stands for the server,
//! lieer's on the Google grant minting its token from a scripted token endpoint. What lands is
//! read back from the maildir on disk.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use suzatary_core::capability::Secret;
use suzatary_core::conformance::puller::{Rig, check};
use suzatary_core::connect::Failure;
use suzatary_core::mail::account::{Authority, MailAccount, MsImap, Sync};
use suzatary_core::mail::account_email::AccountEmail;
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::mail::folders::Folders;
use suzatary_core::mail::imap::ImapError;
use suzatary_core::mail::mailroom::Server;
use suzatary_io::google::{Google as Api, Grants};
use suzatary_io::http::{HttpError, Resp, Scripted};
use suzatary_io::imap::Outlook;
use suzatary_io::pullers::{Google, Lieer, Mbsync, Microsoft, Open};
use suzatary_io::secrets::{Resolves, SecretError};

const ADA: &str = "ada@example.test";

fn since() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 1).expect("a date")
}

fn arrivals() -> Vec<Vec<Vec<u8>>> {
    let message = |id: &str, body: &str| {
        format!(
            "Message-ID: <{id}@example.test>\r\nDate: Mon, 05 Oct 2026 10:00:00 +0000\r\n\
             Subject: {id}\r\n\r\n{body}\r\n"
        )
        .into_bytes()
    };
    vec![
        vec![message("one", "The lease renews."), message("two", "")],
        vec![],
        vec![message("three", "A line.\r\n\r\nAnd one after a blank.")],
    ]
}

/// Every file under the maildir leaf `leaf`'s `new/` and `cur/`.
fn landed(leaf: &Path) -> Vec<Vec<u8>> {
    ["new", "cur"]
        .iter()
        .flat_map(|half| read(&leaf.join(half)))
        .collect()
}

/// Every file in `dir`, none when it is not there.
fn read(dir: &Path) -> Vec<Vec<u8>> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|file| std::fs::read(file.path()).expect("a message"))
        .collect()
}

/// The far end as a directory, one file per message in the order it arrived; a server that
/// cannot be reached has none.
fn serve(far: &Path, server: &Server) {
    let _ = std::fs::remove_dir_all(far);
    match server {
        Server::Holding(messages) => {
            std::fs::create_dir_all(far).expect("the far end");
            for (at, message) in messages.iter().enumerate() {
                std::fs::write(far.join(format!("{at:04}")), message).expect("a message");
            }
        }
        Server::Unreachable => {}
    }
}

/// Copies each message of the far end `far` the maildir leaf `leaf` lacks into its `half`.
fn copying(far: &Path, leaf: &str, half: &str) -> String {
    format!(
        r#"[ -d '{far}' ] || {{ echo "cannot reach the server" >&2; exit 1; }}
mkdir -p "{leaf}/new" "{leaf}/cur" "{leaf}/tmp"
for f in '{far}'/*; do
  [ -e "$f" ] || continue
  n=$(basename "$f")
  [ -e "{leaf}/new/$n" ] || [ -e "{leaf}/cur/$n" ] || {{ cp "$f" "{leaf}/tmp/$n" && mv "{leaf}/tmp/$n" "{leaf}/{half}/$n"; }}
done
echo synced"#,
        far = far.display()
    )
}

// --- mbsync, from the Bridge ----------------------------------------------------

/// mbsync's command line, `-c <config> <channel>`, reading the far end and the maildir leaf
/// from the config; anything else exits 99.
struct Bridge(tempfile::TempDir, PathBuf);

impl Bridge {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a scratch dir");
        std::fs::write(
            dir.path().join("mbsyncrc"),
            format!(
                "Channel ada\nNear {}\n",
                dir.path().join("Mail/ada/Inbox").display()
            ),
        )
        .expect("the config");
        let far = dir.path().join("far");
        let program = suzatary_testkit::fake_program(
            dir.path(),
            "mbsync",
            &format!(
                "#!/bin/sh\n[ \"$1\" = -c ] && [ $# -eq 3 ] || {{ echo \"refused: $*\" >&2; exit 99; }}\n\
                 grep -qx \"Channel $3\" \"$2\" || {{ echo \"no channel $3\" >&2; exit 98; }}\n\
                 near=$(sed -n 's/^Near //p' \"$2\")\n{}\n",
                copying(&far, "$near", "new")
            ),
        );
        Self(dir, program)
    }

    fn puller(&self) -> Mbsync {
        Mbsync {
            program: self.1.clone(),
            config: self.0.path().join("mbsyncrc"),
            channel: "ada".to_owned(),
        }
    }
}

impl Rig for Bridge {
    type Puller = Mbsync;

    const UNREACHABLE: Failure = Failure::Unclassified;

    fn holding(&self, messages: &[Vec<u8>]) -> Mbsync {
        serve(
            &self.0.path().join("far"),
            &Server::Holding(messages.to_vec()),
        );
        self.puller()
    }

    fn unreachable(&self) -> Mbsync {
        serve(&self.0.path().join("far"), &Server::Unreachable);
        self.puller()
    }

    fn landed(&self) -> Vec<Vec<u8>> {
        landed(&self.0.path().join("Mail/ada/Inbox"))
    }
}

#[test]
fn mbsync_keeps_every_law_behind_a_fake_bridge() {
    assert_eq!(check(&Bridge::new(), since(), &arrivals()), Ok(()));
}

// --- lieer, from Gmail ---------------------------------------------------------

/// lieer's command lines in the account's maildir: `sync` runs `first`, then copies the far end
/// into `mail/cur`, and `init --no-auth <account>` begins the repository; anything else exits 99.
fn gmi(dir: &Path, first: &str) -> PathBuf {
    suzatary_testkit::fake_program(
        dir,
        "gmi",
        &format!(
            "#!/bin/sh\nif [ \"$*\" = sync ]; then\n{first}\n{}\n\
             elif [ \"$*\" = 'init --no-auth {ADA}' ]; then\n  echo '{{}}' > .gmailieer.json\n\
             else\n  echo \"refused: $*\" >&2; exit 99\nfi\n",
            copying(&dir.join("far"), "mail", "cur")
        ),
    )
}

struct Gmail(tempfile::TempDir);

impl Gmail {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a scratch dir");
        gmi(dir.path(), ":");
        std::fs::create_dir_all(dir.path().join("Mail/ada")).expect("the maildir");
        Self(dir)
    }

    fn puller(&self) -> Lieer {
        Lieer {
            program: self.0.path().join("gmi"),
            maildir: self.0.path().join("Mail/ada"),
        }
    }
}

impl Rig for Gmail {
    type Puller = Lieer;

    const UNREACHABLE: Failure = Failure::Unclassified;

    fn holding(&self, messages: &[Vec<u8>]) -> Lieer {
        serve(
            &self.0.path().join("far"),
            &Server::Holding(messages.to_vec()),
        );
        self.puller()
    }

    fn unreachable(&self) -> Lieer {
        serve(&self.0.path().join("far"), &Server::Unreachable);
        self.puller()
    }

    fn landed(&self) -> Vec<Vec<u8>> {
        landed(&self.0.path().join("Mail/ada/mail"))
    }
}

#[test]
fn lieer_keeps_every_law_behind_a_fake_gmail() {
    assert_eq!(check(&Gmail::new(), since(), &arrivals()), Ok(()));
}

// --- lieer, on the box's Google grant -------------------------------------------

/// A secret store holding the box's Google client and Ada's grant.
struct Kept;

impl Resolves for Kept {
    fn get(&self, name: &str, _reason: &str) -> Result<Option<Secret>, SecretError> {
        let grant = suzatary_core::google::grant_name(&address());
        Ok(match name {
            suzatary_core::google::CLIENT_ID => Some("box.apps.googleusercontent.com"),
            suzatary_core::google::CLIENT_SECRET => Some("box-client-secret"),
            _ if name == grant => Some("mail,calendar 1//fake-refresh"),
            _ => None,
        }
        .map(Secret::new))
    }
}

fn address() -> EmailAddress {
    EmailAddress::new(ADA).expect("an address")
}

const MINTED: &str = "box-minted-token";

struct Signed(tempfile::TempDir);

impl Signed {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a scratch dir");
        // lieer reads its mail on the token the box handed it, and on nothing else.
        gmi(
            dir.path(),
            &format!(
                "grep -q '{MINTED}' .credentials.gmailieer.json || \
                 {{ echo 'no token handed' >&2; exit 3; }}"
            ),
        );
        Self(dir)
    }

    fn puller(&self, token: Result<Resp, HttpError>) -> Google {
        Google {
            lieer: Lieer {
                program: self.0.path().join("gmi"),
                maildir: self.0.path().join("Mail/ada"),
            },
            account: address(),
            grants: Grants::new(
                Api::with_http(
                    suzatary_core::google::Endpoints::at("https://google.example.test"),
                    Arc::new(Scripted::always(token)),
                ),
                Arc::new(Kept),
            ),
        }
    }
}

impl Rig for Signed {
    type Puller = Google;

    const UNREACHABLE: Failure = Failure::Unclassified;

    fn holding(&self, messages: &[Vec<u8>]) -> Google {
        serve(
            &self.0.path().join("far"),
            &Server::Holding(messages.to_vec()),
        );
        self.puller(Ok(Resp {
            status: 200,
            headers: Vec::new(),
            body: serde_json::json!({"access_token": MINTED, "expires_in": 3600})
                .to_string()
                .into_bytes(),
        }))
    }

    fn unreachable(&self) -> Google {
        self.puller(Err(HttpError::NotSent(
            "no route to Google's token endpoint".to_owned(),
        )))
    }

    fn landed(&self) -> Vec<Vec<u8>> {
        landed(&self.0.path().join("Mail/ada/mail"))
    }
}

#[test]
fn lieer_on_the_google_grant_keeps_every_law_behind_a_fake_gmail() {
    assert_eq!(check(&Signed::new(), since(), &arrivals()), Ok(()));
}

// --- Microsoft, over IMAP -------------------------------------------------------

/// The one message the fake server's Sent folder holds.
const SENT: &[u8] = b"Message-ID: <sent@example.test>\r\nSubject: Re: the lease\r\n\r\nThanks.\r\n";

/// An IMAP server whose INBOX holds `inbox`, UID `n` its `n`th message, and whose Sent folder
/// holds [`SENT`]; `selected` holds the selected folder's messages, and a command it does not
/// know is `BAD`.
struct Imap {
    inbox: Vec<Vec<u8>>,
    selected: Vec<Vec<u8>>,
    written: Vec<u8>,
    answer: VecDeque<u8>,
}

impl Imap {
    fn say(&mut self, text: &[u8]) {
        self.answer.extend(text);
    }

    fn command(&mut self, line: &str) {
        let (tag, command) = line.split_once(' ').expect("a tagged command");
        let ok = format!("{tag} OK done\r\n");
        if command.starts_with("LOGIN ") || command.starts_with("UID STORE ") {
            self.say(ok.as_bytes());
        } else if command == "SELECT \"INBOX\"" {
            self.selected = self.inbox.clone();
            let exists = self.inbox.len();
            self.say(
                format!(
                    "* {exists} EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n{tag} OK [READ-WRITE] done\r\n"
                )
                .as_bytes(),
            );
        } else if command == "EXAMINE \"Sent Items\"" {
            self.selected = vec![SENT.to_vec()];
            self.say(
                format!("* 1 EXISTS\r\n* OK [UIDVALIDITY 9] ok\r\n{tag} OK [READ-ONLY] done\r\n")
                    .as_bytes(),
            );
        } else if command.starts_with("UID SEARCH ") {
            let uids: Vec<String> = (1..=self.selected.len())
                .map(|uid| uid.to_string())
                .collect();
            self.say(format!("* SEARCH {}\r\n{ok}", uids.join(" ")).as_bytes());
        } else if let Some(rest) = command.strip_prefix("UID FETCH ") {
            let (uid, what) = rest.split_once(' ').expect("a uid and a query");
            assert_eq!(what, "(BODY.PEEK[])", "every fetch peeks");
            let uid: usize = uid.parse().expect("one uid");
            if let Some(raw) = self.selected.get(uid - 1).cloned() {
                self.say(
                    format!("* {uid} FETCH (UID {uid} BODY[] {{{}}}\r\n", raw.len()).as_bytes(),
                );
                self.say(&raw);
                self.say(b")\r\n");
            }
            self.say(ok.as_bytes());
        } else if command == "LOGOUT" {
            self.say(format!("* BYE\r\n{ok}").as_bytes());
        } else {
            self.say(format!("{tag} BAD not scripted\r\n").as_bytes());
        }
    }
}

impl Write for Imap {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buf);
        while let Some(end) = self.written.windows(2).position(|w| w == b"\r\n") {
            let line: Vec<u8> = self.written.drain(..end + 2).collect();
            self.command(String::from_utf8(line).expect("text").trim_end());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Read for Imap {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.answer.len());
        for (slot, byte) in buf.iter_mut().zip(self.answer.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }
}

/// Opens a session on a fresh [`Imap`] serving what the far end holds.
struct Dial(Server);

impl Open for Dial {
    type Stream = Imap;

    fn open(&self, _account: &MailAccount) -> Result<Outlook<Imap>, ImapError> {
        let inbox = match &self.0 {
            Server::Holding(messages) => messages.clone(),
            Server::Unreachable => {
                return Err(ImapError::Server {
                    what: "the connection".to_owned(),
                    why: "no route to the server".to_owned(),
                });
            }
        };
        let mut client = imap::Client::new(Imap {
            inbox,
            selected: Vec::new(),
            written: Vec::new(),
            answer: b"* OK scripted IMAP ready\r\n".iter().copied().collect(),
        });
        client.read_greeting().expect("a greeting");
        let session = client
            .login(ADA, "not-a-token")
            .map_err(|(failed, _)| failed)
            .expect("a login");
        Ok(Outlook::over(session))
    }
}

struct Outlook365(tempfile::TempDir);

impl Outlook365 {
    fn puller(&self, server: Server) -> Microsoft<Dial> {
        Microsoft {
            account: MailAccount {
                email: AccountEmail::new(ADA).expect("a lower-case address"),
                subdir: "ada".to_owned(),
                sync: Sync::MsImap(MsImap {
                    authority: Authority::Organizations,
                    inbox_folders: Folders::new(vec!["Inbox".to_owned()]).expect("a folder"),
                    sent_folder: "Sent Items".to_owned(),
                }),
            },
            sent_folder: "Sent Items".to_owned(),
            maildir: self.0.path().join("Mail/ada"),
            state_dir: self.0.path().join("state"),
            open: Dial(server),
        }
    }
}

impl Rig for Outlook365 {
    type Puller = Microsoft<Dial>;

    const UNREACHABLE: Failure = Failure::Unclassified;

    fn holding(&self, messages: &[Vec<u8>]) -> Microsoft<Dial> {
        self.puller(Server::Holding(messages.to_vec()))
    }

    fn unreachable(&self) -> Microsoft<Dial> {
        self.puller(Server::Unreachable)
    }

    fn landed(&self) -> Vec<Vec<u8>> {
        landed(&self.0.path().join("Mail/ada/Inbox"))
    }
}

#[test]
fn the_microsoft_pull_keeps_every_law_behind_a_fake_imap_server() {
    let rig = Outlook365(tempfile::tempdir().expect("a scratch dir"));
    assert_eq!(check(&rig, since(), &arrivals()), Ok(()));
    let sent = rig.0.path().join("Mail/ada/Sent");
    assert_eq!(
        (read(&sent.join("cur")), read(&sent.join("new"))),
        (vec![SENT.to_vec()], vec![]),
        "the Sent folder lands read in its own leaf, once"
    );
}
