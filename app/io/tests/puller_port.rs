//! Mail-in's conformance suite over the four real pullers, each behind a fake far end: a
//! scripted IMAP server under the `imap` crate's own client for Microsoft, and shell scripts
//! answering mbsync's and lieer's command lines over a directory that stands for the server,
//! lieer's on the Google grant minting its token from a scripted token endpoint. What lands is
//! read back from the maildir on disk.

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use suzatary_base::capability::Secret;
use suzatary_core::conformance::puller::{Rig, check};
use suzatary_core::mail::account::{Authority, MailAccount, MsImap, Sync};
use suzatary_core::mail::account_email::AccountEmail;
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::mail::fault::Fault;
use suzatary_core::mail::folders::Folders;
use suzatary_core::mail::imap::ImapError;
use suzatary_core::mail::mailroom::Server;
use suzatary_core::mail::puller::Puller as _;
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

    const UNREACHABLE: Fault = Fault::Unclassified;

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

    const UNREACHABLE: Fault = Fault::Unclassified;

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

    const UNREACHABLE: Fault = Fault::Unclassified;

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

/// An IMAP server holding `folders`, each by its server name, UID `n` its `n`th message;
/// `selected` holds the selected folder's messages. A folder it does not hold is refused with
/// `NO`, and a command it does not know is `BAD`.
struct Imap {
    folders: BTreeMap<String, Vec<Vec<u8>>>,
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
        let opened = command
            .strip_prefix("SELECT ")
            .map(|folder| (folder, "READ-WRITE"))
            .or_else(|| {
                command
                    .strip_prefix("EXAMINE ")
                    .map(|folder| (folder, "READ-ONLY"))
            });
        if command.starts_with("LOGIN ") || command.starts_with("UID STORE ") {
            self.say(ok.as_bytes());
        } else if command.starts_with("LIST ") {
            let listed: String = self
                .folders
                .keys()
                .flat_map(|name| ["* LIST (\\HasNoChildren) \"/\" \"", name, "\"\r\n"])
                .collect();
            self.say(format!("{listed}{ok}").as_bytes());
        } else if let Some((folder, mode)) = opened {
            let name = folder.trim_matches('"');
            match self.folders.get(name).cloned() {
                Some(messages) => {
                    let exists = messages.len();
                    self.selected = messages;
                    self.say(
                        format!(
                            "* {exists} EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n{tag} OK [{mode}] done\r\n"
                        )
                        .as_bytes(),
                    );
                }
                None => self.say(format!("{tag} NO no such folder\r\n").as_bytes()),
            }
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

/// Opens a session on a fresh [`Imap`] holding `folders`, or none at all.
struct Dial(Option<BTreeMap<String, Vec<Vec<u8>>>>);

impl Open for Dial {
    type Stream = Imap;

    fn open(&self, _account: &MailAccount) -> Result<Outlook<Imap>, ImapError> {
        let Some(folders) = self.0.clone() else {
            return Err(ImapError::Server {
                what: "the connection".to_owned(),
                why: "no route to the server".to_owned(),
            });
        };
        let mut client = imap::Client::new(Imap {
            folders,
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

/// Server folders by name, each with its messages, oldest first.
type Held<'a> = [(&'a str, &'a [Vec<u8>])];

/// Ada's Microsoft account, declaring `inbox_folders` and a Sent folder named `Sent Items`,
/// with its maildir and state under `dir`.
fn microsoft(dir: &Path, inbox_folders: &[&str], folders: Option<&Held<'_>>) -> Microsoft<Dial> {
    let declares = MsImap {
        authority: Authority::Organizations,
        inbox_folders: Folders::new(inbox_folders.iter().map(|&leaf| leaf.to_owned()).collect())
            .expect("a folder"),
        sent_folder: "Sent Items".to_owned(),
    };
    Microsoft {
        account: MailAccount {
            email: AccountEmail::new(ADA).expect("a lower-case address"),
            subdir: "ada".to_owned(),
            sync: Sync::MsImap(declares.clone()),
        },
        declares,
        maildir: dir.join("Mail/ada"),
        state_dir: dir.join("state"),
        open: Dial(folders.map(|folders| {
            folders
                .iter()
                .map(|(name, messages)| ((*name).to_owned(), messages.to_vec()))
                .collect()
        })),
    }
}

struct Outlook365(tempfile::TempDir);

impl Rig for Outlook365 {
    type Puller = Microsoft<Dial>;

    const UNREACHABLE: Fault = Fault::Unclassified;

    fn holding(&self, messages: &[Vec<u8>]) -> Microsoft<Dial> {
        microsoft(
            self.0.path(),
            &["Inbox"],
            Some(&[("INBOX", messages), ("Sent Items", &[SENT.to_vec()])]),
        )
    }

    fn unreachable(&self) -> Microsoft<Dial> {
        microsoft(self.0.path(), &["Inbox"], None)
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

/// Every folder an account declares syncs, each on its own: an inbox, a junk folder named in
/// `inbox_folders` and the sent folder all land, the first two unread and the third read. A
/// declared folder the server does not list fails the account by its name while the others
/// still land, and once the server lists it again the next sync lands what it holds.
#[test]
fn every_declared_folder_syncs_and_one_the_server_does_not_list_fails_by_name() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    let message = |id: &str| {
        format!("Message-ID: <{id}@example.test>\r\nSubject: {id}\r\n\r\n{id}\r\n").into_bytes()
    };
    let (inbox, junk, sent) = (message("inbox"), message("junk"), message("sent"));
    let leaf = |name: &str, half: &str| {
        let mut held = read(&dir.path().join("Mail/ada").join(name).join(half));
        held.sort();
        held
    };
    let pull = |folders: &Held<'_>| {
        microsoft(dir.path(), &["Inbox", "Junk Email"], Some(folders)).pull(since())
    };

    let said = pull(&[
        ("INBOX", std::slice::from_ref(&inbox)),
        ("Junk Email", std::slice::from_ref(&junk)),
        ("Sent Items", std::slice::from_ref(&sent)),
    ])
    .expect("every folder syncs");
    assert_eq!(said, "INBOX: 1 new; Junk Email: 1 new; Sent Items: 1 new");
    assert_eq!(leaf("Inbox", "new"), vec![inbox.clone()]);
    assert_eq!(leaf("Junk Email", "new"), vec![junk.clone()]);
    assert_eq!(leaf("Sent", "cur"), vec![sent.clone()]);

    let (later, replied) = (message("later"), message("replied"));
    let failed = pull(&[
        ("INBOX", &[inbox.clone(), later.clone()]),
        ("Sent Items", &[sent.clone(), replied.clone()]),
    ])
    .expect_err("a declared folder the server does not list fails the account");
    assert_eq!(failed.class, Fault::Unclassified);
    assert_eq!(
        failed.said,
        "INBOX: 1 new; Junk Email: the server lists no folder of this name; Sent Items: 1 new"
    );
    let mut inbox_now = vec![inbox.clone(), later];
    inbox_now.sort();
    let mut sent_now = vec![sent.clone(), replied];
    sent_now.sort();
    assert_eq!(leaf("Inbox", "new"), inbox_now, "the inbox still syncs");
    assert_eq!(leaf("Sent", "cur"), sent_now, "the sent folder still syncs");

    let spam = message("spam");
    let caught_up = pull(&[
        ("INBOX", &[]),
        ("Junk Email", &[junk.clone(), spam.clone()]),
        ("Sent Items", &[]),
    ])
    .expect("the folder syncs again");
    assert_eq!(
        caught_up,
        "INBOX: 0 new; Junk Email: 1 new; Sent Items: 0 new"
    );
    let mut junk_now = vec![junk, spam];
    junk_now.sort();
    assert_eq!(
        leaf("Junk Email", "new"),
        junk_now,
        "the next sync catches up"
    );
}
