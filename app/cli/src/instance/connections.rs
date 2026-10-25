//! A connection's tables written into the instance file's text, and taken out again.
//!
//! Each edit is a function of the text: it reads the tables as [`Declared`], asks the algebra
//! for the tables it must leave ([`Declared::with`], [`Declared::without`],
//! [`Declared::with_origin`]), which refuses a duplicate or a table the file would refuse,
//! writes them with `toml_edit` so every other byte stays, and reads the result back: an edit
//! whose text does not read as the algebra said is refused, never written.

use std::path::Path;

use suzatary_core::connect::{Connection, Declared, Table};
use suzatary_core::plugin::{Origin, PluginName};
use suzatary_core::settings::EditError;
use toml_edit::{ArrayOfTables, DocumentMut, Item, TableLike, Value};

/// `text` with `connection`'s table added.
///
/// # Errors
/// The algebra's refusal, or a file that is not the document this edit expects.
pub(crate) fn add(path: &Path, text: &str, connection: &Connection) -> Result<String, EditError> {
    let expected = declared(path, text)?.with(connection)?;
    let mut doc = parse(text)?;
    match connection.template().table {
        Table::Account(kind) => {
            let account = kind.account(connection.params())?;
            let spelled = toml::to_string(&account)
                .map_err(|failed| EditError::Malformed(failed.to_string()))?;
            let table = parse(&spelled)?.as_table().clone();
            append(table_mut(&mut doc, "mail")?, "accounts", table)?;
        }
        Table::Link(fragment) => {
            let credential = link(connection)?;
            let plugins = plugins_mut(&mut doc)?;
            let held = plugins
                .iter()
                .position(|plugin| named(plugin, fragment.plugin));
            if let Some(plugin) = held.and_then(|at| plugins.get_mut(at)) {
                push(plugin, "credentials", &credential)?;
            } else {
                let mut plugin = toml_edit::Table::new();
                plugin["name"] = toml_edit::value(fragment.plugin);
                plugin["component"] = toml_edit::value(fragment.component);
                plugin["schema"] = toml_edit::value(fragment.schema);
                plugin["origins"] = toml_edit::value(
                    fragment
                        .origins
                        .iter()
                        .copied()
                        .collect::<toml_edit::Array>(),
                );
                push(&mut plugin, "credentials", &credential)?;
                plugins.push(plugin);
            }
        }
        Table::Folder => push(
            table_mut(&mut doc, "secretary")?,
            "drive_allow",
            &connection.key().name,
        )?,
        Table::Channel(channel) => {
            push(table_mut(&mut doc, "notify")?, "channels", channel.as_str())?;
        }
        // The algebra refused these before any text was touched.
        Table::Always | Table::Model => {}
    }
    written(path, &doc, &expected)
}

/// `text` without `connection`'s table; a plugin's manifest stays, as the algebra keeps it.
///
/// # Errors
/// The algebra's refusal, or a file that is not the document this edit expects.
pub(crate) fn remove(
    path: &Path,
    text: &str,
    connection: &Connection,
) -> Result<String, EditError> {
    let expected = declared(path, text)?.without(connection)?;
    let mut doc = parse(text)?;
    let key = connection.key();
    match connection.template().table {
        Table::Account(_) => {
            let mail = table_mut(&mut doc, "mail")?;
            let left = take(mail, "accounts", |held| {
                held.get("email").and_then(Item::as_str) == Some(key.name.as_str())
            })?;
            // The roster is a required key: none left is the empty list, spelled.
            if left == 0 {
                mail.insert("accounts", toml_edit::value(toml_edit::Array::new()));
            }
        }
        Table::Link(fragment) => {
            let name = link(connection)?;
            for plugin in plugins_mut(&mut doc)?
                .iter_mut()
                .filter(|plugin| named(plugin, fragment.plugin))
            {
                pull(plugin, "credentials", |held| held == name)?;
            }
        }
        Table::Folder => pull_top(&mut doc, "secretary", "drive_allow", |held| {
            held.parse::<suzatary_core::drive::DrivePath>()
                .is_ok_and(|folder| folder.to_string() == key.name)
        })?,
        Table::Channel(channel) => {
            pull_top(&mut doc, "notify", "channels", |held| {
                held == channel.as_str()
            })?;
        }
        Table::Always | Table::Model => {}
    }
    written(path, &doc, &expected)
}

