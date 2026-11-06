//! The undocumented endpoint Claude Code's `/usage` reads: what the subscription's allowance
//! stands at, per window, which the compactor reads before it spends any of it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suzatary_core::usage::{Allowance as AllowanceSum, Reported, Window};

use crate::http::{Http, Req, Ureq};
use crate::secrets::{Resolves, Secretspec};

/// Where Claude Code's own `/usage` asks; undocumented, and the OAuth token is in
/// scope for it.
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug)]
enum Answered {
    Body(Value),
    /// Its `retry-after`, or an hour when it gave none.
    Throttled(Duration),
    /// Any other status but success: the token is refused, and asking again changes nothing.
    Refused {
        status: u16,
        words: String,
    },
}

/// The most of a refusal's words the page is given.
const WORDS_CAP: usize = 300;

/// The error's `type` and `message` when the body is the API's error shape, else the body.
fn words_of(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let shaped = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|answer| {
            let error = answer.get("error")?;
            let words: Vec<&str> = ["type", "message"]
                .iter()
                .filter_map(|key| error.get(key).and_then(Value::as_str))
                .collect();
            (!words.is_empty()).then(|| words.join(": "))
        });
    shaped
        .unwrap_or_else(|| text.trim().to_owned())
        .chars()
        .take(WORDS_CAP)
        .collect()
}

/// A throttle is an answer the caller schedules by, not an error.
fn fetch_allowance(http: &dyn Http, token: &str) -> Result<Answered, String> {
    let resp = http
        .send(
            Req::get(USAGE_URL)
                .header("Authorization", format!("Bearer {token}"))
                .header("anthropic-beta", "oauth-2025-04-20")
                .header("Accept", "application/json")
                .timeout(Duration::from_secs(8)),
        )
        .map_err(|failed| failed.to_string())?;
    if resp.status == 429 {
        let seconds = resp
            .header("retry-after")
            .and_then(|text| text.trim().parse::<u64>().ok())
            .unwrap_or(3600);
        return Ok(Answered::Throttled(Duration::from_secs(seconds)));
    }
    if !(200..300).contains(&resp.status) {
        return Ok(Answered::Refused {
            status: resp.status,
            words: words_of(&resp.body),
        });
    }
    serde_json::from_slice::<Value>(&resp.body)
        .map(Answered::Body)
        .map_err(|failed| format!("not JSON: {failed}"))
}

/// The endpoint's shape is undocumented, so `utilization` may be a percentage or a
/// fraction and `resets_at` RFC 3339 or unix seconds; anything else yields nothing.
fn reported_of(answer: &Value, as_of: DateTime<Utc>) -> HashMap<Window, Reported> {
    let mut found = HashMap::new();
    let Some(map) = answer.as_object() else {
        return found;
    };
    for (key, entry) in map {
        let Some(window) = Window::named(key) else {
            continue;
        };
        let Some(used) = entry
            .get("utilization")
            .or_else(|| entry.get("used"))
            .and_then(Value::as_f64)
        else {
            continue;
        };
        // A percentage over one is a percentage; a fraction stays one.
        let used = if used > 1.0 { used / 100.0 } else { used };
        let resets_at = entry.get("resets_at").and_then(|reset| {
            reset
                .as_str()
                .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
                .map(|at| at.with_timezone(&Utc))
                .or_else(|| {
                    reset
                        .as_i64()
                        .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
                })
        });
        found.insert(
            window,
            Reported {
                utilization: used.clamp(0.0, 1.0),
                resets_at,
                as_of,
            },
        );
    }
    found
}

/// One window's reading as stored on disk, kept apart from [`Reported`] so the core
/// type does not become the file's schema by accident.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredReported {
    utilization: f64,
    resets_at: Option<DateTime<Utc>>,
    as_of: DateTime<Utc>,
}

impl From<&Reported> for StoredReported {
    fn from(reported: &Reported) -> Self {
        Self {
            utilization: reported.utilization,
            resets_at: reported.resets_at,
            as_of: reported.as_of,
        }
    }
}

