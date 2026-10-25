//! `suzatary-dev fixture-chain`: the end-to-end suite's small chain and `instance.toml`, served
//! by the real `suzatary serve`.
//!
//! A browser on this box is always loopback, which may not write, so the suite's writes rest
//! on `--whois-fixture`; both it and this verb exist only under the `fixture` cargo feature,
//! so the deployed build does not contain them at all.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context as _;
use chrono::{Duration, Timelike};
use prodrome_core::event::{Actor, Hash, mk_completed, mk_created};
use prodrome_core::fpl::{self, mk_decay, mk_flat};
use suzatary_core::capability::Clock;
use suzatary_core::cast::{self, CAST};
use suzatary_core::record::{EventStore, mk_authored, mk_source, mk_subtodo};
use suzatary_core::store::{append_to, linearised};
use suzatary_io::clock::SystemClock;

use suzatary_core::capability::Login;
use suzatary_core::chains::ChainName;
use suzatary_core::mail::account::{MailAccount, Roster, Sync};
use suzatary_core::mail::account_email::AccountEmail;
use suzatary_core::model_name::ModelName;

use crate::instance::{Chain, File, Mail, Secretary, Serving, Web};

/// Loopback, the one plain-http origin browsers treat as secure, which `crypto.subtle` needs.
pub const HOST: &str = "127.0.0.1";

/// The deployment's own writer login: the default `--login`, and the only one
/// `web.act_as` maps to an actor.
pub const WRITER_LOGIN: &str = "e2e-writer@example.invalid";

/// The actor `WRITER_LOGIN` maps to: the cast's owner.
const WRITER_ACTOR: &str = CAST.owner.actor;

/// A login this deployment has never heard of, so every write is refused for the real reason.
const READER_LOGIN: &str = "e2e-reader@example.invalid";

/// The fixture's two signing seeds (SPEC law 42): test values, nobody's keys.
/// `scripts/web-e2e.sh` exports them under `SUZATARY_BOX_KEY` and `SUZATARY_AGENT_KEY`, so every
/// server and the suite resolve them as secretspec would; a guard test pins the two agree.
pub const BOX_SEED: &str = "e2e0000000000000000000000000000000000000000000000000000000000b0c";
/// The agent's, beside it.
pub const AGENT_SEED: &str = "e2e0000000000000000000000000000000000000000000000000000000000a9e";

/// The fixture's root key, the public half of [`BOX_SEED`].
fn fixture_root() -> prodrome_core::sign::PublicKey {
    prodrome_core::sign::Secret::new(BOX_SEED)
        .expect("the fixture's seed is a seed")
        .public()
}