/// `text` with `plugin` allowed to reach `origin`; the same text when it already may.
///
/// # Errors
/// The algebra's refusal, or a file that is not the document this edit expects.
pub(crate) fn add_origin(
    path: &Path,
    text: &str,
    plugin: &PluginName,
    origin: &Origin,
) -> Result<String, EditError> {
    let before = declared(path, text)?;
    let expected = before.with_origin(plugin, origin)?;
    if expected == before {
        return Ok(text.to_owned());
    }
    let mut doc = parse(text)?;
    let table = plugins_mut(&mut doc)?
        .iter_mut()
        .find(|held| named(held, plugin.as_str()))
        .ok_or_else(|| not_a("plugins", "table naming the plugin"))?;
    match table.get_mut("origins") {
        Some(Item::Value(Value::Array(origins))) => origins.push(origin.to_string()),
        Some(_) => return Err(not_a("plugins.origins", "list")),
        None => {
            table["origins"] = toml_edit::value(toml_edit::Array::from_iter([origin.to_string()]));
        }
    }
    written(path, &doc, &expected)
}

/// The tables connections are read from, as the instance at `path` holding `text` declares
/// them: the same projection the running box reads ([`super::Instance::declared`]).
///
/// # Errors
/// A text that does not read as an instance.
pub(crate) fn declared(path: &Path, text: &str) -> Result<Declared, EditError> {
    let file: super::File =
        toml::from_str(text).map_err(|failed| EditError::Malformed(failed.to_string()))?;
    // The tables are the file's alone, so any environment roots them the same.
    super::Instance::of(path, file, &crate::paths::Env::under("/", "/"))
        .map(|instance| instance.declared())
        .map_err(|failed| EditError::Malformed(format!("{failed:#}")))
}

fn parse(text: &str) -> Result<DocumentMut, EditError> {
    text.parse()
        .map_err(|failed: toml_edit::TomlError| EditError::Malformed(failed.to_string()))
}

/// The edited text, once it reads as `expected`.
fn written(path: &Path, doc: &DocumentMut, expected: &Declared) -> Result<String, EditError> {
    let text = doc.to_string();
    if declared(path, &text)? == *expected {
        Ok(text)
    } else {
        Err(EditError::Malformed(
            "the edit would not read back as the tables it was to leave".to_owned(),
        ))
    }
}

/// The credential a link connection names.
fn link(connection: &Connection) -> Result<String, EditError> {
    connection
        .secrets()
        .into_iter()
        .next()
        .ok_or_else(|| EditError::Malformed(format!("{} names no link", connection.key())))
}

fn not_a(key: &str, what: &str) -> EditError {
    EditError::Malformed(format!("{key} is present but is not a {what}"))
}

fn named(plugin: &toml_edit::Table, name: &str) -> bool {
    plugin.get("name").and_then(Item::as_str) == Some(name)
}

/// The top-level table `key`, made at the end of the file when it has none.
fn table_mut<'d>(doc: &'d mut DocumentMut, key: &str) -> Result<&'d mut dyn TableLike, EditError> {
    doc.entry(key)
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .ok_or_else(|| not_a(key, "table"))
}

/// `[[plugins]]`, made at the end of the file when it has none.
fn plugins_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, EditError> {
    doc.entry("plugins")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| not_a("plugins", "list of tables"))
}

