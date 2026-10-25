//! The edits to the instance file, every one made under one lock.

use std::path::{Path, PathBuf};

use suzatary_core::settings::EditError;

/// The lock every edit of the instance file at `path` holds from its read to its write, in this
/// process or another (`suzatary instance set` takes it too): an `flock` on a file beside it,
/// never on the file itself, which every write renames a new file over.
///
/// # Errors
/// The lock file cannot be made or locked.
pub fn editing(path: &Path) -> Result<suzatary_io::durable::Held, EditError> {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(".lock");
    suzatary_io::durable::hold(Path::new(&sidecar))
        .map_err(|failed| EditError::Io(format!("locking the instance file: {failed}")))
}

/// Edits to the instance file that keep every other byte of it.
///
/// Through `toml_edit` rather than the serde `File`, because a serde round trip
/// would keep the data and drop every comment the operator wrote.
pub struct InstanceEdits {
    /// Owned, so the editor can outlive one `Instance`.
    path: PathBuf,
}

impl InstanceEdits {
    /// Edit the file at `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_text(&self) -> Result<String, EditError> {
        std::fs::read_to_string(&self.path).map_err(|failed| EditError::Io(failed.to_string()))
    }

    /// Under the lock: the file read once, `edit` asked what it becomes, and that written whole,
    /// so a reader sees only the old file or the new one; a text `edit` leaves as it was is not
    /// written.
    fn edit(
        &self,
        edit: impl FnOnce(&Path, &str) -> Result<String, EditError>,
    ) -> Result<(), EditError> {
        use suzatary_io::durable;
        let _held = editing(&self.path)?;
        let text = self.read_text()?;
        let edited = edit(&self.path, &text)?;
        if edited != text {
            durable::replace(&self.path, edited.as_bytes(), durable::Mode::Readable)
                .map_err(|failed| EditError::Io(failed.to_string()))?;
        }
        Ok(())
    }

    /// The file's concrete syntax changed by `edit`, so every comment stays.
    fn edit_doc(
        &self,
        edit: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<(), EditError>,
    ) -> Result<(), EditError> {
        self.edit(|_, text| {
            let mut doc = doc(text)?;
            edit(&mut doc)?;
            Ok(doc.to_string())
        })
    }
}

/// `text` as concrete syntax.
fn doc(text: &str) -> Result<toml_edit::DocumentMut, EditError> {
    text.parse()
        .map_err(|failed: toml_edit::TomlError| EditError::Malformed(failed.to_string()))
}

impl suzatary_core::settings::Configuration for InstanceEdits {
    fn declared(&self) -> Result<suzatary_core::connect::Declared, EditError> {
        super::connections::declared(&self.path, &self.read_text()?)
    }
}

impl suzatary_core::settings::Edits for InstanceEdits {
    fn allow_web(
        &self,
        rule: &suzatary_core::web::AllowRule,
    ) -> Result<(), suzatary_core::settings::EditError> {
        self.allow("web_allow", rule.to_string())
    }

    fn allow_drive(
        &self,
        folder: &suzatary_core::drive::DrivePath,
    ) -> Result<(), suzatary_core::settings::EditError> {
        self.allow("drive_allow", folder.to_string())
    }

    fn add_connection(
        &self,
        connection: &suzatary_core::connect::Connection,
    ) -> Result<(), suzatary_core::settings::EditError> {
        self.edit(|path, text| super::connections::add(path, text, connection))
    }

    fn remove_connection(
        &self,
        connection: &suzatary_core::connect::Connection,
    ) -> Result<(), suzatary_core::settings::EditError> {
        self.edit(|path, text| super::connections::remove(path, text, connection))
    }

    fn add_origin(
        &self,
        plugin: &suzatary_core::plugin::PluginName,
        origin: &suzatary_core::plugin::Origin,
    ) -> Result<(), suzatary_core::settings::EditError> {
        self.edit(|path, text| super::connections::add_origin(path, text, plugin, origin))
    }
}

impl InstanceEdits {
    /// Append `text` to the `[secretary]` list `key`, once: an "Always allow"
    /// already given is the same answer.
    fn allow(&self, key: &str, text: String) -> Result<(), EditError> {
        self.edit_doc(|doc| {
            let secretary = doc
                .entry("secretary")
                .or_insert_with(toml_edit::table)
                .as_table_like_mut()
                .ok_or_else(|| {
                    EditError::Malformed("secretary is present but is not a table".to_owned())
                })?;
            let list = secretary
                .entry(key)
                .or_insert(toml_edit::value(toml_edit::Array::new()))
                .as_array_mut()
                .ok_or_else(|| {
                    EditError::Malformed(format!("secretary.{key} is present but is not an array"))
                })?;
            if !list.iter().any(|held| held.as_str() == Some(text.as_str())) {
                list.push(text);
            }
            Ok(())
        })
    }
}
