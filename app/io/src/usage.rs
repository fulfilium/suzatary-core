//! The reader behind [`suzatary_core::capability::Usage`]: Claude Code's
//! transcripts, walked for token counts, plus the undocumented endpoint `/usage` reads.
//!
//! Transcripts quote mail, so a line is deserialised into a struct with no field for
//! content and only numbers leave this module. The author is read off the project
//! directory, never the prompt, and files outside the lookback are not opened.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suzatary_core::capability::{Usage, UsageError};
use suzatary_core::usage::{
    Allowance as AllowanceSum, Overage, Refused, Reported, Request, Scope, Tokens, Who, Window,
};

use crate::http::{Http, Req, Ureq};
use crate::present::Present as _;
use crate::secrets::{Resolves, Secretspec};

/// One transcript's requests keyed by request id, and its refusals.
type Reduced = (Vec<(String, Request)>, Vec<Refused>);

/// The report's longest window is seven days; the extra day covers a transcript that
/// was still being written when that window opened.
const LOOKBACK_DAYS: i64 = 8;

/// The transcripts directory, how to place a transcript's author, and what has
/// already been read.
#[derive(Debug)]
pub struct Transcripts {
    root: PathBuf,
    session: String,
    automated: String,
    lookback: chrono::Duration,
    /// Keyed by the (modified, length) a file had when read, so only a grown file is
    /// re-read.
    cache: Mutex<HashMap<PathBuf, CachedFile>>,
}

#[derive(Debug)]
struct CachedFile {
    modified: SystemTime,
    len: u64,
    reduced: Reduced,
}

impl Transcripts {
    /// `root` is the projects directory (`~/.claude/projects`); `session_cwd`
    /// and `automated_cwd` are the two working directories whose transcripts
    /// this box knows the author of.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, session_cwd: &Path, automated_cwd: &Path) -> Self {
        Self {
            root: root.into(),
            session: project_name(session_cwd),
            automated: project_name(automated_cwd),
            lookback: chrono::Duration::days(LOOKBACK_DAYS),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Look back a different number of days.
    #[must_use]
    pub fn looking_back(mut self, days: i64) -> Self {
        self.lookback = chrono::Duration::days(days);
        self
    }

    /// Read everything inside the lookback into the cache and return the file count.
    pub fn warm(&self) -> Result<usize, UsageError> {
        let files = self.files()?;
        for (path, modified, len) in &files {
            self.read(path, *modified, *len)?;
        }
        Ok(files.len())
    }

    fn horizon_now(&self) -> DateTime<Utc> {
        Utc::now() - self.lookback
    }

    fn who(&self, transcript: &Path) -> Who {
        let name = transcript
            .strip_prefix(&self.root)
            .ok()
            .and_then(|rest| rest.components().next())
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == self.session {
            Who::Session
        } else if name == self.automated {
            Who::Automated
        } else {
            Who::Other
        }
    }

    /// Every `.jsonl` under the root modified inside the lookback, with the
    /// (modified, length) it has now.
    fn files(&self) -> Result<Vec<(PathBuf, SystemTime, u64)>, UsageError> {
        let cutoff: SystemTime = self.horizon_now().into();
        let mut found = Vec::new();
        let mut pending = vec![self.root.clone()];
        while let Some(dir) = pending.pop() {
            let unreadable = |failed: std::io::Error| unreadable(&dir, &failed);
            // A missing root is a box with no transcripts, and a directory gone since its
            // parent was listed holds none: neither is a failure.
            let Some(entries) = std::fs::read_dir(&dir).present().map_err(unreadable)? else {
                continue;
            };
            for entry in entries {
                let entry = entry.map_err(unreadable)?;
                let path = entry.path();
                // A file that went between the listing and this look was never read.
                let unreadable_file = |failed: std::io::Error| self::unreadable(&path, &failed);
                let Some(meta) = std::fs::metadata(&path)
                    .present()
                    .map_err(unreadable_file)?
                else {
                    continue;
                };
                if meta.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "jsonl") {
                    continue;
                }
                let modified = meta.modified().map_err(unreadable_file)?;
                if modified < cutoff {
                    continue;
                }
                found.push((path, modified, meta.len()));
            }
        }
        Ok(found)
    }