impl From<StoredReported> for Reported {
    fn from(stored: StoredReported) -> Self {
        Self {
            utilization: stored.utilization,
            resets_at: stored.resets_at,
            as_of: stored.as_of,
        }
    }
}

/// The remembered reading is a cache of the endpoint's last answer: a file missing, unreadable
/// or of another shape is a miss, which costs one read of the endpoint and changes no answer.
fn read_remembered(path: &Path) -> HashMap<Window, Reported> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(stored) = serde_json::from_str::<HashMap<Window, StoredReported>>(&text) else {
        return HashMap::new();
    };
    stored
        .into_iter()
        .map(|(window, reported)| (window, Reported::from(reported)))
        .collect()
}

/// Replaced whole. A failure is ignored: it is only a cache miss on the next start, and this
/// crate does not log.
fn write_remembered(path: &Path, last_reported: &HashMap<Window, Reported>) {
    let stored: HashMap<Window, StoredReported> = last_reported
        .iter()
        .map(|(window, reported)| (*window, StoredReported::from(reported)))
        .collect();
    let Ok(text) = serde_json::to_string(&stored) else {
        return;
    };
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let _ = crate::durable::replace(path, text.as_bytes(), crate::durable::Mode::Private);
}

/// The allowance endpoint.
///
/// The endpoint is asked at most hourly. The last successful reading is kept in memory and on
/// disk, so a 429 reports `RateLimited` alongside it.
pub struct Allowance {
    secrets: Box<dyn Resolves + Send + Sync>,
    reason: &'static str,
    http: Box<dyn Http>,
    remembered_path: PathBuf,
    cache: Mutex<Cached>,
}

/// The read schedule, the last successful answer per window, and whether the
/// endpoint is currently refusing.
#[derive(Debug, Default)]
struct Cached {
    last_reported: HashMap<Window, Reported>,
    throttled_since: Option<DateTime<Utc>>,
    retry_after_seconds: u64,
    next_allowed: Option<DateTime<Utc>>,
    refused: Option<AllowanceSum>,
}

impl Allowance {
    /// Over the network, remembering under `cache`; `reason` is what secretspec logs
    /// for the token read.
    #[must_use]
    pub fn new(secrets: Secretspec, reason: &'static str, cache: &Path) -> Self {
        Self::over(
            Box::new(secrets),
            reason,
            Box::new(Ureq::new()),
            cache.join("last-reported.json"),
        )
    }

    /// [`Allowance::new`] over an injected transport, secrets and remembered path.
    #[must_use]
    pub fn over(
        secrets: Box<dyn Resolves + Send + Sync>,
        reason: &'static str,
        http: Box<dyn Http>,
        remembered_path: PathBuf,
    ) -> Self {
        let last_reported = read_remembered(&remembered_path);
        Self {
            secrets,
            reason,
            http,
            remembered_path,
            cache: Mutex::new(Cached {
                last_reported,
                ..Cached::default()
            }),
        }
    }
}

/// A change in whether the endpoint is answering, handed back for the binary to log
/// once per change because this crate does not log.
#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    /// The first 429 after a success, or the first read ever.
    StartedThrottling {
        /// The endpoint's own `retry-after`, in seconds.
        retry_after_seconds: u64,
    },
    /// The first success after being throttled.
    StoppedThrottling,
}

impl Allowance {
    /// What the allowance stands at per window; a window it never read is unknown.
    #[must_use]
    pub fn allowance(&self) -> Vec<(Window, AllowanceSum)> {
        self.read_allowance().0
    }

    /// The same, plus the [`Transition`] this call caused, if any.
    #[must_use]
    pub fn read_allowance(&self) -> (Vec<(Window, AllowanceSum)>, Option<Transition>) {
        let now = Utc::now();
        let Ok(mut cache) = self.cache.lock() else {
            return (Vec::new(), None);
        };
        let transition = if cache.next_allowed.is_some_and(|next| now < next) {
            None
        } else {
            self.read(&mut cache, now)
        };
        (summarise(&cache), transition)
    }

