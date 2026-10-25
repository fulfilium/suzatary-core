//! `suzatary instance get|set`: one key of the instance file, read or written by the parser the
//! box reads it with, so no script spells TOML with awk, sed or grep.

use std::path::Path;

use anyhow::{Context, bail};

use crate::instance::{File, Instance};
use crate::paths::Env;

/// What `suzatary instance` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Read or write.
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, clap::Subcommand)]
enum Action {
    /// Print one value: a string as itself, anything else as TOML writes it. Answered from the
    /// file as written and never from the environment, so the leak guard reads a deployment's
    /// names even from a file the box would refuse.
    Get {
        /// The key, dotted as TOML spells one: `web.app_url`, `chains.0.name`,
        /// `web.act_as."her@example.test"`.
        key: Key,
    },
    /// Write one value, keeping every other byte of the file, and only when the box would read
    /// the result: a key holding a number or a boolean takes one, anything else a string.
    Set {
        /// The key, as `get` takes it.
        key: Key,
        /// The value.
        value: String,
    },
}

/// A key of the instance file: TOML's dotted key, a segment that is a number naming a member of
/// an array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key(Vec<String>);

impl std::str::FromStr for Key {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let keys =
            toml_edit::Key::parse(text).map_err(|e| format!("{text:?} is not a key: {e}"))?;
        Ok(Self(keys.iter().map(|key| key.get().to_owned()).collect()))
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let segments: Vec<String> = self
            .0
            .iter()
            .map(|segment| {
                toml_edit::Key::new(segment.as_str())
                    .display_repr()
                    .into_owned()
            })
            .collect();
        f.write_str(&segments.join("."))
    }
}

/// `suzatary instance`.
///
/// # Errors
/// A key the file does not hold, a file the box would not read, or a write refused.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    match &args.action {
        Action::Get { key } => {
            println!("{}", get(path, key)?);
            Ok(())
        }
        Action::Set { key, value } => set(path, key, value, &Env::read()?),
    }
}

/// The value at `key` in the file at `path`, as `get` prints it.
///
/// # Errors
/// The file not TOML, or `key` naming no value in it.
pub fn get(path: &Path, key: &Key) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading the instance file {}", path.display()))?;
    let root = toml::Value::Table(
        toml::from_str(&text)
            .with_context(|| format!("parsing the instance file {}", path.display()))?,
    );
    let value = at(&root, key).with_context(|| format!("{} holds no {key}", path.display()))?;
    shown(value).with_context(|| format!("{key} in {}", path.display()))
}

/// The value at `key` beneath `root`.
pub(crate) fn at<'a>(root: &'a toml::Value, key: &Key) -> Option<&'a toml::Value> {
    key.0.iter().try_fold(root, |value, segment| match value {
        toml::Value::Table(table) => table.get(segment),
        toml::Value::Array(array) => array.get(segment.parse::<usize>().ok()?),
        _ => None,
    })
}

/// A value as `get` prints it: a string as itself, a table refused, anything else as TOML.
///
/// # Errors
/// A table, which is many values.
pub(crate) fn shown(value: &toml::Value) -> anyhow::Result<String> {
    match value {
        toml::Value::String(text) => Ok(text.clone()),
        toml::Value::Table(table) => {
            let keys: Vec<&str> = table.keys().map(String::as_str).collect();
            bail!("is a table: name one of its keys ({})", keys.join(", "))
        }
        other => Ok(other.to_string()),
    }
}

