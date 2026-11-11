//! The morning briefing saved as a file, `briefings/YYYY-MM-DD.typ`: typst markup with no
//! actor and no chain event, trusted the way the CLI is because only a human-directed session
//! writes one (see `suzatary_core::secretary::briefings::Briefings`).

use std::path::PathBuf;

use crate::present::Present as _;
use chrono::NaiveDate;
use suzatary_core::secretary::briefings::{BriefingError, Briefings};

/// A `NaiveDate` always prints as exactly this many characters, none of them `/`, which is what
/// makes [`BriefingFiles::read`] safe against a traversal.
const DATE_LEN: usize = "YYYY-MM-DD".len();

/// The briefing directory this deployment reads from and writes to.
#[derive(Debug, Clone)]
pub struct BriefingFiles {
    dir: PathBuf,
}

impl BriefingFiles {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Briefings for BriefingFiles {
    fn all(&self) -> Result<Vec<(NaiveDate, String)>, BriefingError> {
        let unreadable = |err: std::io::Error| BriefingError::Unreadable(err.to_string());
        let Some(entries) = std::fs::read_dir(&self.dir).present().map_err(unreadable)? else {
            return Ok(Vec::new());
        };
        let mut rows: Vec<(NaiveDate, String)> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(unreadable)?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("typ") {
                continue;
            }
            // A stray file is skipped, so it cannot take the whole listing down.
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(day) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") else {
                continue;
            };
            // A file that vanishes between the listing and the read loses one entry, not all.
            let Some(source) = std::fs::read_to_string(&path)
                .present()
                .map_err(unreadable)?
            else {
                continue;
            };
            rows.push((day, source));
        }
        rows.sort_by_key(|(day, _)| std::cmp::Reverse(*day));
        Ok(rows)
    }

    fn read(&self, day: NaiveDate) -> Result<String, BriefingError> {
        let stamp = day.format("%Y-%m-%d").to_string();
        debug_assert_eq!(stamp.len(), DATE_LEN);
        let path = self.dir.join(format!("{stamp}.typ"));
        std::fs::read_to_string(&path)
            .present()
            .map_err(|err| BriefingError::Unreadable(err.to_string()))?
            .ok_or(BriefingError::Absent(stamp))
    }
}
