//! The routine's fire endpoint. The body is the task and nothing else: the routine's saved
//! prompt is the containment and the task is data. The token comes only through [`Resolves`]
//! by [`ROUTINE_TOKEN`], per fire; no constructor takes one as a value.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use suzatary_base::capability::Secret;
use suzatary_core::coding::{Fired, SessionId, SessionUrl, Task};
use suzatary_core::connect::{ROUTINE_TOKEN, RoutineId};

use crate::http::{Http, Req, Ureq};
use crate::secrets::Resolves;

pub const API: &str = "https://api.anthropic.com";

/// The beta the fire endpoint is behind; a new date is a new header.
pub const BETA: &str = "experimental-cc-routine-2026-04-01";

pub const VERSION: &str = "2023-06-01";

/// A fire starts a session and answers; it does not wait for the session.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Why a fire did not start a session. Never the token.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FireError {
    #[error("{ROUTINE_TOKEN} is not set; set it from Settings, then tap again")]
    NoToken,
    /// The secret store could not say whether it holds one.
    #[error("{0}; nothing was fired")]
    Locked(crate::secrets::SecretError),
    #[error("the routine endpoint could not be reached: {0}")]
    Unreachable(String),
    #[error("the routine refused the token ({0}); set a fresh {ROUTINE_TOKEN} from Settings")]
    Unauthorized(u16),
    /// The account's usage or the daily cap on routine runs.
    #[error("the routine is throttled{}", .0.as_ref().map(|after| format!(" (retry after {after}s)")).unwrap_or_default())]
    Throttled(Option<String>),
    #[error("the routine endpoint answered {status}: {message}")]
    Refused { status: u16, message: String },
    /// A 2xx whose body is not the documented answer.
    #[error("the routine endpoint answered something this cannot read: {0}")]
    Malformed(String),
}

#[derive(Debug)]
pub struct Endpoint<H: Http = Ureq> {
    base: String,
    routine: RoutineId,
    token: Secret,
    http: H,
}

impl Endpoint<Ureq> {
    /// The routine at [`API`], its token read through `secrets`.
    pub fn resolve(routine: RoutineId, secrets: &dyn Resolves) -> Result<Self, FireError> {
        Self::resolve_over(API, routine, secrets, Ureq::new())
    }
}

impl<H: Http> Endpoint<H> {
    /// [`Endpoint::resolve`] over another base and transport.
    pub fn resolve_over(
        base: impl Into<String>,
        routine: RoutineId,
        secrets: &dyn Resolves,
        http: H,
    ) -> Result<Self, FireError> {
        let token = secrets
            .get(ROUTINE_TOKEN, "fire the coding routine the owner tapped")
            .map_err(FireError::Locked)?
            .ok_or(FireError::NoToken)?;
        Ok(Self {
            base: base.into(),
            routine,
            token,
            http,
        })
    }
}

/// The documented answer.
#[derive(Deserialize)]
struct Answer {
    claude_code_session_id: String,
    claude_code_session_url: String,
}

impl<H: Http> Endpoint<H> {
    /// Start one session on `task`, and answer which.
    pub fn fire(&self, task: &Task) -> Result<Fired, FireError> {
        let url = format!(
            "{}/v1/claude_code/routines/{}/fire",
            self.base.trim_end_matches('/'),
            self.routine
        );
        let resp = self
            .http
            .send(
                Req::json(url, &json!({ "text": task.as_str() }))
                    .header("Authorization", format!("Bearer {}", self.token.expose()))
                    .header("anthropic-beta", BETA)
                    .header("anthropic-version", VERSION)
                    .timeout(TIMEOUT),
            )
            .map_err(|failed| FireError::Unreachable(failed.to_string()))?;
        match resp.status {
            200..=299 => fired(&resp.text()),
            401 | 403 => Err(FireError::Unauthorized(resp.status)),
            429 => Err(FireError::Throttled(
                resp.header("retry-after").map(str::to_owned),
            )),
            status => Err(FireError::Refused {
                status,
                message: message_of(&resp.text()),
            }),
        }
    }
}

fn fired(text: &str) -> Result<Fired, FireError> {
    let malformed = |why: String| FireError::Malformed(why);
    let answer: Answer =
        serde_json::from_str(text).map_err(|failed| malformed(failed.to_string()))?;
    Ok(Fired {
        id: SessionId::try_from(answer.claude_code_session_id)
            .map_err(|failed| malformed(failed.to_string()))?,
        url: SessionUrl::try_from(answer.claude_code_session_url)
            .map_err(|failed| malformed(failed.to_string()))?,
    })
}

