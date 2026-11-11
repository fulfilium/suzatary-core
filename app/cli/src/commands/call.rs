//! `suzatary call`: any route of the server's table, asked of the running server by the operator.
//! The request meets the same gate, guards, door and handler as a device's, signed with the root
//! key, so every act is recorded as the operator's.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_core::event::{Actor, Hash};
use prodrome_core::sign::Secret;
use suzatary_core::gate::OPERATOR_HEADER;
use suzatary_core::signatures::{Signers, body_digest, request_name};
use suzatary_core::wire::lines as wire;
use suzatary_io::http::{Http as _, Method, Req, Ureq};

use super::serve::asked::{AT_HEADER, KEY_HEADER, NONCE_HEADER, SIGNATURE_HEADER};
use super::serve::{route, table};
use crate::instance::Instance;

/// Which request.
#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    asked: Asked,
}

/// The requests.
#[derive(Debug, Subcommand)]
pub enum Asked {
    /// Read a route, its path and query filled in: `suzatary call get /api/waiting`.
    Get {
        /// The path and query.
        target: String,
    },
    /// Write to a route with a JSON body.
    Post {
        /// The path and query.
        target: String,
        /// The body; `-` reads it from stdin, and none sends `{}`.
        #[arg(long)]
        body: Option<String>,
    },
    /// Every route of the table, one `METHOD path guard asks` line each.
    Routes,
}

/// `suzatary call <request>`: the answer's body on stdout.
///
/// # Errors
/// The instance file's, the keys', the connection's, and a route's refusal.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let operator = || Operator::of(&Instance::open(path)?);
    let answer = match &args.asked {
        Asked::Routes => {
            for line in table::surface() {
                println!("{line}");
            }
            return Ok(());
        }
        Asked::Get { target } => operator()?.request(route::Method::Get, target, b"")?,
        Asked::Post { target, body } => {
            let body = match body.as_deref() {
                None => b"{}".to_vec(),
                Some("-") => {
                    let mut read = Vec::new();
                    std::io::Read::read_to_end(&mut std::io::stdin(), &mut read)
                        .context("reading the body")?;
                    read
                }
                Some(text) => text.as_bytes().to_vec(),
            };
            operator()?.request(route::Method::Post, target, &body)?
        }
    };
    println!("{}", String::from_utf8_lossy(&answer.body));
    eprintln!("request: {}", answer.request.as_str());
    Ok(())
}

/// The box's operator as a client of its own server: where the app answers, the root key that
/// signs every request, and the actor the requests write as.
#[derive(Clone)]
pub struct Operator {
    base: String,
    key: Arc<Secret>,
    actor: Actor,
    http: Ureq,
    wall: fn() -> chrono::DateTime<chrono::Utc>,
}

/// How long a request that is not a stream may take.
const PATIENCE: Duration = Duration::from_mins(2);

/// The largest answer read.
const ANSWER_CAP: u64 = 64 << 20;