/// `value` written at `key`, the tables on its way made where absent, every other byte kept,
/// and the file replaced whole only when it reads in `env` as an instance the box would start.
///
/// # Errors
/// A key through a value or an array, a value its key's type does not take, or a result the
/// box would refuse; the file is then untouched.
pub fn set(path: &Path, key: &Key, value: &str, env: &Env) -> anyhow::Result<()> {
    // Held to the write, so a tap's edit is never made between this read and this write.
    let _held = crate::instance::editing(path)?;
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading the instance file {}", path.display()))?;
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("parsing the instance file {}", path.display()))?;
    let Some((last, ways)) = key.0.split_last() else {
        bail!("an empty key names nothing");
    };
    let mut table = doc.as_table_mut() as &mut dyn toml_edit::TableLike;
    let mut ways = ways.iter();
    while let Some(way) = ways.next() {
        let item = table
            .entry(way)
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
        // An array of tables is entered by the member the next segment numbers.
        table = match item {
            toml_edit::Item::ArrayOfTables(tables) => ways
                .next()
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| tables.get_mut(index))
                .map(|table| table as &mut dyn toml_edit::TableLike),
            other => other.as_table_like_mut(),
        }
        .with_context(|| format!("{key}: {way:?} is not a table"))?;
    }
    let typed = match table.get(last).and_then(toml_edit::Item::as_value) {
        Some(toml_edit::Value::Integer(_)) => value
            .parse::<i64>()
            .map(toml_edit::Value::from)
            .with_context(|| format!("{key} holds a number, and {value:?} is not one"))?,
        Some(toml_edit::Value::Boolean(_)) => value
            .parse::<bool>()
            .map(toml_edit::Value::from)
            .with_context(|| format!("{key} holds a boolean, and {value:?} is not one"))?,
        Some(toml_edit::Value::Float(_)) => value
            .parse::<f64>()
            .map(toml_edit::Value::from)
            .with_context(|| format!("{key} holds a number, and {value:?} is not one"))?,
        Some(toml_edit::Value::String(_)) | None => toml_edit::Value::from(value),
        Some(_) => bail!("{key} holds neither a string, a number nor a boolean"),
    };
    match table.get_mut(last) {
        Some(toml_edit::Item::Value(held)) => {
            let decor = held.decor().clone();
            *held = typed;
            *held.decor_mut() = decor;
        }
        Some(_) => bail!("{key} is a table, not a value"),
        None => {
            table.insert(last, toml_edit::Item::Value(typed));
        }
    }
    let written = doc.to_string();
    let file: File = toml::from_str(&written)
        .with_context(|| format!("{key} = {value:?} leaves a file the box would not read"))?;
    Instance::of(path, file, env)
        .with_context(|| format!("{key} = {value:?} leaves a file the box would not start"))?;
    suzatary_io::durable::replace(path, written.as_bytes(), suzatary_io::durable::Mode::Kept)
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../instance.sample.toml")
    }

    /// Every leaf of `value` and the key it is at.
    fn leaves(value: &toml::Value, key: &mut Vec<String>, out: &mut Vec<(Key, toml::Value)>) {
        let walk = |segment: String, value: &toml::Value, key: &mut Vec<String>, out: &mut _| {
            key.push(segment);
            leaves(value, key, out);
            key.pop();
        };
        match value {
            toml::Value::Table(table) => {
                for (segment, value) in table {
                    walk(segment.clone(), value, key, out);
                }
            }
            toml::Value::Array(array) if array.iter().any(toml::Value::is_table) => {
                for (index, value) in array.iter().enumerate() {
                    walk(index.to_string(), value, key, out);
                }
            }
            leaf => out.push((Key(key.clone()), leaf.clone())),
        }
    }

    /// LAW: on every key `instance.sample.toml` sets, `get` answers the value `Instance::read`
    /// parsed there: the file's typed reading, printed back, holds that key and that value.
    #[test]
    fn get_agrees_with_the_reader_on_every_key_of_the_sample() {
        let path = sample();
        let text = std::fs::read_to_string(&path).expect("the sample");
        let raw: toml::Value = toml::Value::Table(toml::from_str(&text).expect("toml"));
        let read = toml::Value::try_from(File::read(&path).expect("the reader reads it"))
            .expect("the reading prints");
        let dir = tempfile::tempdir().expect("scratch");
        Instance::read(&path, &Env::under(dir.path(), dir.path())).expect("and starts");
        let mut every = Vec::new();
        leaves(&raw, &mut Vec::new(), &mut every);
        assert!(every.len() > 40, "{} keys", every.len());
        for (key, written) in every {
            let parsed = at(&read, &key).unwrap_or_else(|| panic!("the reader dropped {key}"));
            assert_eq!(parsed, &written, "{key}");
            assert_eq!(
                get(&path, &key).expect("get answers"),
                shown(parsed).expect("a value"),
                "{key}"
            );
        }
    }

    /// `set` keeps every other byte, takes its key's type, makes the tables on its way, and
    /// writes nothing the box would refuse.
    #[test]
    fn set_writes_one_value_and_nothing_the_box_would_refuse() {
        let dir = tempfile::tempdir().expect("scratch");
        let env = Env::under(dir.path(), dir.path());
        let path = dir.path().join("instance.toml");
        let before = format!(
            "# the operator's words\n{}",
            std::fs::read_to_string(sample()).expect("the sample")
        );
        std::fs::write(&path, &before).expect("written");
        let key = |text: &str| text.parse::<Key>().expect("a key");

        set(&path, &key("web.serving.port"), "8788", &env).expect("a number");
        let after = std::fs::read_to_string(&path).expect("reads");
        assert_eq!(get(&path, &key("web.serving.port")).expect("reads"), "8788");
        let changed: Vec<_> = before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(changed, vec![("port = 8787", "port = 8788")]);

        assert!(set(&path, &key("web.serving.port"), "many", &env).is_err());
        let refused = set(&path, &key("chains.0.owner"), "nobody", &env).expect_err("refused");
        assert!(format!("{refused:#}").contains("would not"), "{refused:#}");
        assert_eq!(std::fs::read_to_string(&path).expect("reads"), after);

        set(&path, &key("coding.chain"), "life", &env).expect_err("a [coding] with no routine");
        set(&path, &key("persona.city"), "Paris", &env).expect("a string");
        assert_eq!(get(&path, &key("persona.city")).expect("reads"), "Paris");
    }

    /// `set` and a tap's edit take the one lock beside the file: while another process holds
    /// it, neither writes, and once it is let go both do, neither losing the other's change.
    #[test]
    fn set_and_a_tap_wait_for_the_lock_beside_the_file() {
        use suzatary_core::settings::Edits as _;
        let dir = tempfile::tempdir().expect("scratch");
        let env = Env::under(dir.path(), dir.path());
        let path = dir.path().join("instance.toml");
        std::fs::write(
            &path,
            std::fs::read_to_string(sample()).expect("the sample"),
        )
        .expect("written");
        let rule: suzatary_core::web::AllowRule = "*.gov.uk".parse().expect("a rule");
        let held = crate::instance::editing(&path).expect("the lock");
        let (done, finished) = std::sync::mpsc::channel();
        let setting = {
            let (path, env, done) = (path.clone(), env.clone(), done.clone());
            std::thread::spawn(move || {
                let key = "persona.city".parse::<Key>().expect("a key");
                set(&path, &key, "Paris", &env).expect("set");
                done.send("set").expect("sent");
            })
        };
        let tapping = {
            let path = path.clone();
            std::thread::spawn(move || {
                let edits = crate::instance::InstanceEdits::new(path);
                edits.allow_web(&rule).expect("allowed");
                done.send("tap").expect("sent");
            })
        };
        assert!(
            finished
                .recv_timeout(std::time::Duration::from_millis(300))
                .is_err(),
            "nothing writes while the lock is held"
        );
        drop(held);
        setting.join().expect("set ends");
        tapping.join().expect("the tap ends");
        assert_eq!(finished.iter().take(2).count(), 2);
        let key = |text: &str| text.parse::<Key>().expect("a key");
        assert_eq!(get(&path, &key("persona.city")).expect("reads"), "Paris");
        assert_eq!(
            get(&path, &key("secretary.web_allow.0")).expect("reads"),
            "*.gov.uk"
        );
    }
}
