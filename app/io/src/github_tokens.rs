//! Each GitHub token's expiry, asked of GitHub at most once a day and cached per secret.
//! The token's value goes only into the `Authorization` header, which `Req`'s `Debug` never
//! prints, and into no log or file.

use std::path::PathBuf;
use std::time::Duration;

use chrono::NaiveDateTime;
use suzatary_core::settings::Settable;

use crate::http::{Http, Req, Ureq};
use crate::secrets::Resolves;

/// How long a kept reading stands.
const DAY: chrono::Duration = chrono::Duration::days(1);

const KEPT: &str = "%Y-%m-%dT%H:%M:%S";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expiry {
    /// UTC.
    At(NaiveDateTime),
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reading {
    asked: NaiveDateTime,
    expiry: Expiry,
}

impl Reading {
    fn print(self) -> String {
        let expiry = match self.expiry {
            Expiry::At(at) => at.format(KEPT).to_string(),
            Expiry::Never => "never".to_owned(),
        };
        format!("{}\n{expiry}\n", self.asked.format(KEPT))
    }

    fn parse(kept: &str) -> Option<Self> {
        let mut lines = kept.lines();
        let asked = NaiveDateTime::parse_from_str(lines.next()?, KEPT).ok()?;
        let expiry = match lines.next()? {
            "never" => Expiry::Never,
            at => Expiry::At(NaiveDateTime::parse_from_str(at, KEPT).ok()?),
        };
        Some(Self { asked, expiry })
    }
}

enum Asked {
    Read(Reading),
    Unset,
    /// A failing secret, with no expiry to say.
    Refused,
}

/// The box's GitHub tokens.
pub struct GithubTokens {
    dir: PathBuf,
    secrets: Box<dyn Resolves + Send + Sync>,
    http: Box<dyn Http>,
}

impl GithubTokens {
    /// Readings kept under `dir`.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>, secrets: Box<dyn Resolves + Send + Sync>) -> Self {
        Self {
            dir: dir.into(),
            secrets,
            http: Box::new(Ureq::new()),
        }
    }

    /// Asks GitHub through `http`.
    #[must_use]
    pub fn with_http(self, http: Box<dyn Http>) -> Self {
        Self { http, ..self }
    }

    /// Each of `names` that expires, with its expiry. An unset or refused token is left out, one
    /// GitHub could not answer keeps its last reading, and the log names each by name alone.
    #[must_use]
    pub fn expiries(
        &self,
        names: &[Settable],
        now: NaiveDateTime,
    ) -> Vec<(Settable, NaiveDateTime)> {
        names
            .iter()
            .filter_map(|name| match self.reading(name, now)?.expiry {
                Expiry::At(at) => Some((name.clone(), at)),
                Expiry::Never => None,
            })
            .collect()
    }

    fn reading(&self, name: &Settable, now: NaiveDateTime) -> Option<Reading> {
        let path = self.dir.join(name.handle());
        let kept = std::fs::read_to_string(&path)
            .ok()
            .and_then(|kept| Reading::parse(&kept));
        if let Some(kept) = kept.filter(|kept| now - kept.asked < DAY) {
            return Some(kept);
        }
        match self.ask(name, now) {
            Ok(Asked::Read(read)) => {
                let written = std::fs::create_dir_all(&self.dir)
                    .and_then(|()| std::fs::write(path.with_extension("new"), read.print()))
                    .and_then(|()| std::fs::rename(path.with_extension("new"), &path));
                if let Err(failed) = written {
                    eprintln!(
                        "github tokens: {}'s reading was not kept: {failed}",
                        name.name()
                    );
                }
                Some(read)
            }
            Ok(Asked::Unset) => None,
            Ok(Asked::Refused) => {
                eprintln!("github tokens: GitHub refused {}", name.name());
                let _ = std::fs::remove_file(&path);
                None
            }
            Err(why) => {
                eprintln!("github tokens: {} was not read: {why}", name.name());
                kept
            }
        }
    }

    fn ask(&self, name: &Settable, now: NaiveDateTime) -> Result<Asked, String> {
        let Some(token) = self
            .secrets
            .get(name.name(), "reading when this GitHub token expires")
            .map_err(|failed| failed.to_string())?
        else {
            return Ok(Asked::Unset);
        };
        let resp = self
            .http
            .send(
                Req::get("https://api.github.com/")
                    .header("Authorization", format!("Bearer {}", token.expose()))
                    .header("Accept", "application/vnd.github+json")
                    .header("User-Agent", "suzatary")
                    .timeout(Duration::from_secs(20)),
            )
            .map_err(|failed| format!("GitHub: {failed}"))?;
        if resp.status == 401 {
            return Ok(Asked::Refused);
        }
        if resp.status != 200 {
            return Err(format!("GitHub answered {}", resp.status));
        }
        let expiry = resp
            .header("github-authentication-token-expiration")
            .and_then(expiration)
            .map_or(Expiry::Never, Expiry::At);
        Ok(Asked::Read(Reading { asked: now, expiry }))
    }
}

