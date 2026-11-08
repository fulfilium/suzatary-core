//! The three send transports: Microsoft SMTP over `lettre`, the local Proton Bridge by
//! hand, and the Gmail API.
//!
//! Sending is structurally human-only: this crate is unreachable from `suzatary_core::agent`.
//! A message reaches a transport already checked (`Outgoing::new`); each transport says
//! whether a failed send may still have gone.

mod submission;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use lettre::Address;
use lettre::address::Envelope;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{SmtpConnection, TlsParameters};
use lettre::transport::smtp::commands::{Data, Mail, Rcpt};
use lettre::transport::smtp::extension::{ClientId, MailBodyParameter, MailParameter};
use lettre::transport::smtp::response::{Category, Code, Detail, Severity};
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject as _;
use suzatary_core::capability::{Secret, SmtpToken};
use suzatary_core::mail::send::{Outgoing, SendError, Sent, Step, angle, build_mime};
use suzatary_core::mail::transport::Transport;

use crate::http::{Http, HttpError, Req, Ureq};
use crate::secrets::Resolves;
use crate::tokens::{Bearer, unix_now};

/// Microsoft's submission endpoint.
pub const SMTP_HOST: &str = "smtp.office365.com";
/// STARTTLS on the submission port, not implicit TLS, as Microsoft requires.
pub const SMTP_PORT: u16 = 587;

/// A wedged server fails the send rather than hanging the caller.
const MS_TIMEOUT: Duration = Duration::from_mins(1);

/// Microsoft's submission endpoint, sending `out` as `email`, authenticated by `token`.
///
/// Driven step by step over `SmtpConnection` rather than `send_raw`, because only the failed
/// step says whether the message may have gone.
///
/// # Errors
/// `WrongIdentity` when `out`'s sender is not `email`, before any connection; otherwise `Transport` naming the failed step, or `Unanswered` when the
/// connection failed after the message was handed over.
pub struct Microsoft<'a> {
    /// The account, which `From` must be.
    pub email: &'a str,
    pub token: &'a SmtpToken,
}

impl Transport for Microsoft<'_> {
    fn send(&self, out: &Outgoing, message_id: &str, date: i64) -> Result<Sent, SendError> {
        let Self { email, token } = *self;
        identity(out, email)?;

        let mime = build_mime(out, message_id, date)?;
        let envelope = envelope(out)?;

        let tls =
            TlsParameters::new(SMTP_HOST.to_owned()).map_err(|error| at(Step::Tls, &error))?;
        let mut connection = open((SMTP_HOST, SMTP_PORT), &tls)?;
        let credentials = Credentials::new(email.to_owned(), token.secret().expose().to_owned());
        submit(&mut connection, &credentials, &envelope, &mime)?;

        Ok(Sent {
            message_id: angle(message_id),
            recipients: out.recipients(),
            bytes_sent: mime.len(),
        })
    }
}

/// `WrongIdentity` unless `out` is from `email`, in any case.
fn identity(out: &Outgoing, email: &str) -> Result<(), SendError> {
    if out.sender().as_str().eq_ignore_ascii_case(email) {
        Ok(())
    } else {
        Err(SendError::WrongIdentity {
            from: out.sender().to_string(),
            account: email.to_owned(),
        })
    }
}

/// The connection, its greeting and EHLO, then STARTTLS and the EHLO over it.
fn open(
    server: impl std::net::ToSocketAddrs,
    tls: &TlsParameters,
) -> Result<SmtpConnection, SendError> {
    let hello = ClientId::default();
    let mut connection = SmtpConnection::connect(server, Some(MS_TIMEOUT), &hello, None, None)
        .map_err(|error| at(Step::Connect, &error))?;
    connection
        .starttls(tls, &hello)
        .map_err(|error| at(Step::Tls, &error))?;
    Ok(connection)
}

