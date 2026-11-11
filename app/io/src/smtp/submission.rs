//! The Bridge's SMTP conversation by hand, over one pinned certificate.
//!
//! The Bridge's self-signed certificate is marked as a CA, which webpki refuses as an end entity,
//! and `lettre`'s only escape accepts any certificate. So this speaks the few submission commands
//! a send needs over rustls with [`Pinned`]: the certificate must be the pin byte for byte, and
//! the handshake signature is still checked against its key, so a replayed certificate fails.

use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use lettre::address::Envelope;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::{CertificateError, DigitallySignedStruct, OtherError, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use suzatary_base::capability::Secret;
use suzatary_core::mail::send::{SendError, Step};

/// The Bridge's address, its pinned certificate, and the per-read/write timeout.
pub(super) struct Server {
    pub address: SocketAddr,
    pub pin: CertificateDer<'static>,
    pub timeout: Duration,
}

/// Submit `mime` for `envelope` to `server`, authenticated as `user`.
///
/// # Errors
/// [`SendError::Transport`] naming the [`Step`] that failed or was refused, with the server's
/// reply; a connection lost after DATA's go-ahead is [`SendError::Unanswered`], since the Bridge
/// may have taken it. Errors name the step, never the line sent, so no password leaks.
pub(super) fn submit(
    server: Server,
    user: &str,
    password: &Secret,
    envelope: &Envelope,
    mime: &[u8],
) -> Result<(), SendError> {
    let refused = |step: Step, why: String| SendError::Transport { step, why };
    let address = server.address;
    let tcp = TcpStream::connect_timeout(&address, server.timeout)
        .map_err(|error| refused(Step::Connect, format!("{address}: {error}")))?;
    tcp.set_read_timeout(Some(server.timeout))
        .and_then(|()| tcp.set_write_timeout(Some(server.timeout)))
        .map_err(|error| {
            refused(
                Step::Connect,
                format!("setting a timeout on {address}: {error}"),
            )
        })?;

    let mut plain = Conversation::new(tcp);
    plain.reply(Step::Connect, Expect::Completion)?;
    plain.command(Step::Connect, "EHLO localhost", Expect::Completion)?;
    plain.command(Step::Tls, "STARTTLS", Expect::Completion)?;
    let mut tcp = plain.into_stream()?;

    let mut tls = rustls::ClientConnection::new(
        Arc::new(client_config(server.pin)?),
        ServerName::IpAddress(address.ip().into()),
    )
    .map_err(|error| refused(Step::Tls, format!("starting TLS with {address}: {error}")))?;
    while tls.is_handshaking() {
        tls.complete_io(&mut tcp).map_err(|error| {
            refused(Step::Tls, format!("TLS handshake with {address}: {error}"))
        })?;
    }

    let mut secure = Conversation::new(rustls::StreamOwned::new(tls, tcp));
    secure.command(Step::Tls, "EHLO localhost", Expect::Completion)?;
    let credentials = base64::engine::general_purpose::STANDARD
        .encode(format!("\0{user}\0{}", password.expose()));
    secure.command(
        Step::Auth,
        &format!("AUTH PLAIN {credentials}"),
        Expect::Completion,
    )?;
    let from = envelope
        .from()
        .map_or_else(String::new, ToString::to_string);
    secure.command(
        Step::Mail,
        &format!("MAIL FROM:<{from}>"),
        Expect::Completion,
    )?;
    for to in envelope.to() {
        secure.command(Step::Rcpt, &format!("RCPT TO:<{to}>"), Expect::Completion)?;
    }
    secure.command(Step::Data, "DATA", Expect::Intermediate)?;
    secure.send(Step::Message, &data_block(mime), Expect::Completion)?;
    // The message is accepted: a failed QUIT must not report it unsent, or a second tap sends it
    // twice.
    let _ = secure.command(Step::Message, "QUIT", Expect::Completion);
    Ok(())
}

/// A [`ServerCertVerifier`] that accepts exactly one certificate.
#[derive(Debug)]
pub(super) struct Pinned {
    certificate: CertificateDer<'static>,
    algorithms: WebPkiSupportedAlgorithms,
}

/// The server presented something other than the pinned certificate. Its
/// `Debug` is the sentence, because rustls prints the inner error with `{:?}`.
#[derive(thiserror::Error)]
#[error("the server's certificate is not the pinned Bridge certificate")]
struct NotThePin;

impl fmt::Debug for NotThePin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.as_ref() == self.certificate.as_ref() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(CertificateError::Other(
                OtherError(Arc::new(NotThePin)),
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// A client config over `ring` that trusts `pin` and nothing else; the provider is passed, so
/// this works before any process default is installed.
fn client_config(pin: CertificateDer<'static>) -> Result<rustls::ClientConfig, SendError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Pinned {
        certificate: pin,
        algorithms: provider.signature_verification_algorithms,
    };
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| SendError::Transport {
            step: Step::Tls,
            why: error.to_string(),
        })?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

/// Which reply class a step expects: 2xx, or 3xx (DATA's "go ahead").
#[derive(Clone, Copy)]
enum Expect {
    Completion,
    Intermediate,
}

impl Expect {
    fn accepts(self, code: u16) -> bool {
        match self {
            Expect::Completion => (200..300).contains(&code),
            Expect::Intermediate => (300..400).contains(&code),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Reply {
    code: u16,
    lines: Vec<String>,
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.lines.join(" / "))
    }
}

/// Read one reply: lines of `DDD-text` continued until a `DDD text` (or a
/// bare `DDD`) ends it.
fn read_reply(reader: &mut impl BufRead) -> Result<Reply, String> {
    let mut lines = Vec::new();
    loop {
        let mut raw = Vec::new();
        let read = reader
            .read_until(b'\n', &mut raw)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("the connection closed mid-reply".to_owned());
        }
        let text = String::from_utf8_lossy(&raw);
        let line = text.trim_end_matches(['\r', '\n']);
        let code = line
            .get(..3)
            .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|digits| digits.parse::<u16>().ok())
            .ok_or_else(|| format!("not an SMTP reply: {line:?}"))?;
        let (last, rest) = match line.get(3..4) {
            None => (true, ""),
            Some(" ") => (true, &line[4..]),
            Some("-") => (false, &line[4..]),
            Some(_) => return Err(format!("not an SMTP reply: {line:?}")),
        };
        lines.push(rest.to_owned());
        if last {
            return Ok(Reply { code, lines });
        }
    }
}

/// `mime` as DATA's payload: every line ended by CRLF (a bare LF is
/// normalised), a line starting with `.` given a second one (RFC 5321
/// §4.5.2), and the `.` line that ends it.
fn data_block(mime: &[u8]) -> Vec<u8> {
    let body = mime.strip_suffix(b"\n").unwrap_or(mime);
    let mut out = Vec::with_capacity(mime.len() + mime.len() / 32 + 5);
    for line in body.split(|&byte| byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.first() == Some(&b'.') {
            out.push(b'.');
        }
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}

struct Conversation<S> {
    stream: BufReader<S>,
}

impl<S: Read + Write> Conversation<S> {
    fn new(stream: S) -> Self {
        Self {
            stream: BufReader::new(stream),
        }
    }

    fn command(&mut self, step: Step, line: &str, expect: Expect) -> Result<(), SendError> {
        self.send(step, format!("{line}\r\n").as_bytes(), expect)
    }

    fn send(&mut self, step: Step, bytes: &[u8], expect: Expect) -> Result<(), SendError> {
        let stream = self.stream.get_mut();
        stream
            .write_all(bytes)
            .and_then(|()| stream.flush())
            .map_err(|error| lost(step, &error.to_string()))?;
        self.reply(step, expect)
    }

    /// Only a 4xx or 5xx says no; after the message, any other reply that is
    /// not the expected one is no answer for it.
    fn reply(&mut self, step: Step, expect: Expect) -> Result<(), SendError> {
        let reply = read_reply(&mut self.stream).map_err(|why| lost(step, &why))?;
        if expect.accepts(reply.code) {
            Ok(())
        } else if reply.code >= 400 {
            Err(SendError::Transport {
                step,
                why: format!("the Bridge answered {reply}"),
            })
        } else {
            Err(lost(step, &format!("the Bridge answered {reply}")))
        }
    }
}

impl Conversation<TcpStream> {
    /// The socket back, for the TLS handshake. Bytes buffered past STARTTLS's reply are refused,
    /// since they would be read as if they came over TLS (STARTTLS command injection).
    fn into_stream(self) -> Result<TcpStream, SendError> {
        if self.stream.buffer().is_empty() {
            Ok(self.stream.into_inner())
        } else {
            Err(SendError::Transport {
                step: Step::Tls,
                why: "the Bridge sent data after its STARTTLS reply, before TLS".to_owned(),
            })
        }
    }
}

/// The connection failed at `step` with no reply saying no: before the
/// message a failed step, after DATA's go-ahead a message nobody answered for.
fn lost(step: Step, why: &str) -> SendError {
    match step {
        Step::Message => SendError::Unanswered(why.to_owned()),
        Step::Credentials
        | Step::Connect
        | Step::Tls
        | Step::Auth
        | Step::Mail
        | Step::Rcpt
        | Step::Data => SendError::Transport {
            step,
            why: why.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::ServerConnection;
    use rustls_pki_types::PrivateKeyDer;
    use rustls_pki_types::pem::PemObject as _;
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    /// Self-signed, `CA:TRUE`, CN and IP SAN 127.0.0.1: the Bridge's certificate's shape.
    const BRIDGE_CERT: &[u8] = include_bytes!("../../tests/fixtures/bridge-cert.pem");
    const BRIDGE_KEY: &[u8] = include_bytes!("../../tests/fixtures/bridge-key.pem");
    /// Another certificate of the same shape, whose key was thrown away.
    const OTHER_CERT: &[u8] = include_bytes!("../../tests/fixtures/other-cert.pem");

    const PASSWORD: &str = "hunter2-bridge-password";
    const TIMEOUT: Duration = Duration::from_secs(10);

    fn cert(pem: &[u8]) -> CertificateDer<'static> {
        CertificateDer::from_pem_slice(pem).expect("the fixture parses")
    }

    fn envelope() -> Envelope {
        Envelope::new(
            Some("her@example.test".parse().expect("an address")),
            vec![
                "him@example.test".parse().expect("an address"),
                "third@example.test".parse().expect("an address"),
            ],
        )
        .expect("an envelope")
    }

    /// What the fake Bridge saw: every command line, and the message with
    /// dot-stuffing undone.
    #[derive(Debug, Default)]
    struct Seen {
        commands: Vec<String>,
        message: Vec<u8>,
    }

    type Fail = Box<dyn std::error::Error + Send + Sync>;

    fn line<S: Read>(reader: &mut BufReader<S>, seen: &mut Seen) -> Result<String, Fail> {
        let mut raw = Vec::new();
        reader.read_until(b'\n', &mut raw)?;
        let text = String::from_utf8(raw)?.trim_end_matches("\r\n").to_owned();
        seen.commands.push(text.clone());
        Ok(text)
    }

    fn say<S: Read + Write>(reader: &mut BufReader<S>, text: &str) -> Result<(), Fail> {
        reader.get_mut().write_all(text.as_bytes())?;
        Ok(reader.get_mut().flush()?)
    }

    /// Where the fake Bridge ends the conversation: nowhere, with a reply
    /// saying no at a step, or by closing the connection where that step's
    /// reply was due.
    #[derive(Debug, Clone, Copy)]
    enum Stop {
        Nowhere,
        Refuse(Step),
        Drop(Step),
    }

    /// The reply due at `step`: `ok`, or what `ending` says instead. `true`
    /// when the conversation ends here.
    fn answer<S: Read + Write>(
        reader: &mut BufReader<S>,
        ending: Stop,
        step: Step,
        ok: &str,
    ) -> Result<bool, Fail> {
        match ending {
            Stop::Refuse(at) if at == step => {
                let no = if step == Step::Auth {
                    "535 5.7.8 Authentication failed\r\n"
                } else {
                    "554 5.0.0 not today\r\n"
                };
                say(reader, no)?;
                Ok(true)
            }
            Stop::Drop(at) if at == step => Ok(true),
            _ => {
                say(reader, ok)?;
                Ok(false)
            }
        }
    }

    /// One conversation, scripted: STARTTLS with the Bridge-shaped
    /// certificate, a multi-line EHLO both times, ending where `ending` says.
    /// `taken` sees the message once it is read whole, before the reply that accepts it.
    fn converse(listener: &TcpListener, ending: Stop, taken: &dyn Fn(&Seen)) -> Result<Seen, Fail> {
        let mut seen = Seen::default();
        let (tcp, _) = listener.accept()?;
        tcp.set_read_timeout(Some(TIMEOUT))?;
        let mut plain = BufReader::new(tcp);
        if answer(
            &mut plain,
            ending,
            Step::Connect,
            "220 fake.bridge ESMTP\r\n",
        )? {
            return Ok(seen);
        }
        line(&mut plain, &mut seen)?;
        say(
            &mut plain,
            "250-fake.bridge\r\n250-8BITMIME\r\n250 STARTTLS\r\n",
        )?;
        line(&mut plain, &mut seen)?;
        if answer(
            &mut plain,
            ending,
            Step::Tls,
            "220 2.0.0 Ready to start TLS\r\n",
        )? {
            return Ok(seen);
        }

        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(
            vec![cert(BRIDGE_CERT)],
            PrivateKeyDer::from_pem_slice(BRIDGE_KEY)?,
        )?;
        let connection = ServerConnection::new(Arc::new(config))?;
        let mut tls = BufReader::new(rustls::StreamOwned::new(connection, plain.into_inner()));
        line(&mut tls, &mut seen)?;
        say(
            &mut tls,
            "250-fake.bridge\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n",
        )?;
        line(&mut tls, &mut seen)?;
        if answer(
            &mut tls,
            ending,
            Step::Auth,
            "235 2.7.0 Authentication successful\r\n",
        )? {
            return Ok(seen);
        }
        line(&mut tls, &mut seen)?;
        if answer(&mut tls, ending, Step::Mail, "250 2.1.0 Ok\r\n")? {
            return Ok(seen);
        }
        while line(&mut tls, &mut seen)?.starts_with("RCPT") {
            if answer(&mut tls, ending, Step::Rcpt, "250 2.1.5 Ok\r\n")? {
                return Ok(seen);
            }
        }
        if answer(
            &mut tls,
            ending,
            Step::Data,
            "354 End data with <CR><LF>.<CR><LF>\r\n",
        )? {
            return Ok(seen);
        }
        loop {
            let mut raw = Vec::new();
            tls.read_until(b'\n', &mut raw)?;
            if raw == b".\r\n" {
                break;
            }
            let unstuffed = raw.strip_prefix(b".").unwrap_or(&raw);
            seen.message.extend_from_slice(unstuffed);
        }
        taken(&seen);
        if answer(&mut tls, ending, Step::Message, "250 2.0.0 Ok: queued\r\n")? {
            return Ok(seen);
        }
        line(&mut tls, &mut seen)?;
        say(&mut tls, "221 2.0.0 Bye\r\n")?;
        Ok(seen)
    }

    fn fake_bridge(ending: Stop) -> (SocketAddr, JoinHandle<Result<Seen, Fail>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let address = listener.local_addr().expect("its address");
        (
            address,
            std::thread::spawn(move || converse(&listener, ending, &|_| {})),
        )
    }

    fn server(address: SocketAddr, pin: &[u8]) -> Server {
        Server {
            address,
            pin: cert(pin),
            timeout: TIMEOUT,
        }
    }

    const MIME: &[u8] = b"From: her@example.test\r\nSubject: dots\r\n\r\n\
        .one leading dot\r\n..two leading dots\r\n.\r\nplain\r\n";

    #[test]
    fn the_pinned_ca_certificate_is_accepted_and_the_message_arrives_intact() {
        let (address, bridge) = fake_bridge(Stop::Nowhere);
        let sent = submit(
            server(address, BRIDGE_CERT),
            "her@example.test",
            &Secret::new(PASSWORD),
            &envelope(),
            MIME,
        );
        assert_eq!(sent, Ok(()));
        let seen = bridge.join().expect("the fake ran").expect("it conversed");
        assert_eq!(seen.message, MIME);
        let credentials = base64::engine::general_purpose::STANDARD
            .encode(format!("\0her@example.test\0{PASSWORD}"));
        assert_eq!(
            seen.commands,
            vec![
                "EHLO localhost".to_owned(),
                "STARTTLS".to_owned(),
                "EHLO localhost".to_owned(),
                format!("AUTH PLAIN {credentials}"),
                "MAIL FROM:<her@example.test>".to_owned(),
                "RCPT TO:<him@example.test>".to_owned(),
                "RCPT TO:<third@example.test>".to_owned(),
                "DATA".to_owned(),
                "QUIT".to_owned(),
            ]
        );
    }

    #[test]
    fn a_different_certificate_is_refused_by_name() {
        let (address, bridge) = fake_bridge(Stop::Nowhere);
        let sent = submit(
            server(address, OTHER_CERT),
            "her@example.test",
            &Secret::new(PASSWORD),
            &envelope(),
            MIME,
        );
        match sent {
            Err(SendError::Transport { step, why }) => {
                assert_eq!(step, Step::Tls);
                assert!(why.contains("TLS handshake"), "{why}");
                assert!(why.contains("not the pinned Bridge certificate"), "{why}");
            }
            other => panic!("expected a refused handshake, got {other:?}"),
        }
        assert!(bridge.join().expect("the fake ran").is_err());
    }

    #[test]
    fn an_auth_failure_names_the_step_and_never_the_password() {
        let (address, bridge) = fake_bridge(Stop::Refuse(Step::Auth));
        let sent = submit(
            server(address, BRIDGE_CERT),
            "her@example.test",
            &Secret::new(PASSWORD),
            &envelope(),
            MIME,
        );
        let credentials = base64::engine::general_purpose::STANDARD
            .encode(format!("\0her@example.test\0{PASSWORD}"));
        match sent {
            Err(SendError::Transport { step, why }) => {
                assert_eq!(step, Step::Auth);
                assert!(why.contains("535 5.7.8 Authentication failed"), "{why}");
                assert!(!why.contains(PASSWORD), "{why}");
                assert!(!why.contains(&credentials), "{why}");
            }
            other => panic!("expected a refused AUTH, got {other:?}"),
        }
        bridge.join().expect("the fake ran").expect("it conversed");
    }

    #[test]
    fn a_failure_at_each_step_says_whether_the_message_may_have_gone() {
        use suzatary_core::mail::send::Delivery;
        let cases = [
            (Stop::Refuse(Step::Connect), Some(Step::Connect)),
            (Stop::Drop(Step::Connect), Some(Step::Connect)),
            (Stop::Refuse(Step::Tls), Some(Step::Tls)),
            (Stop::Drop(Step::Tls), Some(Step::Tls)),
            (Stop::Refuse(Step::Auth), Some(Step::Auth)),
            (Stop::Drop(Step::Auth), Some(Step::Auth)),
            (Stop::Refuse(Step::Mail), Some(Step::Mail)),
            (Stop::Refuse(Step::Rcpt), Some(Step::Rcpt)),
            (Stop::Refuse(Step::Data), Some(Step::Data)),
            (Stop::Drop(Step::Data), Some(Step::Data)),
            (Stop::Refuse(Step::Message), Some(Step::Message)),
            (Stop::Drop(Step::Message), None),
        ];
        for (ending, refused_at) in cases {
            let (address, bridge) = fake_bridge(ending);
            let failed = submit(
                server(address, BRIDGE_CERT),
                "her@example.test",
                &Secret::new(PASSWORD),
                &envelope(),
                MIME,
            )
            .expect_err("the Bridge stopped it");
            bridge.join().expect("the fake ran").expect("it conversed");
            match (refused_at, &failed) {
                (Some(at), SendError::Transport { step, .. }) => {
                    assert_eq!(*step, at, "{ending:?}: {failed}");
                    assert_eq!(failed.delivery(), Delivery::NotSent, "{ending:?}");
                }
                (None, SendError::Unanswered(_)) => {
                    assert_eq!(failed.delivery(), Delivery::Unknown, "{ending:?}");
                }
                _ => panic!("{ending:?}: {failed:?}"),
            }
        }
    }

    #[test]
    fn the_verifier_accepts_the_pin_and_nothing_else() {
        let pinned = Pinned {
            certificate: cert(BRIDGE_CERT),
            algorithms: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        };
        let name =
            ServerName::IpAddress(std::net::IpAddr::from(std::net::Ipv4Addr::LOCALHOST).into());
        let now = UnixTime::now();
        assert!(
            pinned
                .verify_server_cert(&cert(BRIDGE_CERT), &[], &name, &[], now)
                .is_ok()
        );
        assert!(
            pinned
                .verify_server_cert(&cert(OTHER_CERT), &[], &name, &[], now)
                .is_err()
        );
    }

    struct BridgePassword;

    impl crate::secrets::Resolves for BridgePassword {
        fn get(
            &self,
            name: &str,
            _reason: &str,
        ) -> Result<Option<Secret>, crate::secrets::SecretError> {
            Ok((name == "PROTON_BRIDGE_PASSWORD").then(|| Secret::new(PASSWORD)))
        }
    }

    #[test]
    fn the_bridge_keeps_every_transport_law_behind_a_fake_bridge() {
        use suzatary_core::conformance::transport::{Case, check};
        use suzatary_core::mail::send::{Composed, Outgoing};
        use suzatary_core::mail::transport::Delivered;

        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let address = listener.local_addr().expect("its address");
        let delivered = Arc::new(std::sync::Mutex::new(Vec::new()));
        let kept = Arc::clone(&delivered);
        std::thread::spawn(move || {
            let take = |seen: &Seen| {
                let after = |prefix: &str| {
                    seen.commands
                        .iter()
                        .filter_map(|line| line.strip_prefix(prefix))
                        .map(|rest| rest.trim_end_matches('>').to_owned())
                        .collect::<Vec<_>>()
                };
                kept.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(Delivered {
                        from: after("MAIL FROM:<").concat(),
                        to: after("RCPT TO:<"),
                        message: seen.message.clone(),
                    });
            };
            while converse(&listener, Stop::Nowhere, &take).is_ok() {}
        });
        let dir = tempfile::tempdir().expect("a scratch dir");
        let cert = dir.path().join("bridge-cert.pem");
        std::fs::write(&cert, BRIDGE_CERT).expect("the pin");
        let bridge = crate::smtp::Bridge {
            address,
            secrets: &BridgePassword,
            cert: &cert,
            smtp_user: "her@example.test",
        };
        let case = |to: &[&str], cc: &[&str], body: &str| Case {
            out: Outgoing::new(Composed {
                sender: "her@example.test".to_owned(),
                to: to.iter().map(|one| (*one).to_owned()).collect(),
                cc: cc.iter().map(|one| (*one).to_owned()).collect(),
                subject: "Re: the lease".to_owned(),
                body: body.to_owned(),
                ..Composed::default()
            })
            .expect("a sendable message"),
            message_id: format!("{}@example.test", to.len() + cc.len()),
            at: 1_790_000_000,
        };
        let cases = [
            case(&["him@example.test"], &[], "One line.\n"),
            case(
                &["him@example.test", "third@example.test"],
                &["copy@example.test"],
                ".a leading dot\n..two\n.\nand a last line",
            ),
        ];
        let read = || {
            delivered
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        };
        // A message refused by the guards never connects, so the fake sees only the sends.
        assert_eq!(check(&bridge, read, &cases), Ok(()));
    }
}
