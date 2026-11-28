//! A fake Microsoft on loopback: the sign-in endpoints (device code and token) and the Graph
//! calendar API, checking what Microsoft checks (the client, the scope, a bearer token, a
//! refresh token it issued). The cargo tests serve it beside the fake Google.

use std::collections::BTreeSet;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use super::fake_google::{Request, form, read, respond};

/// The one scope the calendar consent asks for.
pub const SCOPE: &str = "https://graph.microsoft.com/Calendars.ReadWrite offline_access";

/// The access token every refresh mints.
pub const ACCESS: &str = "EwB-fake-access";

pub const USER_CODE: &str = "ABCD-EFGH";

/// What the organisation's administrator has done about the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tenant {
    #[default]
    Open,
    /// Only an administrator may approve the app, so every sign-in is refused.
    AdminOnly,
}

#[derive(Debug, Default)]
struct Held {
    tenant: Tenant,
    /// How many polls answer `authorization_pending` before the code is entered.
    pending: usize,
    refresh: BTreeSet<String>,
    events: usize,
    inserted: Vec<serde_json::Value>,
    /// The requests Graph saw, as `METHOD path`.
    asked: Vec<String>,
}

#[derive(Clone)]
pub struct FakeGraph {
    pub base: String,
    held: Arc<Mutex<Held>>,
}

impl FakeGraph {
    #[must_use]
    pub fn start(events: usize) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("binds a loopback port");
        let base = format!("http://{}", listener.local_addr().expect("a bound address"));
        let held = Arc::new(Mutex::new(Held {
            events,
            ..Held::default()
        }));
        let serving = Arc::clone(&held);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let held = Arc::clone(&serving);
                std::thread::spawn(move || {
                    let _ = answer(stream, &held);
                });
            }
        });
        Self { base, held }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Let an administrator approve, or refuse, the app for the organisation.
    pub fn tenant(&self, tenant: Tenant) {
        self.held().tenant = tenant;
    }

    /// End every grant the person gave, as a lapse or a password change does.
    pub fn ending_every_grant(&self) {
        self.held().refresh.clear();
    }

    #[must_use]
    pub fn inserted(&self) -> Vec<serde_json::Value> {
        self.held().inserted.clone()
    }

    /// The requests Graph itself (not the sign-in) answered, as `METHOD path`.
    #[must_use]
    pub fn asked(&self) -> Vec<String> {
        self.held().asked.clone()
    }

    #[must_use]
    pub fn grants(&self) -> usize {
        self.held().refresh.len()
    }
}

fn refused(stream: TcpStream, status: u16, error: &str, description: &str, codes: &[u32]) {
    respond(
        stream,
        status,
        &[],
        &serde_json::json!({
            "error": error,
            "error_description": description,
            "error_codes": codes,
        })
        .to_string(),
    );
}

fn answer(stream: TcpStream, held: &Mutex<Held>) -> std::io::Result<()> {
    let request = read(&stream)?;
    let mut held = held
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", path) if path.ends_with("/oauth2/v2.0/devicecode") => {
            device_code(stream, &request);
        }
        ("POST", path) if path.ends_with("/oauth2/v2.0/token") => {
            token(stream, &request, &mut held);
        }
        (_, path) if path.starts_with("/v1.0/") => graph(stream, &request, &mut held),
        _ => respond(stream, 404, &[], "{}"),
    }
    Ok(())
}

fn device_code(stream: TcpStream, request: &Request) {
    let fields = form(&String::from_utf8_lossy(&request.body));
    if fields.get("client_id").map(String::as_str) != Some(suzatary_mail::mail::oauth::CLIENT_ID)
        || fields.get("scope").map(String::as_str) != Some(SCOPE)
    {
        refused(
            stream,
            400,
            "invalid_scope",
            "AADSTS70011: the scope is not the one the calendar consent asks for",
            &[70011],
        );
        return;
    }
    respond(
        stream,
        200,
        &[],
        &serde_json::json!({
            "device_code": "the-fake-device-code",
            "user_code": USER_CODE,
            "verification_uri": "https://microsoft.com/devicelogin",
            "message": format!(
                "To sign in, use a web browser to open https://microsoft.com/devicelogin and \
                 enter the code {USER_CODE} to authenticate."
            ),
            "interval": 0,
            "expires_in": 900,
        })
        .to_string(),
    );
}

