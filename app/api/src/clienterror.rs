//! A browser's failure, told to the box as one warning line and never stored, so there is
//! nothing to read back, age out or leak.
//!
//! Every field is an [`Excerpt`], so the line is bounded, one line whatever the tab sends (a
//! stack cannot forge a second log entry) and names no address or key; the page is the route's
//! KIND alone, since an address can carry a todo's text or a mail search. Each login has a
//! [`Limiter`] budget.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use suzatary_base::capability::Login;
use suzatary_base::refusal::Refusal;
use suzatary_core::excerpt::Excerpt;

/// The most a report's body may carry, in bytes.
pub const MAX_BODY_BYTES: usize = 16_384;

/// What failed in the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "ClientErrorKind")]
pub enum Kind {
    /// An uncaught error (`window.onerror`).
    Error,
    /// A promise nobody handled (`unhandledrejection`).
    Rejection,
    /// The Wasm core would not load or instantiate: the tab has no evaluator.
    Core,
    /// The tab's tabulation of the box's HTML holds two values for one key: the box rendered
    /// one source twice, differently.
    Finding,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Error => "error",
            Kind::Rejection => "rejection",
            Kind::Core => "core",
            Kind::Finding => "finding",
        }
    }
}

/// `POST /api/client-errors`: a report as the tab sends it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(rename = "ClientReport")]
pub struct Sent {
    /// What failed.
    pub kind: Kind,
    /// The failure's words.
    pub message: String,
    /// The build the tab runs, `""` when it does not know.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub build: String,
    /// The device's user agent.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub agent: String,
    /// The route's kind, never its address.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub page: String,
    /// The stack, `""` for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub stack: String,
}

/// One report, its fields already cut to their bounds by [`Report::of`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    kind: Kind,
    build: Excerpt,
    agent: Excerpt,
    page: Page,
    message: Excerpt,
    stack: Excerpt,
}

/// The page a report was made on: a route's kind, a lowercase word (`todo`, `mail`), never the
/// address it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Page(String);

/// The longest route kind.
const PAGE_CHARS: usize = 24;

impl Page {
    fn parse(text: &str) -> Result<Self, Refusal> {
        if text.is_empty()
            || (text.len() <= PAGE_CHARS && text.bytes().all(|b| b.is_ascii_lowercase()))
        {
            return Ok(Page(text.to_owned()));
        }
        Err(Refusal::bad_request(format!(
            "page is a route's kind, a lowercase word of at most {PAGE_CHARS} letters, never its address"
        )))
    }
}

/// Each field's most characters in the log line: enough to name a frame and a
/// device, never enough for one tab to fill the journal.
const BUILD_CHARS: usize = 64;
const AGENT_CHARS: usize = 200;
const MESSAGE_CHARS: usize = 500;
const STACK_CHARS: usize = 1_500;

impl Report {
    /// A report out of what the tab sent, each field cut to its bound.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] naming the field: `message` blank, `page` not a route's kind.
    pub fn of(sent: &Sent) -> Result<Self, Refusal> {
        if sent.message.trim().is_empty() {
            return Err(Refusal::bad_request("message is required"));
        }
        Ok(Report {
            kind: sent.kind,
            build: Excerpt::head(&sent.build, BUILD_CHARS),
            agent: Excerpt::head(&sent.agent, AGENT_CHARS),
            page: Page::parse(&sent.page)?,
            message: Excerpt::head(&sent.message, MESSAGE_CHARS),
            stack: Excerpt::head(&sent.stack, STACK_CHARS),
        })
    }

    /// The one log line: what failed, where, on which build and device.
    #[must_use]
    pub fn line(&self, login: &Login) -> String {
        format!(
            "client {} from {login} on build {} at {} ({}): {} | stack: {}",
            self.kind.name(),
            or_unknown(&self.build),
            if self.page.0.is_empty() {
                "?"
            } else {
                &self.page.0
            },
            or_unknown(&self.agent),
            self.message,
            or_unknown(&self.stack),
        )
    }
}

fn or_unknown(text: &Excerpt) -> String {
    if text.is_empty() {
        "?".to_owned()
    } else {
        text.to_string()
    }
}

