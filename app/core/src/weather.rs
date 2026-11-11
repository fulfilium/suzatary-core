//! One line of weather for the briefing, pure.
//!
//! A briefing asks whether an umbrella is needed today, a question about the hours still ahead,
//! so [`verdict`] reads the forecast slots rather than the current observation `wttr.in`'s
//! simple format returns. The hour is an argument, resolved in the owner's zone by the caller,
//! because the feed's slots are city-local and the box runs on UTC. `suspect` holds two
//! deliberately loose plausibility thresholds: it catches a broken feed, never a real cold front.

mod effect;
pub use effect::Weather;

/// One forecast slot.
///
/// `precip_mm` is a float because the feed reports tenths of a millimetre the line prints.
#[derive(Debug, Clone, PartialEq)]
pub struct Hour {
    /// 0..=23, local to the place being forecast.
    pub hour: u32,
    /// Degrees Celsius.
    pub(crate) temp_c: i32,
    /// Chance of rain, 0..=100.
    pub(crate) rain_pct: u32,
    /// Millimetres.
    pub(crate) precip_mm: f64,
    /// The feed's own words for the sky.
    pub(crate) desc: String,
}

/// The day's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The day's low.
    pub(crate) low_c: i32,
    /// The day's high.
    pub(crate) high_c: i32,
    /// Whether to carry one.
    pub umbrella: bool,
    /// The sentence the briefing prints.
    pub line: String,
}

/// Rain chance at or above which the umbrella is worth carrying.
///
/// wttr.in's `chanceofrain` is coarse; 50% is the point where "it'll probably
/// be fine" stops being a defensible thing to tell someone.
const UMBRELLA_PCT: u32 = 50;
/// Below [`UMBRELLA_PCT`] and at or above this, the umbrella is the safe call.
const MAYBE_PCT: u32 = 30;

/// A coastal city does not lose this much between two three-hour slots.
const MAX_SLOT_SWING_C: i32 = 15;
/// Nor swing this much between its daily minimum and maximum.
const MAX_DAILY_SPREAD_C: i32 = 20;

/// The forecast slots still ahead: a past downpour must not drive the verdict, nor an early dry
/// spell hide an evening one. When every slot is behind, the last one stands in.
#[must_use]
pub fn remaining(hours: &[Hour], now_hour: u32) -> Vec<Hour> {
    let ahead: Vec<Hour> = hours
        .iter()
        .filter(|slot| slot.hour >= now_hour)
        .cloned()
        .collect();
    if ahead.is_empty() {
        hours.last().cloned().into_iter().collect()
    } else {
        ahead
    }
}

/// When the wettest hour ahead falls, in the words a person uses.
fn when(peak: &Hour) -> String {
    let part = if peak.hour >= 20 {
        "tonight"
    } else if peak.hour >= 17 {
        "this evening"
    } else if peak.hour >= 12 {
        "this afternoon"
    } else {
        "this morning"
    };
    format!(
        "{}% {part} ({:02}:00, {}mm)",
        peak.rain_pct, peak.hour, peak.precip_mm
    )
}