fn token(stream: TcpStream, request: &Request, held: &mut Held) {
    let fields = form(&String::from_utf8_lossy(&request.body));
    let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
    match field("grant_type").as_str() {
        "urn:ietf:params:oauth:grant-type:device_code" => {
            if field("device_code") != "the-fake-device-code" {
                refused(
                    stream,
                    400,
                    "bad_verification_code",
                    "unknown code",
                    &[70000],
                );
            } else if held.pending > 0 {
                held.pending -= 1;
                refused(
                    stream,
                    400,
                    "authorization_pending",
                    "AADSTS70016: pending",
                    &[70016],
                );
            } else if held.tenant == Tenant::AdminOnly {
                refused(
                    stream,
                    400,
                    "consent_required",
                    "AADSTS90094: The grant requires admin permission.",
                    &[90094],
                );
            } else {
                let refresh = format!("fake-refresh-{}", held.refresh.len() + 1);
                held.refresh.insert(refresh.clone());
                respond(
                    stream,
                    200,
                    &[],
                    &serde_json::json!({
                        "access_token": ACCESS,
                        "refresh_token": refresh,
                        "expires_in": 3599,
                        "scope": SCOPE,
                    })
                    .to_string(),
                );
            }
        }
        "refresh_token" if field("scope") != SCOPE => refused(
            stream,
            400,
            "invalid_scope",
            "AADSTS70011: another consent's scope",
            &[70011],
        ),
        "refresh_token" if !held.refresh.contains(&field("refresh_token")) => refused(
            stream,
            400,
            "invalid_grant",
            "AADSTS700082: The refresh token has expired due to inactivity.",
            &[700_082],
        ),
        "refresh_token" => respond(
            stream,
            200,
            &[],
            &serde_json::json!({"access_token": ACCESS, "expires_in": 3599}).to_string(),
        ),
        _ => refused(
            stream,
            400,
            "unsupported_grant_type",
            "AADSTS70003",
            &[70003],
        ),
    }
}

fn graph(stream: TcpStream, request: &Request, held: &mut Held) {
    if request.authorization.as_deref() != Some(&format!("Bearer {ACCESS}")) {
        respond(
            stream,
            401,
            &[],
            r#"{"error": {"code": "InvalidAuthenticationToken"}}"#,
        );
        return;
    }
    held.asked
        .push(format!("{} {}", request.method, request.path));
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1.0/me/calendars") => respond(
            stream,
            200,
            &[],
            &serde_json::json!({"value": [
                {"id": "AAMk-own", "name": "Calendar", "isDefaultCalendar": true},
                {"id": "AAMk-team", "name": "Team", "isDefaultCalendar": false},
            ]})
            .to_string(),
        ),
        ("GET", events) if events.ends_with("/calendarView") => {
            let items: Vec<serde_json::Value> = (0..held.events)
                .map(|n| {
                    serde_json::json!({
                        "id": format!("AAMk-e{n}"),
                        "iCalUId": format!("e{n}@example.test"),
                        "subject": format!("Meeting {n}"),
                        "start": {"dateTime": "2026-10-19T09:00:00.0000000", "timeZone": "UTC"},
                        "end": {"dateTime": "2026-10-19T09:30:00.0000000", "timeZone": "UTC"},
                    })
                })
                .collect();
            respond(
                stream,
                200,
                &[],
                &serde_json::json!({ "value": items }).to_string(),
            );
        }
        ("POST", "/v1.0/me/calendar/events") => {
            held.inserted
                .push(serde_json::from_slice(&request.body).unwrap_or_default());
            respond(stream, 201, &[], r#"{"id": "AAMk-inserted"}"#);
        }
        _ => respond(stream, 404, &[], "{}"),
    }
}