/// GitHub's `2026-12-01 00:00:00 UTC`.
fn expiration(value: &str) -> Option<NaiveDateTime> {
    let value = value.trim();
    value
        .strip_suffix(" UTC")
        .and_then(|at| NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S").ok())
        .or_else(|| {
            chrono::DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S %z")
                .ok()
                .map(|at| at.naive_utc())
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use suzatary_core::capability::Secret;
    use suzatary_core::settings::{Declaration, Settables};

    use super::*;
    use crate::http::{Resp, Scripted};

    /// Each token by name, and the value it holds.
    struct Tokens;

    impl Resolves for Tokens {
        fn get(
            &self,
            name: &str,
            _reason: &str,
        ) -> Result<Option<Secret>, crate::secrets::SecretError> {
            Ok(match name {
                suzatary_core::repos::TOKEN => Some(Secret::new("repos-value".to_owned())),
                suzatary_core::settings::DATA_PUSH_TOKEN => {
                    Some(Secret::new("push-value".to_owned()))
                }
                _ => None,
            })
        }
    }

    fn at(day: u32, hour: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, day)
            .and_then(|day| day.and_hms_opt(hour, 0, 0))
            .expect("an instant")
    }

    /// Every GitHub token is read with its own value, at most once a day, and keeps its own
    /// expiry; a token GitHub refuses has none.
    #[test]
    fn each_token_is_read_with_its_own_value_once_a_day() {
        let temp = tempfile::tempdir().expect("a temp dir");
        let refused = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let refusing = Arc::clone(&refused);
        let github = Arc::new(Scripted::new(move |req: &Req| {
            let answer = |status, headers: Vec<(String, String)>| {
                Ok(Resp {
                    status,
                    headers,
                    body: Vec::new(),
                })
            };
            match crate::http::header(&req.headers, "Authorization") {
                Some("Bearer repos-value") => answer(
                    200,
                    vec![(
                        "github-authentication-token-expiration".to_owned(),
                        "2027-01-09 08:00:00 UTC".to_owned(),
                    )],
                ),
                Some("Bearer push-value")
                    if !refusing.load(std::sync::atomic::Ordering::SeqCst) =>
                {
                    answer(
                        200,
                        vec![(
                            "github-authentication-token-expiration".to_owned(),
                            "2027-01-10 08:00:00 +0000".to_owned(),
                        )],
                    )
                }
                _ => answer(401, Vec::new()),
            }
        }));
        let tokens = GithubTokens::new(temp.path(), Box::new(Tokens))
            .with_http(Box::new(Arc::clone(&github)));
        let declared = Settables::declared(suzatary_core::repos::TOKEN, Declaration::Repos).join(
            Settables::declared(
                suzatary_core::settings::DATA_PUSH_TOKEN,
                Declaration::DataPush,
            ),
        );
        let names: Vec<Settable> = declared.iter().collect();
        let day = |d| chrono::NaiveDate::from_ymd_opt(2027, 1, d).expect("a day");
        let read = |now| -> Vec<(String, chrono::NaiveDate)> {
            tokens
                .expiries(&names, now)
                .into_iter()
                .map(|(name, at)| (name.name().to_owned(), at.date()))
                .collect()
        };
        let both = vec![
            (suzatary_core::settings::DATA_PUSH_TOKEN.to_owned(), day(10)),
            (suzatary_core::repos::TOKEN.to_owned(), day(9)),
        ];
        assert_eq!(read(at(4, 9)), both);
        assert_eq!(github.sent().len(), 2, "one call per token");
        assert!(
            github
                .sent()
                .iter()
                .all(|req| req.url == "https://api.github.com/")
        );
        refused.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(read(at(4, 20)), both, "within the day, as kept");
        assert_eq!(github.sent().len(), 2, "and nothing is asked");
        assert_eq!(
            read(at(5, 10)),
            vec![(suzatary_core::repos::TOKEN.to_owned(), day(9))],
            "a day on, each is asked again, and a refused token has no expiry"
        );
        assert_eq!(github.sent().len(), 4);
    }
}
