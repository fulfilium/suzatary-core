//! Which part of the deployment a line of the overview, and so a connection, belongs to.

use serde::Serialize;
use ts_rs::TS;

/// What a connection is for; a connection is in exactly one, and the overview draws them in
/// [`Group::ALL`]'s order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Where the data is kept: the data repository and where it is pushed.
    Storage,
    /// The mail accounts.
    Mail,
    /// The calendars: every credential of a plugin whose store the calendar reads.
    Calendar,
    /// The folders of the owner's Drive the secretary may read.
    Drive,
    /// Which model answers each job.
    Models,
    /// The channels that wake the owner's phones.
    Notifications,
    /// The routine a coding session fires.
    Coding,
    /// Every other plugin.
    Plugins,
    /// The program this box runs.
    Box,
}

impl Group {
    /// Every group, in the order the overview draws them: what the others wait on first (a model
    /// answers everything else), then mail, the calendar and the Drive, then where the data goes.
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

    /// Its name, as the wire and a connection's key print it (`calendar/WORK`).
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
