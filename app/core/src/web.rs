//! What the secretary may ask of the web.
//!
//! A request is a closed value: a search, or a fetch of one URL. A URL is
//! parsed into scheme, host and path, and refused outright when its host is
//! this box, its network, or the tailnet, so no request can reach the app's
//! own routes. An [`AllowRule`] is a host or a domain, optionally narrowed
//! to a path, matched label by label on the parsed host: a look-alike such as
//! `wikipedia.org.evil.example` never matches `*.wikipedia.org`.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The longest URL accepted.
pub const MAX_URL: usize = 2048;

/// The longest search query accepted.
pub const MAX_QUERY: usize = 300;

/// The host a search is a fetch of; allowing it lets searches run untapped.
pub const SEARCH_HOST: &str = "html.duckduckgo.com";

/// Why a URL, query or rule is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebError {
    /// Not an `http` or `https` URL this grammar reads.
    #[error("not an http(s) URL: {0}")]
    NotAUrl(String),
    /// A host this box must never be asked to reach.
    #[error("{0} is this box, its network or the tailnet, and is never fetched")]
    Private(String),
    /// Credentials in the URL, which would travel with the request.
    #[error("a URL may not carry credentials")]
    Credentials,
    /// Over the length bound.
    #[error("longer than {0} characters")]
    TooLong(usize),
    /// An empty search.
    #[error("the search is empty")]
    EmptyQuery,
    /// A rule this grammar does not read, or one too broad to allow.
    #[error("not an allow rule: {0}")]
    NotARule(String),
}

/// A URL's host: a DNS name (lowercase, no trailing dot) or an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    /// A name, at least two labels.
    Name(String),
    /// An address literal, already checked to be public.
    Ip(IpAddr),
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Host::Name(name) => f.write_str(name),
            Host::Ip(IpAddr::V6(ip)) => write!(f, "[{ip}]"),
            Host::Ip(ip) => write!(f, "{ip}"),
        }
    }
}

/// An `http(s)` URL with a public host, as sent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct WebUrl {
    text: String,
    host: Host,
    path: String,
}

impl WebUrl {
    /// The URL exactly as it will be requested.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Its host.
    #[must_use]
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// Its path, `/` when the URL names none; the query is not part of it.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl FromStr for WebUrl {
    type Err = WebError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.len() > MAX_URL {
            return Err(WebError::TooLong(MAX_URL));
        }
        let wrong = || WebError::NotAUrl(text.to_owned());
        if text
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || !c.is_ascii())
        {
            return Err(wrong());
        }
        let lower = text.to_ascii_lowercase();
        let rest = lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"))
            .ok_or_else(wrong)?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, after) = rest.split_at(end);
        if authority.contains('@') {
            return Err(WebError::Credentials);
        }
        let host = host_of(authority).ok_or_else(wrong)?;
        check_public(&host)?;
        let path_end = after.find(['?', '#']).unwrap_or(after.len());
        let path = match &after[..path_end] {
            "" => "/".to_owned(),
            path => path.to_owned(),
        };
        Ok(Self {
            text: text.to_owned(),
            host,
            path,
        })
    }
}

