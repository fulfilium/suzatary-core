//! The instance file's editor, noting whether a route moved the file.

use std::sync::atomic::{AtomicBool, Ordering};

use suzatary_connect::connect::Connection;
use suzatary_connect::settings::{EditError, Edits};
use suzatary_health::health::plugin;

pub(super) struct Noting<'a> {
    edits: &'a dyn Edits,
    moved: AtomicBool,
}

impl<'a> Noting<'a> {
    pub(super) fn new(edits: &'a dyn Edits) -> Self {
        Self {
            edits,
            moved: AtomicBool::new(false),
        }
    }

    pub(super) fn moved(&self) -> bool {
        self.moved.load(Ordering::Relaxed)
    }

    fn noted<T>(&self, done: Result<T, EditError>) -> Result<T, EditError> {
        if done.is_ok() {
            self.moved.store(true, Ordering::Relaxed);
        }
        done
    }
}

impl Edits for Noting<'_> {
    fn allow_web(&self, rule: &suzatary_services::web::AllowRule) -> Result<(), EditError> {
        self.noted(self.edits.allow_web(rule))
    }

    fn allow_drive(&self, folder: &suzatary_base::drive_path::DrivePath) -> Result<(), EditError> {
        self.noted(self.edits.allow_drive(folder))
    }

    fn add_connection(&self, connection: &Connection) -> Result<(), EditError> {
        self.noted(self.edits.add_connection(connection))
    }

    fn remove_connection(&self, connection: &Connection) -> Result<(), EditError> {
        self.noted(self.edits.remove_connection(connection))
    }

    fn add_origin(
        &self,
        plugin: &plugin::PluginName,
        origin: &plugin::Origin,
    ) -> Result<(), EditError> {
        self.noted(self.edits.add_origin(plugin, origin))
    }
}

impl suzatary_connect::settings::Configuration for Noting<'_> {
    fn declared(&self) -> Result<suzatary_connect::connect::Declared, EditError> {
        self.edits.declared()
    }
}