/// The day summary and the umbrella call, from the hours still ahead; `None` when the feed held
/// no hours at all.
#[must_use]
pub fn verdict(hours: &[Hour], now_hour: u32, low_c: i32, high_c: i32) -> Option<Verdict> {
    let ahead = remaining(hours, now_hour);
    let peak = ahead.iter().max_by(|a, b| {
        (a.rain_pct, a.precip_mm)
            .partial_cmp(&(b.rain_pct, b.precip_mm))
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let temps = format!("{low_c}-{high_c}°C");
    let (umbrella, line) = if peak.rain_pct >= UMBRELLA_PCT {
        (
            true,
            format!("{temps}, rain likely — {}. Take an umbrella.", when(peak)),
        )
    } else if peak.rain_pct >= MAYBE_PCT {
        (
            true,
            format!(
                "{temps}, rain possible — {}. Umbrella is the safe call.",
                when(peak)
            ),
        )
    } else {
        (
            false,
            format!(
                "{temps}, {}, rain chance peaks at {}% — no umbrella needed.",
                ahead[0].desc.to_lowercase(),
                peak.rain_pct
            ),
        )
    };
    Some(Verdict {
        low_c,
        high_c,
        umbrella,
        line,
    })
}

/// Why this payload should not be trusted, or `None` if it looks like weather.
///
/// A reason rather than a refusal, so the briefing still has a line; it judges the data, so the
/// warning covers the whole line, rain included.
#[must_use]
pub(crate) fn suspect(hours: &[Hour], low_c: i32, high_c: i32) -> Option<String> {
    if high_c - low_c > MAX_DAILY_SPREAD_C {
        return Some(format!(
            "daily range {low_c}-{high_c}°C spans {}°C",
            high_c - low_c
        ));
    }
    hours.windows(2).find_map(|pair| {
        let (a, b) = (&pair[0], &pair[1]);
        let swing = (b.temp_c - a.temp_c).abs();
        (swing > MAX_SLOT_SWING_C).then(|| {
            format!(
                "{swing}°C jump between {:02}:00 ({}°C) and {:02}:00 ({}°C)",
                a.hour, a.temp_c, b.hour, b.temp_c
            )
        })
    })
}

/// The one line, with any caveat on the same line because the only consumer reads one line.
#[must_use]
pub fn line(hours: &[Hour], now_hour: u32, low_c: i32, high_c: i32) -> String {
    let Some(verdict) = verdict(hours, now_hour, low_c, high_c) else {
        return "no forecast: the feed named no hours for today.".to_owned();
    };
    match suspect(hours, low_c, high_c) {
        None => verdict.line,
        Some(why) => format!(
            "{}  [UNTRUSTED FEED: {why} — cross-check before reporting]",
            verdict.line
        ),
    }
}

/// wttr.in's `j1` payload, read into today's slots and the day's range.
///
/// A missing or misshapen field is a refusal here rather than a `0` that reaches a sentence; the
/// feed's numbers are strings, so each is parsed.
pub fn parse(payload: &serde_json::Value) -> Result<(Vec<Hour>, i32, i32), WeatherError> {
    let today = payload
        .get("weather")
        .and_then(|w| w.get(0))
        .ok_or(WeatherError::Unreadable("no weather[0]"))?;
    let number =
        |value: Option<&serde_json::Value>, what: &'static str| -> Result<i32, WeatherError> {
            value
                .and_then(serde_json::Value::as_str)
                .and_then(|text| text.trim().parse().ok())
                .ok_or(WeatherError::Unreadable(what))
        };
    let hours = today
        .get("hourly")
        .and_then(serde_json::Value::as_array)
        .ok_or(WeatherError::Unreadable("no hourly"))?
        .iter()
        .map(|slot| {
            Ok(Hour {
                // wttr.in spells the hour as "0", "300", "600" …
                hour: u32::try_from(number(slot.get("time"), "no time")? / 100)
                    .map_err(|_| WeatherError::Unreadable("a negative hour"))?,
                temp_c: number(slot.get("tempC"), "no tempC")?,
                rain_pct: u32::try_from(number(slot.get("chanceofrain"), "no chanceofrain")?)
                    .map_err(|_| WeatherError::Unreadable("a negative chanceofrain"))?,
                precip_mm: slot
                    .get("precipMM")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|text| text.trim().parse().ok())
                    .ok_or(WeatherError::Unreadable("no precipMM"))?,
                desc: slot
                    .get("weatherDesc")
                    .and_then(|d| d.get(0))
                    .and_then(|d| d.get("value"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            })
        })
        .collect::<Result<Vec<Hour>, WeatherError>>()?;
    Ok((
        hours,
        number(today.get("mintempC"), "no mintempC")?,
        number(today.get("maxtempC"), "no maxtempC")?,
    ))
}

/// The forecast could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WeatherError {
    /// The payload is not the shape this feed documents.
    #[error("the forecast could not be read: {0}")]
    Unreadable(&'static str),
    /// The feed could not be reached.
    #[error("the forecast could not be fetched: {0}")]
    Unreachable(String),
}
