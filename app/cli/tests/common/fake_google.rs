//! A fake Google for the sign-in with Google and the Calendar API: its consent page, its token
//! endpoint and the three Calendar API routes the box asks, over plain http on loopback. It
//! checks what Google checks (the redirect the consent named, the S256 challenge, a code used
//! once) and records what it was sent. The cargo tests and the browser suite's fixture binary
//! (`suzatary-dev fake-google`) run this one file.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// The client the fake knows.
pub const CLIENT_ID: &str = "1234-fake.apps.googleusercontent.com";

/// That client's secret.
pub const CLIENT_SECRET: &str = "GOCSPX-fake-client-secret";

/// The access token every refresh mints.
pub const ACCESS: &str = "ya29.fake-access";

/// The client file Google's Download JSON would give for the fake's client, listing `redirect`.
#[must_use]
pub fn client_file(redirect: &str) -> String {
    serde_json::json!({"web": {
        "client_id": CLIENT_ID,
        "project_id": "suzatary-fake",
        "auth_uri": "https://accounts.google.com/o/oauth2/auth",
        "token_uri": "https://oauth2.googleapis.com/token",
        "client_secret": CLIENT_SECRET,
        "redirect_uris": [redirect],
    }})
    .to_string()
}

/// One code the consent issued.
#[derive(Debug, Clone)]
struct Issued {
    challenge: String,
    redirect: String,
    scope: String,
}

/// What the fake holds and saw.
#[derive(Debug, Default)]
struct Held {
    /// Who signs in on the consent page.
    account: String,
    /// Whether the consent page signs in the account its `login_hint` names instead.
    hinted: bool,
    /// Whether the Gmail API refuses to read the mailbox, as an account with Gmail turned off
    /// does.
    mailless: bool,
    codes: BTreeMap<String, Issued>,
    used: BTreeSet<String>,
    refresh: BTreeSet<String>,
    revoked: bool,
    /// The scopes the person unticks on the consent page.
    unticked: BTreeSet<String>,
    /// Every scope the account has granted the client, which a consent with
    /// `include_granted_scopes=true` adds to.
    granted: BTreeSet<String>,
    /// How many events each calendar answers with, whatever `maxResults` asks.
    events: usize,
    exchanges: usize,
    inserted: Vec<serde_json::Value>,
    asked: Vec<String>,
}

/// A running fake.
#[derive(Clone)]
pub struct FakeGoogle {
    /// Its base URL, `http://127.0.0.1:<port>`.
    pub base: String,
    held: Arc<Mutex<Held>>,
}

impl FakeGoogle {
    /// Serve on `port` (0 for any) in the background, signing `account` in.
    ///
    /// # Panics
    /// When the port cannot be bound.
    #[must_use]
    pub fn start(port: u16, account: &str) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("binds a loopback port");
        let base = format!("http://{}", listener.local_addr().expect("a bound address"));
        let held = Arc::new(Mutex::new(Held {
            account: account.to_owned(),
            events: 3,
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

    /// Sign `account` in on the consent page from now on.
    pub fn signing_in(&self, account: &str) {
        account.clone_into(&mut self.held().account);
    }

    /// Sign in, from now on, the account each consent's `login_hint` names, as a person signed in
    /// to that account would.
    pub fn following_hints(&self) {
        self.held().hinted = true;
    }

    /// Refuse every read of a mailbox from now on, as an account with Gmail turned off does, or
    /// answer them again.
    pub fn refusing_mail(&self, refusing: bool) {
        self.held().mailless = refusing;
    }

    /// Revoke every grant: a refresh answers `invalid_grant`, as a revoked or 7-day grant does.
    pub fn revoke(&self) {
        self.held().revoked = true;
    }

    /// Untick `scopes` on the consent page from now on, or tick every one again with none.
    pub fn unticking(&self, scopes: &[&str]) {
        self.held().unticked = scopes.iter().map(|&scope| scope.to_owned()).collect();
    }

    /// Answer each calendar's events with `count`, whatever the request asks for.
    pub fn answering_events(&self, count: usize) {
        self.held().events = count;
    }

    /// How many code exchanges reached the token endpoint.
    #[must_use]
    pub fn exchanges(&self) -> usize {
        self.held().exchanges
    }

    /// Every event inserted, as posted.
    #[must_use]
    pub fn inserted(&self) -> Vec<serde_json::Value> {
        self.held().inserted.clone()
    }

    /// Every request line it was sent, in order.
    #[must_use]
    pub fn asked(&self) -> Vec<String> {
        self.held().asked.clone()
    }
}

/// One request, read.
struct Request {
    method: String,
    path: String,
    query: BTreeMap<String, String>,
    authorization: Option<String>,
    body: Vec<u8>,
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| bytes.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            out.push(byte);
            at += 3;
        } else {
            out.push(if bytes[at] == b'+' { b' ' } else { bytes[at] });
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn form(text: &str) -> BTreeMap<String, String> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(key), decode(value))
        })
        .collect()
}