/// The name, never a value, of the secret the fixture's standing secret card asks for: the
/// coding routine's token, which the writing server's `[coding]` declares. Not the model's, which
/// only its sign-in card asks for.
pub const SECRET_ASK_KEY: &str = suzatary_core::coding::ROUTINE_TOKEN;
/// The one calendar link `--settings` declares, on the `ics` plugin, which nothing stores.
pub const SETTINGS_CALENDAR_LINK: &str = "PLUGIN_ICS_WORK";
/// The `[[plugins]]` table `--settings` appends, as an operator adds one by hand: `File` never
/// writes a plugin back.
const SETTINGS_CALENDAR: &str = "
[[plugins]]
name = \"ics\"
component = \"ics.wasm\"
schema = \"ics.schema\"
origins = [\"https://calendar.example.test\"]
credentials = [\"PLUGIN_ICS_WORK\"]
";

/// `suzatary-dev fixture-chain`: build the chain and instance file at `root`, and print what
/// the e2e script needs to talk to it.
///
/// `settings` is opt-in because a real `secretspec` must never reach this box's own store;
/// the script sets it only for a server with an isolated `$HOME`.
pub fn build(args: &Args) -> anyhow::Result<()> {
    let Args {
        root,
        port,
        login,
        repo,
        propose,
        reviewed,
        settings,
        model,
        connected,
        connections,
        lines,
        conversations,
        wide,
    } = args;
    let lapsed = *connections == Connections::Lapsed;
    let (root, port, propose, reviewed, settings) =
        (root.as_path(), *port, *propose, *reviewed, *settings);
    let no_model = *model == Model::Unanswered;
    std::fs::create_dir_all(root)?;
    std::fs::create_dir_all(root.join("briefings"))?;
    fake_drive(root)?;
    let tip = build_chain(root)?;
    build_project_chain(root)?;
    // Built before the instance file so the file names a config that already exists.
    let muted = std::env::var_os("SUZATARY_FIXTURE_NO_MAIL").is_some();
    let mail_config = mail_fixture(root, muted)?;
    // The same typed writer `suzatary init` uses, so fixture and deployment cannot drift.
    instance_file(
        root,
        port,
        repo,
        mail_config.clone(),
        settings,
        propose,
        lapsed,
    )
    .write(&root.join("instance.toml"), FIXTURE_HEADER, true)?;
    if settings {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(root.join("instance.toml"))?;
        std::io::Write::write_all(&mut file, SETTINGS_CALENDAR.as_bytes())?;
        secretspec(root)?;
    }
    standing_cards(root, *connected)?;
    // The fixture adopts signatures as a box's first writer does: every chain registers the
    // box's key and the agent's, and its epoch makes what was built above history.
    let signers = suzatary_core::signatures::Signers::Held(suzatary_core::signatures::BoxKeys::of(
        &fixture_root(),
        &suzatary_core::capability::Secret::new(BOX_SEED),
        &suzatary_core::capability::Secret::new(AGENT_SEED),
    )?);
    let read = crate::instance::Instance::open(&root.join("instance.toml"))?;
    answers(&read, no_model)?;
    if settings {
        a_calendar(&read, repo)?;
    }
    if lapsed {
        reading_not_sending(&read, root)?;
    }
    bulk_lines(&read, &sizes(*lines, *conversations, *wide))?;
    read.adopt(&signers)
        .whole()
        .context("adopting signatures on every chain")?;
    // Opt-in, so one server can show the empty Inbox and another a waiting card,
    // independent of which test ran first.
    if propose {
        triage_run(&root.join("instance.toml"))?;
    }
    if reviewed {
        a_review(root)?;
    }
    // Only the writer's server sets both flags, so only it gets the standing acts.
    let secret_ask = propose && settings;
    if secret_ask {
        a_secret_ask(root)?;
        a_coding_session(root)?;
    }
    let todos = &CAST.todos;
    println!(
        "{}",
        serde_json::json!({
            "base": format!("http://{HOST}:{port}"),
            "root": root.display().to_string(),
            "instance": root.join("instance.toml").display().to_string(),
            "login": login,
            "writes": login == WRITER_LOGIN,
            "tip": tip.as_ref().map_or("", Hash::as_str),
            "open": [todos.urgent.id, todos.flat.id, todos.agents.id, todos.checklist.id, todos.high.id, CAST.unpriced.id],
            "done": [todos.resolved.id],
            "unconfirmed": todos.agents.id,
            "proposed": if propose { vec![CAST.proposed.id] } else { vec![] },
            "reviewed": reviewed,
            "held": if reviewed { HELD_WHY } else { "" },
            "secretAsk": secret_ask,
            "mail": mail_config.is_some(),
            "mailSubjects": if mail_config.is_some() { CAST.mail.iter().map(|m| m.subject).collect() } else { vec![] },
        })
    );
    Ok(())
}

/// The maildir folder every fixture account's mail lives under, matching
/// [`instance_file`]'s one account.
const MAIL_SUBDIR: &str = "her";

/// The fixture's maildir root made absolute, because notmuch resolves a relative
/// `database.path` against `$HOME`, which is the live store.
fn fixture_mail_root(root: &Path) -> anyhow::Result<PathBuf> {
    let root = std::path::absolute(root)
        .with_context(|| format!("making the fixture root {} absolute", root.display()))?;
    Ok(root.join("Mail"))
}

/// The fixture's own notmuch database under `root`, indexed with its own `NOTMUCH_CONFIG`
/// so browser tests can never be answered with the owner's real mail.
///
/// `None` when `muted` or when `notmuch` is missing, never a fallback to the box's config.
fn mail_fixture(root: &Path, muted: bool) -> anyhow::Result<Option<PathBuf>> {
    if muted {
        eprintln!("SUZATARY_FIXTURE_NO_MAIL is set: this fixture starts with no mail index at all");
        return Ok(None);
    }
    if Command::new("notmuch").arg("--version").output().is_err() {
        eprintln!(
            "notmuch is not on PATH: this fixture REFUSES to start mail search rather than \
             falling back to whatever this box has configured for itself"
        );
        return Ok(None);
    }

    let mail_root = fixture_mail_root(root)?;
    let cur = mail_root.join(MAIL_SUBDIR).join("cur");
    for leaf in ["cur", "new", "tmp"] {
        std::fs::create_dir_all(mail_root.join(MAIL_SUBDIR).join(leaf))
            .with_context(|| format!("creating the fixture's own maildir under {leaf}"))?;
    }
    // The first matches the agent's todo, so one palette search finds a local and a mail row.
    for mail in &CAST.mail {
        write_seed_message(&cur, mail)?;
    }
    write_notice(&cur, &CAST.notice)?;

    let config = root.join("notmuch-config");
    std::fs::write(
        &config,
        format!(
            "# The fixture's own notmuch database — never this box's.\n\
             [database]\n\
             path={}\n\n\
             [user]\n\
             name=Fixture\n\
             primary_email={}\n\n\
             [new]\n\
             tags=new\n\n\
             [search]\n\
             exclude_tags=deleted;spam\n\n\
             [maildir]\n\
             synchronize_flags=true\n",
            mail_root.display(),
            CAST.owner.email,
        ),
    )
    .context("writing the fixture's own notmuch config")?;

    // Set explicitly rather than inherited, so it can only name the fixture's own config.
    let output = Command::new("notmuch")
        .arg("new")
        .env("NOTMUCH_CONFIG", &config)
        .output()
        .context("running notmuch new for the fixture's own database")?;
    if !output.status.success() {
        anyhow::bail!(
            "notmuch new exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(Some(config))
}

/// One synthetic message, filed as already delivered and seen.
fn write_seed_message(cur: &Path, mail: &cast::Mail) -> anyhow::Result<()> {
    let cast::Mail {
        from,
        subject,
        msgid,
        body,
    } = mail;
    let path = cur.join(format!("{msgid}:2,S"));
    let text = format!(
        "Message-Id: <{msgid}>\r\n\
         From: {}\r\n\
         To: {}\r\n\
         Subject: {subject}\r\n\
         Date: Mon, 01 Sep 2025 09:00:00 +0000\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         {body}\r\n",
        from.email, CAST.owner.email,
    );
    std::fs::write(path, text)
        .with_context(|| format!("writing the fixture's seed message {msgid}"))
}

/// The notice: its text and its PDF, built by the same MIME builder a send uses.
fn write_notice(cur: &Path, notice: &cast::Notice) -> anyhow::Result<()> {
    use suzatary_core::mail::send::{Attachment, Composed, Outgoing, build_mime};
    let cast::Notice { mail, file } = notice;
    let out = Outgoing::new(Composed {
        sender: mail.from.email.to_owned(),
        to: vec![CAST.owner.email.to_owned()],
        subject: mail.subject.to_owned(),
        body: mail.body.to_owned(),
        attachments: vec![Attachment {
            filename: file.name.to_owned(),
            content: file.content.as_bytes().to_vec(),
            mime_type: file.media.to_owned(),
        }],
        ..Composed::default()
    })
    .map_err(|failed| anyhow::anyhow!("the fixture's notice: {failed}"))?;
    // 2025-09-01T10:00:00Z, beside the seed messages.
    let bytes = build_mime(&out, mail.msgid, 1_756_720_800)
        .map_err(|failed| anyhow::anyhow!("building the fixture's notice: {failed}"))?;
    std::fs::write(cur.join(format!("{}:2,S", mail.msgid)), bytes)
        .with_context(|| format!("writing the fixture's notice {}", mail.msgid))
}

/// The secretary's words beside the triage card the fixture leaves open.
const HELD_WHY: &str =
    "This one adds a todo and completes it in one breath, so it is yours to say.";

/// One day's review written through the real stores without a model: a held line for each open
/// triage card, as `secretary::review` writes on a real box.
fn a_review(root: &Path) -> anyhow::Result<()> {
    use suzatary_core::secretary::store::{Decision, Entry};

    let read = crate::instance::Instance::open(&root.join("instance.toml"))?;
    let inbox = read.inbox_store();
    let open = suzatary_core::secretary::verbs::standing(&inbox)
        .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    anyhow::ensure!(
        !open.is_empty(),
        "the fixture's triage run left a card to hold"
    );
    let at = SystemClock.now();
    let day = read.persona.zone.day(fpl::instant_of(at));
    let id = suzatary_core::secretary::store::ConversationId::daily(
        suzatary_core::secretary::daily::Daily::Review,
        day,
    );
    let store = read.conversation_stores();
    suzatary_core::secretary::transcript::start(
        store.conversations(),
        &id,
        &Entry::Opened {
            at,
            title: suzatary_core::secretary::words::Words::kept("Review".to_owned()),
            model: "scripted/fixture".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Review,
        },
    )?;
    for (card, changeset) in open {
        suzatary_core::secretary::transcript::push(
            store.conversations(),
            &id,
            &Entry::Reviewed {
                at,
                chain: changeset.chain().to_owned(),
                batch: card.to_string(),
                decision: Decision::Held,
                why: suzatary_core::secretary::words::Words::kept(HELD_WHY.to_owned()),
            },
        )?;
    }
    Ok(())
}

/// The fixture's model is scripted, so it answers, and the model health says so; under
/// `no_model` its last call failed instead, so the box asks for a model itself.
fn answers(read: &crate::instance::Instance, no_model: bool) -> anyhow::Result<()> {
    use suzatary_core::capability::Clock as _;
    use suzatary_core::mail::health::Outcome;
    use suzatary_core::store::Doc as _;
    let at = prodrome_core::fpl::instant_of(SystemClock.now());
    suzatary_io::model_health::beside(&read.mail.heartbeat)
        .set(
            &(),
            &suzatary_core::mail::health::Healths::one(
                &suzatary_io::model::name_of(read.secretary.model.as_str()),
                at,
                if no_model {
                    Outcome::Failed(suzatary_core::connect::Failure::KeyRefused)
                } else {
                    Outcome::Succeeded
                },
                "the fixture's model is not connected",
            ),
        )
        .context("writing the fixture's model health")
}

/// Why the fixture's standing secret ask names its key.
const SECRET_ASK_WHY: &str = "this deployment declares the key and it does not resolve yet";

/// The secret ask's own conversation, never `default`, so scenes counting `default`'s
/// rounds are unaffected.
const SECRET_ASK_CONVERSATION: &str = "secretask";

/// One standing `Act::Secret` proposal from a real scripted round, asking for the same key
/// the Settings page shows.
fn a_secret_ask(root: &Path) -> anyhow::Result<()> {
    let read = crate::instance::Instance::open(&root.join("instance.toml"))?;
    let asking = serde_json::json!({
        "say": "I will ask for the key.",
        "calls": [{"verb": "propose_act", "args": {
            "act": "secret",
            "name": SECRET_ASK_KEY,
            "why": SECRET_ASK_WHY,
            "sources": format!("todo:life/{}", CAST.todos.urgent.id),
        }}],
    });
    crate::commands::secretary::scripted_round(
        &read,
        &suzatary_core::secretary::store::ConversationId::new(SECRET_ASK_CONVERSATION)?,
        "Secret ask",
        "ask for the coding routine's token",
        Scripted::answering(&[
            ("ask for the coding routine's token", asking.to_string()),
            ("", r#"{"say": "Asked."}"#.to_owned()),
        ]),
    )
}

/// The fixture's own `secretspec.toml`, its code root's, declaring no name: every name a scene
/// sets (the calendar links, the subscription's token) is one its instance
/// derives, which the server declares in the file it writes under its own state directory, as a
/// deployment's does beside a read-only code root.
fn secretspec(root: &Path) -> anyhow::Result<()> {
    std::fs::write(
        root.join("secretspec.toml"),
        "[project]\nname = \"suzatary-e2e-fixture\"\nrevision = \"1.0\"\n\n[profiles.default]\n",
    )
    .context("writing the fixture's own secretspec.toml")
}

/// The cards a fixture's `--connected` leaves standing.
fn standing_cards(root: &Path, connected: Connected) -> anyhow::Result<()> {
    match connected {
        Connected::Nothing => Ok(()),
        Connected::Calendar => a_connected_calendar(root),
        Connected::Google => a_google_account_to_connect(root),
    }
}

/// The calendar a `--connected calendar` fixture has connected, whose link the box asks for.
const CONNECTED: &str = "Home";

/// The conversation the calendar's removal is proposed in, as the setup agent will file it.
pub const CONNECT_CONVERSATION: &str = "connect";

/// A calendar connected through the instance file's own editor, its link not yet set, and its
/// removal standing as a card.
fn a_connected_calendar(root: &Path) -> anyhow::Result<()> {
    use suzatary_core::secretary::store::{ConversationId, Entry, Kind, Proposal, Reasons};
    use suzatary_core::settings::Edits as _;

    let calendar = suzatary_core::connect::Connection::parse("ics", &[CONNECTED.to_owned()])?;
    crate::instance::InstanceEdits::new(root.join("instance.toml")).add_connection(&calendar)?;
    let read = crate::instance::Instance::open(&root.join("instance.toml"))?;
    let store = read.conversation_stores();
    let conversations = store.conversations();
    let inbox = read.inbox_store();
    let id = ConversationId::new(CONNECT_CONVERSATION)?;
    let now = SystemClock.now();
    suzatary_core::secretary::transcript::start(
        conversations,
        &id,
        &Entry::Opened {
            at: now,
            title: suzatary_core::secretary::words::Words::kept("Calendars".to_owned()),
            model: "scripted".to_owned(),
            kind: Kind::Topic,
        },
    )?;
    suzatary_core::secretary::filed::file(
        conversations,
        &inbox,
        now,
        suzatary_core::secretary::filed::Filing {
            conversation: &id,
            proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Act(suzatary_core::secretary::acts::Act::Disconnect {
                connection: calendar,
            }),
            reasons: Reasons::new(
                "the home calendar moved, and Ada asked to remove it",
                vec![format!("todo:life/{}", CAST.todos.urgent.id).parse()?],
            )?,
            serves: None,
            revises: None,
        },
    )
    .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    Ok(())
}

/// The Google account a `--connected google` fixture's Connect card connects.
pub const GOOGLE_ACCOUNT: &str = "ada@example.test";

/// A Connect card for a Google account standing in the `connect` conversation, as the setup
/// agent files it: its tap writes the account, and the box asks for the client and the sign-in.
fn a_google_account_to_connect(root: &Path) -> anyhow::Result<()> {
    use suzatary_core::secretary::store::{ConversationId, Entry, Kind, Proposal, Reasons};

    let account =
        suzatary_core::connect::Connection::parse("google", &[GOOGLE_ACCOUNT.to_owned()])?;
    let read = crate::instance::Instance::open(&root.join("instance.toml"))?;
    let store = read.conversation_stores();
    let conversations = store.conversations();
    let inbox = read.inbox_store();
    let id = ConversationId::new(CONNECT_CONVERSATION)?;
    let now = SystemClock.now();
    suzatary_core::secretary::transcript::start(
        conversations,
        &id,
        &Entry::Opened {
            at: now,
            title: suzatary_core::secretary::words::Words::kept("Google".to_owned()),
            model: "scripted".to_owned(),
            kind: Kind::Topic,
        },
    )?;
    suzatary_core::secretary::filed::file(
        conversations,
        &inbox,
        now,
        suzatary_core::secretary::filed::Filing {
            conversation: &id,
            proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Act(suzatary_core::secretary::acts::Act::Connect {
                connection: account,
            }),
            reasons: Reasons::new(
                "Ada asked to connect her Google account's mail and calendars",
                vec![format!("todo:life/{}", CAST.todos.urgent.id).parse()?],
            )?,
            serves: None,
            revises: None,
        },
    )
    .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    Ok(())
}

/// What `suzatary-dev fake-google` was told.
#[derive(Debug, clap::Args)]
pub struct FakeGoogleArgs {
    /// The port to serve on.
    #[arg(long)]
    port: u16,
    /// The account its consent page signs in.
    #[arg(long, default_value_t = GOOGLE_ACCOUNT.to_owned())]
    account: String,
}

/// `suzatary-dev fake-google`: serve the fake Google the cargo tests use, printing its base URL,
/// until killed.
///
/// # Errors
/// Never; a port that cannot be bound panics with the fake's own words.
pub fn fake_google(args: &FakeGoogleArgs) -> anyhow::Result<()> {
    let fake = fake_google::FakeGoogle::start(args.port, &args.account);
    println!("{}", serde_json::json!({ "base": fake.base }));
    loop {
        std::thread::park();
    }
}

/// The fake Google, one file with the cargo tests'.
#[path = "../../tests/common/fake_google.rs"]
#[allow(
    dead_code,
    reason = "the scene drives the consent and the callback; the counters are the cargo tests'"
)]
pub mod fake_google;

/// The calendar `--settings` declares as one that synced once and then lost its link: its store
/// begun at the reference plugin's schema, so the calendar reads it, and its ledger's last run a
/// success, so storing the link again makes its line healthy without a sync.
fn a_calendar(read: &crate::instance::Instance, repo: &Path) -> anyhow::Result<()> {
    use suzatary_core::plugin::{Answer, Came, LastRun, Ledger, PluginName};
    let name = PluginName::parse("ics")?;
    let text = std::fs::read_to_string(repo.join("app/plugin-ics/ics.schema"))
        .context("reading the calendar plugin's schema")?;
    let schema = prodrome_core::declared::Declaration::admit(&text)
        .map_err(|refused| anyhow::anyhow!("the calendar's schema: {refused}"))?;
    suzatary_io::plugin_store::PluginStore::at(read.plugins.store(&name)).begin(&schema)?;
    let run = |hour| -> anyhow::Result<LastRun> {
        Ok(LastRun {
            at: chrono::NaiveDate::from_ymd_opt(2026, 10, 8)
                .and_then(|day| day.and_hms_opt(hour, 0, 0))
                .context("the fixture's instant")?,
            came: Came::Learnt {
                summary: "plugin ics: 3 items".to_owned(),
            },
        })
    };
    let link = |answer| [(SETTINGS_CALENDAR_LINK.to_owned(), answer)];
    let ledger = Ledger::default().after(run(9)?, &link(Answer::Status(200)));
    std::fs::write(
        read.plugins.beside(&name).join("ledger.json"),
        serde_json::to_vec(&ledger)?,
    )?;
    Ok(())
}

/// The fixture's mail accounts: the owner's, and with `lapsed` the Microsoft account the links
/// scenes read.
fn mail_accounts(lapsed: bool) -> Roster {
    Roster::new(
        std::iter::once(MailAccount {
            email: AccountEmail::new(CAST.owner.email)
                .expect("the cast's address is a lower-case address"),
            subdir: MAIL_SUBDIR.to_owned(),
            sync: Sync::Gmail,
        })
        .chain(
            lapsed.then(|| MailAccount {
                email: AccountEmail::new(LINKS_ACCOUNT)
                    .expect("the links account is a lower-case address"),
                subdir: suzatary_core::mail::account::slug_of(LINKS_ACCOUNT),
                sync: Sync::MsImap(suzatary_core::mail::account::MsImap {
                    authority: suzatary_core::mail::account::Authority::Consumers,
                    inbox_folders: suzatary_core::mail::folders::Folders::inbox(),
                    sent_folder: "Sent".to_owned(),
                }),
            }),
        )
        .collect(),
    )
    .expect("distinct accounts are a roster")
}

/// `--connections lapsed`'s Microsoft account as signed in to read and not to send: its grant file holds the
/// IMAP slot alone, as `suzatary auth` without `--scope smtp` leaves it. Written only under
/// `root`: a state directory outside it (`XDG_STATE_HOME` not pointed into the fixture) is this
/// box's own, and is refused.
fn reading_not_sending(read: &crate::instance::Instance, root: &Path) -> anyhow::Result<()> {
    let dir = &read.paths.oauth;
    anyhow::ensure!(
        dir.starts_with(std::path::absolute(root)?),
        "--connections lapsed writes a grant under {}, outside the fixture's root: set XDG_STATE_HOME inside it",
        dir.display()
    );
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join(format!(
            "{}.json",
            suzatary_core::mail::account::slug_of(LINKS_ACCOUNT)
        )),
        serde_json::json!({
            "version": 1,
            "email": LINKS_ACCOUNT,
            "authority": "consumers",
            "imap": { "refresh_token": "fixture", "granted": "2026-10-08T09:00:00+00:00" },
        })
        .to_string(),
    )
    .context("writing the fixture's reading grant")?;
    Ok(())
}

/// The fixture's conversation holding a tapped coding session whose branch has come back.
pub const CODING_CONVERSATION: &str = "codingsession";

/// One coding proposal taken through the real round, submit, `fired` and `advance`, with
/// nothing actually fired.
fn a_coding_session(root: &Path) -> anyhow::Result<()> {
    use suzatary_core::coding::Fired;
    use suzatary_core::secretary::responded::{Done, Performed};
    use suzatary_core::secretary::store::ConversationId;

    let read = crate::instance::Instance::open(&root.join("instance.toml"))?;
    let store = read.conversation_stores();
    let inbox = read.inbox_store();
    let id = ConversationId::new(CODING_CONVERSATION)?;
    let now = SystemClock.now();
    let url = "https://claude.ai/code/session_01fixture";
    let asking = serde_json::json!({
        "say": "A session would do it.",
        "calls": [{"verb": "propose_coding_session", "args": {
            "task": "Retitle the CV.\nRun the gates.",
            "why": "the title is out of date",
            "sources": format!("todo:life/{}", CAST.todos.urgent.id),
        }}],
    });
    crate::commands::secretary::scripted_round(
        &read,
        &id,
        "A coding session",
        "retitle my cv properly",
        Scripted::answering(&[
            ("retitle my cv properly", asking.to_string()),
            (
                "",
                r#"{"say": "Proposed: tap Start to run it."}"#.to_owned(),
            ),
        ]),
    )?;
    let stored = suzatary_core::secretary::read(store.conversations(), &id)
        .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    let object = &stored
        .proposals
        .first()
        .context("the round proposed the session")?
        .object;
    let owner = prodrome_core::event::Actor::new(CAST.owner.actor)?;
    suzatary_core::secretary::inbox::submit(&inbox, object, &owner, now, |_| {
        Ok(Performed::Done(Done::Acted {
            what: format!("started a coding session: {url}"),
        }))
    })
    .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    let fired = Fired {
        id: "session_01fixture".parse()?,
        url: url.parse()?,
    };
    suzatary_core::secretary::coding::fired(store.conversations(), &id, 0, fired, now)?;
    let seen = the_branch_back(url)?;
    suzatary_core::secretary::coding::advance(store.conversations(), &[seen], now)?;
    Ok(())
}

/// The `claude/` branch [`a_coding_session`]'s poll sees: its report names
/// `url`, and it is not merged.
fn the_branch_back(url: &str) -> anyhow::Result<suzatary_core::coding::Seen> {
    use suzatary_core::coding::{FileStat, Lines, Seen};
    let path = |text: &str| suzatary_core::repos::RepoPath::try_from(text.to_owned());
    Ok(Seen {
        branch: "claude/retitle".parse()?,
        tip: suzatary_core::repos::CommitId::try_from("c0de".repeat(10))?,
        merged: false,
        reports: vec![(
            path("docs/reports/retitle.md")?,
            format!("# Retitle\n\nSession: {url}\n\nGates: cargo test and web-test green.\n"),
        )],
        stat: vec![
            FileStat {
                path: path("cv.typ")?,
                lines: Lines::Text {
                    added: 1,
                    removed: 1,
                },
            },
            FileStat {
                path: path("docs/reports/retitle.md")?,
                lines: Lines::Text {
                    added: 5,
                    removed: 0,
                },
            },
        ],
    })
}

/// One triage run written as a real run writes it, through the agent's own verbs: a new todo and
/// its completion, filed as one card in the Inbox.
///
/// Every field is fixed, so a rerun files nothing new: the same acts are the card already there.
///
/// # Errors
/// The instance being unreadable, or a verb refusing.
pub fn triage_run(instance: &Path) -> anyhow::Result<()> {
    let proposed = &CAST.proposed;
    filed(
        instance,
        &serde_json::json!({
            "todo_id": proposed.id,
            "body": proposed.body,
            "rationale": proposed.rationale.concat(),
            "category": proposed.category,
            "priority": "75%",
            "why": "A message asks for this.",
            "sources": "briefing:2025-09-01",
        }),
        &serde_json::json!({
            "kind": "completed",
            "todo": proposed.id,
            "note": "the rain did it",
            "why": "A later message says it is done.",
            "sources": "briefing:2025-09-01",
        }),
    )
}

/// One run adding `todo` at 40% and completing it, filed as one triage card a writing login can
/// tap in the Inbox.
///
/// # Errors
/// The instance being unreadable, or a verb refusing.
pub fn held_run(instance: &Path, todo: &str) -> anyhow::Result<()> {
    filed(
        instance,
        &serde_json::json!({
            "todo_id": todo,
            "body": "A triage card, read from its summary",
            "rationale": "testing a triage card's summary",
            "category": CAST.proposed.category,
            "priority": "40%",
            "why": "testing a triage card",
            "sources": "briefing:2025-09-01",
        }),
        &serde_json::json!({
            "kind": "completed",
            "todo": todo,
            "note": "done, testing",
            "why": "testing a triage card",
            "sources": "briefing:2025-09-01",
        }),
    )
}

/// `todo_add` with `adding`, then `record` with `recording`, through the triage agent's surface
/// over the fixture's stores.
fn filed(
    instance: &Path,
    adding: &serde_json::Value,
    recording: &serde_json::Value,
) -> anyhow::Result<()> {
    let read = crate::instance::Instance::open(instance)?;
    let caps = crate::commands::mcp::Interpreters::assemble(&read, read.first())?;
    let chains = std::collections::BTreeMap::from([(
        read.first().policy.name.as_str().to_owned(),
        caps.agent(),
    )]);
    let conversations = read.conversation_stores();
    let inbox = read.inbox_store();
    let cards =
        suzatary_core::secretary::filed::TriageCards::new(conversations.conversations(), &inbox);
    for (verb, args) in [("todo_add", adding), ("record", recording)] {
        let args = args
            .as_object()
            .context("a verb's arguments are an object")?;
        suzatary_core::agent::call(&chains, &cards, &read.deployment, verb, args)
            .map_err(|refusal| anyhow::anyhow!("{verb}: {}", refusal.message()))?;
    }
    Ok(())
}

/// The fixture's Drive CLI, in its root.
const FAKE_DRIVE: &str = "fake-proton-drive";

/// A Drive CLI that answers only what keeping a private upload asks (the listings down to
/// its folder, making a folder, an upload into `drive/`) and refuses the rest.
fn fake_drive(root: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let kept = root.join("drive");
    let script = format!(
        "#!/bin/sh\n\
         case \"$1 $2\" in\n\
         \x20 'filesystem list')\n\
         \x20   case \"$4\" in\n\
         \x20     /) echo '[{{\"path\":\"/my-files\"}}]' ;;\n\
         \x20     /my-files|/my-files/Suzatary) echo '[]' ;;\n\
         \x20     *) echo \"not found: $4\" >&2; exit 3 ;;\n\
         \x20   esac ;;\n\
         \x20 'filesystem create-folder') ;;\n\
         \x20 'filesystem upload') mkdir -p '{kept}' && cp \"$6\" '{kept}/' ;;\n\
         \x20 *) echo \"refused: $*\" >&2; exit 99 ;;\n\
         esac\n",
        kept = kept.display()
    );
    let path = root.join(FAKE_DRIVE);
    std::fs::write(&path, script).context("writing the fixture's Drive CLI")?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .context("making the fixture's Drive CLI executable")?;
    Ok(())
}

/// The second chain: one resolved todo, so it has a head but no open work, since a replica
/// cannot write an empty chain's first object.
fn build_project_chain(root: &Path) -> anyhow::Result<()> {
    let now = fpl::instant_of(SystemClock.now())
        .with_nanosecond(0)
        .expect("zero is always a valid nanosecond");
    let untrusted = prodrome_core::policy::Untrusted::of([Actor::new("triage")?]);
    let store = EventStore::new(root.join("events-suzatary"), untrusted);
    append_to(
        &store,
        suzatary_core::record::GENESIS,
        mk_created(
            "seedmarker",
            fpl::datetime_of(now - Duration::days(30))?,
            CAST.owner.actor,
            "the seed the operator lays by hand",
            "",
        )?,
    )?;
    append_to(
        &store,
        suzatary_core::record::GENESIS,
        mk_completed(
            "seedmarker",
            fpl::datetime_of(now - Duration::days(29))?,
            CAST.owner.actor,
            "",
        )?,
    )?;
    Ok(())
}

/// Six todos from `CAST`, each carrying one thing the browser must get right, and the tip.
///
/// Dates are relative to now so a Decay prices the same each morning.
fn build_chain(root: &Path) -> anyhow::Result<Option<Hash>> {
    // Whole seconds, so the chain does not differ from itself between runs.
    let now = fpl::instant_of(SystemClock.now())
        .with_nanosecond(0)
        .expect("zero is always a valid nanosecond");
    let born = now - Duration::days(30);
    let made = now - Duration::days(20);
    let born_at = fpl::datetime_of(born)?;
    let made_at = fpl::datetime_of(made)?;
    let owner = CAST.owner.actor;
    let todos = &CAST.todos;

    let untrusted = prodrome_core::policy::Untrusted::of([Actor::new("triage")?]);
    let store = EventStore::new(root.join("events"), untrusted);

    // Every todo but the agent's is authored by the owner at `made`.
    let authored = |todo: &cast::Todo, spec, detail: &str| -> anyhow::Result<()> {
        append_to(
            &store,
            suzatary_core::record::GENESIS,
            mk_authored(
                todo.id,
                made_at,
                owner,
                "todo",
                made_at,
                suzatary_core::record::MarkupSource::from_chain(todo.body),
                spec,
                todo.rationale.iter().map(|&line| line.to_owned()).collect(),
                todo.category,
                "",
                suzatary_core::record::MarkupSource::from_chain(detail),
                None,
                todo.subtodos
                    .iter()
                    .map(|sub| mk_subtodo(sub.text, sub.done))
                    .collect::<Result<_, _>>()?,
                Vec::new(),
                "",
            )?,
        )?;
        Ok(())
    };

    for todo in todos.all() {
        append_to(
            &store,
            suzatary_core::record::GENESIS,
            mk_created(todo.id, born_at, owner, todo.created, "")?,
        )?;
        match todo.id {
            // Two days from its deadline, so it prices low and sorts first.
            id if id == todos.urgent.id => authored(
                todo,
                mk_decay(
                    0.6,
                    0.05,
                    now + Duration::days(2),
                    Duration::days(14),
                    Some(now - Duration::days(12)),
                )?,
                todo.detail,
            )?,
            id if id == todos.flat.id => authored(todo, mk_flat(0.55)?, todo.detail)?,
            // A reference and an outbound link in one body, so a scene can assert that
            // `rich.ts` treats the two differently.
            id if id == todos.checklist.id => authored(
                todo,
                mk_flat(0.72)?,
                &format!(
                    "{} Weights: #link(\"https://example.invalid/baggage\")[the airline's page].",
                    todo.detail
                ),
            )?,
            id if id == todos.resolved.id => {
                authored(todo, mk_flat(0.4)?, todo.detail)?;
                append_to(
                    &store,
                    suzatary_core::record::GENESIS,
                    mk_completed(
                        todo.id,
                        fpl::datetime_of(made + Duration::days(1))?,
                        owner,
                        "",
                    )?,
                )?;
            }
            // High, so "Done" removes a middle card without disturbing the keyboard tests'.
            id if id == todos.high.id => authored(todo, mk_flat(0.88)?, todo.detail)?,
            _ => {
                append_to(
                    &store,
                    suzatary_core::record::GENESIS,
                    agents_todo(made_at, made)?,
                )?;
            }
        }
    }
    // Written as every other todo is, with FPL's `Absent` for its spec: the one spelling of
    // "nothing prices it", so the fixture's list holds a todo with no value.
    let unpriced = &CAST.unpriced;
    append_to(
        &store,
        suzatary_core::record::GENESIS,
        mk_created(unpriced.id, born_at, owner, unpriced.created, "")?,
    )?;
    authored(unpriced, fpl::mk_absent(), unpriced.detail)?;
    Ok(linearised(&store)?.pop().map(|(name, _)| name))
}

/// The agent's untrusted todo, served as typst source rather than HTML; its body holds
/// markup on purpose so the page must show the asterisks.
fn agents_todo(
    made_at: prodrome_core::literal::Datetime,
    made: fpl::Instant,
) -> anyhow::Result<suzatary_core::record::TodoEvent> {
    let todo = &CAST.todos.agents;
    let mail = &CAST.mail[0];
    Ok(mk_authored(
        todo.id,
        fpl::datetime_of(made + Duration::days(2))?,
        "triage",
        "todo",
        made_at,
        suzatary_core::record::MarkupSource::from_chain(todo.body),
        mk_flat(0.66)?,
        Vec::new(),
        todo.category,
        "",
        suzatary_core::record::MarkupSource::empty(),
        Some(mk_source(
            mail.from.email,
            mail.subject,
            made_at,
            "e2e-message-hash",
            "",
            "",
        )),
        Vec::new(),
        Vec::new(),
        "",
    )?)
}

/// The header of the fixture's instance file, saying why each part is shaped as it is.
const FIXTURE_HEADER: &str = "\
# GENERATED by `suzatary-dev fixture-chain`. A fixture deployment: every path
# below is under a temp directory that goes away with the run.
#
# TWO CHAINS, like the real deployment (§10.2). The first is the cast's
# fixture every existing scene reads; the second is EMPTY, which is the
# interesting one — the browser suite's evidence that switching to a chain
# with no objects shows an empty list rather than the other chain's todos,
# and that a todo added there lands in `events-suzatary/` and nowhere else.
#
# BOTH LOGINS ARE VIEWERS; only the writer is in `act_as`. That is the whole
# difference between a READING fixture server and the writing one, and it is
# the production rule rather than a test one: reading is `viewers`, writing
# is the strictly narrower door, and a login absent from it gets the real 403.
#
# No certificate: the binary refuses to run the fixture with one, so a
# fixture behind a real certificate is a fixture somebody could reach on
# purpose.
#
# THE SECRETARY (§10.3): `model` is a NAME nothing runs — the fixture server
# is handed `--model-fixture`, whose scripted arm answers instead. Naming it
# anyway keeps this file the same shape as the real one.
#
# A STATE FILE THAT DOES NOT EXIST. `state.json` is dedup metadata this
# fixture has never needed to write, so the disposition badge and every
# `suzatary mail new` path answer their ordinary absence.
#
# THE MAILDIR IS THE FIXTURE'S OWN (`fixturemailisolation`): `mail_fixture`
# seeds it and names its own notmuch config, so a search here can never
# answer with this box's real mail — and when it could not (no `notmuch` on
# `$PATH`, or `SUZATARY_FIXTURE_NO_MAIL=1`), `mail.notmuch_config` is simply
# absent below and the corpus refuses every search on purpose.
";

/// The fixture server's whole deployment with every path inside `root`, so a fixture can
/// never touch the live chains or the box's mail.
fn instance_file(
    root: &Path,
    port: u16,
    repo: &Path,
    mail_config: Option<PathBuf>,
    settings: bool,
    propose: bool,
    lapsed: bool,
) -> File {
    let actor = |name: &str| Actor::new(name).expect("the fixture's actors are actors");
    let roster = || {
        ["assistant", "backfill", CAST.owner.actor, "triage"]
            .map(actor)
            .to_vec()
    };
    let chain = |name: &str, events: &str, proposals: &str| Chain {
        name: ChainName::new(name).expect("a slug"),
        events: PathBuf::from(events),
        proposals: PathBuf::from(proposals),
        untrusted: vec![actor("triage")],
        actors: roster(),
        owner: actor(CAST.owner.actor),
    };
    let web = Web {
        viewers: vec![
            Login::from_whois(WRITER_LOGIN),
            Login::from_whois(READER_LOGIN),
        ],
        app_url: format!("http://{HOST}:{port}"),
        act_as: BTreeMap::from([(Login::from_whois(WRITER_LOGIN), actor(WRITER_ACTOR))]),
        serving: Serving {
            host: HOST.to_owned(),
            port,
            tls_dir: root.join("tls"),
            dist: repo.join("web/dist"),
            briefings: root.join("briefings"),
        },
    };
    let mail = Mail {
        root: root.join("Mail"),
        state: root.join("state.json"),
        notmuch: PathBuf::from("notmuch"),
        notmuch_config: mail_config,
        mbsyncrc: root.join("mbsyncrc"),
        heartbeat: root.join("triage-heartbeat"),
        accounts: mail_accounts(lapsed),
        junk: suzatary_core::mail::junk::ChurnList::default(),
    };
    File {
        repo: root.to_path_buf(),
        data: root.to_path_buf(),
        chains: vec![
            chain("life", "events", "events-triage"),
            chain("suzatary", "events-suzatary", "events-triage-suzatary"),
        ],
        web,
        categories: BTreeMap::from([
            ("work".to_owned(), 0),
            ("study".to_owned(), 1),
            ("project".to_owned(), 2),
            ("home".to_owned(), 3),
            ("other".to_owned(), 4),
        ]),
        mail,
        secretary: Secretary {
            conversations: root.join("secretary"),
            renders: root.join("secretary-render"),
            inbox: root.join("inbox"),
            memory: root.join("memory"),
            model: ModelName::parse("scripted").expect("a model name"),
            // A fake that keeps a private upload and refuses every other read, never the box's
            // Drive.
            drive_cli: Some(root.join(FAKE_DRIVE)),
            ..Secretary::default()
        },
        agent: crate::instance::Triage::default(),
        persona: suzatary_core::persona::Persona::default(),
        // Absent, so push setup never reads this box's secret store.
        notify: None,
        // A routine nothing can fire, so a tapped session fails in words.
        coding: (settings && propose).then(|| suzatary_core::coding::Coding {
            routine: "trig01fixture".parse().expect("a routine id"),
            repo: "bmabsout/fixture".parse().expect("a repository"),
            chain: ChainName::new("suzatary").expect("a slug"),
        }),
        cache: None,
        plugins: Vec::new(),
        signatures: crate::instance::Signatures {
            root: fixture_root(),
        },
    }
}

/// What `suzatary-dev fixture-chain` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The temp directory to build in.
    #[arg(long)]
    root: PathBuf,
    /// The port the fixture server will bind.
    #[arg(long)]
    port: u16,
    /// The Tailscale login the fixture whois answers with; the default
    /// may write.
    #[arg(long, default_value_t = WRITER_LOGIN.to_owned())]
    login: String,
    /// The checkout `instance.toml`'s `dist` points into.
    #[arg(long)]
    repo: PathBuf,
    /// Leave one triage card waiting in the Inbox; off by default so a server can show
    /// the empty Inbox.
    #[arg(long)]
    propose: bool,
    /// Write today's review: the open triage card held with a reason.
    #[arg(long, requires = "propose")]
    reviewed: bool,
    /// Declare a calendar and write the fixture's own `secretspec.toml`; the caller must isolate
    /// `$HOME` and `$SECRETSPEC_PROVIDER`.
    #[arg(long)]
    settings: bool,
    /// Whether the fixture's model answered its last call; `unanswered` makes the box file the
    /// card that connects a model.
    #[arg(long, value_enum, default_value_t = Model::Answers)]
    model: Model,
    /// What is connected beyond the fixture's own: `calendar` connects one whose link is not
    /// set and leaves its removal standing as a card.
    #[arg(long, value_enum, default_value_t = Connected::Nothing, requires = "settings")]
    connected: Connected,
    /// What the connections beyond the fixture's own stand at: `lapsed` adds a Microsoft
    /// account that reads its mail and holds no grant to send.
    #[arg(long, value_enum, default_value_t = Connections::Fresh, requires = "settings")]
    connections: Connections,
    /// How many more lines to write to the conversations, generated, for measuring a tab that
    /// holds a large store: half in one conversation, the rest in smaller ones.
    #[arg(long, default_value_t = 0)]
    lines: usize,
    /// How many conversations the lines are spread over, the shape of a store a year old: the
    /// first holds up to 400 of them and the rest are shared evenly. Zero halves them instead.
    #[arg(long, default_value_t = 0, requires = "lines")]
    conversations: usize,
    /// How many more conversations to open, one line each, so the store has that many heads: a
    /// tab holding it names its replica by a frontier no longer than it would name one head by.
    #[arg(long, default_value_t = 0)]
    wide: usize,
}

/// Each bulk conversation and how many lines it holds. With no `conversations`, half the `count`
/// lines in `bulk-0` and each next one half the size of the last; else up to 400 in `bulk-0` and
/// the rest shared evenly, at least two to a conversation (its opening and a question), so fewer
/// conversations where the lines do not stretch. Then `wide` of one line each.
fn sizes(count: usize, conversations: usize, wide: usize) -> Vec<(String, usize)> {
    let mut sizes = Vec::new();
    if conversations == 0 {
        let mut left = count;
        while left > 0 {
            let size = (left / 2).max(left.min(20));
            sizes.push(size);
            left -= size;
        }
    } else {
        let first = count.min(400);
        sizes.push(first);
        let rest = count - first;
        let others = (conversations - 1).min(rest / 2);
        sizes.extend((0..others).map(|n| rest / others + usize::from(n < rest % others)));
    }
    let mut named: Vec<(String, usize)> = sizes
        .into_iter()
        .filter(|size| *size > 0)
        .enumerate()
        .map(|(n, size)| (format!("bulk-{n}"), size))
        .collect();
    // `bulk-0` begins the store, so a store of `wide` conversations alone opens it with one line.
    if named.is_empty() && wide > 0 {
        named.push(("bulk-0".to_owned(), 1));
    }
    named.extend((0..wide).map(|n| (format!("wide-{n}"), 1)));
    named
}

/// The lines `sizes` names written to new conversations, each with its words locked under a key of its own
/// as the box locks them: a model's answer of about 6 kB in three blocks and a person's question,
/// alternating; `wide-0` onwards, if any, hold their opening line alone.
fn bulk_lines(read: &crate::instance::Instance, sizes: &[(String, usize)]) -> anyhow::Result<()> {
    use prodrome_core::event::{Actor, canonical_envelope};
    use prodrome_core::store::{MemoryStore, Replica as _};
    use suzatary_core::capability::Turn;
    use suzatary_core::secretary::line_event::LineEvent;
    use suzatary_core::secretary::store::{ConversationId, Entry, Kind};
    use suzatary_core::secretary::words::Words;

    let count: usize = sizes.iter().map(|(_, size)| size).sum();
    if count == 0 {
        return Ok(());
    }
    let stores = read.conversation_stores();
    let conversations = stores.conversations();
    let opened = |n: usize, at| Entry::Opened {
        at,
        title: Words::kept(format!("Bulk conversation {n}")),
        model: "scripted/fixture".to_owned(),
        kind: Kind::Topic,
    };
    // Through the store itself first, which begins it if nothing has.
    let start = fpl::instant_of(SystemClock.now()) - Duration::seconds(i64::try_from(count)? + 60);
    suzatary_core::secretary::transcript::start(
        conversations,
        &ConversationId::new("bulk-0")?,
        &opened(0, fpl::datetime_of(start)?),
    )?;
    let held = conversations.lines.objects()?;
    let memory: MemoryStore<LineEvent> = MemoryStore::default();
    let prints: std::collections::BTreeMap<Hash, Vec<u8>> = held
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object).into_bytes()))
        .collect();
    memory.receive(prints.keys().cloned().collect(), &|name| {
        prints.get(name).cloned()
    })?;
    let writer = Actor::new(suzatary_core::secretary::WRITES_AS.actor())?;
    let mut written = 0u32;
    for (n, (id, size)) in sizes.iter().enumerate() {
        let id = ConversationId::new(id.clone())?;
        // The first conversation's opening line is the store's own, above.
        for line in usize::from(n == 0)..*size {
            written += 1;
            // Every line a second after the one before, all before now, so none is dated in the future.
            let at = fpl::datetime_of(start + Duration::seconds(i64::from(written)))?;
            let entry = if line == 0 {
                opened(n, at)
            } else if line % 2 == 1 {
                Entry::Said {
                    at,
                    turn: Turn::Person {
                        text: format!("Question {line} about the permit, asked again."),
                        references: Vec::new(),
                        attachments: Vec::new(),
                    },
                    by: Some("ada".into()),
                }
            } else {
                Entry::Said {
                    at,
                    turn: Turn::Model {
                        text: [0, 1, 2]
                            .map(|part| {
                                format!("Answer {line}, part {part}: the registry has the form. ")
                                    .repeat(30)
                            })
                            .join("\n\n"),
                        calls: Vec::new(),
                    },
                    by: None,
                }
            };
            let held = u32::try_from(line)?;
            let (event, keys) = LineEvent::write(
                id.clone(),
                writer.clone(),
                held,
                &entry,
                conversations.entropy,
            )?;
            for key in &keys {
                conversations.keys.set(&key.name(), key)?;
            }
            memory.append(event)?;
        }
    }
    let all = memory.held()?.dag;
    let new: Vec<(Hash, String)> = all
        .objects()
        .iter()
        .filter(|(name, _)| !prints.contains_key(*name))
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();
    conversations.lines.accept(&new)?;
    Ok(())
}