/// AUTH through the message; once the message is handed over, a lost connection is
/// `Unanswered` rather than a refusal, and a failed QUIT is not a failed send.
fn submit(
    connection: &mut SmtpConnection,
    credentials: &Credentials,
    envelope: &Envelope,
    mime: &[u8],
) -> Result<(), SendError> {
    // Only XOAUTH2, so the bearer token is never sent as a PLAIN password.
    connection
        .auth(&[Mechanism::Xoauth2], credentials)
        .map_err(|error| {
            if is_auth_failure(error.status()) {
                // M365 disables SMTP AUTH per tenant by default, which no correct token fixes.
                SendError::Transport {
                    step: Step::Auth,
                    why: format!(
                        "SMTP auth rejected (is SMTP AUTH enabled for this mailbox?): {error}"
                    ),
                }
            } else {
                at(Step::Auth, &error)
            }
        })?;
    let mut parameters = Vec::new();
    let non_ascii = |address: &Address| !AsRef::<str>::as_ref(address).is_ascii();
    if envelope.from().is_some_and(non_ascii) || envelope.to().iter().any(non_ascii) {
        parameters.push(MailParameter::SmtpUtfEight);
    }
    if !mime.is_ascii() {
        parameters.push(MailParameter::Body(MailBodyParameter::EightBitMime));
    }
    connection
        .command(Mail::new(envelope.from().cloned(), parameters))
        .map_err(|error| at(Step::Mail, &error))?;
    for to in envelope.to() {
        connection
            .command(Rcpt::new(to.clone(), Vec::new()))
            .map_err(|error| at(Step::Rcpt, &error))?;
    }
    connection
        .command(Data)
        .map_err(|error| at(Step::Data, &error))?;
    connection.message(mime).map_err(|error| {
        if error.status().is_some() {
            at(Step::Message, &error)
        } else {
            SendError::Unanswered(error.to_string())
        }
    })?;
    let _ = connection.quit();
    Ok(())
}

/// A lettre failure at `step`, where the message was not yet handed over.
fn at(step: Step, error: &lettre::transport::smtp::Error) -> SendError {
    SendError::Transport {
        step,
        why: error.to_string(),
    }
}

/// Is `status` the SMTP AUTH failure code (`535`, RFC 4954)?
fn is_auth_failure(status: Option<Code>) -> bool {
    matches!(
        status,
        Some(Code {
            severity: Severity::PermanentNegativeCompletion,
            category: Category::Unspecified3,
            detail: Detail::Five,
        })
    )
}

/// The local Proton Bridge's SMTP port.
pub const BRIDGE: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1025));

/// A wedged Bridge fails the send rather than hanging the caller.
const PROTON_TIMEOUT: Duration = Duration::from_secs(30);

/// A fixed string, because secretspec's audit log is searched for exactly this text.
const PROTON_REASON: &str = "suzatary send: proton smtp";

/// The local Proton Bridge, sending `out` authenticated as `smtp_user`.
///
/// The Bridge's self-signed certificate is pinned rather than trusted through a root store,
/// because webpki refuses a CA certificate as an end entity.
/// `smtp_user` is kept apart from `out`'s sender, which may be any address the account holds.
///
/// # Errors
/// `Refused` for an envelope the RFC 5321 grammar refuses, before anything is opened;
/// `Transport` at `Credentials` for an unreadable certificate or unresolved password, or at the step the Bridge failed;
/// `Unanswered` when the connection failed after the message was handed over.
pub struct Bridge<'a> {
    /// Where it listens: [`BRIDGE`] on the box.
    pub address: SocketAddr,
    /// Where `PROTON_BRIDGE_PASSWORD` resolves.
    pub secrets: &'a dyn Resolves,
    pub cert: &'a Path,
    pub smtp_user: &'a str,
}

impl Transport for Bridge<'_> {
    fn send(&self, out: &Outgoing, message_id: &str, date: i64) -> Result<Sent, SendError> {
        let Self {
            address,
            secrets,
            cert,
            smtp_user,
        } = *self;

        let mime = build_mime(out, message_id, date)?;
        let envelope = envelope(out)?;
        let pin = read_bridge_certificate(cert)?;
        let password = secrets
            .get("PROTON_BRIDGE_PASSWORD", PROTON_REASON)
            .map_err(|locked| SendError::Transport {
                step: Step::Credentials,
                why: locked.to_string(),
            })?
            .ok_or_else(|| SendError::Transport {
                step: Step::Credentials,
                why: "no PROTON_BRIDGE_PASSWORD in secretspec or the environment".to_owned(),
            })?;

        submission::submit(
            submission::Server {
                address,
                pin,
                timeout: PROTON_TIMEOUT,
            },
            smtp_user,
            &password,
            &envelope,
            &mime,
        )?;

        Ok(Sent {
            message_id: angle(message_id),
            recipients: out.recipients(),
            bytes_sent: mime.len(),
        })
    }
}