/// `table` appended to the list of tables `key` of `parent`, as that list is spelled (tables,
/// or one inline list), made as tables when absent.
fn append(parent: &mut dyn TableLike, key: &str, table: toml_edit::Table) -> Result<(), EditError> {
    match parent.get_mut(key) {
        Some(Item::ArrayOfTables(list)) => list.push(table),
        Some(Item::Value(Value::Array(list))) => {
            list.push(Value::InlineTable(table.into_inline_table()));
        }
        Some(_) => return Err(not_a(key, "list of tables")),
        None => {
            let mut list = ArrayOfTables::new();
            list.push(table);
            parent.insert(key, Item::ArrayOfTables(list));
        }
    }
    Ok(())
}

/// Every table `is` picks taken out of the list of tables `key` of `parent`, answering how
/// many are left.
fn take(
    parent: &mut dyn TableLike,
    key: &str,
    is: impl Fn(&dyn TableLike) -> bool,
) -> Result<usize, EditError> {
    match parent.get_mut(key) {
        Some(Item::ArrayOfTables(list)) => {
            list.retain(|held| !is(held));
            Ok(list.len())
        }
        Some(Item::Value(Value::Array(list))) => {
            list.retain(|held| !held.as_inline_table().is_some_and(|held| is(held)));
            Ok(list.len())
        }
        _ => Err(not_a(key, "list of tables")),
    }
}

/// `text` appended to the list of strings `key` of `parent`, made when absent.
fn push(parent: &mut dyn TableLike, key: &str, text: &str) -> Result<(), EditError> {
    parent
        .entry(key)
        .or_insert(toml_edit::value(toml_edit::Array::new()))
        .as_array_mut()
        .ok_or_else(|| not_a(key, "list"))?
        .push(text);
    Ok(())
}

/// Every entry `is` picks taken out of the list of strings `key` of `parent`; a list left empty
/// goes, as it was before the add that made it.
fn pull(parent: &mut dyn TableLike, key: &str, is: impl Fn(&str) -> bool) -> Result<(), EditError> {
    let list = parent
        .get_mut(key)
        .and_then(Item::as_array_mut)
        .ok_or_else(|| not_a(key, "list"))?;
    list.retain(|value| !value.as_str().is_some_and(&is));
    if list.is_empty() {
        parent.remove(key);
    }
    Ok(())
}

