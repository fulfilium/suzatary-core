//! Who may call a route, as data, and the witness each guard mints from the door's proof.

use axum::http::HeaderMap;
use prodrome_core::event::Actor;
use prodrome_core::sign::PublicKey;
use suzatary_core::capability::Login;
use suzatary_core::gate::{ActHeaders, Admitted, actor_of, check_csrf, writer_of};
use suzatary_core::record::TodoEvent;
use suzatary_core::refusal::Refusal;
use suzatary_core::secretary::tap::Tap;
use suzatary_core::signatures::{Asking, Possessed, Proven, Trusted, Unproven};
use suzatary_core::store::Prodrome;

use super::State;

/// Who may call a route, beyond the gate every request already passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// Any viewer.
    Read,
    /// A peer mapped to an actor, with no triple.
    Actor,
    /// A peer mapped to an actor, with the CSRF triple.
    Write,
    /// The same check, the actor handed to the work.
    Act,
    /// The same check, as a [`Tap`]: the owner asked for this one proposal.
    Tap,
}

impl Guard {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Guard::Read => "read",
            Guard::Actor => "actor",
            Guard::Write => "write",
            Guard::Act => "act",
            Guard::Tap => "tap",
        }
    }

    /// Checks the gate's answers before the request is read. A guard whose witness asks no
    /// proof refuses the operator in its own check.
    ///
    /// # Errors
    /// The refusal of whichever check failed.
    pub fn admits(
        self,
        state: &State,
        admitted: &Admitted,
        headers: &ActHeaders<'_>,
    ) -> Result<(), Refusal> {
        match self {
            Guard::Read => Ok(()),
            Guard::Actor => actor_of(&state.deployment, admitted).map(|_| ()),
            Guard::Write | Guard::Act | Guard::Tap => {
                writer_of(&state.deployment, admitted)?;
                check_csrf(headers)
            }
        }
    }
}

/// What a request must show of the device that sent it, under a deployment that signs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asks {
    /// Nothing: a read, an error report, or a post whose objects carry their own signatures.
    Nothing,
    /// A signature that verifies under the key it names: a device asking for that key.
    Possession,
    /// A signature by a key the history trusts for the gate's actor: every other write.
    Trusted,
}

impl Asks {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Asks::Nothing => "-",
            Asks::Possession => "possession",
            Asks::Trusted => "signed",
        }
    }
}

/// What the door proves of a request's device, by the type a route's witness asks for.
pub trait Proof: Sized + Send + 'static {
    const ASKS: Asks;

    /// The proof from the request, held to every one of `trusting` under `root`. Run on the
    /// blocking pool: it may read a chain.
    ///
    /// # Errors
    /// Why the request does not prove it.
    fn proved(
        asking: Option<Asking>,
        trusting: &[&dyn Prodrome<TodoEvent>],
        root: &PublicKey,
    ) -> Result<Self, Unproven>;

    /// `None` for a proof that asked nothing.
    fn proven(&self) -> Option<&Proven>;
}

impl Proof for () {
    const ASKS: Asks = Asks::Nothing;

    fn proved(
        _: Option<Asking>,
        _: &[&dyn Prodrome<TodoEvent>],
        _: &PublicKey,
    ) -> Result<(), Unproven> {
        Ok(())
    }

    fn proven(&self) -> Option<&Proven> {
        None
    }
}

impl Proof for Possessed {
    const ASKS: Asks = Asks::Possession;

    fn proved(
        asking: Option<Asking>,
        _: &[&dyn Prodrome<TodoEvent>],
        _: &PublicKey,
    ) -> Result<Self, Unproven> {
        asking.ok_or(Unproven::Unsigned)?.possessed()
    }

    fn proven(&self) -> Option<&Proven> {
        Some(Possessed::proven(self))
    }
}

impl Proof for Trusted {
    const ASKS: Asks = Asks::Trusted;

    fn proved(
        asking: Option<Asking>,
        trusting: &[&dyn Prodrome<TodoEvent>],
        root: &PublicKey,
    ) -> Result<Self, Unproven> {
        asking.ok_or(Unproven::Unsigned)?.trusted(trusting, root)
    }

    fn proven(&self) -> Option<&Proven> {
        Some(Trusted::proven(self))
    }
}

