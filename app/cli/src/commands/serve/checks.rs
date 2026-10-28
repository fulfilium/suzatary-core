//! A link's check at the tap that stored it: its plugin synced once, as the 15-minute timer
//! syncs it, so the link's line says what the check found before the person leaves the page.

use std::path::PathBuf;
use std::sync::Arc;

use suzatary_core::plugin::PluginName;

use crate::instance::Instance;

/// Syncs a plugin of the instance file at `instance`, read again for each check, so a link of
/// a plugin declared since the server started is checked too.
pub struct Synced {
    instance: PathBuf,
}

impl Synced {
    /// Over the instance file at `instance`.
    #[must_use]
    pub const fn of(instance: PathBuf) -> Self {
        Self { instance }
    }
}

impl suzatary_core::settings::Checks for Synced {
    fn check(&self, plugin: &PluginName) {
        let instance = match Instance::open(&self.instance) {
            Ok(instance) => instance,
            Err(failed) => {
                tracing::warn!("check: {plugin} was not synced: the instance file: {failed:#}");
                return;
            }
        };
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
        let effects = crate::commands::plugin::effects(&instance);
        let report = crate::commands::plugin::sync(
            &host,
            manifest,
            &crate::commands::plugin::components(&instance, None),
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
    fn check(&self, _: &PluginName) {}
}

/// The checks a state's taps run.
pub type Checks = Arc<dyn suzatary_core::settings::Checks + Send + Sync>;
