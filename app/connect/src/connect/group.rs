//! Which part of the deployment a line of the overview, and so a connection, belongs to.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a connection is for; a connection is in exactly one group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Storage,
    Mail,
    /// Every credential of a plugin whose store the calendar reads.
    Calendar,
    Drive,
    Models,
    Notifications,
    Coding,
    /// Every other plugin.
    Plugins,
    Box,
}

impl Group {
    /// Every group in the overview's order; models come first because the others need a model.
    pub const ALL: [Group; 9] = [
        Group::Models,
        Group::Mail,
        Group::Calendar,
        Group::Drive,
        Group::Storage,
        Group::Notifications,
        Group::Coding,
        Group::Plugins,
        Group::Box,
    ];

    /// The wire name, also the first segment of a connection's key (`calendar/WORK`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Group::Storage => "storage",
            Group::Mail => "mail",
            Group::Calendar => "calendar",
            Group::Drive => "drive",
            Group::Models => "models",
            Group::Notifications => "notifications",
            Group::Coding => "coding",
            Group::Plugins => "plugins",
            Group::Box => "box",
        }
    }
}
