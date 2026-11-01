//! A source's check at the act that made it readable: a link's plugin synced once, as the
//! 15-minute timer syncs it, and a Google account's calendars read once, as `plugin sync` reads
//! them, so the line says what the check found before the person leaves the page.

use std::path::PathBuf;
use std::sync::Arc;

use suzatary_core::plugin::PluginName;
use suzatary_core::source_key::SourceKey;

use crate::instance::Instance;

/// Syncs a source of the instance file at `instance`, read again for each check, so a link of
/// a plugin declared since the server started is checked too; Google is asked at `google`.
pub struct Synced {
    instance: PathBuf,
    google: suzatary_core::google::Endpoints,
}

impl Synced {
    /// Over `instance`'s file, asking Google at `google`.
    #[must_use]
    pub fn of(instance: &Instance, google: &suzatary_core::google::Endpoints) -> Self {
        Self {
            instance: instance.path.clone(),
            google: google.clone(),
        }
    }
}

impl suzatary_core::settings::Checks for Synced {
    fn check(&self, source: &SourceKey) {
        let instance = match Instance::open(&self.instance) {
            Ok(instance) => instance,
            Err(failed) => {
                tracing::warn!("check: {source} was not read: the instance file: {failed:#}");
                return;
            }
        };
        match source {
            SourceKey::Plugin(plugin) | SourceKey::Link { plugin, .. } => {
                Self::plugin(&instance, plugin);
            }
            SourceKey::GoogleCalendar(_) => {
                let (lines, _) = crate::commands::google::sync_calendars(
                    &instance,
                    suzatary_io::google::Google::new(self.google.clone()),
                    &suzatary_io::source_health::beside(&instance.mail.heartbeat),
                );
                for line in lines {
                    tracing::info!("check: {line}");
                }
            }
            // The mail and the Drive are read by their own timers alone.
            SourceKey::Mail(_) | SourceKey::Drive => {
                tracing::info!("check: {source} is read by its timer alone");
            }
        }
    }
}

impl Synced {
    fn plugin(instance: &Instance, plugin: &PluginName) {
        let Some(manifest) = instance.plugins.named(plugin.as_str()) else {
            tracing::warn!("check: {plugin} is not declared in the instance file");
            return;
        };
        let host = match suzatary_plugin::Host::new(suzatary_plugin::Budget::default()) {
            Ok(host) => host,
            Err(failed) => {
                tracing::warn!("check: {plugin} was not synced: the plugin host: {failed}");
                return;
            }
        };
        let effects = crate::commands::plugin::effects(instance);
        let report = crate::commands::plugin::sync(
            &host,
            manifest,
            &crate::commands::plugin::components(instance, None),
            &instance.plugins,
            &effects,
            &suzatary_io::source_health::beside(&instance.mail.heartbeat),
        );
        tracing::info!("check: {}", report.line);
    }
}

/// Syncs nothing at a tap: a server with no plugin host, as a test's is unless it says.
pub struct Unsynced;

impl suzatary_core::settings::Checks for Unsynced {
    fn check(&self, _: &SourceKey) {}
}

/// The checks a state's taps run.
pub type Checks = Arc<dyn suzatary_core::settings::Checks + Send + Sync>;