/// The fix names the IMAP port because the Bridge presents the same certificate on both.
fn read_bridge_certificate(path: &Path) -> Result<CertificateDer<'static>, SendError> {
    let failed = |doing: &str, error: &dyn std::fmt::Display| SendError::Transport {
        step: Step::Credentials,
        why: format!(
            "{doing} the pinned Bridge certificate at {}: {error} — re-extract with: \
             openssl s_client -starttls imap -connect 127.0.0.1:1143 </dev/null 2>/dev/null \
             | openssl x509 > {}",
            path.display(),
            path.display()
        ),
    };
    let pem = std::fs::read(path).map_err(|error| failed("reading", &error))?;
    CertificateDer::from_pem_slice(&pem).map_err(|error| failed("parsing", &error))
}

const GMAIL_SEND_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";

const GMAIL_SEND_TIMEOUT: Duration = Duration::from_mins(1);

impl From<crate::gmail::GrantError> for SendError {
    fn from(failed: crate::gmail::GrantError) -> Self {
        let step = match failed {
            crate::gmail::GrantError::Credentials(_) => Step::Credentials,
            crate::gmail::GrantError::Auth(_) => Step::Auth,
        };
        SendError::Transport {
            step,
            why: failed.why().to_owned(),
        }
    }
}

/// A POST that never left, or a 4xx, did not send; one lost after leaving, or a 5xx, may have.
fn gmail_send_raw(http: &dyn Http, token: &Secret, raw: &str) -> Result<(), SendError> {
    let resp = http
        .send(
            Req::json(GMAIL_SEND_URL, &serde_json::json!({ "raw": raw }))
                .header("Authorization", format!("Bearer {}", token.expose()))
                .timeout(GMAIL_SEND_TIMEOUT),
        )
        .map_err(|error| match error {
            HttpError::NotSent(why) => SendError::Transport {
                step: Step::Connect,
                why,
            },
            private @ HttpError::Private { .. } => SendError::Transport {
                step: Step::Connect,
                why: private.to_string(),
            },
            HttpError::TimedOut(why) | HttpError::Lost(why) => SendError::Unanswered(why),
        })?;
    if !resp.ok() {
        let why = format!(
            "HTTP {}: {}",
            resp.status,
            suzatary_core::spelling::shorten(&resp.text(), crate::gmail::ERROR_CAP)
        );
        return Err(if resp.status < 500 {
            SendError::Transport {
                step: Step::Message,
                why,
            }
        } else {
            SendError::Unanswered(why)
        });
    }
    Ok(())
}

/// Send `out` via the Gmail API, borrowing lieer's grant from `maildir`.
///
/// # Errors
/// `Transport` at `Credentials`, `Auth`, `Connect` or `Message` (a 4xx); `Unanswered` for a POST lost on the way or a 5xx.
pub struct Gmail<'a> {
    /// The account's lieer maildir, which holds its grant.
    pub maildir: &'a Path,
}

impl Transport for Gmail<'_> {
    fn send(&self, out: &Outgoing, message_id: &str, date: i64) -> Result<Sent, SendError> {
        let maildir = self.maildir;

        let mime = build_mime(out, message_id, date)?;
        let creds = crate::gmail::Lieer::read(maildir)?;
        let http = Ureq::new();
        let token = Bearer::new(creds).token(&http, unix_now())?;
        gmail_api(&http, &token, out, message_id, &mime)
    }
}

/// Send `out` via the Gmail API with a `token` the box minted from a Google sign-in.
///
/// # Errors
/// As [`Gmail`]'s, but for the credentials, which the caller read.
pub struct GoogleApi<'a> {
    /// An access token holding `gmail.modify`.
    pub token: &'a Secret,
}

impl Transport for GoogleApi<'_> {
    fn send(&self, out: &Outgoing, message_id: &str, date: i64) -> Result<Sent, SendError> {
        let mime = build_mime(out, message_id, date)?;
        gmail_api(&Ureq::new(), self.token, out, message_id, &mime)
    }
}

fn gmail_api(
    http: &dyn Http,
    token: &Secret,
    out: &Outgoing,
    message_id: &str,
    mime: &[u8],
) -> Result<Sent, SendError> {
    // Padded, because Gmail's API documents only the padded url-safe form.
    let raw = base64::engine::general_purpose::URL_SAFE.encode(mime);
    gmail_send_raw(http, token, &raw)?;
    Ok(Sent {
        message_id: angle(message_id),
        recipients: out.recipients(),
        // The posted base64's length, so about 4/3 of the other transports' for one message.
        bytes_sent: raw.len(),
    })
}

