//! The sign-ins this server holds open: each a `claude setup-token` in a pseudo-terminal, begun
//! and finished by taps. Held in no capability record (only a tap reaches one) and in memory
//! alone (a restart closes every one).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use suzatary_core::capability::{Datetime, Secret};
use suzatary_core::secretary::inbox::ProposalId;
use suzatary_core::secretary::responded::{Performed, Tried};
use suzatary_core::secretary::sign_in::{Sessions, SignInUrl};
use suzatary_core::secretary::store::stamp;
use suzatary_io::setup_token::SignIn;

/// How long a sign-in stays open: its Connect card's expiry, after which it may be tapped again.
pub const LASTS: Duration = Duration::from_mins(15);

/// How long the CLI is given to draw its link, and then to exchange a code for a token.
const WAIT: Duration = Duration::from_mins(1);

/// What to do at a terminal instead, said when the box cannot drive the sign-in itself.
pub const AT_A_TERMINAL: &str = "or, at a terminal on the box: claude setup-token, then \
                                 secretspec set CLAUDE_CODE_OAUTH_TOKEN with the token it prints";

struct Open {
    session: SignIn,
    began: ProposalId,
    until: Instant,
}

/// Every sign-in this server holds open, by its link.
pub struct SignIns {
    program: String,
    open: Arc<Mutex<BTreeMap<SignInUrl, Open>>>,
    closed: Arc<dyn Fn() + Send + Sync>,
}

impl SignIns {
    /// `program` is found on the unit's `PATH`.
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            open: Arc::default(),
            closed: Arc::new(|| {}),
        }
    }

    /// Rings `closed` when a sign-in lapses, so its code card is withdrawn at once.
    #[must_use]
    pub fn ringing(self, closed: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            closed: Arc::new(closed),
            ..self
        }
    }

    /// Begin one for the card `began`, running until [`LASTS`] passes.
    #[must_use]
    pub fn begin(&self, began: &ProposalId, now: Datetime) -> Performed {
        match SignIn::start(&self.program, WAIT) {
            Ok(session) => {
                let url = session.url().clone();
                let mut open = super::locks::held(&self.open);
                let _lapsed = lapsed(&mut open);
                open.insert(
                    url,
                    Open {
                        session,
                        began: began.clone(),
                        until: Instant::now() + LASTS,
                    },
                );
                drop(open);
                self.close_when_lapsed();
                let lapses = prodrome_core::fpl::instant_of(now)
                    + chrono::Duration::from_std(LASTS).unwrap_or_default();
                Performed::Tried(Tried::Running {
                    began: stamp(now),
                    expires_at: prodrome_core::fpl::iso(lapses),
                })
            }
            Err(failed) => Performed::Tried(Tried::NotSent {
                reason: format!("the sign-in did not start: {failed}; {AT_A_TERMINAL}"),
            }),
        }
    }
}

impl SignIns {
    /// Once [`LASTS`] has passed, close what lapsed, so an abandoned sign-in's CLI ends on time
    /// even if nothing reads the table again.
    fn close_when_lapsed(&self) {
        let (open, closed) = (Arc::clone(&self.open), Arc::clone(&self.closed));
        std::thread::spawn(move || {
            std::thread::sleep(LASTS);
            let lapsing = lapsed(&mut super::locks::held(&open));
            if lapsing > 0 {
                closed();
            }
        });
    }
}

/// Close every sign-in past its time, answering how many; dropping one ends its CLI.
fn lapsed(open: &mut BTreeMap<SignInUrl, Open>) -> usize {
    let now = Instant::now();
    let before = open.len();
    open.retain(|_, one| one.until > now);
    before - open.len()
}

impl Sessions for SignIns {
    fn open(&self) -> Vec<SignInUrl> {
        let mut open = super::locks::held(&self.open);
        let _lapsed = lapsed(&mut open);
        open.keys().cloned().collect()
    }

    fn finish(
        &self,
        url: &SignInUrl,
        code: &Secret,
    ) -> Option<(ProposalId, Result<Secret, String>)> {
        let one = {
            let mut open = super::locks::held(&self.open);
            let _lapsed = lapsed(&mut open);
            open.remove(url)?
        };
        Some((
            one.began,
            one.session
                .finish(code, WAIT)
                .map_err(|failed| failed.to_string()),
        ))
    }
}