/// What the `--settings` fixture's further connections stand at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Connections {
    /// None beyond the fixture's own.
    Fresh,
    /// An account signed in to read and not to send.
    Lapsed,
}

/// The Microsoft account `--connections lapsed` adds, signed in to read and not to send.
pub const LINKS_ACCOUNT: &str = "ada-work@example.test";

/// What a fixture has connected beyond its own tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Connected {
    /// Nothing more.
    Nothing,
    /// A calendar whose link is not set, and its removal standing.
    Calendar,
    /// A Google account's Connect card standing, as the setup agent files it, for the scene
    /// that signs it in with the fake Google (`suzatary-dev fake-google`).
    Google,
}

/// What the fixture's model health says of its scripted model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Model {
    /// It answers, as a scripted model does.
    Answers,
    /// Its last call failed: no model answers on this box.
    Unanswered,
}

/// `suzatary-dev fixture-chain`: build the chain and print what the suite needs.
pub fn run(args: &Args) -> anyhow::Result<()> {
    build(args)
}

/// What `suzatary-dev fixture-triage` was told.
#[derive(Debug, clap::Args)]
pub struct TriageArgs {
    /// The fixture deployment's instance file.
    #[arg(long)]
    instance: PathBuf,
    /// Instead, add and complete this todo at 40%, filed as its own card.
    #[arg(long)]
    hold: Option<String>,
}