impl fmt::Display for WebUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Serialize for WebUrl {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl TryFrom<String> for WebUrl {
    type Error = WebError;
    fn try_from(text: String) -> Result<Self, WebError> {
        text.parse()
    }
}

/// The host part of an authority (port dropped), or `None` when it is not
/// a name or an address literal.
fn host_of(authority: &str) -> Option<Host> {
    if let Some(bracketed) = authority.strip_prefix('[') {
        let (inside, tail) = bracketed.split_once(']')?;
        if !(tail.is_empty() || port_ok(tail.strip_prefix(':')?)) {
            return None;
        }
        return inside
            .parse::<Ipv6Addr>()
            .ok()
            .map(|ip| Host::Ip(IpAddr::V6(ip)));
    }
    let host = match authority.split_once(':') {
        Some((host, port)) if port_ok(port) => host,
        Some(_) => return None,
        None => authority,
    };
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(Host::Ip(IpAddr::V4(ip)));
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    // A bare private name (`localhost`) is let through to be refused as
    // private, which says why, rather than as malformed.
    (name_ok(host) || PRIVATE_SUFFIXES.contains(&host)).then(|| Host::Name(host.to_owned()))
}

fn port_ok(port: &str) -> bool {
    !port.is_empty() && port.len() <= 5 && port.bytes().all(|b| b.is_ascii_digit())
}

/// Two or more labels of letters, digits and inner hyphens.
fn name_ok(name: &str) -> bool {
    let labels: Vec<&str> = name.split('.').collect();
    name.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Names that are this box, its network or the tailnet.
const PRIVATE_SUFFIXES: [&str; 5] = ["localhost", "local", "internal", "ts.net", "home.arpa"];

fn check_public(host: &Host) -> Result<(), WebError> {
    let private = match host {
        Host::Ip(ip) => !is_public(*ip),
        Host::Name(name) => PRIVATE_SUFFIXES
            .iter()
            .any(|suffix| name == suffix || name.ends_with(&format!(".{suffix}"))),
    };
    if private {
        Err(WebError::Private(host.to_string()))
    } else {
        Ok(())
    }
}

/// Whether an address is on the public internet: not loopback, private,
/// link-local, CGNAT (which is where the tailnet lives), multicast,
/// broadcast, unspecified or documentation. An IPv6 address that carries an
/// IPv4 one (mapped, compatible, NAT64's `64:ff9b::/96`, 6to4's `2002::/16`)
/// is as public as the address it carries, since a translator or relay
/// delivers it there; local NAT64 and Teredo are never public. The fetcher
/// asks this of every address a name resolves to, before connecting.
#[must_use]
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = carried(v6) {
                return is_public(IpAddr::V4(v4));
            }
            let [first, second, third, ..] = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || first == 0x2001 && second == 0x0db8
                || first == 0x2001 && second == 0
                || first == 0x0064 && second == 0xff9b && third == 1)
        }
    }
}

/// The IPv4 address an IPv6 one delivers to, where it carries one.
fn carried(v6: std::net::Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    let segments = v6.segments();
    let low = |high: u16, low: u16| {
        let [first, second] = high.to_be_bytes();
        let [third, fourth] = low.to_be_bytes();
        std::net::Ipv4Addr::new(first, second, third, fourth)
    };
    match segments {
        [0, 0, 0, 0, 0, 0xffff, ..] => v6.to_ipv4_mapped(),
        [0, 0, 0, 0, 0, 0, high, low_half] if !v6.is_loopback() && !v6.is_unspecified() => {
            Some(low(high, low_half))
        }
        [0x0064, 0xff9b, 0, 0, 0, 0, high, low_half] | [0x2002, high, low_half, ..] => {
            Some(low(high, low_half))
        }
        _ => None,
    }
}

/// A search, as typed: non-empty, bounded, one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SearchQuery(String);

impl SearchQuery {
    /// The words searched for.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SearchQuery {
    type Error = WebError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(WebError::EmptyQuery);
        }
        if trimmed.chars().count() > MAX_QUERY {
            return Err(WebError::TooLong(MAX_QUERY));
        }
        Ok(Self(
            trimmed
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect(),
        ))
    }
}

impl From<SearchQuery> for String {
    fn from(query: SearchQuery) -> Self {
        query.0
    }
}

/// Which of the two things a [`WebRequest`] asks, as a card names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum WebKind {
    /// A search.
    Search,
    /// A page read.
    Fetch,
}

/// One thing the secretary asks of the web.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum WebRequest {
    /// Search the provider for these words.
    Search {
        /// What to search for.
        query: SearchQuery,
    },
    /// Read one page.
    Fetch {
        /// The page.
        url: WebUrl,
    },
}

impl WebRequest {
    /// Which of the two it is.
    #[must_use]
    pub fn kind(&self) -> WebKind {
        match self {
            WebRequest::Search { .. } => WebKind::Search,
            WebRequest::Fetch { .. } => WebKind::Fetch,
        }
    }

