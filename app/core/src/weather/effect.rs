//! Today's forecast as one line, the one authority that reaches the network.
//!
//! It is on the secretary's bundle only, never [`Agent`](crate::capability::Agent), and what comes back is
//! data, never instructions.

pub use crate::weather::WeatherError;

/// Today's forecast for the person this deployment serves.
pub trait Weather: Sync {
    /// One line on the rest of today, with the umbrella verdict; the city and zone are the
    /// interpreter's, from `[persona]`.
    ///
    /// # Errors
    /// The feed is unreachable or answers something that is not weather.
    fn today(&self) -> Result<String, WeatherError>;
}