/// `suzatary-dev fixture-triage`: run the agent's writer again, so a scene can check that the
/// same acts file no second card, or with `--hold` leave a card a writing login can tap.
///
/// # Errors
/// The instance being unreadable.
pub fn triage(args: &TriageArgs) -> anyhow::Result<()> {
    let ran = match &args.hold {
        Some(todo) => held_run(&args.instance, todo),
        None => triage_run(&args.instance),
    };
    match ran {
        Ok(()) => {
            println!("{}", serde_json::json!({ "ran": true, "refused": "" }));
            Ok(())
        }
        // A refusal is the answer the scene asks about, so it is printed and exits zero.
        Err(refused) => {
            println!(
                "{}",
                serde_json::json!({ "ran": false, "refused": refused.to_string() })
            );
            Ok(())
        }
    }
}

/// A model that reads its answers off a script, for tests that cannot depend on a real
/// model; everything past the model is real.
///
/// Lines are matched, not consumed in order, so no scene depends on another: the first
/// whose `when` is a substring of the last turn answers, and an empty `when` is the fallback.
#[derive(Debug)]
pub struct Scripted {
    lines: Vec<Line>,
    name: String,
}

/// One scripted answer; the reply is raw text through the real `Reply::parse`, so a
/// malformed script fails as a malformed model answer would.
#[derive(Debug, serde::Deserialize)]
struct Line {
    #[serde(default)]
    when: String,
    reply: String,
    /// A file the model waits for before it answers, and deletes as it does, so a scene sees the
    /// round running for as long as it looks; the round's own halt, pulled before the model was
    /// asked or while it waits, ends the wait as well, so a scene's Stop always comes before the
    /// answer, which the box then drops.
    #[serde(default)]
    release: Option<PathBuf>,
}

