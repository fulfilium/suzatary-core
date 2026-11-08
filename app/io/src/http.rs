//! Every HTTP request this crate makes, through one [`Http`] with a `ureq` interpreter and a
//! scripted fake. A non-2xx status is an answer the caller reads, never an error.

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use suzatary_core::web::is_public;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

/// Bytes of body read unless a request says otherwise.
const DEFAULT_LIMIT: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// One request. Its `Debug` prints its origin alone: a path, a query or a header can be a
/// credential, as an ntfy topic or a bearer token is.
#[derive(Clone)]
pub struct Req {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Empty for a GET.
    pub body: Vec<u8>,
    /// `None` waits.
    pub timeout: Option<Duration>,
    /// Bytes of the answer's body read.
    pub limit: u64,
}

/// One answer, whatever its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resp {
    pub status: u16,
    /// Names lowercased.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Why no answer came.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    /// It never left this box: the name, the connection or its TLS failed.
    #[error("{0}")]
    NotSent(String),
    /// It never left this box: [`Ureq::public_only`] refused a name resolving off the public
    /// internet.
    #[error("{host} resolves to {ip}, which is not on the public internet")]
    Private { host: String, ip: IpAddr },
    /// It left, and no answer came in time.
    #[error("{0}")]
    TimedOut(String),
    /// It left, and the answer was lost or could not be read.
    #[error("{0}")]
    Lost(String),
}

/// The one way a request leaves this box.
pub trait Http: Send + Sync {
    fn send(&self, req: Req) -> Result<Resp, HttpError>;
}

impl<T: Http + ?Sized> Http for Arc<T> {
    fn send(&self, req: Req) -> Result<Resp, HttpError> {
        (**self).send(req)
    }
}

impl Req {
    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
            timeout: None,
            limit: DEFAULT_LIMIT,
        }
    }

    #[must_use]
    pub fn post(url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        Self {
            method: Method::Post,
            body: body.into(),
            ..Self::get(url)
        }
    }

    /// A POST of `body` as JSON; panics on a body that does not serialise (none this crate
    /// builds).
    #[must_use]
    pub fn json(url: impl Into<String>, body: &impl serde::Serialize) -> Self {
        Self::post(
            url,
            serde_json::to_vec(body).expect("a map with string keys serialises"),
        )
        .header("Content-Type", "application/json")
    }

    #[must_use]
    pub fn form(url: impl Into<String>, fields: &[(&str, &str)]) -> Self {
        Self::post(url, suzatary_core::url::encode_form(fields))
            .header("Content-Type", "application/x-www-form-urlencoded")
    }

    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn timeout(mut self, after: Duration) -> Self {
        self.timeout = Some(after);
        self
    }

    /// Read at most `bytes` of the answer's body.
    #[must_use]
    pub fn limit(mut self, bytes: u64) -> Self {
        self.limit = bytes;
        self
    }
}

/// The value of the first of `headers` named `name`, ignoring case.
pub(crate) fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

impl std::fmt::Debug for Req {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let origin = suzatary_core::plugin::PluginUrl::parse(&self.url).map_or_else(
            || "an unreadable URL".to_owned(),
            |url| url.origin().to_string(),
        );
        f.debug_struct("Req")
            .field("method", &self.method)
            .field("origin", &origin)
            .finish_non_exhaustive()
    }
}

impl Resp {
    #[must_use]
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    #[must_use]
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_ascii_lowercase(), value.into()));
        self
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        header(&self.headers, name)
    }

    /// The body, decoded lossily.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    #[must_use]
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// `ureq` over rustls.
#[derive(Debug, Clone)]
pub struct Ureq {
    agent: ureq::Agent,
}

impl Default for Ureq {
    fn default() -> Self {
        Self::new()
    }
}

impl Ureq {
    /// An agent that follows redirects.
    #[must_use]
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into(),
        }
    }

    /// An agent for addresses a model chose: no redirects, and every name must resolve only to
    /// public addresses, checked inside the resolution so nothing changes between check and
    /// connect. No proxy from the environment: the resolver would check the proxy's address.
    #[must_use]
    pub fn public_only() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .proxy(None)
            .build();
        Self {
            agent: ureq::Agent::with_parts(config, DefaultConnector::new(), PublicOnly::default()),
        }
    }
}

impl Http for Ureq {
    fn send(&self, req: Req) -> Result<Resp, HttpError> {
        fn configured<B>(builder: ureq::RequestBuilder<B>, req: &Req) -> ureq::RequestBuilder<B> {
            req.headers.iter().fold(
                builder.config().timeout_global(req.timeout).build(),
                |builder, (name, value)| builder.header(name, value),
            )
        }
        let answered = match req.method {
            Method::Get => configured(self.agent.get(&req.url), &req).call(),
            Method::Post => configured(self.agent.post(&req.url), &req).send(&req.body[..]),
        };
        let mut response = answered.map_err(|failed| failure(&failed))?;
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
            })
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(req.limit)
            .read_to_vec()
            .map_err(|failed| failure(&failed))?;
        Ok(Resp {
            status: response.status().as_u16(),
            headers,
            body,
        })
    }
}