    /// Which project directories contributed a file inside the lookback, and which were found
    /// but skipped.
    pub fn read_scope(&self) -> Result<Scope, UsageError> {
        let all = shallow_project_dirs(&self.root)?;
        let mut counted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (path, _, _) in self.files()? {
            if let Some(name) = path
                .strip_prefix(&self.root)
                .ok()
                .and_then(|rest| rest.components().next())
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
            {
                counted.insert(name);
            }
        }
        let mut not_counted: Vec<String> = all
            .into_iter()
            .filter(|name| !counted.contains(name))
            .collect();
        not_counted.sort();
        Ok(Scope {
            counted: counted.into_iter().collect(),
            not_counted,
        })
    }

    fn read(&self, path: &Path, modified: SystemTime, len: u64) -> Result<Reduced, UsageError> {
        if let Ok(cache) = self.cache.lock()
            && let Some(hit) = cache.get(path)
            && hit.modified == modified
            && hit.len == len
        {
            return Ok(hit.reduced.clone());
        }
        let reduced = reduce(path, self.who(path))?;
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(
                path.to_path_buf(),
                CachedFile {
                    modified,
                    len,
                    reduced: reduced.clone(),
                },
            );
        }
        Ok(reduced)
    }
}

/// One transcript line, with no field for content on purpose: serde skips what it
/// does not name, so a mail body never becomes a `String` in this process.
#[derive(Deserialize)]
struct Line<'a> {
    #[serde(rename = "type", borrow)]
    kind: Option<&'a str>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    timestamp: Option<String>,
    message: Option<Message>,
    #[serde(rename = "quotaLimits")]
    quota_limits: Option<QuotaLimits>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<UsageFields>,
}

#[derive(Deserialize)]
struct UsageFields {
    #[serde(rename = "input_tokens", default)]
    fresh: u64,
    #[serde(rename = "cache_creation_input_tokens", default)]
    cache_write: u64,
    #[serde(rename = "cache_read_input_tokens", default)]
    cache_read: u64,
    #[serde(rename = "output_tokens", default)]
    output: u64,
}

/// What the limit wrote on a refusal; `resetsAt` is in unix seconds.
#[derive(Deserialize)]
struct QuotaLimits {
    #[serde(rename = "rateLimitType")]
    rate_limit_type: Option<String>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
    #[serde(rename = "overageStatus", default)]
    overage_status: String,
    #[serde(rename = "overageDisabledReason", default)]
    overage_disabled_reason: String,
    #[serde(rename = "isUsingOverage", default)]
    is_using_overage: bool,
}

fn reduce(path: &Path, who: Who) -> Result<Reduced, UsageError> {
    let text = std::fs::read_to_string(path)
        .map_err(|failed| UsageError::Unreadable(format!("{}: {failed}", path.display())))?;
    let mut requests = Vec::new();
    let mut refusals = Vec::new();
    for raw in text.lines() {
        // Most bytes are tool results with neither word; skipping them before the
        // parse is the speed, and can only skip lines the parse would skip.
        if !raw.contains("\"usage\"") && !raw.contains("quotaLimits") {
            continue;
        }
        let Ok(line) = serde_json::from_str::<Line<'_>>(raw) else {
            continue;
        };
        if let Some(refused) = refusal_of(&line) {
            refusals.push(refused);
        }
        if let Some((id, request)) = request_of(&line, who) {
            requests.push((id, request));
        }
    }
    Ok((requests, refusals))
}

/// Claude Code's project-directory name for a working directory.
fn project_name(cwd: &Path) -> String {
    cwd.to_string_lossy().replace('/', "-")
}