impl Scripted {
    /// A script written in code: `(when, reply)`, matched as a file's are.
    #[must_use]
    pub fn answering(lines: &[(&str, String)]) -> Self {
        Self {
            lines: lines
                .iter()
                .map(|(when, reply)| Line {
                    when: (*when).to_owned(),
                    reply: reply.clone(),
                    release: None,
                })
                .collect(),
            name: "scripted/fixture".to_owned(),
        }
    }

    /// Read a script.
    ///
    /// # Errors
    /// A file that is not there, or one that is not a list of `{when, reply}`.
    pub fn reading(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading the model script {}", path.display()))?;
        let lines: Vec<Line> = serde_json::from_str(&text)
            .with_context(|| format!("{} is a list of {{when, reply}}", path.display()))?;
        Ok(Self {
            lines,
            name: format!("scripted/{}", path.display()),
        })
    }

    /// What the last turn offers to match on: its text, or the verb that just answered.
    ///
    /// A question with a reference attached ends on the `reference` verb's answer, since
    /// the box loads it first, so such a scene's script needs a line for that.
    fn cue(conversation: &suzatary_core::capability::Conversation) -> String {
        match conversation.turns.last() {
            Some(
                suzatary_core::capability::Turn::Person { text, .. }
                | suzatary_core::capability::Turn::Model { text, .. }
                | suzatary_core::capability::Turn::Act { text, .. },
            ) => text.clone(),
            Some(suzatary_core::capability::Turn::Tool { verb, .. }) => verb.to_string(),
            Some(suzatary_core::capability::Turn::Erased { words, .. }) => {
                suzatary_core::secretary::turn_shape::prose(words)
            }
            None => String::new(),
        }
    }
}