    /// The host the request goes to: the page's, or the search provider's.
    #[must_use]
    pub fn host(&self) -> Host {
        match self {
            WebRequest::Search { .. } => Host::Name(SEARCH_HOST.to_owned()),
            WebRequest::Fetch { url } => url.host().clone(),
        }
    }

    /// The path the rule matching checks: the page's, or `/` for a search.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            WebRequest::Search { .. } => "/",
            WebRequest::Fetch { url } => url.path(),
        }
    }

    /// What goes out, in words, for the card and the transcript.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            WebRequest::Search { query } => {
                format!(
                    "search {SEARCH_HOST} for \u{201c}{}\u{201d}",
                    query.as_str()
                )
            }
            WebRequest::Fetch { url } => format!("open {url}"),
        }
    }
}

/// Which hosts a rule covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostScope {
    /// This host and nothing else.
    Exact(String),
    /// This domain and every name under it.
    Domain(String),
}

/// A host or domain the secretary may read without asking, optionally
/// narrowed to a path prefix. Its text form is `host`, `*.domain`, either
/// followed by `/path`: `docs.rs`, `*.wikipedia.org`, `github.com/bmabsout`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct AllowRule {
    scope: HostScope,
    path: Option<String>,
}

impl AllowRule {
    /// Whether this rule lets `request` run without a tap. Only names match;
    /// an address literal is always asked.
    #[must_use]
    pub fn allows(&self, request: &WebRequest) -> bool {
        let Host::Name(host) = request.host() else {
            return false;
        };
        let host_ok = match &self.scope {
            HostScope::Exact(exact) => &host == exact,
            HostScope::Domain(domain) => &host == domain || host.ends_with(&format!(".{domain}")),
        };
        host_ok
            && self.path.as_deref().is_none_or(|prefix| {
                let path = request.path();
                path == prefix
                    || path
                        .strip_prefix(prefix)
                        .is_some_and(|rest| prefix.ends_with('/') || rest.starts_with('/'))
            })
    }

    /// The rule in words, for the card.
    #[must_use]
    pub fn sentence(&self) -> String {
        let hosts = match &self.scope {
            HostScope::Exact(host) => format!("any page on {host}"),
            HostScope::Domain(domain) => format!("any page on {domain} or its subdomains"),
        };
        match &self.path {
            None => hosts,
            Some(path) => format!("{hosts} under {path}"),
        }
    }
}

/// Whether any rule lets `request` run without a tap.
#[must_use]
pub fn allowed(rules: &[AllowRule], request: &WebRequest) -> bool {
    rules.iter().any(|rule| rule.allows(request))
}

impl FromStr for AllowRule {
    type Err = WebError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let wrong = || WebError::NotARule(text.to_owned());
        let lower = text.trim().to_ascii_lowercase();
        let (hostpart, path) = match lower.split_once('/') {
            Some((host, path)) => (host.to_owned(), Some(format!("/{path}"))),
            None => (lower.clone(), None),
        };
        if path
            .as_deref()
            .is_some_and(|p| p.contains(['?', '#']) || p.chars().any(char::is_whitespace))
        {
            return Err(wrong());
        }
        let (scope, name) = match hostpart.strip_prefix("*.") {
            Some(domain) => (HostScope::Domain(domain.to_owned()), domain.to_owned()),
            None => (HostScope::Exact(hostpart.clone()), hostpart.clone()),
        };
        if !name_ok(&name) {
            return Err(wrong());
        }
        check_public(&Host::Name(name))?;
        Ok(Self { scope, path })
    }
}

impl fmt::Display for AllowRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.scope {
            HostScope::Exact(host) => f.write_str(host)?,
            HostScope::Domain(domain) => write!(f, "*.{domain}")?,
        }
        if let Some(path) = &self.path {
            f.write_str(path)?;
        }
        Ok(())
    }
}

