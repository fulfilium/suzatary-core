//! What a live half's row was judged at beside its tree: the owner's stores as they stood.

use crate::oid::Oid;
use crate::{Error, Half, git};
use std::fmt;
use std::path::{Path, PathBuf};

/// A digest of the heads of the stores beside the instance file (git's name for their objects'
/// sorted paths): a hash alone. A store elsewhere (a roadmap clone, say) and the mail move none.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Data(Oid);

impl Data {
    /// The stores now; with no instance file, the digest of no object.
    pub fn now(instance: Option<&Path>) -> Result<Data, Error> {
        let mut objects = Vec::new();
        if let Some(file) = instance {
            let dir = file
                .canonicalize()
                .map_err(|source| Error::Io {
                    path: file.to_owned(),
                    source,
                })?
                .parent()
                .map(Path::to_owned)
                .unwrap_or_default();
            walk(&dir, &dir, &mut objects)?;
        }
        objects.sort();
        let list: String = objects
            .iter()
            .flat_map(|path| [path.as_str(), "\n"])
            .collect();
        git::hash_text(&list).map(Data)
    }

    /// The stores of the instance a gate hands the live half: `SUZATARY_INSTANCE`, else the link;
    /// a link to nothing is an error, never the digest of no store.
    pub(crate) fn of_checkout(root: &Path) -> Result<Data, Error> {
        let link = root.join("instance.toml");
        let instance = std::env::var_os("SUZATARY_INSTANCE")
            .filter(|named| !named.is_empty())
            .map(PathBuf::from)
            .or_else(|| link.symlink_metadata().is_ok().then_some(link));
        Data::now(instance.as_deref())
    }

    pub(crate) fn parse(word: &str) -> Result<Data, Error> {
        Oid::parse(word).map(Data)
    }

    #[must_use]
    pub(crate) fn short(&self) -> &str {
        self.0.short()
    }
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Each object's path under `root`, below `dir`; `.git` and every link left unread.
fn walk(root: &Path, dir: &Path, objects: &mut Vec<String>) -> Result<(), Error> {
    let io = |path: PathBuf| move |source| Error::Io { path, source };
    let in_objects = dir.file_name().is_some_and(|name| name == "objects");
    for entry in std::fs::read_dir(dir).map_err(io(dir.to_owned()))? {
        let entry = entry.map_err(io(dir.to_owned()))?;
        let (path, kind) = (entry.path(), entry.file_type().map_err(io(entry.path()))?);
        let name = entry.file_name();
        if kind.is_dir() && name != ".git" {
            walk(root, &path, objects)?;
        } else if kind.is_file() && in_objects && is_object(&name.to_string_lossy()) {
            let under = path.strip_prefix(root).unwrap_or(&path);
            objects.push(under.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn is_object(name: &str) -> bool {
    name.strip_suffix(".py").is_some_and(|hash| {
        hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// A key beside its tree: nothing for the tree half and for a live row of the old form.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Beside {
    Nothing,
    Data(Data),
}

impl Beside {
    #[must_use]
    pub(crate) fn of(half: Half, data: &Data) -> Beside {
        match half {
            Half::Tree => Beside::Nothing,
            Half::Live => Beside::Data(data.clone()),
        }
    }
}

impl Beside {
    /// `-` for nothing, `data:<digest>` for the data: its word in a set-aside.
    pub(crate) fn parse(word: &str) -> Result<Beside, Error> {
        match word.strip_prefix("data:") {
            Some(digest) => Data::parse(digest).map(Beside::Data),
            None if word == "-" => Ok(Beside::Nothing),
            None => Err(Error::Malformed(format!(
                "\"{word}\" is neither - nor data:<digest>"
            ))),
        }
    }
}

impl fmt::Display for Beside {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Beside::Nothing => f.write_str("-"),
            Beside::Data(data) => write!(f, "data:{data}"),
        }
    }
}
