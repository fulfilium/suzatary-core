//! The weather feed: one GET, and the hour in the forecast city's zone (the feed's slots are
//! local; the city and its zone are both `[persona]`'s).

use std::time::Duration;

use suzatary_core::persona::Persona;
use suzatary_core::weather::WeatherError;

use crate::http::{Http, Req, Ureq};

/// The forecast's service: it is asked for one city by name, and told nothing else.
pub const FEED: &str = "https://wttr.in";

/// The full forecast (`format=j1`); only spaces are escaped in `city`.
fn url(city: &str) -> String {
    format!("{FEED}/{}?format=j1", city.replace(' ', "%20"))
}

/// Today's forecast for `city`.
///
/// # Errors
/// [`WeatherError::Unreachable`] when the feed does not answer 2xx JSON.
pub fn fetch(
    http: &dyn Http,
    city: &str,
    timeout: Duration,
) -> Result<serde_json::Value, WeatherError> {
    // wttr.in answers a browser with terminal art and curl with JSON.
    let resp = http
        .send(
            Req::get(url(city))
                .header("User-Agent", "curl/8")
                .timeout(timeout),
        )
        .map_err(|failed| WeatherError::Unreachable(failed.to_string()))?;
    if !resp.ok() {
        return Err(WeatherError::Unreachable(format!(
            "the feed answered HTTP {}",
            resp.status
        )));
    }
    serde_json::from_slice(&resp.body)
        .map_err(|failed| WeatherError::Unreachable(format!("the feed was not JSON: {failed}")))
}

/// The one line the briefing prints.
pub fn line(now: chrono::DateTime<chrono::Utc>, persona: &Persona) -> Result<String, WeatherError> {
    use chrono::Timelike as _;
    let hour = persona.zone.wall(now.naive_utc()).hour();
    let payload = fetch(&Ureq::new(), &persona.city, Duration::from_secs(20))?;
    let (hours, low, high) = suzatary_core::weather::parse(&payload)?;
    Ok(suzatary_core::weather::line(&hours, hour, low, high))
}