/// The SMTP envelope (`to + cc`, hence no Bcc), refusing an address the RFC 5321 grammar
/// rejects even when [`suzatary_core::mail::address::EmailAddress`]'s thin check passed it.
fn envelope(out: &Outgoing) -> Result<Envelope, SendError> {
    let parse = |address: &str| -> Result<Address, SendError> {
        address
            .parse::<Address>()
            .map_err(|error| SendError::Refused(format!("not an address: {address} ({error})")))
    };
    let from = parse(out.sender().as_str())?;
    let to = out
        .recipients()
        .iter()
        .map(|address| parse(address))
        .collect::<Result<Vec<Address>, SendError>>()?;
    Envelope::new(Some(from), to).map_err(|error| SendError::Refused(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::capability::Secret;
    use suzatary_core::mail::send::Composed;

    fn composed() -> Composed {
        Composed {
            sender: "her@example.test".to_owned(),
            to: vec!["him@example.test".to_owned()],
            subject: "Re: the thing".to_owned(),
            body: "One line.\n".to_owned(),
            ..Composed::default()
        }
    }

    fn outgoing() -> Outgoing {
        Outgoing::new(composed()).expect("a sendable message")
    }

    // Passes with no network, which proves no connection was attempted.
    #[test]
    fn send_microsoft_checks_identity_before_any_connection() {
        let out = Outgoing::new(Composed {
            sender: "someone-else@example.test".to_owned(),
            ..composed()
        })
        .expect("a sendable message");
        let token = SmtpToken::from_refresh(Secret::new("unused"));
        let result = Microsoft {
            email: "her@example.test",
            token: &token,
        }
        .send(&out, "id@x.test", 0);
        assert_eq!(
            result,
            Err(SendError::WrongIdentity {
                from: "someone-else@example.test".to_owned(),
                account: "her@example.test".to_owned(),
            })
        );
    }

    #[test]
    fn the_identity_check_is_case_insensitive() {
        let out = Outgoing::new(Composed {
            sender: "HER@Example.Test".to_owned(),
            ..composed()
        })
        .expect("a sendable message");
        assert_eq!(identity(&out, "her@example.test"), Ok(()));
    }

    #[test]
    fn envelope_builds_from_the_sender_and_to_plus_cc() {
        let out = Outgoing::new(Composed {
            cc: vec!["third@example.test".to_owned()],
            ..composed()
        })
        .expect("a sendable message");
        let built = envelope(&out).expect("a valid outgoing builds");
        assert_eq!(
            built.from().map(std::string::ToString::to_string),
            Some("her@example.test".to_owned())
        );
        let to: Vec<String> = built
            .to()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(
            to,
            vec![
                "him@example.test".to_owned(),
                "third@example.test".to_owned()
            ]
        );
    }

    #[test]
    fn an_address_the_transport_would_refuse_is_refused_before_it() {
        assert!(
            Outgoing::new(Composed {
                to: vec!["a<b@example.test".to_owned()],
                ..composed()
            })
            .is_err()
        );
    }

    /// Where the fake ends: nowhere, with a refusal at a step, or by closing where its reply
    /// was due.
    #[derive(Debug, Clone, Copy)]
    enum Stop {
        Nowhere,
        Refuse(Step),
        Drop(Step),
    }

    type Fail = Box<dyn std::error::Error + Send + Sync>;

    /// The reply due at `step`, or what `ending` says instead; `true` when the
    /// conversation ends here.
    fn answer(
        stream: &mut std::net::TcpStream,
        ending: Stop,
        step: Step,
        ok: &str,
    ) -> Result<bool, Fail> {
        use std::io::Write as _;
        match ending {
            Stop::Refuse(at) if at == step => {
                stream.write_all(b"554 5.0.0 not today\r\n")?;
                Ok(true)
            }
            Stop::Drop(at) if at == step => Ok(true),
            _ => {
                stream.write_all(ok.as_bytes())?;
                Ok(false)
            }
        }
    }

    /// A plain SMTP server that answers by verb, ending where `ending` says:
    /// STARTTLS is offered and, unless stopped, its handshake never comes.
    fn fake(
        ending: Stop,
    ) -> (
        std::net::SocketAddr,
        std::thread::JoinHandle<Result<(), Fail>>,
    ) {
        use std::io::BufRead as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a local port");
        let address = listener.local_addr().expect("its address");
        let serve = move || -> Result<(), Fail> {
            let (mut tcp, _) = listener.accept()?;
            tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
            let mut reader = std::io::BufReader::new(tcp.try_clone()?);
            if answer(&mut tcp, ending, Step::Connect, "220 fake ESMTP\r\n")? {
                return Ok(());
            }
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line)? == 0 {
                    return Ok(());
                }
                let verb = line
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_uppercase();
                let (step, ok) = match verb.as_str() {
                    "EHLO" => (
                        Step::Connect,
                        "250-fake\r\n250-AUTH XOAUTH2\r\n250-8BITMIME\r\n250 STARTTLS\r\n",
                    ),
                    "STARTTLS" => (Step::Tls, "220 go ahead\r\n"),
                    "AUTH" => (Step::Auth, "235 2.7.0 Accepted\r\n"),
                    "MAIL" => (Step::Mail, "250 2.1.0 Ok\r\n"),
                    "RCPT" => (Step::Rcpt, "250 2.1.5 Ok\r\n"),
                    "DATA" => (Step::Data, "354 go ahead\r\n"),
                    _ => {
                        answer(&mut tcp, Stop::Nowhere, Step::Message, "221 bye\r\n")?;
                        return Ok(());
                    }
                };
                if answer(&mut tcp, ending, step, ok)? {
                    return Ok(());
                }
                if verb == "DATA" {
                    loop {
                        let mut body = String::new();
                        if reader.read_line(&mut body)? == 0 || body == ".\r\n" {
                            break;
                        }
                    }
                    if answer(&mut tcp, ending, Step::Message, "250 2.0.0 queued\r\n")? {
                        return Ok(());
                    }
                }
            }
        };
        (address, std::thread::spawn(serve))
    }

    /// Opened in the clear, since the fake cannot finish a real TLS handshake.
    fn submitted(ending: Stop) -> Result<(), SendError> {
        let (address, server) = fake(ending);
        let connection = SmtpConnection::connect(
            address,
            Some(Duration::from_secs(10)),
            &ClientId::default(),
            None,
            None,
        );
        let sent = connection
            .map_err(|error| at(Step::Connect, &error))
            .and_then(|mut connection| {
                submit(
                    &mut connection,
                    &Credentials::new("her@example.test".to_owned(), "a token".to_owned()),
                    &envelope(&outgoing()).expect("an envelope"),
                    b"Subject: hi\r\n\r\nOne line.\r\n",
                )
            });
        server.join().expect("the fake ran").expect("it conversed");
        sent
    }

    #[test]
    fn a_failure_at_each_microsoft_step_says_whether_the_message_may_have_gone() {
        use suzatary_core::mail::send::Delivery;
        assert_eq!(submitted(Stop::Nowhere), Ok(()));
        for step in [
            Step::Connect,
            Step::Auth,
            Step::Mail,
            Step::Rcpt,
            Step::Data,
            Step::Message,
        ] {
            let failed = submitted(Stop::Refuse(step)).expect_err("refused");
            assert!(
                matches!(&failed, SendError::Transport { step: at, .. } if *at == step),
                "{step}: {failed:?}"
            );
            assert_eq!(failed.delivery(), Delivery::NotSent, "{step}");
        }
        for step in [Step::Auth, Step::Mail, Step::Rcpt, Step::Data] {
            let failed = submitted(Stop::Drop(step)).expect_err("dropped");
            assert_eq!(failed.delivery(), Delivery::NotSent, "{step}: {failed:?}");
        }
        let lost = submitted(Stop::Drop(Step::Message)).expect_err("dropped after the dot");
        assert!(matches!(lost, SendError::Unanswered(_)), "{lost:?}");
        assert_eq!(lost.delivery(), Delivery::Unknown);
    }

    #[test]
    fn the_microsoft_connection_fails_at_connect_or_tls_and_sends_nothing() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a local port");
        let address = listener.local_addr().expect("its address");
        drop(listener);
        let tls = TlsParameters::new("localhost".to_owned()).expect("parameters");
        assert!(matches!(
            open(address, &tls).map(|_| ()),
            Err(SendError::Transport {
                step: Step::Connect,
                ..
            })
        ));
        for ending in [Stop::Refuse(Step::Tls), Stop::Drop(Step::Tls)] {
            let (address, server) = fake(ending);
            let opened = open(address, &tls).map(|_| ());
            assert!(
                matches!(
                    opened,
                    Err(SendError::Transport {
                        step: Step::Tls,
                        ..
                    })
                ),
                "{ending:?}: {opened:?}"
            );
            let _ = server.join();
        }
    }
}