impl Ureq {
    /// Send a GET and hand each line of its body to `each` as it arrives, until `each` breaks
    /// or the body ends: a server-sent stream, read with no timeout. Answers the status; a
    /// non-2xx body is handed over all the same.
    pub fn lines(
        &self,
        req: &Req,
        each: &mut dyn FnMut(&str) -> std::ops::ControlFlow<()>,
    ) -> Result<u16, HttpError> {
        use std::io::BufRead as _;
        let builder = req
            .headers
            .iter()
            .fold(self.agent.get(&req.url), |builder, (name, value)| {
                builder.header(name, value)
            });
        let mut response = builder.call().map_err(|failed| failure(&failed))?;
        let status = response.status().as_u16();
        let reader = std::io::BufReader::new(response.body_mut().as_reader());
        for line in reader.lines() {
            let line = line.map_err(|failed| HttpError::Lost(failed.to_string()))?;
            if each(&line).is_break() {
                break;
            }
        }
        Ok(status)
    }
}

/// The resolver's [`HttpError::Private`] comes back as itself, read from `ureq`'s error by its
/// type, never by its words.
fn failure(error: &ureq::Error) -> HttpError {
    if let ureq::Error::Io(io) = error
        && let Some(private @ HttpError::Private { .. }) = io
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<HttpError>())
    {
        return private.clone();
    }
    let said = error.to_string();
    match error {
        // These quote the whole URL.
        ureq::Error::BadUri(_) | ureq::Error::RequireHttpsOnly(_) => {
            HttpError::NotSent("the URL is malformed, or not https".to_owned())
        }
        ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Tls(_)
        | ureq::Error::Rustls(_)
        | ureq::Error::TlsRequired
        | ureq::Error::InvalidProxyUrl
        | ureq::Error::ConnectProxyFailed(_)
        | ureq::Error::Timeout(ureq::Timeout::Resolve | ureq::Timeout::Connect) => {
            HttpError::NotSent(said)
        }
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied => {
            HttpError::NotSent(said)
        }
        ureq::Error::Timeout(_) => HttpError::TimedOut(said),
        _ => HttpError::Lost(said),
    }
}

#[derive(Debug, Default)]
struct PublicOnly(DefaultResolver);

impl Resolver for PublicOnly {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = self.0.resolve(uri, config, timeout)?;
        if let Some(private) = addresses.iter().find(|address| !is_public(address.ip())) {
            return Err(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                HttpError::Private {
                    host: uri.host().unwrap_or("the host").to_owned(),
                    ip: private.ip(),
                },
            )));
        }
        Ok(addresses)
    }
}

type Answer = dyn Fn(&Req) -> Result<Resp, HttpError> + Send + Sync;

/// A fake [`Http`] answering from a function of the request, keeping every request sent.
pub struct Scripted {
    answer: Box<Answer>,
    sent: Mutex<Vec<Req>>,
}

impl std::fmt::Debug for Scripted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scripted").finish_non_exhaustive()
    }
}

impl Scripted {
    #[must_use]
    pub fn new(answer: impl Fn(&Req) -> Result<Resp, HttpError> + Send + Sync + 'static) -> Self {
        Self {
            answer: Box::new(answer),
            sent: Mutex::new(Vec::new()),
        }
    }

    /// Answer the requests in order; panics when asked more often than it has answers.
    #[must_use]
    pub fn answering(answers: Vec<Result<Resp, HttpError>>) -> Self {
        let queue = Mutex::new(std::collections::VecDeque::from(answers));
        Self::new(move |req| {
            queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_front()
                .unwrap_or_else(|| panic!("no scripted answer left for {req:?}"))
        })
    }

    #[must_use]
    pub fn always(answer: Result<Resp, HttpError>) -> Self {
        Self::new(move |_| answer.clone())
    }

    #[must_use]
    pub fn sent(&self) -> Vec<Req> {
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Http for Scripted {
    fn send(&self, req: Req) -> Result<Resp, HttpError> {
        let answer = (self.answer)(&req);
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(req);
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The attack: a log line that prints a request. Its path, its query and its headers can each
    /// be the credential (an ntfy topic is the whole access control), so none is printed.
    #[test]
    fn a_request_prints_its_origin_alone() {
        let req = Req::get("https://ntfy.example.test:8443/topic-hunter1?auth=hunter2")
            .header("Authorization", "Bearer hunter3");
        let shown = format!("{req:?}");
        assert!(shown.contains("https://ntfy.example.test:8443"), "{shown}");
        for secret in ["hunter1", "hunter2", "hunter3"] {
            assert!(!shown.contains(secret), "{shown}");
        }
    }

    /// The same attack through the transport's own error: a URL it cannot send is not quoted.
    #[test]
    fn a_url_the_transport_refuses_is_not_quoted_in_its_error() {
        let failed = Ureq::new()
            .send(Req::get("/topic-hunter1").timeout(Duration::from_secs(5)))
            .expect_err("not sent");
        assert!(!failed.to_string().contains("hunter1"), "{failed}");
    }

    #[test]
    fn a_private_address_is_refused_before_anything_is_sent() {
        let failed = Ureq::public_only()
            .send(Req::get("http://127.0.0.1:9/").timeout(Duration::from_secs(5)))
            .expect_err("refused");
        assert_eq!(
            failed,
            HttpError::Private {
                host: "127.0.0.1".to_owned(),
                ip: IpAddr::from([127, 0, 0, 1]),
            }
        );
    }

    #[test]
    fn a_public_only_agent_takes_no_proxy_from_the_environment() {
        assert!(Ureq::public_only().agent.config().proxy().is_none());
    }
}