impl Serialize for AllowRule {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl TryFrom<String> for AllowRule {
    type Error = WebError;
    fn try_from(text: String) -> Result<Self, WebError> {
        text.parse()
    }
}

// The literal of each is its text, as an inbox act stores it.
prodrome_core::text_literal!(
    SearchQuery => |query| String::from(query.clone()),
    WebUrl => ToString::to_string,
    AllowRule => ToString::to_string,
);

/// `Search(query)`.
pub const SEARCH: prodrome_core::literal::codec::Constructor = ("Search", &["query"]);
/// `Fetch(url)`.
pub const FETCH: prodrome_core::literal::codec::Constructor = ("Fetch", &["url"]);

impl prodrome_core::literal::codec::Literal for WebRequest {
    fn to_value(&self) -> prodrome_core::literal::Value {
        use prodrome_core::literal::codec::record;
        match self {
            WebRequest::Search { query } => record(SEARCH, vec![query.to_value()]),
            WebRequest::Fetch { url } => record(FETCH, vec![url.to_value()]),
        }
    }

    fn from_value(
        value: &prodrome_core::literal::Value,
    ) -> Result<Self, prodrome_core::literal::ProdromeError> {
        use prodrome_core::literal::codec::{Fields, arm, no_arm};
        Ok(match arm(value)? {
            "Search" => WebRequest::Search {
                query: Fields::of(value, SEARCH)?.get("query")?,
            },
            "Fetch" => WebRequest::Fetch {
                url: Fields::of(value, FETCH)?.get("url")?,
            },
            other => return Err(no_arm("a web request", other)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fetch(url: &str) -> WebRequest {
        WebRequest::Fetch {
            url: url.parse().expect("a public URL"),
        }
    }

    fn rule(text: &str) -> AllowRule {
        text.parse().expect("a rule")
    }

    #[test]
    fn a_url_keeps_its_text_and_reads_its_host_and_path() {
        let url: WebUrl = "https://En.Wikipedia.org/wiki/Tenancy_deposit?x=1#top"
            .parse()
            .expect("parses");
        assert_eq!(
            url.as_str(),
            "https://En.Wikipedia.org/wiki/Tenancy_deposit?x=1#top"
        );
        assert_eq!(url.host(), &Host::Name("en.wikipedia.org".to_owned()));
        assert_eq!(url.path(), "/wiki/tenancy_deposit");
        let bare: WebUrl = "http://example.com".parse().expect("parses");
        assert_eq!(bare.path(), "/");
        let port: WebUrl = "https://example.com:8443/a".parse().expect("parses");
        assert_eq!(port.host(), &Host::Name("example.com".to_owned()));
    }

    #[test]
    fn this_box_its_network_and_the_tailnet_are_never_urls() {
        for private in [
            "http://127.0.0.1:8787/api/chain",
            "http://localhost/",
            "https://box.example.ts.net/api/fold",
            "http://100.64.0.1/",
            "http://10.0.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/",
            "http://[fd7a:115c:a1e0::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://printer.local/",
            "http://0.0.0.0/",
        ] {
            assert!(
                matches!(private.parse::<WebUrl>(), Err(WebError::Private(_))),
                "{private} must be refused as private"
            );
        }
    }

    /// An IPv6 address that a translator or relay delivers to an IPv4 one is as public as that
    /// address: so the box, its network and the tailnet are unreachable through each spelling.
    #[test]
    fn an_ipv6_address_carrying_a_private_ipv4_one_is_private() {
        let ip = |text: &str| text.parse::<IpAddr>().expect("an address");
        for private in [
            "64:ff9b::7f00:1",
            "64:ff9b::a00:1",
            "64:ff9b::6440:1",
            "2002:7f00:1::",
            "2002:c0a8:101::1",
            "2002:a9fe:a9fe::",
            "::7f00:1",
            "::a00:1",
            "::ffff:a00:1",
            "64:ff9b:1::5db8:d822",
            "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        ] {
            assert!(!is_public(ip(private)), "{private} must not read as public");
        }
        for public in [
            "64:ff9b::5db8:d822",
            "2002:5db8:d822::1",
            "::5db8:d822",
            "2606:4700::1111",
        ] {
            assert!(is_public(ip(public)), "{public} is public");
        }
    }

    #[test]
    fn anything_outside_the_grammar_is_refused() {
        assert_eq!(
            "https://user:pw@example.com/".parse::<WebUrl>(),
            Err(WebError::Credentials)
        );
        for wrong in [
            "ftp://example.com/",
            "javascript:alert(1)",
            "https://example.com/a b",
            "https://intranet/",
            "https://exa_mple.com/",
            "https://example.com:99999999/",
            "https://ex\u{e4}mple.com/",
        ] {
            assert!(wrong.parse::<WebUrl>().is_err(), "{wrong} must be refused");
        }
        assert!(
            ("https://example.com/".to_owned() + &"a".repeat(MAX_URL))
                .parse::<WebUrl>()
                .is_err()
        );
    }

    #[test]
    fn a_domain_rule_covers_its_subdomains_and_never_a_look_alike() {
        let wiki = rule("*.wikipedia.org");
        assert!(wiki.allows(&fetch("https://en.wikipedia.org/wiki/X")));
        assert!(wiki.allows(&fetch("https://wikipedia.org/")));
        assert!(!wiki.allows(&fetch("https://wikipedia.org.evil.example/?q=secret")));
        assert!(!wiki.allows(&fetch("https://notwikipedia.org/")));
        let exact = rule("docs.rs");
        assert!(exact.allows(&fetch("https://docs.rs/serde")));
        assert!(!exact.allows(&fetch("https://evil.docs.rs/")));
    }

    #[test]
    fn a_path_rule_stops_at_a_segment_boundary() {
        let mine = rule("github.com/bmabsout");
        assert!(mine.allows(&fetch("https://github.com/bmabsout")));
        assert!(mine.allows(&fetch("https://github.com/bmabsout/prodrome")));
        assert!(!mine.allows(&fetch("https://github.com/bmabsoutevil/x")));
        assert!(!mine.allows(&fetch("https://github.com/")));
    }

    #[test]
    fn a_search_is_allowed_by_its_provider_s_host() {
        let search = WebRequest::Search {
            query: SearchQuery::try_from("tenancy deposit rules".to_owned()).expect("a query"),
        };
        assert!(!allowed(&[rule("*.wikipedia.org")], &search));
        assert!(allowed(&[rule(SEARCH_HOST)], &search));
        assert!(search.sentence().contains("tenancy deposit rules"));
    }

    #[test]
    fn a_rule_is_never_broader_than_a_registrable_name_or_private() {
        for wrong in [
            "*.com",
            "com",
            "*.ts.net",
            "localhost",
            "*",
            "",
            "a b.com",
            "example.com/x?y",
        ] {
            assert!(
                wrong.parse::<AllowRule>().is_err(),
                "{wrong} must not be a rule"
            );
        }
        for (text, shown) in [
            ("*.Wikipedia.org", "*.wikipedia.org"),
            ("github.com/bmabsout", "github.com/bmabsout"),
        ] {
            assert_eq!(rule(text).to_string(), shown);
            let json = serde_json::to_string(&rule(text)).expect("serialises");
            assert_eq!(
                serde_json::from_str::<AllowRule>(&json).expect("round trip"),
                rule(text)
            );
        }
        assert_eq!(
            rule("*.wikipedia.org").sentence(),
            "any page on wikipedia.org or its subdomains"
        );
    }

    #[test]
    fn a_request_round_trips_and_refuses_a_private_url_in_json() {
        let request = fetch("https://example.com/a");
        let json = serde_json::to_string(&request).expect("serialises");
        assert_eq!(json, r#"{"kind":"fetch","url":"https://example.com/a"}"#);
        assert_eq!(
            serde_json::from_str::<WebRequest>(&json).expect("round trip"),
            request
        );
        assert!(
            serde_json::from_str::<WebRequest>(r#"{"kind":"fetch","url":"http://127.0.0.1/"}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<WebRequest>(r#"{"kind":"search","query":"  "}"#).is_err());
    }
}