/// Every directory directly under `root`; a project directory is always a direct
/// child, so no file is opened.
fn shallow_project_dirs(root: &Path) -> Result<Vec<String>, UsageError> {
    let unreadable = |failed: std::io::Error| unreadable(root, &failed);
    let Some(entries) = std::fs::read_dir(root).present().map_err(unreadable)? else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(unreadable)?;
        let looked = std::fs::metadata(entry.path())
            .present()
            .map_err(unreadable)?;
        if looked.is_some_and(|meta| meta.is_dir()) {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

fn unreadable(path: &Path, failed: &std::io::Error) -> UsageError {
    UsageError::Unreadable(format!("{}: {failed}", path.display()))
}

fn instant_of(line: &Line<'_>) -> Option<DateTime<Utc>> {
    line.timestamp
        .as_deref()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|at| at.with_timezone(&Utc))
}

/// Streaming writes one message several times as it grows, so the request id (or the
/// message id) is the identity and the caller keeps the last line under it.
fn request_of(line: &Line<'_>, who: Who) -> Option<(String, Request)> {
    if line.kind? != "assistant" {
        return None;
    }
    let message = line.message.as_ref()?;
    let usage = message.usage.as_ref()?;
    let model = message.model.as_deref()?;
    // The error placeholder Claude Code writes is not a request anyone made.
    if model == "<synthetic>" {
        return None;
    }
    let id = line
        .request_id
        .as_deref()
        .or(message.id.as_deref())?
        .to_owned();
    Some((
        id,
        Request {
            at: instant_of(line)?,
            model: model.to_owned(),
            who,
            tokens: Tokens {
                fresh: usage.fresh,
                cache_write: usage.cache_write,
                cache_read: usage.cache_read,
                output: usage.output,
            },
        },
    ))
}

/// Overage counts as enabled only when not `rejected` and no disabled reason is given.
fn refusal_of(line: &Line<'_>) -> Option<Refused> {
    let limits = line.quota_limits.as_ref()?;
    let window = Window::named(limits.rate_limit_type.as_deref()?)?;
    Some(Refused {
        at: instant_of(line)?,
        window,
        resets_at: limits
            .resets_at
            .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single()),
        overage: Overage {
            enabled: limits.overage_status != "rejected"
                && limits.overage_disabled_reason.is_empty(),
            using: limits.is_using_overage,
            disabled_reason: limits.overage_disabled_reason.clone(),
        },
    })
}

impl Usage for Transcripts {
    fn requests(&self) -> Result<Vec<Request>, UsageError> {
        let mut by_id: HashMap<String, Request> = HashMap::new();
        for (path, modified, len) in self.files()? {
            let (requests, _) = self.read(&path, modified, len)?;
            for (id, request) in requests {
                // The last line under an id is the complete one.
                by_id.insert(id, request);
            }
        }
        Ok(by_id.into_values().collect())
    }

    fn refusals(&self) -> Result<Vec<Refused>, UsageError> {
        let mut all = Vec::new();
        for (path, modified, len) in self.files()? {
            let (_, refusals) = self.read(&path, modified, len)?;
            all.extend(refusals);
        }
        Ok(all)
    }

    fn horizon(&self) -> Option<DateTime<Utc>> {
        Some(self.horizon_now())
    }

    fn allowance(&self) -> Vec<(Window, AllowanceSum)> {
        // The endpoint is `Allowance`'s; bare transcripts report none.
        Vec::new()
    }

    fn scope(&self) -> Result<Scope, UsageError> {
        Transcripts::read_scope(self)
    }
}

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

/// The transcripts plus the allowance endpoint.
///
/// The endpoint is asked at most hourly. The last successful reading is kept in memory and on
/// disk, so a 429 reports `RateLimited` alongside it.
pub struct Allowance {
    transcripts: Transcripts,
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
    pub fn new(
        transcripts: Transcripts,
        secrets: Secretspec,
        reason: &'static str,
        cache: &Path,
    ) -> Self {
        Self::over(
            transcripts,
            Box::new(secrets),
            reason,
            Box::new(Ureq::new()),
            cache.join("last-reported.json"),
        )
    }