/// How many reports a login may make in how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Reports admitted per window.
    pub most: u32,
    /// The window.
    pub per: Duration,
}

/// Thirty reports per login per ten minutes: a crash loop tells the box what
/// is wrong in its first few lines, and the rest are the same line.
pub const BUDGET: Budget = Budget {
    most: 30,
    per: Duration::from_mins(10),
};

/// What the limiter answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Log it.
    Admit,
    /// Past the budget: 429.
    Refuse,
}

/// A fixed window per login; the caller passes `now`, so the rule is tested without a clock.
#[derive(Debug, Clone)]
pub struct Limiter {
    budget: Budget,
    windows: HashMap<Login, (Instant, u32)>,
}

impl Limiter {
    /// An empty limiter over a budget.
    #[must_use]
    pub fn new(budget: Budget) -> Self {
        Limiter {
            budget,
            windows: HashMap::new(),
        }
    }

    /// Count one report from `login` at `now`. Closed windows are dropped, so the table holds
    /// only logins heard from lately.
    pub fn admit(&mut self, login: &Login, now: Instant) -> Verdict {
        let per = self.budget.per;
        self.windows
            .retain(|_, (start, _)| now.saturating_duration_since(*start) < per);
        let (_, count) = self.windows.entry(login.clone()).or_insert((now, 0));
        if *count >= self.budget.most {
            return Verdict::Refuse;
        }
        *count += 1;
        Verdict::Admit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn login(name: &str) -> Login {
        Login::from_whois(name.to_owned())
    }

    /// A body as the route reads it: serde's parse, then [`Report::of`].
    fn parse(body: &serde_json::Value) -> Result<Report, Refusal> {
        serde_json::from_value::<Sent>(body.clone())
            .map_err(|failed| Refusal::bad_request(failed.to_string()))
            .and_then(|sent| Report::of(&sent))
    }

    #[test]
    fn the_line_is_one_line_and_bounded_whatever_the_tab_sends() {
        let long = "x\ny\u{1b}".repeat(10_000);
        let report = parse(&json!({
            "kind": "rejection",
            "build": long,
            "agent": long,
            "message": long,
            "stack": long,
        }))
        .expect("parses");
        let line = report.line(&login("owner@example.invalid"));
        assert!(!line.chars().any(char::is_control), "{line}");
        let most = BUILD_CHARS + AGENT_CHARS + MESSAGE_CHARS + STACK_CHARS;
        // An escaped character prints at most six, plus the fixed words.
        assert!(line.chars().count() <= most * 6 + 200, "{}", line.len());
        assert!(line.contains('…'));
    }

    #[test]
    fn a_page_is_a_routes_kind_never_its_address() {
        for address in [
            "#/add?text=a%20secret",
            "#/mail?q=from:someone",
            "Todo",
            "#/",
        ] {
            let refused = parse(&json!({"kind": "error", "message": "m", "page": address}))
                .expect_err(address);
            assert!(refused.message().contains("page"), "{address}");
        }
        let line = parse(&json!({
            "kind": "error",
            "message": "TypeError at https://box.example/x?token=s3cr3tvalue for a@b.example",
        }))
        .expect("parses")
        .line(&login("owner@example.invalid"));
        assert!(
            !line.contains("s3cr3tvalue") && !line.contains("a@b"),
            "{line}"
        );
    }

    #[test]
    fn the_budget_is_per_login_and_per_window() {
        let budget = Budget {
            most: 3,
            per: Duration::from_mins(1),
        };
        let mut limiter = Limiter::new(budget);
        let t0 = Instant::now();
        let (a, b) = (login("a@example.invalid"), login("b@example.invalid"));
        for _ in 0..3 {
            assert_eq!(limiter.admit(&a, t0), Verdict::Admit);
        }
        assert_eq!(limiter.admit(&a, t0), Verdict::Refuse);
        assert_eq!(limiter.admit(&b, t0), Verdict::Admit);
        let later = t0 + Duration::from_secs(61);
        assert_eq!(limiter.admit(&a, later), Verdict::Admit);
        assert_eq!(limiter.windows.len(), 1, "b's closed window was dropped");
    }
}
