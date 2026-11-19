//! The morning briefing, saved as a file.

/// A briefing could not be read.
#[derive(Debug, thiserror::Error)]
pub enum BriefingError {
    /// No file for that day.
    #[error("no briefing for {0}")]
    Absent(String),
    /// The directory could not be read.
    #[error("the briefings could not be read: {0}")]
    Unreadable(String),
}

/// The saved morning briefings, trusted by provenance and so rendered unconfirmed.
///
/// Only a human-directed assistant session writes one, so this rests on that convention.
pub trait Briefings: Sync {
    /// Every day there is a briefing for, each with its whole source, not a summary.
    fn all(&self) -> Result<Vec<(chrono::NaiveDate, String)>, BriefingError>;

    /// One day's typst markup.
    fn read(&self, day: chrono::NaiveDate) -> Result<String, BriefingError>;
}