/// An error body's `error.message`, or its text, bounded.
fn message_of(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| text.trim().chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpError, Resp, Scripted};
    use std::io::{BufRead, BufReader, Write};

    const TOKEN: &str = "sk-ant-routine-fake-token";

    struct Holding(Option<&'static str>);

    impl Resolves for Holding {
        fn get(
            &self,
            name: &str,
            _reason: &str,
        ) -> Result<Option<Secret>, crate::secrets::SecretError> {
            assert_eq!(
                name, ROUTINE_TOKEN,
                "the token is asked for by its one name"
            );
            Ok(self.0.map(Secret::new))
        }
    }

    fn routine() -> RoutineId {
        "trig_01fake".parse().expect("a routine id")
    }

    fn task() -> Task {
        Task::try_from("Fix the typo in the README.\nRun the gates.".to_owned()).expect("a task")
    }

    const ANSWER: &str = r#"{"claude_code_session_id":"cse_01fake","claude_code_session_url":"https://claude.ai/code/cse_01fake"}"#;

    fn answering(status: u16, body: &str) -> Scripted {
        Scripted::always(Ok(Resp::new(status, body)))
    }

    fn over(http: Scripted) -> Endpoint<Scripted> {
        Endpoint::resolve_over(API, routine(), &Holding(Some(TOKEN)), http).expect("a token")
    }

    #[test]
    fn a_fire_is_exactly_the_documented_request_and_reads_its_answer() {
        let endpoint = over(answering(200, ANSWER));
        let fired = endpoint.fire(&task()).expect("fires");
        assert_eq!(fired.id.to_string(), "cse_01fake");
        assert_eq!(fired.url.as_str(), "https://claude.ai/code/cse_01fake");
        let sent = endpoint.http.sent();
        let req = &sent[0];
        assert_eq!(
            req.url,
            "https://api.anthropic.com/v1/claude_code/routines/trig_01fake/fire"
        );
        assert_eq!(
            crate::http::header(&req.headers, "authorization"),
            Some(format!("Bearer {TOKEN}").as_str())
        );
        assert_eq!(
            crate::http::header(&req.headers, "anthropic-beta"),
            Some(BETA)
        );
        assert_eq!(
            crate::http::header(&req.headers, "anthropic-version"),
            Some(VERSION)
        );
        let body: Value = serde_json::from_slice(&req.body).expect("json");
        assert_eq!(body, json!({ "text": task().as_str() }));
    }

    #[test]
    fn no_token_is_refused_before_anything_is_sent() {
        let refused =
            Endpoint::resolve_over(API, routine(), &Holding(None), answering(200, ANSWER));
        assert_eq!(refused.err(), Some(FireError::NoToken));
        assert!(FireError::NoToken.to_string().contains("Settings"));
    }

    #[test]
    fn statuses_are_named_and_no_error_carries_the_token() {
        let cases = [
            (401, "{}", FireError::Unauthorized(401)),
            (429, "{}", FireError::Throttled(None)),
            (
                400,
                r#"{"error":{"message":"routine is paused"}}"#,
                FireError::Refused {
                    status: 400,
                    message: "routine is paused".to_owned(),
                },
            ),
            (
                200,
                r#"{"claude_code_session_id":"a b","claude_code_session_url":"https://claude.ai/x"}"#,
                FireError::Malformed(
                    "a session id is cse_ or session_ then up to 256 letters, digits, '_' and '-', got \"a b\""
                        .to_owned(),
                ),
            ),
        ];
        for (status, body, expected) in cases {
            let failed = over(answering(status, body))
                .fire(&task())
                .expect_err("refused");
            assert_eq!(failed, expected);
            assert!(!failed.to_string().contains(TOKEN));
            assert!(!format!("{failed:?}").contains(TOKEN));
        }
        let unreachable = over(Scripted::always(Err(HttpError::Lost(
            "connection refused".to_owned(),
        ))));
        assert_eq!(
            unreachable.fire(&task()),
            Err(FireError::Unreachable("connection refused".to_owned()))
        );
        assert!(!format!("{:?}", over(answering(200, ANSWER))).contains(TOKEN));
    }

    #[test]
    fn the_live_transport_sends_the_headers_and_body_over_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a connection");
            let mut reader = BufReader::new(stream.try_clone().expect("clones"));
            let mut head = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("a line");
                if line.trim_end().is_empty() {
                    break;
                }
                head.push(line.trim_end().to_owned());
            }
            let length: usize = head
                .iter()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse().expect("a length"))
                })
                .expect("a content-length");
            let mut body = vec![0; length];
            std::io::Read::read_exact(&mut reader, &mut body).expect("the body");
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ANSWER}",
                ANSWER.len()
            )
            .expect("answers");
            (head, String::from_utf8(body).expect("utf-8"))
        });
        let endpoint = Endpoint::resolve_over(base, routine(), &Holding(Some(TOKEN)), Ureq::new())
            .expect("a token");
        let fired = endpoint.fire(&task()).expect("fires over the wire");
        assert_eq!(fired.id.to_string(), "cse_01fake");
        let (head, body) = server.join().expect("the server ran");
        assert_eq!(
            head[0],
            "POST /v1/claude_code/routines/trig_01fake/fire HTTP/1.1"
        );
        let lower: Vec<String> = head.iter().map(|line| line.to_ascii_lowercase()).collect();
        for expected in [
            format!("authorization: bearer {}", TOKEN.to_ascii_lowercase()),
            format!("anthropic-beta: {BETA}"),
            format!("anthropic-version: {VERSION}"),
        ] {
            assert!(lower.contains(&expected), "{expected} in {head:?}");
        }
        assert_eq!(
            serde_json::from_str::<Value>(&body).expect("json"),
            json!({ "text": task().as_str() })
        );
    }
}