    /// [`Allowance::new`] over an injected transport, secrets and remembered path.
    #[must_use]
    pub fn over(
        transcripts: Transcripts,
        secrets: Box<dyn Resolves + Send + Sync>,
        reason: &'static str,
        http: Box<dyn Http>,
        remembered_path: PathBuf,
    ) -> Self {
        let last_reported = read_remembered(&remembered_path);
        Self {
            transcripts,
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

    /// Warm the transcript cache.
    pub fn warm(&self) -> Result<usize, UsageError> {
        self.transcripts.warm()
    }
}

impl Usage for Allowance {
    fn requests(&self) -> Result<Vec<Request>, UsageError> {
        self.transcripts.requests()
    }

    fn refusals(&self) -> Result<Vec<Refused>, UsageError> {
        self.transcripts.refusals()
    }

    fn horizon(&self) -> Option<DateTime<Utc>> {
        self.transcripts.horizon()
    }

    fn allowance(&self) -> Vec<(Window, AllowanceSum)> {
        self.read_allowance().0
    }

    fn scope(&self) -> Result<Scope, UsageError> {
        self.transcripts.scope()
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
    /// [`Usage::allowance`], plus the [`Transition`] this call caused, if any.
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

    const MAIL_BODY: &str = "Please wire the deposit to account 4471 before Friday, Diego";

    /// A transcript line shaped like Claude Code's, with a mail body in it.
    fn line(id: &str, model: &str, at: &str, out: u64) -> String {
        format!(
            r#"{{"type":"assistant","requestId":"{id}","timestamp":"{at}","message":{{"id":"m-{id}","model":"{model}","content":[{{"type":"text","text":"{MAIL_BODY}"}}],"usage":{{"input_tokens":3,"cache_creation_input_tokens":40,"cache_read_input_tokens":600,"output_tokens":{out},"iterations":[{{"input_tokens":3,"output_tokens":{out}}}]}}}}}}"#
        )
    }

    /// A tool-result line, which the prefilter skips without parsing.
    fn tool_line(at: &str) -> String {
        format!(
            r#"{{"type":"user","timestamp":"{at}","message":{{"role":"user","content":[{{"type":"tool_result","content":"{MAIL_BODY} {MAIL_BODY}"}}]}}}}"#
        )
    }

    fn refusal_line(at: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{at}","message":{{"model":"<synthetic>","content":[{{"type":"text","text":"You've hit your limit. {MAIL_BODY}"}}]}},"quotaLimits":{{"status":"rejected","resetsAt":1789221600,"rateLimitType":"seven_day","overageStatus":"rejected","overageDisabledReason":"org_level_disabled_until","isUsingOverage":false}},"error":"rate_limit","isApiErrorMessage":true}}"#
        )
    }

    fn fixture(name: &str) -> (tempfile::TempDir, Transcripts) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("projects");
        let session = root.join("-home-ada");
        let automated = root.join("-home-ada-Documents-Suzatary");
        std::fs::create_dir_all(&session).expect("mkdir");
        std::fs::create_dir_all(&automated).expect("mkdir");
        std::fs::write(
            session.join(format!("{name}.jsonl")),
            [
                line("r1", "claude-opus-5", "2026-09-12T10:00:00.000Z", 10),
                line("r1", "claude-opus-5", "2026-09-12T10:00:00.500Z", 159), // the same request, complete
                tool_line("2026-09-12T10:00:01.000Z"),
                "not json at all".to_owned(),
                refusal_line("2026-09-11T14:12:09.000Z"),
            ]
            .join("\n"),
        )
        .expect("write");
        std::fs::write(
            automated.join("triage.jsonl"),
            line("r2", "claude-sonnet-5", "2026-09-12T10:05:00.000Z", 7),
        )
        .expect("write");
        let transcripts = Transcripts::new(
            root,
            Path::new("/home/ada"),
            Path::new("/home/ada/Documents/Suzatary"),
        );
        (dir, transcripts)
    }

    #[test]
    fn requests_are_deduplicated_and_placed_by_directory() {
        let (_dir, transcripts) = fixture("session");
        let mut requests = transcripts.requests().expect("readable");
        requests.sort_by_key(|request| request.at);
        assert_eq!(requests.len(), 2, "two ids, not three lines: {requests:?}");
        assert_eq!(requests[0].who, Who::Session);
        assert_eq!(
            requests[0].tokens.output, 159,
            "the later, complete line wins"
        );
        assert_eq!(requests[0].tokens.cache_read, 600);
        assert_eq!(requests[1].who, Who::Automated);
        assert_eq!(requests[1].model, "claude-sonnet-5");
    }

    /// The fixture puts a mail body in every line's content, a tool result and a refusal.
    #[test]
    fn nothing_returned_contains_any_text_from_the_transcript() {
        let (_dir, transcripts) = fixture("private");
        let requests = transcripts.requests().expect("readable");
        let refusals = transcripts.refusals().expect("readable");
        let mut strings: Vec<String> = Vec::new();
        for request in &requests {
            strings.push(request.model.clone());
            strings.push(format!("{request:?}"));
        }
        for refused in &refusals {
            strings.push(refused.overage.disabled_reason.clone());
            strings.push(format!("{refused:?}"));
        }
        for word in MAIL_BODY.split_whitespace() {
            let word = word.trim_matches(',');
            if word.len() < 4 {
                continue;
            }
            assert!(
                strings.iter().all(|text| !text.contains(word)),
                "a transcript word leaked: {word:?} in {strings:?}"
            );
        }
    }

    #[test]
    fn a_file_older_than_the_lookback_is_not_opened_and_the_horizon_says_so() {
        let (_dir, transcripts) = fixture("old");
        let stale = transcripts.root.join("-home-ada").join("old.jsonl");
        // A month back, by the file's own clock.
        let then = SystemTime::now() - Duration::from_hours(30 * 24);
        std::fs::File::options()
            .write(true)
            .open(&stale)
            .expect("open")
            .set_modified(then)
            .expect("touch backwards");
        let requests = transcripts.requests().expect("readable");
        assert!(
            requests.iter().all(|request| request.who == Who::Automated),
            "the session's file is a month old and must not be read: {requests:?}"
        );
        let horizon = transcripts
            .horizon()
            .expect("a horizon, since not everything was read");
        assert!(horizon < Utc::now() && horizon > Utc::now() - chrono::Duration::days(9));
        let (dir2, wide) = fixture("wide");
        let wide = wide.looking_back(60);
        let stale2 = dir2
            .path()
            .join("projects")
            .join("-home-ada")
            .join("wide.jsonl");
        std::fs::File::options()
            .write(true)
            .open(&stale2)
            .expect("open")
            .set_modified(then)
            .expect("touch backwards");
        assert_eq!(wide.requests().expect("readable").len(), 2);
    }

    #[test]
    fn an_unmoved_file_is_served_from_the_cache_and_a_grown_one_is_reread() {
        let (_dir, transcripts) = fixture("cache");
        assert_eq!(transcripts.warm().expect("readable"), 2, "two files warmed");
        assert_eq!(
            transcripts.cache.lock().expect("not poisoned").len(),
            2,
            "both files are now cached"
        );
        let first = transcripts.requests().expect("readable");
        assert_eq!(first.len(), 2);
        let path = transcripts.root.join("-home-ada").join("cache.jsonl");
        let mut text = std::fs::read_to_string(&path).expect("read");
        text.push('\n');
        text.push_str(&line(
            "r3",
            "claude-haiku-4-5",
            "2026-09-12T11:00:00.000Z",
            1,
        ));
        std::fs::write(&path, text).expect("append");
        let second = transcripts.requests().expect("readable");
        assert_eq!(second.len(), 3, "the grown file was re-read: {second:?}");
    }

    #[test]
    fn an_unreadable_root_is_an_error_never_nothing() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("projects");
        std::fs::write(&root, "not a directory").expect("write");
        let transcripts =
            Transcripts::new(root, Path::new("/home/ada"), Path::new("/home/ada/repo"));
        assert!(transcripts.requests().is_err());
        assert!(transcripts.refusals().is_err());
        assert!(transcripts.warm().is_err());
        assert!(Usage::scope(&transcripts).is_err());
    }

