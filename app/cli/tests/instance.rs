//! The instance file's parse, as text: every field of a live-shaped instance
//! as the binary reads it, then every startup refusal, compared with
//! `tests/instance.golden`, so a change to how the file is parsed shows as a
//! diff of what it reads and what it refuses.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use suzatary_cli::instance::Instance;

/// Every table and key the owner's file uses, with the cast's names.
const LIVE_SHAPED: &str = r#"
repo = "/srv/code"
data = "/srv/data"

[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["ada", "assistant", "triage"]
owner = "ada"

[[chains]]
name = "suzatary"
events = "events-suzatary"
untrusted = []
actors = ["ada", "assistant", "triage"]
owner = "ada"

[web]
viewers = ["her@example.test", "phone@example.test"]
app_url = "https://box.example.test/"

[web.act_as]
"her@example.test" = "ada"
"phone@example.test" = "assistant"

[web.serving]
host = "100.64.0.1"
port = 443
tls_dir = "/var/lib/suzatary/tls"
dist = "web/dist"
briefings = "briefings"

[categories]
home = 0
work = 1

[mail]
root = "Mail"
state = "state.json"
notmuch = "/run/current-system/sw/bin/notmuch"
notmuch_config = "notmuch-config"
mbsyncrc = "mbsyncrc"
heartbeat = ".local/state/triage-heartbeat"

[[mail.accounts]]
email = "her@example.test"
subdir = "her-gmail"
sync = "gmail"

[[mail.accounts]]
email = "work@example.test"
subdir = "work"
sync = "ms-imap"
authority = "organizations"
inbox_folders = ["Inbox", "Other"]
sent_folder = "Sent Items"

[[mail.accounts]]
email = "private@example.test"
subdir = "proton"
sync = "proton"
channel = "proton"
smtp_user = "private@example.test"
send_addresses = ["private@example.test", "alias@example.test"]
inbox_folders = ["INBOX"]

[mail.junk]
churn = ["Offers.Example.NET"]
keep = ["deals.offers.example.net"]

[secretary]
conversations = "secretary"
renders = "secretary-render"
inbox = "inbox"
memory = "memory"
model = "claude-opus-5-5"
reserve = 0.4
settings = ".claude/secretary-settings.json"
web_allow = ["*.gov.uk", "example.org"]
drive_allow = ["/my-files/Docs/"]
drive_cli = "bin/proton-drive"

[[secretary.repos]]
name = "bmabsout/Suzatary"

[[secretary.repos]]
name = "bmabsout/resume_typst"
check = "typst compile cv.typ"
public = true

[persona]
name = "Ada"
city = "London"
zone = "Europe/London"

[notify]
channels = ["fcm", "ntfy"]
fcm_sender = "100000000000"

[coding]
routine = "trig_01fake"
repo = "bmabsout/Suzatary"
chain = "suzatary"
"#;

/// Each fact the binary read, one `name = value` line each.
// One line per fact is the golden file's shape, so this is not split.
#[allow(clippy::too_many_lines)]
fn printed(instance: &Instance) -> String {
    let mut out = String::new();
    let mut line = |name: &str, value: String| {
        let _ = writeln!(out, "{name} = {value}");
    };
    let path = |path: &Path| path.display().to_string();
    line("code", path(instance.code()));
    line("data", path(instance.data()));
    for chain in &instance.chains {
        let name = chain.policy.name.as_str();
        line(&format!("chain.{name}.events"), path(&chain.events));
        line(
            &format!("chain.{name}.untrusted"),
            format!("{:?}", chain.policy.untrusted_names()),
        );
        line(
            &format!("chain.{name}.roster"),
            format!(
                "{:?}",
                chain
                    .policy
                    .roster
                    .iter()
                    .map(prodrome_core::event::Actor::as_str)
                    .collect::<Vec<_>>()
            ),
        );
        line(
            &format!("chain.{name}.owner"),
            chain.owner.as_str().to_owned(),
        );
    }
    let deployment = &instance.deployment;
    line(
        "deployment.chains",
        format!("{:?}", deployment.chains.names()),
    );
    line(
        "deployment.viewers",
        format!(
            "{:?}",
            deployment
                .viewers
                .iter()
                .map(suzatary_core::capability::Login::as_str)
                .collect::<Vec<_>>()
        ),
    );
    for (login, actor) in &deployment.actors {
        line(
            &format!("deployment.act_as.{login}"),
            actor.as_str().to_owned(),
        );
    }
    line(
        "deployment.categories",
        format!("{:?}", deployment.categories),
    );
    line("deployment.app_url", deployment.app_url.clone());
    line(
        "instance.settables",
        format!("{:?}", instance.settables.names()),
    );
    line("deployment.coding", format!("{:?}", deployment.coding));
    let serving = &instance.serving;
    line("serving.host", serving.host.clone());
    line("serving.port", serving.port.to_string());
    line("serving.tls_dir", path(&serving.tls_dir));
    line("serving.dist", path(&serving.dist));
    line("serving.briefings", path(&serving.briefings));
    let secretary = &instance.secretary;
    line("secretary.conversations", path(&secretary.conversations));
    line("secretary.renders", path(&secretary.renders));
    line("secretary.inbox", path(&secretary.inbox));
    line("secretary.memory", path(&secretary.memory));
    line("secretary.model", secretary.model.as_str().to_owned());
    line("secretary.reserve", secretary.reserve.get().to_string());
    line(
        "secretary.compacting",
        format!("{:?}", secretary.compacting()),
    );
    line(
        "secretary.settings",
        secretary.settings.display().to_string(),
    );
    line(
        "secretary.web_allow",
        format!(
            "{:?}",
            secretary
                .web_allow
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        ),
    );
    line(
        "secretary.drive_allow",
        format!(
            "{:?}",
            secretary
                .drive_allow
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        ),
    );
    line("secretary.drive_cli", format!("{:?}", secretary.drive_cli));
    line("secretary.repos", format!("{:?}", secretary.repos));
    line("mail.root", path(&instance.mail.root));
    line("mail.state", path(&instance.mail.state));
    line("mail.notmuch", path(&instance.mail.notmuch));
    line(
        "mail.notmuch_config",
        format!("{:?}", instance.mail.notmuch_config),
    );
    line("mail.mbsyncrc", path(&instance.mail.mbsyncrc));
    line("mail.heartbeat", path(&instance.mail.heartbeat));
    for account in instance.mail.accounts.accounts() {
        line(
            &format!("mail.account.{}", account.email),
            format!("{account:?}"),
        );
    }
    line("mail.junk", format!("{:?}", instance.mail.junk));
    line("persona", format!("{:?}", instance.persona));
    line("local", path(instance.local.path()));
    line(
        "notify",
        format!(
            "{:?}",
            suzatary_core::nonempty::NonEmpty::listed(instance.notify.as_ref())
        ),
    );
    line("portal", format!("{:?}", instance.portal));
    out
}

/// Every startup refusal: a name, and the edit to the live-shaped file that
/// provokes it, `from` replaced by `to` (an empty `from` is the whole file).
/// The few edits the parse lets through are here too, so a change to what it
/// lets through shows.
const REFUSALS: &[(&str, &str, &str)] = &[
    ("not toml", "", "repo = "),
    ("no repo", "repo = \"/srv/code\"\n", ""),
    (
        "no root: every deployment signs",
        "[signatures]\nroot = \"836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818\"\n",
        "",
    ),
    (
        "an unknown key in a chain",
        "untrusted = []\n",
        "untrusted = []\ncolour = 1\n",
    ),
    ("no chains", "[[chains]]", "[[nothing]]"),
    (
        "a chain name that is not a slug",
        "name = \"suzatary\"",
        "name = \"../etc\"",
    ),
    (
        "a reserved chain name",
        "name = \"suzatary\"",
        "name = \"act\"",
    ),
    (
        "a chain named twice",
        "name = \"suzatary\"",
        "name = \"life\"",
    ),
    (
        "an actor that is not one",
        "actors = [\"ada\", \"assistant\", \"triage\"]\nowner = \"ada\"\n\n[web]",
        "actors = [\"Ada\"]\nowner = \"ada\"\n\n[web]",
    ),
    ("no owner", "owner = \"ada\"\n", ""),
    (
        "an owner off the roster",
        "owner = \"ada\"",
        "owner = \"stranger\"",
    ),
    (
        "an untrusted owner",
        "owner = \"ada\"",
        "owner = \"triage\"",
    ),
    (
        "an untrusted actor off the roster",
        "untrusted = [\"triage\"]",
        "untrusted = [\"stranger\"]",
    ),
    (
        "a writer who is not a viewer",
        "\"phone@example.test\" = \"assistant\"",
        "\"phone@example.test\" = \"assistant\"\n\"ghost@example.test\" = \"ada\"",
    ),
    (
        "a writer's actor off the roster",
        "\"phone@example.test\" = \"assistant\"",
        "\"phone@example.test\" = \"nobody\"",
    ),
    (
        "a writer's actor that is not one",
        "\"phone@example.test\" = \"assistant\"",
        "\"phone@example.test\" = \"No Body\"",
    ),
    ("an unknown sync", "sync = \"gmail\"", "sync = \"pigeon\""),
    (
        "a gmail account with folders",
        "sync = \"gmail\"",
        "sync = \"gmail\"\ninbox_folders = [\"Inbox\"]",
    ),
    (
        "an unknown key under ms-imap",
        "sent_folder = \"Sent Items\"",
        "sent_folder = \"Sent Items\"\ncolour = 1",
    ),
    (
        "an unknown authority",
        "authority = \"organizations\"",
        "authority = \"everyone\"",
    ),
    (
        "two accounts at one address",
        "email = \"work@example.test\"",
        "email = \"her@example.test\"",
    ),
    (
        "an account address with a capital",
        "email = \"work@example.test\"",
        "email = \"Work@example.test\"",
    ),
    (
        "an account that counts no folder",
        "inbox_folders = [\"Inbox\", \"Other\"]",
        "inbox_folders = []",
    ),
    (
        "a blank inbox folder",
        "inbox_folders = [\"Inbox\", \"Other\"]",
        "inbox_folders = [\"Inbox\", \" \"]",
    ),
    (
        "a churn domain that is not one",
        "churn = [\"Offers.Example.NET\"]",
        "churn = [\"not a domain\"]",
    ),
    (
        "a keep domain that is not one",
        "keep = [\"deals.offers.example.net\"]",
        "keep = [\"not a domain\"]",
    ),
    (
        "an unknown key in [mail.junk]",
        "keep = [",
        "colour = 1\nkeep = [",
    ),
    (
        "a bad model name",
        "model = \"claude-opus-5-5\"",
        "model = \"claude opus\"",
    ),
    (
        "a bad web rule",
        "web_allow = [\"*.gov.uk\", \"example.org\"]",
        "web_allow = [\"http://\"]",
    ),
    (
        "a bad drive folder",
        "drive_allow = [\"/my-files/Docs/\"]",
        "drive_allow = [\"my-files/../trash\"]",
    ),
    (
        "a bad repo name",
        "name = \"bmabsout/resume_typst\"",
        "name = \"nope\"",
    ),
    (
        "an unknown key in [secretary]",
        "model = \"claude-opus-5-5\"",
        "model = \"claude-opus-5-5\"\ncolour = 1",
    ),
    (
        "an unknown key in [persona]",
        "city = \"London\"",
        "city = \"London\"\ncolour = 1",
    ),
    (
        "the person's prose in the pushed [persona]",
        "city = \"London\"",
        "city = \"London\"\nemail_voice = [\"short\"]",
    ),
    (
        "a zone no database names",
        "zone = \"Europe/London\"",
        "zone = \"Mars/Olympus\"",
    ),
    (
        "the offset a zone replaced",
        "zone = \"Europe/London\"",
        "timezone_offset_hours = -4",
    ),
    (
        "a channel no template connects",
        "channels = [\"fcm\", \"ntfy\"]",
        "channels = [\"fcm\", \"carrier-pigeon\"]",
    ),
    (
        "an unknown key in [notify]",
        "channels = [\"fcm\", \"ntfy\"]",
        "channels = []\ncolour = 1",
    ),
    (
        "a Firebase sender that is no number",
        "fcm_sender = \"100000000000\"",
        "fcm_sender = \"sender-1\"",
    ),
    (
        "a model provider, a table the binary does not read",
        "[coding]",
        "[[providers]]\nname = \"phala\"\nbase_url = \"https://inference.phala.com/v1\"\n\
         key = \"PHALA_API_KEY\"\nattested = false\n\n[coding]",
    ),
    (
        "a model order, a table the binary does not read",
        "[coding]",
        "[models]\nsecretary = [\"claude-cli/claude-opus-5-5\"]\nreserve = 0.4\n\n[coding]",
    ),
    ("a reserve over one", "reserve = 0.4", "reserve = 1.5"),
    (
        "a coding chain not served",
        "chain = \"suzatary\"\n",
        "chain = \"work\"\n",
    ),
    (
        "a coding routine that is not one",
        "routine = \"trig_01fake\"",
        "routine = \"trig 01\"",
    ),
    (
        "a coding repo that is not one",
        "repo = \"bmabsout/Suzatary\"\nchain",
        "repo = \"nope\"\nchain",
    ),
    (
        "a coding repo of another account",
        "repo = \"bmabsout/Suzatary\"\nchain",
        "repo = \"someorg/Suzatary\"\nchain",
    ),
    (
        "a secretary repo of another account",
        "name = \"bmabsout/resume_typst\"",
        "name = \"someorg/resume_typst\"",
    ),
];

/// The live-shaped file with one edit.
fn edited(from: &str, to: &str) -> String {
    if from.is_empty() {
        return to.to_owned();
    }
    assert!(LIVE_SHAPED.contains(from), "{from:?} is in the file");
    LIVE_SHAPED.replace(from, to)
}

/// The refusal as `doctor` and a failed start print it, the file's own path
/// replaced so the text does not depend on where the test ran; `accepted`
/// when the edit is one the parse lets through.
fn refused(dir: &Path, name: &str, text: &str) -> String {
    let file = dir.join(format!("{}.toml", name.replace(' ', "-")));
    std::fs::write(&file, text).expect("writes");
    match Instance::read(&file, &suzatary_cli::paths::Env::under(dir, dir)) {
        Ok(_) => "accepted".to_owned(),
        Err(failed) => format!("{failed:#}").replace(&file.display().to_string(), "<file>"),
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("suzatary-instance-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

#[test]
fn the_instance_file_reads_and_refuses_as_the_golden_file_says() {
    let dir = scratch("golden");
    let file = dir.join("instance.toml");
    std::fs::write(&file, LIVE_SHAPED).expect("writes");
    let read = Instance::read(&file, &suzatary_cli::paths::Env::under(&dir, &dir))
        .expect("the live-shaped instance reads");
    let mut out = format!("# read\n{}", printed(&read));
    out.push_str("\n# refused\n");
    for (name, from, to) in REFUSALS {
        let _ = writeln!(
            out,
            "## {name}\n{}\n",
            refused(&dir, name, &edited(from, to))
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    if std::env::var_os("SUZATARY_UPDATE_INSTANCE").is_some() {
        std::fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/instance.golden"),
            &out,
        )
        .expect("updates");
    }
    assert_eq!(out, include_str!("instance.golden"));
}

/// The compactor is off unless the file turns it on; on, it runs the latest Sonnet within its
/// caps unless the file says otherwise, and `compact = false` turns it off whatever the other
/// keys say.
#[test]
fn the_compactor_is_off_by_default_and_on_by_its_switch() {
    use suzatary_core::secretary::compact_budget::Budget;
    use suzatary_core::secretary::compacting::Compacting;
    let dir = scratch("compacting");
    let file = dir.join("instance.toml");
    let read = |text: &str| {
        std::fs::write(&file, text).expect("writes");
        Instance::read(&file, &suzatary_cli::paths::Env::under(&dir, &dir))
            .expect("reads")
            .secretary
            .compacting()
    };
    let model = "model = \"claude-opus-5-5\"\n";
    assert_eq!(read(LIVE_SHAPED), Compacting::Off);
    let on = edited(model, &format!("{model}compact = true\n"));
    assert_eq!(
        read(&on),
        Compacting::On {
            model: suzatary_core::model_name::ModelName::parse("claude-sonnet-5-5")
                .expect("a model"),
            budget: Budget {
                calls_per_hour: 30,
                bytes_per_day: 32 * 1024 * 1024,
            },
            workers: std::num::NonZeroUsize::MIN,
        }
    );
    let tuned = edited(
        model,
        &format!(
            "{model}compact = true\ncompactor = \"claude-haiku-4-5\"\ncompact_calls_per_hour = 4\ncompact_workers = 4\n"
        ),
    );
    assert!(matches!(
        read(&tuned),
        Compacting::On { budget: Budget { calls_per_hour: 4, .. }, model, workers } if model.as_str() == "claude-haiku-4-5" && workers.get() == 4
    ));
    let off = edited(
        model,
        &format!("{model}compact = false\ncompactor = \"claude-haiku-4-5\"\n"),
    );
    assert_eq!(read(&off), Compacting::Off);
    let _ = std::fs::remove_dir_all(&dir);
}