impl Operator {
    /// The operator of the app at `base`, signing with `key` and writing as `actor`.
    #[must_use]
    pub fn new(base: &str, key: Arc<Secret>, actor: Actor) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            key,
            actor,
            http: Ureq::new(),
            wall: chrono::Utc::now,
        }
    }

    /// The same operator stamping its requests with `wall`'s instant: a test server's window
    /// reads a fixed one.
    #[must_use]
    pub fn stamping(self, wall: fn() -> chrono::DateTime<chrono::Utc>) -> Self {
        Self { wall, ..self }
    }

    /// This box's operator: the instance's `app_url`, the root key from secretspec, and the
    /// first chain's owner.
    ///
    /// # Errors
    /// The keys not at hand.
    pub fn of(instance: &Instance) -> anyhow::Result<Self> {
        let keys = match instance.signers() {
            Signers::Held(keys) => keys,
            Signers::Missing(why) => bail!("the operator signs with the box's root key: {why}"),
        };
        Ok(Self::new(
            &instance.deployment.app_url,
            Arc::clone(keys.root()),
            instance.chains.first().owner.clone(),
        ))
    }

    #[must_use]
    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    /// `method` at `target`, the path and query of a route in the table, with `body`: the
    /// answer where it is 2xx, and the name the box keeps the request's record under.
    ///
    /// # Errors
    /// A target no route answers, the connection's failure, or the route's refusal, with its
    /// status and words.
    pub fn request(
        &self,
        method: route::Method,
        target: &str,
        body: &[u8],
    ) -> anyhow::Result<Answered> {
        routed(method, target)?;
        let (request, headers) = self.signed(method, target, body)?;
        let answer = self
            .http
            .send(Req {
                method: match method {
                    route::Method::Get => Method::Get,
                    route::Method::Post => Method::Post,
                },
                url: format!("{}{target}", self.base),
                headers,
                body: body.to_vec(),
                timeout: Some(PATIENCE),
                limit: ANSWER_CAP,
            })
            .with_context(|| format!("asking {target} of {}", self.base))?;
        if !answer.ok() {
            bail!("{} {target}: {}", answer.status, answer.text());
        }
        Ok(Answered {
            request,
            body: answer.body,
        })
    }

    /// Each line of a stream route's answer, handed to `each` until `each` breaks.
    ///
    /// # Errors
    /// A target no route answers, the connection's failure, or a status that is not 2xx.
    pub fn stream(
        &self,
        target: &str,
        each: &mut dyn FnMut(&str) -> ControlFlow<()>,
    ) -> anyhow::Result<()> {
        routed(route::Method::Get, target)?;
        let req = Req {
            method: Method::Get,
            url: format!("{}{target}", self.base),
            headers: self.signed(route::Method::Get, target, b"")?.1,
            body: Vec::new(),
            timeout: None,
            limit: ANSWER_CAP,
        };
        let status = self
            .http
            .lines(&req, each)
            .with_context(|| format!("following {target} of {}", self.base))?;
        if !(200..300).contains(&status) {
            bail!("{status} {target}");
        }
        Ok(())
    }

    /// The lines as a tab holds those of conversation `id`: the skeleton pulled piece by piece,
    /// each asked from the frontier the last one named, and the content of that conversation's
    /// nodes asked by hash, each literal held and each husk taken in its place.
    ///
    /// # Errors
    /// A route's refusal, or an answer that does not read.
    pub fn lines(
        &self,
        id: &suzatary_core::conversation_id::ConversationId,
    ) -> anyhow::Result<suzatary_core::secretary::held_replica::HeldReplica> {
        let read = |target: &str| -> anyhow::Result<Vec<u8>> {
            Ok(self.request(route::Method::Get, target, b"")?.body)
        };
        let mut replica = suzatary_core::secretary::held_replica::HeldReplica::pulled(|since| {
            let piece: wire::SkeletonPayload = serde_json::from_slice(&read(&format!(
                "/api/stores/lines/skeleton?since={since}"
            ))?)
            .context("a piece of the lines' skeleton")?;
            anyhow::Ok(piece)
        })?;
        let lacking = replica.lacking(&suzatary_core::secretary::outlines::Scope::Conversations(
            std::collections::BTreeSet::from([id.clone()]),
        ));
        for ask in
            suzatary_core::routes::asks(lacking.iter().map(|node| (node.hash.as_str(), node.size)))
        {
            let content: suzatary_core::wire::ContentPayload = serde_json::from_slice(&read(
                &format!("/api/stores/lines/content?hashes={}", ask.join(",")),
            )?)
            .context("a content answer of the lines")?;
            replica.take(content.objects)?;
        }
        Ok(replica)
    }

    /// The request's name and its headers: a device's signed request's, the two of the CSRF
    /// triple a client sends, and the actor the operator writes as.
    fn signed(
        &self,
        method: route::Method,
        target: &str,
        body: &[u8],
    ) -> anyhow::Result<(Hash, Vec<(String, String)>)> {
        let at = (self.wall)().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let nonce = suzatary_io::requests::fresh_nonce().map_err(|e| anyhow::anyhow!(e))?;
        let method = match method {
            route::Method::Get => "GET",
            route::Method::Post => "POST",
        };
        let Signed { name, headers } = Signed::of(method, target, body, &self.key, &at, &nonce);
        let headers = headers
            .into_iter()
            .map(|(header, value)| (header.to_owned(), value))
            .chain([
                (OPERATOR_HEADER.to_owned(), self.actor.as_str().to_owned()),
                (
                    suzatary_core::signatures::ACT_HEADER.to_owned(),
                    "1".to_owned(),
                ),
                ("content-type".to_owned(), "application/json".to_owned()),
            ])
            .collect();
        Ok((name, headers))
    }
}

