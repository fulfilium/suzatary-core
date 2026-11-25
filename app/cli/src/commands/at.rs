//! `--at <INSTANT>`: the instant a reading is taken at, on `todos open` and `todo show`.

use prodrome_data::literal::Datetime;
use suzatary_base::capability::Clock as _;
use suzatary_io_local::clock::SystemClock;

#[derive(Debug, Clone, Copy, clap::Args)]
pub struct At {
    /// The instant to read at, an ISO date or instant (UTC); absent is now.
    #[arg(long = "at", global = true, value_name = "INSTANT", value_parser = instant)]
    at: Option<Datetime>,
}

impl At {
    pub(crate) fn or_now(self) -> Datetime {
        self.at.unwrap_or_else(|| SystemClock.now())
    }
}

/// An ISO date or instant, read as UTC.
pub(crate) fn instant(text: &str) -> Result<Datetime, String> {
    prodrome_data::instant::parse_iso(text)
        .map_err(|refused| refused.to_string())
        .and_then(|at| {
            prodrome_data::instant::datetime_of(at).map_err(|refused| refused.to_string())
        })
}