    fn read(&self, cache: &mut Cached, now: DateTime<Utc>) -> Option<Transition> {
        // Set first, so a page polled every second never becomes a socket per second.
        cache.next_allowed = Some(now + chrono::Duration::hours(1));
        // Unset and locked both leave the allowance unknown, which is what it then is; neither
        // reads as an allowance of nothing.
        let Ok(Some(token)) = self.secrets.get(crate::model::TOKEN, self.reason) else {
            return None;
        };
        let was_throttled = cache.throttled_since.is_some();
        match fetch_allowance(self.http.as_ref(), token.expose()) {
            Ok(Answered::Body(answer)) => {
                let read = reported_of(&answer, now);
                if read.is_empty() {
                    return None;
                }
                cache.throttled_since = None;
                cache.retry_after_seconds = 0;
                cache.refused = None;
                cache.last_reported.extend(read);
                write_remembered(&self.remembered_path, &cache.last_reported);
                was_throttled.then_some(Transition::StoppedThrottling)
            }
            Ok(Answered::Refused { status, words }) => {
                cache.refused = Some(AllowanceSum::Refused {
                    at: now,
                    status,
                    words,
                });
                None
            }
            Ok(Answered::Throttled(wait)) => {
                cache.next_allowed = chrono::Duration::from_std(wait)
                    .ok()
                    .map(|wait| now + wait)
                    .or(cache.next_allowed);
                cache.retry_after_seconds = wait.as_secs();
                if was_throttled {
                    return None;
                }
                cache.throttled_since = Some(now);
                Some(Transition::StartedThrottling {
                    retry_after_seconds: wait.as_secs(),
                })
            }
            Err(_) => None,
        }
    }
}