/// A 2xx answer to the operator's request.
#[derive(Debug)]
pub struct Answered {
    /// The request's name, under which the box keeps its record when the route asks for a
    /// signature: the record names the root key, so it reads as the operator's.
    pub request: Hash,
    pub body: Vec<u8>,
}

/// A request as a device signs it: its name ([`request_name`]), under which the box keeps its
/// record, and the four headers that carry its instant, nonce, key and the key's signature of
/// the name, which the door checks.
#[derive(Debug)]
pub struct Signed {
    pub name: Hash,
    pub headers: [(&'static str, String); 4],
}

impl Signed {
    /// `method` at `target` with `body`, stamped `at` with `nonce`, signed by `key`.
    #[must_use]
    pub fn of(
        method: &str,
        target: &str,
        body: &[u8],
        key: &Secret,
        at: &str,
        nonce: &str,
    ) -> Self {
        let name = request_name(
            method,
            target,
            at,
            nonce,
            &body_digest(body),
            key.public().as_str(),
        );
        let signature = key.sign(&name);
        Self {
            headers: [
                (AT_HEADER, at.to_owned()),
                (NONCE_HEADER, nonce.to_owned()),
                (KEY_HEADER, signature.key.as_str().to_owned()),
                (SIGNATURE_HEADER, signature.signature.as_str().to_owned()),
            ],
            name,
        }
    }
}

/// Refuses a target that no route of the table answers with `method`, so the CLI reaches the
/// table and nothing else.
fn routed(method: route::Method, target: &str) -> anyhow::Result<()> {
    let path = target.split('?').next().unwrap_or_default();
    let answered = table::table().iter().any(|route| {
        route.method == method && route.paths().iter().any(|pattern| answers(pattern, path))
    });
    if !answered {
        bail!("no route of the table answers {method:?} {path}: `suzatary call routes` lists them");
    }
    Ok(())
}

/// Whether the route pattern `pattern` (`/api/todo/{id}`, `/api/act/{*verb}`) answers `path`.
fn answers(pattern: &str, path: &str) -> bool {
    let mut asked = path.split('/');
    for segment in pattern.split('/') {
        let Some(said) = asked.next() else {
            return false;
        };
        if segment.starts_with("{*") {
            return !said.is_empty();
        }
        let named = segment.starts_with('{') && !said.is_empty();
        if !named && said != segment {
            return false;
        }
    }
    asked.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_reaches_the_table_and_nothing_else() {
        for (method, target) in [
            (route::Method::Get, "/api/waiting"),
            (route::Method::Get, "/api/todo/lease"),
            (route::Method::Get, "/api/life/todo/lease"),
            (route::Method::Get, "/api/mail/search?q=from:a"),
            (route::Method::Post, "/api/secretary/conversations"),
            (
                route::Method::Post,
                "/api/secretary/conversations/c1/proposals/0/tap",
            ),
        ] {
            assert!(routed(method, target).is_ok(), "{target}");
        }
        for (method, target) in [
            (route::Method::Post, "/api/waiting"),
            (route::Method::Get, "/api/todo/"),
            (route::Method::Get, "/api/secretary/send"),
            (
                route::Method::Post,
                "/api/secretary/conversations/c1/proposals/0/tap/x",
            ),
            (route::Method::Get, "/index.html"),
        ] {
            assert!(routed(method, target).is_err(), "{target}");
        }
    }
}