fn read(stream: &TcpStream) -> std::io::Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let (mut length, mut authorization) = (0usize, None);
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "authorization" => authorization = Some(value.trim().to_owned()),
                _ => {}
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    Ok(Request {
        method,
        path: path.to_owned(),
        query: form(query),
        authorization,
        body,
    })
}

fn respond(mut stream: TcpStream, status: u16, headers: &[(&str, String)], body: &str) {
    let mut head = format!(
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

/// base64url without padding, for the ID token's parts.
fn b64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (at, byte)| {
            n | (u32::from(*byte) << (16 - 8 * at))
        });
        for at in 0..=chunk.len() {
            out.push(char::from(ALPHABET[((n >> (18 - 6 * at)) & 63) as usize]));
        }
    }
    out
}

fn refused(stream: TcpStream, error: &str, description: &str) {
    respond(
        stream,
        400,
        &[],
        &serde_json::json!({"error": error, "error_description": description}).to_string(),
    );
}

fn answer(stream: TcpStream, held: &Mutex<Held>) -> std::io::Result<()> {
    let request = read(&stream)?;
    let mut held = held
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    held.asked
        .push(format!("{} {}", request.method, request.path));
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/o/oauth2/v2/auth") => consent(stream, &request, &mut held),
        ("POST", "/token") => token(stream, &request, &mut held),
        (_, path) if path.starts_with("/calendar/v3/") => calendar(stream, &request, &mut held),
        ("GET", "/gmail/v1/users/me/profile") => profile(stream, &request, &held),
        _ => respond(stream, 404, &[], "{}"),
    }
    Ok(())
}

/// The consent page: a code for the challenge, and the browser sent back to the redirect.
fn consent(stream: TcpStream, request: &Request, held: &mut Held) {
    let asked = |key: &str| request.query.get(key).cloned().unwrap_or_default();
    if asked("response_type") != "code"
        || asked("code_challenge_method") != "S256"
        || asked("code_challenge").is_empty()
        || asked("client_id") != CLIENT_ID
    {
        refused(
            stream,
            "invalid_request",
            "a consent needs a code and an S256 challenge",
        );
        return;
    }
    if held.hinted && !asked("login_hint").is_empty() {
        held.account = asked("login_hint");
    }
    // The person grants what was asked and not unticked; with include_granted_scopes, Google
    // answers with that and everything the account granted the client before.
    let ticked: BTreeSet<String> = asked("scope")
        .split(' ')
        .filter(|scope| !held.unticked.contains(*scope))
        .map(str::to_owned)
        .collect();
    let scope: BTreeSet<String> = if asked("include_granted_scopes") == "true" {
        held.granted.extend(ticked);
        held.granted.clone()
    } else {
        ticked
    };
    let code = format!("4/fake-code-{}", held.codes.len() + 1);
    held.codes.insert(
        code.clone(),
        Issued {
            challenge: asked("code_challenge"),
            redirect: asked("redirect_uri"),
            scope: scope.into_iter().collect::<Vec<_>>().join(" "),
        },
    );
    let location = format!(
        "{}?state={}&code={}",
        asked("redirect_uri"),
        suzatary_core::url::percent_encode(&asked("state")),
        suzatary_core::url::percent_encode(&code)
    );
    respond(stream, 302, &[("Location", location)], "{}");
}