impl suzatary_core::capability::Model for Scripted {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn turn(
        &self,
        conversation: &suzatary_core::capability::Conversation,
        _tools: &serde_json::Value,
    ) -> Result<suzatary_core::capability::Reply, suzatary_core::capability::ModelError> {
        let cue = Self::cue(conversation);
        let line = self
            .lines
            .iter()
            .find(|line| line.when.is_empty() || cue.contains(&line.when))
            .ok_or_else(|| {
                // An uncovered cue is a test bug, refused rather than improvised.
                suzatary_core::capability::ModelError::Failed(format!(
                    "the script has no line for {cue:?}"
                ))
            })?;
        if let Some(release) = &line.release {
            // Until the scene lets it answer, or a Stop pulls the round's halt, whose reply the box drops.
            while !suzatary_core::capability::pulled::halted()
                && std::fs::remove_file(release).is_err()
            {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        Ok(suzatary_core::capability::Reply::parse(&line.reply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-fixture-mail-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a writable temp directory");
        dir
    }

    /// Whether a script line that waits for a release nobody writes, asked under `halt`, answered
    /// within five seconds; `pull_after` pulls `halt` that long after it is asked. A line still
    /// waiting is then released, so no test leaves it polling.
    fn line_answers(
        name: &str,
        halt: &suzatary_core::capability::Halt,
        pull_after: Option<std::time::Duration>,
    ) -> bool {
        use suzatary_core::capability::Model;
        let release = scratch(name).join("release");
        let script = Scripted {
            lines: vec![Line {
                when: String::new(),
                reply: r#"{"say": "stopped before it landed"}"#.to_owned(),
                release: Some(release.clone()),
            }],
            name: "scripted/test".to_owned(),
        };
        let (answered, heard) = std::sync::mpsc::channel();
        let asking = halt.clone();
        std::thread::spawn(move || {
            let _asking = suzatary_core::capability::pulled::Asking::of(&asking);
            let asked = suzatary_core::capability::Conversation {
                system: String::new(),
                turns: Vec::new(),
            };
            let _sent = answered.send(script.turn(&asked, &serde_json::json!([])).is_ok());
        });
        if let Some(after) = pull_after {
            std::thread::sleep(after);
            halt.stop(prodrome_core::event::Actor::new("ada").expect("an actor"));
        }
        let within = heard.recv_timeout(std::time::Duration::from_secs(5));
        if within.is_err() {
            std::fs::write(&release, "").expect("the release is written");
            let _ended = heard.recv();
        }
        within.unwrap_or(false)
    }

    /// The scripted model's wait ends on its own round's halt, pulled before it was asked or
    /// while it waits, and never on another round's.
    #[test]
    fn a_waiting_script_line_ends_on_its_own_rounds_halt_whenever_it_is_pulled() {
        let ada = || prodrome_core::event::Actor::new("ada").expect("an actor");
        let before = suzatary_core::capability::Halt::new();
        before.stop(ada());
        assert!(
            line_answers("halt-before", &before, None),
            "pulled before it was asked"
        );
        assert!(
            line_answers(
                "halt-during",
                &suzatary_core::capability::Halt::new(),
                Some(std::time::Duration::from_millis(200))
            ),
            "pulled while it waits"
        );
        suzatary_core::capability::Halt::new().stop(ada());
        assert!(
            !line_answers("halt-other", &suzatary_core::capability::Halt::new(), None),
            "another round's halt, pulled, does not end it"
        );
    }

    #[test]
    fn suzatary_fixture_no_mail_skips_the_database_entirely() {
        let root = scratch("no-mail");
        let built = mail_fixture(&root, true);
        assert!(built.expect("no error, just no mail").is_none());
        assert!(!root.join("notmuch-config").exists());
        assert!(!root.join("Mail").exists());
    }

    #[test]
    fn a_relative_fixture_root_is_anchored_before_notmuch_sees_it() {
        let anchored = fixture_mail_root(Path::new("scratch/fixture")).expect("cwd is readable");
        assert!(anchored.is_absolute(), "{}", anchored.display());
        assert!(
            anchored.ends_with("scratch/fixture/Mail"),
            "{}",
            anchored.display()
        );
        let cwd = std::env::current_dir().expect("cwd is readable");
        assert!(
            anchored.starts_with(&cwd),
            "anchored to the working directory, never $HOME"
        );
    }

    #[test]
    fn a_configured_fixture_seeds_a_searchable_database() {
        let root = scratch("seeded");
        let config = mail_fixture(&root, false)
            .expect("notmuch is on PATH in the shell these tests run in")
            .expect("a config when notmuch is present and nothing muted it");
        assert!(config.exists());

        let output = Command::new("notmuch")
            .args([
                "search",
                "--format=json",
                "--output=messages",
                &CAST.counsel.name.to_lowercase(),
            ])
            .env("NOTMUCH_CONFIG", &config)
            .output()
            .expect("notmuch runs against the fixture's own config");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ids: Vec<String> = serde_json::from_slice(&output.stdout).expect("notmuch's json");
        assert_eq!(
            ids.len(),
            1,
            "exactly the one seeded message from counsel, never this box's mail"
        );
    }

    /// Deleting an account's mail tells the index before the tap answers, so the inbox reads
    /// without it and a message of it reads as no longer stored, never as the box's error.
    #[test]
    fn forgotten_mail_reads_as_no_longer_stored_at_once() {
        use suzatary_core::capability::{MailError, Mailbox as _};
        use suzatary_core::mail::cache::Forget as _;

        let root = scratch("forgotten");
        let config = mail_fixture(&root, false)
            .expect("notmuch is on PATH in the shell these tests run in")
            .expect("a config when notmuch is present and nothing muted it");
        let mail_root = fixture_mail_root(&root).expect("the mail root");
        let account = MailAccount {
            email: AccountEmail::new(CAST.owner.email)
                .expect("the cast's address is a lower-case address"),
            subdir: MAIL_SUBDIR.to_owned(),
            sync: Sync::Gmail,
        };
        let mailbox = suzatary_io::mailbox::Notmuch::new(
            &mail_root,
            suzatary_io::state::Ledger::new(
                root.join("state.json"),
                root.join("local.json"),
                || {
                    suzatary_core::mail::dedup_key::DedupKey::of(
                        &suzatary_core::capability::Secret::new("3c".repeat(32)),
                    )
                    .map_err(|refused| refused.to_string())
                },
            ),
            Roster::new(vec![account.clone()]).expect("one account is a roster"),
        )
        .with_config(&config);
        let id = CAST.mail[0].msgid;
        let first = mailbox.message(id);
        assert!(first.is_ok(), "the seeded message reads first: {first:?}");

        let forgot = crate::commands::serve::mail_cache::MailCache::new(
            mail_root,
            PathBuf::from("notmuch"),
            Some(config),
            root.join("sync.lock"),
            suzatary_io::oauth::TokenStore::new(root.join("oauth")),
        )
        .forget(&account, suzatary_core::mail::removal::Removal::Mail);
        assert!(
            matches!(
                forgot,
                suzatary_core::mail::cache::Forgotten::Mail(
                    suzatary_core::mail::cache::Forgot::Deleted { .. }
                )
            ),
            "{forgot:?}"
        );

        let inbox =
            suzatary_core::routes::inbox(&mailbox, None, 50, None).expect("the inbox reads");
        assert!(inbox.messages.is_empty(), "{:?}", inbox.messages);
        assert!(
            matches!(mailbox.message(id), Err(MailError::Absent { .. })),
            "a message of it is absent, which the route says is no longer stored"
        );
    }
}
