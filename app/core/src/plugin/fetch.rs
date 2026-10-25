//! The fence a plugin's request crosses: its target as the plugin wrote it, and the request the
//! host may send for it.

use crate::capability::Secret;

use super::{CredentialName, Manifest, PluginUrl, Refusal, Said};

/// The most bytes of an answer's body the host reads.
pub const MAX_RESPONSE: u64 = 8 * 1024 * 1024;

/// The most fetches one call may make; past it each is refused unsent.
pub const MAX_FETCHES: usize = 64;

/// Where a plugin asked a request to go: untrusted, unparsed, and only ever turned into an
/// [`Outgoing`] by [`Target::admit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A URL the plugin wrote.
    Url(String),
    /// A credential whose value is the URL, by name.
    Credential(String),
}

/// A request [`Target::admit`] let through. It may hold a credential's value, so its `Debug`
/// prints the origin only, and its answer reaches the plugin only through [`Outgoing::answer`].
#[derive(Debug)]
pub struct Outgoing {
    url: PluginUrl,
    credential: Option<(CredentialName, Secret)>,
}

impl Target {
    /// Check this target against `manifest`, reading a credential's value through `resolve`.
    ///
    /// A name the manifest does not declare is refused before `resolve` is asked anything, so it
    /// never reaches the secret store; a value goes only into the [`Outgoing`]. A store that
    /// could not answer is [`Refusal::CredentialUnavailable`], never "unset".
    pub fn admit<E>(
        self,
        manifest: &Manifest,
        resolve: impl FnOnce(&CredentialName) -> Result<Option<Secret>, E>,
    ) -> Result<Outgoing, Refusal> {
        let (url, credential) = match self {
            Target::Url(text) => (
                PluginUrl::parse(&text).ok_or_else(|| Refusal::BadUrl(Said::clip(text)))?,
                None,
            ),
            Target::Credential(name) => {
                let declared = manifest
                    .declared(&name)
                    .ok_or_else(|| Refusal::CredentialNotDeclared(Said::clip(name)))?
                    .clone();
                let value = resolve(&declared)
                    .map_err(|_| Refusal::CredentialUnavailable(declared.clone()))?
                    .filter(|value| !value.expose().is_empty())
                    .ok_or_else(|| Refusal::CredentialUnset(declared.clone()))?;
                let url = PluginUrl::parse(value.expose())
                    .ok_or_else(|| Refusal::CredentialNotAUrl(declared.clone()))?;
                if !url.origin().is_confidential() {
                    return Err(Refusal::InTheClear(declared));
                }
                (url, Some((declared, value)))
            }
        };
        if !manifest.allows(url.origin()) {
            return Err(Refusal::OriginNotAllowed(url.origin().clone()));
        }
        Ok(Outgoing { url, credential })
    }
}

impl Outgoing {
    /// The URL to send, as reprinted from what was checked.
    #[must_use]
    pub fn url(&self) -> &PluginUrl {
        &self.url
    }