/// The token endpoint: a code redeemed once, with the verifier its challenge was made from and
/// the redirect its consent named; a refresh while the grant is not revoked.
fn token(stream: TcpStream, request: &Request, held: &mut Held) {
    let fields = form(&String::from_utf8_lossy(&request.body));
    let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
    if field("client_id") != CLIENT_ID || field("client_secret") != CLIENT_SECRET {
        refused(stream, "invalid_client", "The OAuth client was not found.");
        return;
    }
    match field("grant_type").as_str() {
        "authorization_code" => {
            held.exchanges += 1;
            let code = field("code");
            let Some(issued) = held.codes.get(&code).cloned() else {
                refused(stream, "invalid_grant", "Malformed auth code.");
                return;
            };
            if !held.used.insert(code) {
                refused(stream, "invalid_grant", "Bad Request");
            } else if field("redirect_uri") != issued.redirect {
                refused(stream, "redirect_uri_mismatch", "Bad Request");
            } else if suzatary_core::google::Challenge::s256(&field("code_verifier")).as_str()
                != issued.challenge
            {
                refused(stream, "invalid_grant", "Invalid code verifier.");
            } else {
                granted(stream, &issued, held);
            }
        }
        "refresh_token" if held.revoked || !held.refresh.contains(&field("refresh_token")) => {
            refused(
                stream,
                "invalid_grant",
                "Token has been expired or revoked.",
            );
        }
        "refresh_token" => respond(
            stream,
            200,
            &[],
            &serde_json::json!({"access_token": ACCESS, "expires_in": 3599}).to_string(),
        ),
        _ => refused(stream, "unsupported_grant_type", "Invalid grant_type"),
    }
}

/// A grant for the scopes `issued` asked, naming the account that signs in.
fn granted(stream: TcpStream, issued: &Issued, held: &mut Held) {
    let refresh = format!("1//fake-refresh-{}", held.refresh.len() + 1);
    held.refresh.insert(refresh.clone());
    let claims = serde_json::json!({"email": held.account, "email_verified": true});
    let id_token = format!(
        "{}.{}.fake-signature",
        b64url(br#"{"alg":"RS256"}"#),
        b64url(claims.to_string().as_bytes())
    );
    respond(
        stream,
        200,
        &[],
        &serde_json::json!({
            "access_token": ACCESS,
            "refresh_token": refresh,
            "expires_in": 3599,
            "scope": issued.scope,
            "token_type": "Bearer",
            "id_token": id_token,
        })
        .to_string(),
    );
}

/// The Calendar API: the account's calendars, each answering `held.events` events whatever it
/// was asked for, and an insert into its own calendar, recorded.
/// The Gmail API's profile of the account signed in, for its access token alone.
fn profile(stream: TcpStream, request: &Request, held: &Held) {
    if request.authorization.as_deref() != Some(&format!("Bearer {ACCESS}")) {
        respond(stream, 401, &[], r#"{"error": {"code": 401}}"#);
    } else if held.mailless {
        respond(
            stream,
            400,
            &[],
            r#"{"error": {"code": 400, "message": "Mail service not enabled"}}"#,
        );
    } else {
        let profile = serde_json::json!({"emailAddress": held.account, "messagesTotal": 3});
        respond(stream, 200, &[], &profile.to_string());
    }
}

fn calendar(stream: TcpStream, request: &Request, held: &mut Held) {
    if request.authorization.as_deref() != Some(&format!("Bearer {ACCESS}")) {
        respond(stream, 401, &[], r#"{"error": {"code": 401}}"#);
        return;
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/calendar/v3/users/me/calendarList") => respond(
            stream,
            200,
            &[],
            &serde_json::json!({"items": [
                {"id": held.account, "primary": true},
                {"id": "team@group.calendar.example.test"},
            ]})
            .to_string(),
        ),
        ("GET", events) if events.ends_with("/events") => {
            let items: Vec<serde_json::Value> = (0..held.events)
                .map(|n| {
                    serde_json::json!({
                        "id": format!("e{n}"),
                        "iCalUID": format!("e{n}@example.test"),
                        "summary": format!("Meeting {n}"),
                        "start": {"dateTime": "2026-10-19T09:00:00+02:00"},
                        "end": {"dateTime": "2026-10-19T09:30:00+02:00"},
                    })
                })
                .collect();
            respond(
                stream,
                200,
                &[],
                &serde_json::json!({ "items": items }).to_string(),
            );
        }
        ("POST", "/calendar/v3/calendars/primary/events") => {
            let event: serde_json::Value =
                serde_json::from_slice(&request.body).unwrap_or_default();
            held.inserted.push(event);
            respond(stream, 200, &[], r#"{"id": "inserted-1"}"#);
        }
        _ => respond(stream, 404, &[], "{}"),
    }
}