/// An empty answer or an unreachable transport is `Unknown`, not rate-limited, since
/// the endpoint itself may never have refused; a refusal outranks a remembered reading,
/// which can only be older than it.
fn summarise(cache: &Cached) -> Vec<(Window, AllowanceSum)> {
    [Window::FiveHour, Window::SevenDay]
        .into_iter()
        .map(|window| {
            let sum = if let Some(refused) = &cache.refused {
                refused.clone()
            } else if let Some(since) = cache.throttled_since {
                AllowanceSum::RateLimited {
                    since,
                    retry_after_seconds: cache.retry_after_seconds,
                    last_reported: cache.last_reported.get(&window).cloned(),
                }
            } else if let Some(reported) = cache.last_reported.get(&window) {
                AllowanceSum::Reported(reported.clone())
            } else {
                AllowanceSum::Unknown
            };
            (window, sum)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Resp, Scripted};

    /// A token that is always there, so no test reaches a `secretspec` subprocess.
    struct FixedSecret;

    impl Resolves for FixedSecret {
        fn get(
            &self,
            _name: &str,
            _reason: &str,
        ) -> Result<Option<suzatary_core::capability::Secret>, crate::secrets::SecretError>
        {
            Ok(Some(suzatary_core::capability::Secret::new("test-token")))
        }
    }

    fn body_200(json: &serde_json::Value) -> Scripted {
        Scripted::answering(vec![Ok(Resp::new(200, json.to_string()))])
    }

    fn throttled_429(retry_after: &str) -> Scripted {
        Scripted::answering(vec![Ok(
            Resp::new(429, "").with_header("retry-after", retry_after)
        )])
    }

    fn allowance_over(http: Scripted, remembered_path: &Path) -> super::Allowance {
        super::Allowance::over(
            Box::new(FixedSecret),
            "test",
            Box::new(http),
            remembered_path.to_path_buf(),
        )
    }

    /// A token without `user:profile` is refused for good.
    #[test]
    fn a_scope_refusal_is_said_in_the_endpoints_words_over_a_remembered_reading() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("last-reported.json");
        std::fs::write(
            &path,
            serde_json::json!({
                "five_hour": {"utilization": 0.42, "resets_at": null, "as_of": "2026-09-14T10:00:00Z"}
            })
            .to_string(),
        )
        .expect("seed the remembered file");
        let body = serde_json::json!({"type": "error", "error": {
            "type": "oauth_scope_insufficient",
            "message": "OAuth token does not meet scope requirement user:profile",
        }});
        let reader = allowance_over(
            Scripted::answering(vec![Ok(Resp::new(403, body.to_string()))]),
            &path,
        );
        for (_, allowance) in reader.allowance() {
            match allowance {
                AllowanceSum::Refused { status, words, .. } => {
                    assert_eq!(status, 403);
                    assert_eq!(
                        words,
                        "oauth_scope_insufficient: OAuth token does not meet scope requirement user:profile"
                    );
                }
                other => panic!("expected Refused: {other:?}"),
            }
        }
    }

    #[test]
    fn a_remembered_answer_on_disk_survives_construction_and_rides_out_a_throttle() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("last-reported.json");
        std::fs::write(
            &path,
            serde_json::json!({
                "five_hour": {"utilization": 0.42, "resets_at": null, "as_of": "2026-09-14T10:00:00Z"}
            })
            .to_string(),
        )
        .expect("seed the remembered file");
        let reader = allowance_over(throttled_429("60"), &path);
        let found: std::collections::HashMap<_, _> = reader.allowance().into_iter().collect();
        match &found[&Window::FiveHour] {
            AllowanceSum::RateLimited { last_reported, .. } => {
                let last = last_reported
                    .as_ref()
                    .expect("remembered across the restart");
                assert!((last.utilization - 0.42).abs() < 1e-9);
            }
            other => panic!("expected RateLimited with a remembered reading: {other:?}"),
        }
    }

    #[test]
    fn a_successful_read_is_remembered_to_disk_and_a_throttle_leaves_it_alone() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("last-reported.json");
        let reader = allowance_over(
            body_200(&serde_json::json!({
                "five_hour": {"utilization": 0.55, "resets_at": null},
            })),
            &path,
        );
        let _ = reader.allowance();
        let written = std::fs::read_to_string(&path).expect("the file was written");
        assert!(written.contains("0.55"), "{written}");

        let path2 = dir.path().join("second.json");
        std::fs::write(&path2, &written).expect("seed the second file");
        let second = allowance_over(throttled_429("10"), &path2);
        let found: std::collections::HashMap<_, _> = second.allowance().into_iter().collect();
        match &found[&Window::FiveHour] {
            AllowanceSum::RateLimited { last_reported, .. } => {
                assert!(
                    (last_reported.as_ref().expect("remembered").utilization - 0.55).abs() < 1e-9
                );
            }
            other => panic!("expected RateLimited with the remembered reading: {other:?}"),
        }
    }

    #[test]
    fn a_second_call_inside_the_hour_never_asks_the_endpoint_again() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("last-reported.json");
        let reader = allowance_over(
            body_200(&serde_json::json!({
                "five_hour": {"utilization": 0.2, "resets_at": null},
            })),
            &path,
        );
        let _ = reader.allowance();
        // The script is now empty, so asking again would read as `Unknown`.
        let found: std::collections::HashMap<_, _> = reader.allowance().into_iter().collect();
        assert!(matches!(
            found[&Window::FiveHour],
            AllowanceSum::Reported(_)
        ));
    }

    #[test]
    fn the_first_throttle_is_a_transition_and_a_cached_read_is_not() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("last-reported.json");
        let reader = allowance_over(throttled_429("120"), &path);
        let (_, first) = reader.read_allowance();
        assert_eq!(
            first,
            Some(Transition::StartedThrottling {
                retry_after_seconds: 120
            })
        );
        let (_, second) = reader.read_allowance();
        assert_eq!(
            second, None,
            "still inside the schedule: no second call, no transition"
        );
    }

    /// Calls `read` directly on a throttled `Cached`, so no clock has to elapse.
    #[test]
    fn the_first_success_after_a_throttle_is_a_transition() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("last-reported.json");
        let reader = allowance_over(
            body_200(&serde_json::json!({"five_hour": {"utilization": 0.2, "resets_at": null}})),
            &path,
        );
        let mut cache = Cached {
            throttled_since: Some(Utc::now()),
            retry_after_seconds: 5,
            ..Cached::default()
        };
        let transition = reader.read(&mut cache, Utc::now());
        assert_eq!(transition, Some(Transition::StoppedThrottling));
        assert_eq!(
            cache.throttled_since, None,
            "the throttle itself clears too"
        );
    }
}