    /// The answer's body as the plugin may see it: withheld whole if it carries the credential's
    /// value, or the path or the query of the URL that value is, since each is the secret.
    /// Refused, never edited.
    pub fn answer(&self, body: Vec<u8>) -> Result<Vec<u8>, Refusal> {
        let Some((name, value)) = &self.credential else {
            return Ok(body);
        };
        let rest = self.url.rest();
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let path = Some(path).filter(|path| path.len() > 1);
        let query = Some(query).filter(|query| !query.is_empty());
        let secrets = [Some(value.expose()), path, query];
        let carried = |part: &str| body.windows(part.len()).any(|at| at == part.as_bytes());
        if secrets.into_iter().flatten().any(carried) {
            Err(Refusal::Echoed(name.clone()))
        } else {
            Ok(body)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use proptest::prelude::*;

    use super::*;
    use crate::plugin::Origin;

    fn manifest() -> Manifest {
        toml::from_str(
            r#"
            name = "fence"
            component = "fence.wasm"
            schema = "fence.schema"
            origins = ["http://127.0.0.1:9000", "https://calendar.example", "http://calendar.example"]
            credentials = ["PLUGIN_FENCE_ICS", "PLUGIN_FENCE_UNSET", "PLUGIN_FENCE_ELSEWHERE", "PLUGIN_FENCE_PLAIN"]
            "#,
        )
        .expect("the fixture manifest")
    }

    fn value(name: &str) -> Option<&'static str> {
        match name {
            "PLUGIN_FENCE_ICS" => Some("https://calendar.example/private-s3cret/basic.ics"),
            "PLUGIN_FENCE_ELSEWHERE" => Some("https://evil.example/private-s3cret"),
            "PLUGIN_FENCE_PLAIN" => Some("http://calendar.example/private-s3cret"),
            "MODEL_TOKEN" => Some("the-app-s-own-key"),
            _ => None,
        }
    }

    /// The secret store, recording every name it was asked for.
    fn admit(target: Target, asked: &RefCell<Vec<String>>) -> Result<Outgoing, Refusal> {
        target.admit(&manifest(), |name| {
            asked.borrow_mut().push(name.to_string());
            Ok::<_, ()>(value(name.as_str()).map(Secret::new))
        })
    }

    fn named(name: &str) -> CredentialName {
        manifest().declared(name).expect("declared").clone()
    }

    /// Each target reaches only an allowed origin, a secret URL only over TLS or loopback, and a
    /// name the manifest does not declare never reaches the secret store.
    #[test]
    fn each_target_is_admitted_or_refused_by_name() {
        let url = |text: &str| Target::Url(text.to_owned());
        let credential = |name: &str| Target::Credential(name.to_owned());
        for (target, admitted, asked) in [
            (
                url("http://127.0.0.1:9000?since=1"),
                Ok("http://127.0.0.1:9000/?since=1"),
                vec![],
            ),
            (
                url("https://evil.example/"),
                Err(Refusal::OriginNotAllowed(
                    Origin::parse("https://evil.example").expect("an origin"),
                )),
                vec![],
            ),
            (
                url("https://calendar.example@evil.example/"),
                Err(Refusal::BadUrl(Said::clip(
                    "https://calendar.example@evil.example/",
                ))),
                vec![],
            ),
            (
                credential("PLUGIN_FENCE_ICS"),
                Ok("https://calendar.example/private-s3cret/basic.ics"),
                vec!["PLUGIN_FENCE_ICS"],
            ),
            (
                credential("MODEL_TOKEN"),
                Err(Refusal::CredentialNotDeclared(Said::clip("MODEL_TOKEN"))),
                vec![],
            ),
            (
                credential("PLUGIN_FENCE_UNSET"),
                Err(Refusal::CredentialUnset(named("PLUGIN_FENCE_UNSET"))),
                vec!["PLUGIN_FENCE_UNSET"],
            ),
            (
                credential("PLUGIN_FENCE_ELSEWHERE"),
                Err(Refusal::OriginNotAllowed(
                    Origin::parse("https://evil.example").expect("an origin"),
                )),
                vec!["PLUGIN_FENCE_ELSEWHERE"],
            ),
            (
                credential("PLUGIN_FENCE_PLAIN"),
                Err(Refusal::InTheClear(named("PLUGIN_FENCE_PLAIN"))),
                vec!["PLUGIN_FENCE_PLAIN"],
            ),
        ] {
            let asked_for = RefCell::new(Vec::new());
            let shown = format!("{target:?}");
            let sent = admit(target, &asked_for);
            assert_eq!(
                sent.as_ref()
                    .map(|sent| sent.url().to_string())
                    .map_err(Clone::clone),
                admitted.map(str::to_owned),
                "{shown}"
            );
            assert_eq!(asked_for.into_inner(), asked, "{shown}");
            if let Ok(sent) = sent {
                assert!(!format!("{sent:?}").contains("s3cret"), "{sent:?}");
            }
        }
    }

    /// A store that cannot answer is never read as an unset credential, and an answer carrying
    /// the credential's value is withheld whole.
    #[test]
    fn a_locked_store_is_unavailable_and_an_echo_is_withheld() {
        assert_eq!(
            Target::Credential("PLUGIN_FENCE_ICS".to_owned())
                .admit(&manifest(), |_| Err::<Option<Secret>, _>("locked"))
                .map(|_| ()),
            Err(Refusal::CredentialUnavailable(named("PLUGIN_FENCE_ICS")))
        );
        let sent = admit(
            Target::Credential("PLUGIN_FENCE_ICS".to_owned()),
            &RefCell::default(),
        )
        .expect("admitted");
        for echoed in [
            "see https://calendar.example/private-s3cret/basic.ics",
            "you asked for /private-s3cret/basic.ics",
        ] {
            assert_eq!(
                sent.answer(echoed.as_bytes().to_vec()),
                Err(Refusal::Echoed(named("PLUGIN_FENCE_ICS"))),
                "{echoed}"
            );
        }
        assert_eq!(
            sent.answer(b"BEGIN:VCALENDAR".to_vec()),
            Ok(b"BEGIN:VCALENDAR".to_vec())
        );
    }

    proptest! {
        /// The fence's law: a name the manifest does not declare is refused by name, whatever
        /// it is, and the secret store is never asked.
        #[test]
        fn an_undeclared_name_never_reaches_the_secret_store(name in "(PLUGIN_FENCE_|APP_|)[A-Z_]{0,12}") {
            prop_assume!(manifest().declared(&name).is_none());
            let asked = RefCell::new(Vec::new());
            prop_assert_eq!(
                admit(Target::Credential(name.clone()), &asked).map(|_| ()),
                Err(Refusal::CredentialNotDeclared(Said::clip(name)))
            );
            prop_assert!(asked.into_inner().is_empty());
        }
    }
}