/// The witness a guard mints when it passes; a route's work asks for one by type.
pub trait Guarded: Sized + Send + 'static {
    const GUARD: Guard;

    type Proof: Proof;

    /// # Errors
    /// The [`Refusal`] of whichever check failed.
    fn check(
        state: &State,
        admitted: &Admitted,
        headers: &ActHeaders<'_>,
        proof: &Self::Proof,
    ) -> Result<Self, Refusal>;
}

/// Any viewer.
pub struct Reader;

impl Guarded for Reader {
    const GUARD: Guard = Guard::Read;
    type Proof = ();

    fn check(_: &State, _: &Admitted, _: &ActHeaders<'_>, (): &()) -> Result<Self, Refusal> {
        Ok(Reader)
    }
}

/// A peer mapped to an actor, asked no triple.
pub struct Mapped;

impl Guarded for Mapped {
    const GUARD: Guard = Guard::Actor;
    type Proof = ();

    fn check(_: &State, _: &Admitted, _: &ActHeaders<'_>, (): &()) -> Result<Self, Refusal> {
        Ok(Mapped)
    }
}

/// A writer whose actor the work does not need.
pub struct Writer;

impl Guarded for Writer {
    const GUARD: Guard = Guard::Write;
    type Proof = Trusted;

    fn check(_: &State, _: &Admitted, _: &ActHeaders<'_>, _: &Trusted) -> Result<Self, Refusal> {
        Ok(Writer)
    }
}

impl Guarded for Actor {
    const GUARD: Guard = Guard::Act;
    type Proof = Trusted;

    fn check(
        _: &State,
        _: &Admitted,
        _: &ActHeaders<'_>,
        proof: &Trusted,
    ) -> Result<Self, Refusal> {
        Ok(proof.proven().actor().clone())
    }
}

impl Guarded for Tap {
    const GUARD: Guard = Guard::Tap;
    type Proof = Trusted;

    fn check(
        _: &State,
        admitted: &Admitted,
        headers: &ActHeaders<'_>,
        proof: &Trusted,
    ) -> Result<Self, Refusal> {
        Tap::of(proof, headers, suzatary_core::seat::Seat::of(admitted))
    }
}

/// A writer with error-report budget left, counted before the body is read.
pub struct Reporter(pub Login);

impl Guarded for Reporter {
    const GUARD: Guard = Guard::Write;
    // A tab whose device is not trusted must still say how it failed.
    type Proof = ();

    fn check(
        state: &State,
        admitted: &Admitted,
        _: &ActHeaders<'_>,
        (): &(),
    ) -> Result<Self, Refusal> {
        use suzatary_core::clienterror::Verdict;
        let Some(login) = admitted.login().cloned() else {
            return Err(Refusal::forbidden(
                "a report must come from a named tailnet peer",
            ));
        };
        let verdict = super::locks::held(&state.reports).admit(&login, std::time::Instant::now());
        if verdict == Verdict::Refuse {
            return Err(Refusal::too_many(
                "this login has sent its error reports for now",
            ));
        }
        Ok(Reporter(login))
    }
}

/// A replica posting objects as its actor: each object's signature is held to the door one by
/// one, so the request needs none of its own.
pub struct Poster(pub Actor);

impl Guarded for Poster {
    const GUARD: Guard = Guard::Act;
    type Proof = ();

    fn check(
        state: &State,
        admitted: &Admitted,
        _: &ActHeaders<'_>,
        (): &(),
    ) -> Result<Self, Refusal> {
        actor_of(&state.deployment, admitted).map(Poster)
    }
}

/// A device asking for its key to be trusted: it shows only that it holds that key.
pub struct Asker(pub Actor, pub PublicKey);

impl Guarded for Asker {
    const GUARD: Guard = Guard::Act;
    type Proof = Possessed;

    fn check(
        _: &State,
        _: &Admitted,
        _: &ActHeaders<'_>,
        proof: &Possessed,
    ) -> Result<Self, Refusal> {
        Ok(Asker(
            proof.proven().actor().clone(),
            proof.proven().device().clone(),
        ))
    }
}

pub fn act_headers(headers: &HeaderMap) -> ActHeaders<'_> {
    let read = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    ActHeaders {
        act: read("x-suzatary-act"),
        client: read(suzatary_core::gate::CLIENT_HEADER),
        content_type: read("content-type"),
        site: read("sec-fetch-site"),
    }
}