/// [`pull`] from the top-level table `table`, which goes too when it is left empty.
fn pull_top(
    doc: &mut DocumentMut,
    table: &str,
    key: &str,
    is: impl Fn(&str) -> bool,
) -> Result<(), EditError> {
    let held = table_mut(doc, table)?;
    pull(held, key, is)?;
    if held.is_empty() {
        doc.remove(table);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use proptest::prelude::*;
    use suzatary_core::connect::{Connection, Params, Refused, TemplateId};
    use suzatary_core::mail::account::Authority;
    use suzatary_core::mail::address::EmailAddress;

    use super::*;

    /// The required tables, as an operator wrote them, comments and all.
    const BASE: &str = r#"# Ada's box, by hand.
repo = "/tmp/suzatary-connections"
data = "/tmp/suzatary-connections"

[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = []
actors = ["ada", "triage"]
owner = "ada"

[web]
viewers = ["her@example.test"]
app_url = "https://example.test"

[web.act_as]
"her@example.test" = "ada"

[web.serving]
host = "127.0.0.1"
port = 8787
tls_dir = "/var/lib/suzatary/tls"
dist = "web/dist"
briefings = "briefings"

[categories]
work = 0

[mail]
root = "Mail"
state = "state.json"
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
last_sync = "/tmp/suzatary-connections-last-sync.json"
heartbeat = ".local/state/triage-heartbeat"

# The first account, kept by hand.
[[mail.accounts]]
email = "her@example.test"
subdir = "her-at-example-test"
sync = "gmail"
"#;

    /// The calendar's plugin as a file may hold it, at most one: written by hand with a link,
    /// or exactly as its template declares it, with none.
    const ICS: [&str; 3] = [
        "",
        "\n# a calendar, by hand\n[[plugins]]\nname = \"ics\"\ncomponent = \"plugins/mine/ics.wasm\"\nschema = \"plugins/mine/ics.schema\"\norigins = [\"https://calendar.example.test\"]\ncredentials = [\"PLUGIN_ICS_HOME\"]\n",
        "\n[[plugins]]\nname = \"ics\"\ncomponent = \"ics.wasm\"\nschema = \"ics.schema\"\norigins = [\"https://calendar.google.com\", \"https://outlook.office365.com\"]\n",
    ];

    /// Tables a file may hold beside the required ones, each spelled as a person might.
    const MORE: [&str; 4] = [
        "\n[secretary]\nmodel = \"claude-opus-5-5\" # pinned\ndrive_allow = [\n    \"/Documents\",\n]\n",
        "\n[notify]\nchannels = [\"fcm\"]\n",
        "\n[[plugins]]\nname = \"dueday\"\ncomponent = \"dueday.wasm\"\nschema = \"dueday.schema\"\norigins = [\"https://dueday.example.test\"]\n",
        "\n[persona]\ncity = \"Springfield\" # the owner's\n",
    ];

    fn path() -> PathBuf {
        std::env::temp_dir().join("suzatary-connections-instance.toml")
    }

    fn file() -> impl Strategy<Value = String> {
        (
            proptest::sample::subsequence(MORE.to_vec(), 0..=MORE.len()),
            proptest::sample::select(ICS.to_vec()),
        )
            .prop_map(|(more, ics)| format!("{BASE}{}{ics}", more.concat()))
    }

    fn email() -> impl Strategy<Value = EmailAddress> {
        "[a-c]{1,3}@example\\.test".prop_map(|text| EmailAddress::new(&text).expect("an address"))
    }

    fn connection() -> impl Strategy<Value = Connection> {
        let made = |id: TemplateId, params: Params| Connection::new(id, params).expect("its shape");
        prop_oneof![
            "/[A-Za-z]{1,6}".prop_map(move |folder| made(
                TemplateId::DriveCli,
                Params::Folder(folder.parse().expect("a folder"))
            )),
            "[A-Za-z]{1,6}( [a-z]{1,3})?".prop_map(move |label| made(
                TemplateId::Ics,
                Params::Label(suzatary_core::connect::Label::parse(&label).expect("a label"))
            )),
            email().prop_map(move |email| made(TemplateId::Gmail, Params::Address(email))),
            email().prop_map(move |email| made(TemplateId::Google, Params::Address(email))),
            email().prop_map(move |email| made(TemplateId::Proton, Params::Address(email))),
            (
                email(),
                prop_oneof![Just(Authority::Organizations), Just(Authority::Consumers)]
            )
                .prop_map(move |(email, tenant)| made(
                    TemplateId::MsImap,
                    Params::Microsoft { email, tenant }
                )),
            Just(made(TemplateId::Ntfy, Params::Nothing)),
            Just(made(TemplateId::Fcm, Params::Nothing)),
        ]
    }

    /// A file, after some connections were added to it.
    fn edited() -> impl Strategy<Value = String> {
        (file(), proptest::collection::vec(connection(), 0..4)).prop_map(|(file, connections)| {
            connections.iter().fold(file, |text, connection| {
                add(&path(), &text, connection).unwrap_or(text)
            })
        })
    }

    /// `after` is `before` with one run of text inserted: nothing else in the file moved.
    fn one_insertion(before: &str, after: &str) -> bool {
        let head = before
            .bytes()
            .zip(after.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        let tail = before[head..]
            .bytes()
            .rev()
            .zip(after[head..].bytes().rev())
            .take_while(|(a, b)| a == b)
            .count();
        head + tail == before.len() && after.len() >= before.len()
    }

    proptest! {
        /// An add inserts one run of text and moves nothing else, and the file it wrote reads as
        /// the algebra's tables; its remove leaves the file byte for byte, but for a plugin's
        /// manifest the add declared, which stays, so from then on the pair is exact.
        #[test]
        fn an_add_and_its_remove_leave_the_file_as_it_was(text in edited(), connection in connection()) {
            let before = declared(&path(), &text).expect("the file reads");
            match add(&path(), &text, &connection) {
                Ok(added) => {
                    prop_assert!(one_insertion(&text, &added), "--- before\n{text}\n--- after\n{added}");
                    let with = before.with(&connection).expect("the algebra agrees");
                    prop_assert_eq!(declared(&path(), &added), Ok(with.clone()));
                    let removed = remove(&path(), &added, &connection).expect("it removes");
                    prop_assert_eq!(declared(&path(), &removed), with.without(&connection).map_err(EditError::from));
                    if with.plugins.len() == before.plugins.len() {
                        prop_assert_eq!(&removed, &text);
                    }
                    let again = add(&path(), &removed, &connection).expect("it adds again");
                    prop_assert_eq!(remove(&path(), &again, &connection), Ok(removed.clone()));
                    prop_assert_eq!(
                        add(&path(), &added, &connection),
                        Err(EditError::Connection(Refused::Duplicate(connection.called())))
                    );
                }
                Err(refused) => prop_assert_eq!(before.with(&connection).err().map(EditError::from), Some(refused)),
            }
        }

        /// A grant inserts the origin once, and granting again changes nothing.
        #[test]
        fn a_grant_inserts_one_origin_once(text in edited(), host in "[a-z]{1,8}") {
            let ics = PluginName::parse("ics").expect("a name");
            let origin = Origin::parse(&format!("https://{host}.example.test")).expect("an origin");
            match add_origin(&path(), &text, &ics, &origin) {
                Ok(granted) => {
                    prop_assert!(one_insertion(&text, &granted));
                    prop_assert_eq!(add_origin(&path(), &granted, &ics, &origin), Ok(granted.clone()));
                    let declared = declared(&path(), &granted).expect("it reads");
                    prop_assert!(declared.plugins.iter().any(|plugin| plugin.allows(&origin)));
                }
                Err(refused) => prop_assert_eq!(refused, EditError::Connection(Refused::NoPlugin("ics".to_owned()))),
            }
        }
    }

    /// The template's manifest is declared with a calendar's first link, after every table the
    /// operator wrote, and stays with its last, written exactly as the template declares it.
    #[test]
    fn a_first_calendar_declares_its_plugin_and_its_last_leaves_it_bare() {
        let work = Connection::parse("ics", &["Work".to_owned()]).expect("a calendar");
        let added = add(&path(), BASE, &work).expect("added");
        assert!(added.starts_with(BASE), "{added}");
        assert!(
            added.contains("credentials = [\"PLUGIN_ICS_WORK\"]"),
            "{added}"
        );
        assert_eq!(
            remove(&path(), &added, &work),
            Ok(format!("{BASE}{}", ICS[2]))
        );
    }

    /// Removing an account takes its table, with the comment written above it, and nothing
    /// else; the last one leaves the roster spelled empty, since the file requires it.
    #[test]
    fn removing_an_account_takes_its_table_only() {
        let hers =
            Connection::parse("lieer", &["her@example.test".to_owned()]).expect("an account");
        let removed = remove(&path(), BASE, &hers).expect("removed");
        let kept = BASE
            .split("\n# The first account")
            .next()
            .expect("the file before the account");
        assert!(removed.starts_with(kept), "{removed}");
        assert!(!removed.contains("her-at-example-test"), "{removed}");
        assert!(removed.contains("accounts = []"), "{removed}");
        assert!(
            declared(&path(), &removed)
                .expect("it reads")
                .accounts
                .is_empty()
        );
    }
}