    /// Three directories: the session's, the automated one, and a third whose only
    /// file is older than the lookback.
    #[test]
    fn scope_names_what_was_read_and_what_was_found_but_not() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("projects");
        let session = root.join("-home-ada");
        let automated = root.join("-home-ada-Documents-Suzatary");
        let elsewhere = root.join("-tmp");
        std::fs::create_dir_all(&session).expect("mkdir");
        std::fs::create_dir_all(&automated).expect("mkdir");
        std::fs::create_dir_all(&elsewhere).expect("mkdir");
        std::fs::write(
            session.join("s.jsonl"),
            line("r1", "claude-opus-5", "2026-09-12T10:00:00.000Z", 10),
        )
        .expect("write");
        std::fs::write(
            automated.join("a.jsonl"),
            line("r2", "claude-sonnet-5", "2026-09-12T10:05:00.000Z", 7),
        )
        .expect("write");
        let stale = elsewhere.join("old.jsonl");
        std::fs::write(
            &stale,
            line("r3", "claude-haiku-4-5", "2025-01-01T00:00:00.000Z", 1),
        )
        .expect("write");
        let then = SystemTime::now() - Duration::from_hours(30 * 24);
        std::fs::File::options()
            .write(true)
            .open(&stale)
            .expect("open")
            .set_modified(then)
            .expect("touch backwards");
        let transcripts = Transcripts::new(
            root,
            Path::new("/home/ada"),
            Path::new("/home/ada/Documents/Suzatary"),
        );
        let scope = Usage::scope(&transcripts).expect("a scope");
        assert_eq!(
            scope.counted,
            vec![
                "-home-ada".to_owned(),
                "-home-ada-Documents-Suzatary".to_owned(),
            ]
        );
        assert_eq!(scope.not_counted, vec!["-tmp".to_owned()]);
    }

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

    fn empty_transcripts() -> Transcripts {
        Transcripts::new(
            "/nonexistent/for/sure",
            Path::new("/home/ada"),
            Path::new("/home/ada/repo"),
        )
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
            empty_transcripts(),
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
