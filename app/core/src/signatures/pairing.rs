use chrono::{DateTime, TimeDelta, Utc};
use hmac::{Hmac, Mac as _};
use prodrome_core::event::Actor;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::store::StoreError;

/// THE LETTERS OF A PAIRING CODE: RFC 8628 §6.1's base-20 set, consonants only, so a code
/// spells no word and holds no letter a person confuses with a digit.
pub const ALPHABET: [u8; 20] = *b"BCDFGHJKLMNPQRSTVWXZ";

/// How many letters a code has: 20^8 is about 2^34.6 codes, and with [`TRIES`] wrong tries
/// before a code dies, one guess run succeeds about once in 5 billion.
pub(crate) const LETTERS: usize = 8;

/// How long a code works after the box prints it, in minutes.
pub const MINUTES: i64 = 10;

/// How many wrong tries a code takes; the last one ends it.
pub const TRIES: u8 = 5;

/// What the box tells a person to run for a new code.
pub(crate) const RUN: &str = "`suzatary device pair`";

/// A PAIRING CODE: `LETTERS` letters of [`ALPHABET`]. It has no `Display`, and its `Debug`
/// prints no letter, so no log line or error can carry one; [`Code::shown`] is the one way out,
/// and only the terminal that minted it calls it.
#[derive(Clone, PartialEq, Eq)]
pub struct Code([u8; LETTERS]);

impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Code(…)")
    }
}

/// Text that is no pairing code, refused before anything is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "a pairing code is {LETTERS} letters, as {RUN} printed them (a dash or spaces between them \
     are fine)"
)]
pub struct NotACode;

impl Code {
    /// The code `random` spells, letter by letter: a byte of 240 or more is skipped, so each
    /// letter is uniform over the 20; `None` when the bytes run out first, and the caller draws
    /// more. `random` comes from the system's generator at the box.
    #[must_use]
    pub fn drawn(random: &[u8]) -> Option<Code> {
        const FAIR: u8 = 240;
        let mut letters = random
            .iter()
            .filter(|byte| **byte < FAIR)
            .map(|byte| ALPHABET[usize::from(byte % 20)]);
        let mut code = [0; LETTERS];
        for slot in &mut code {
            *slot = letters.next()?;
        }
        Some(Code(code))
    }

    /// The code a person typed: letters in either case, a dash or spaces anywhere.
    ///
    /// # Errors
    /// `NotACode` for anything else.
    pub fn typed(text: &str) -> Result<Code, NotACode> {
        let letters: Vec<u8> = text
            .bytes()
            .filter(|byte| *byte != b'-' && !byte.is_ascii_whitespace())
            .map(|byte| byte.to_ascii_uppercase())
            .collect();
        let code: [u8; LETTERS] = letters.try_into().map_err(|_| NotACode)?;
        if code.iter().all(|letter| ALPHABET.contains(letter)) {
            Ok(Code(code))
        } else {
            Err(NotACode)
        }
    }

    /// The code as the terminal prints it, in two groups of four: `BCDF-GHJK`.
    #[must_use]
    pub fn shown(&self) -> String {
        let (first, second) = self.0.split_at(LETTERS / 2);
        format!(
            "{}-{}",
            String::from_utf8_lossy(first),
            String::from_utf8_lossy(second)
        )
    }

    /// The keyed hash the box keeps in place of the code: HMAC-SHA256 under `salt` over the
    /// actor it trusts for and the letters, so a code typed by another login does not match.
    fn mac(&self, salt: &Salt, actor: &Actor) -> Hmac<Sha256> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&salt.0).expect("HMAC takes a key of any length");
        mac.update(actor.as_str().as_bytes());
        mac.update(&[0]);
        mac.update(&self.0);
        mac
    }
}

/// The random key of one code's hash, drawn with it.
#[derive(Clone, PartialEq, Eq)]
pub struct Salt(pub [u8; 32]);

/// WHAT THE BOX KEEPS OF THE LAST CODE IT PRINTED: never the code, only its keyed hash, and
/// whether it still works. One per instance, box-local, never pushed.
#[derive(Clone, PartialEq, Eq)]
pub enum Pairing {
    /// A code waits to be typed.
    Waiting {
        /// The hash's key.
        salt: Salt,
        /// The hash of the actor and the code.
        digest: [u8; 32],
        /// When it stops working.
        expires: DateTime<Utc>,
        /// Wrong tries left, 1 to [`TRIES`].
        left: u8,
    },
    /// A device's key was trusted with it.
    Used {
        /// When.
        at: DateTime<Utc>,
    },
    /// The last wrong try ended it.
    Spent {
        /// When.
        at: DateTime<Utc>,
    },
}

impl std::fmt::Debug for Pairing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pairing::Waiting { expires, left, .. } => f
                .debug_struct("Waiting")
                .field("expires", expires)
                .field("left", left)
                .finish_non_exhaustive(),
            Pairing::Used { at } => f.debug_struct("Used").field("at", at).finish(),
            Pairing::Spent { at } => f.debug_struct("Spent").field("at", at).finish(),
        }
    }
}

/// What a typed code comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presented {
    /// The code matches, and is used now: trust the key that signed the request.
    Matched,
    /// The code trusts nothing.
    Refused(Refused),
}

/// Why a typed code trusts nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Not the code; the try is counted.
    Wrong {
        /// Tries left.
        left: u8,
    },
    /// Not the code, and the last try: the code is dead.
    Died,
    /// The box holds no code.
    NotAsked,
    /// The code's minutes are over.
    Expired,
    /// The code trusted a device already.
    Used,
    /// The code died of wrong tries before.
    Spent,
}

impl Refused {
    /// The box's refusal, in plain words.
    #[must_use]
    pub fn said(self) -> String {
        let again = format!("Run {RUN} at the box for a new one.");
        match self {
            Refused::Wrong { left } => format!(
                "That is not the code the box printed. {left} {} left.",
                if left == 1 { "try" } else { "tries" }
            ),
            Refused::Died => format!(
                "That is not the code the box printed, and it was the last try, so the code no \
                 longer works. {again}"
            ),
            Refused::NotAsked => format!("No pairing code is waiting. {again}"),
            Refused::Expired => {
                format!("This code has expired: a code works for {MINUTES} minutes. {again}")
            }
            Refused::Used => {
                format!("This code was already used: a code trusts one device. {again}")
            }
            Refused::Spent => format!(
                "This code was tried {TRIES} times with the wrong letters and no longer works. \
                 {again}"
            ),
        }
    }
}

impl Pairing {
    /// The waiting state of a code minted at `now` for `actor`.
    #[must_use]
    pub fn minted(code: &Code, salt: Salt, actor: &Actor, now: DateTime<Utc>) -> Pairing {
        let digest = code.mac(&salt, actor).finalize().into_bytes().into();
        Pairing::Waiting {
            salt,
            digest,
            expires: now + TimeDelta::minutes(MINUTES),
            left: TRIES,
        }
    }

    /// `code`, typed by a device of `actor` at `now`, against what the box holds, and what to
    /// keep in its place, `None` to keep it as it is: a match is used at once, so a failure to
    /// trust after it costs a new code, never a second use. The hash is compared in constant time (`Mac::verify_slice`), and
    /// an expired, used or spent code is refused before it is compared.
    #[must_use]
    pub(crate) fn presented(
        held: Option<&Pairing>,
        code: &Code,
        actor: &Actor,
        now: DateTime<Utc>,
    ) -> (Presented, Option<Pairing>) {
        match held {
            None => (Presented::Refused(Refused::NotAsked), None),
            Some(Pairing::Used { .. }) => (Presented::Refused(Refused::Used), None),
            Some(Pairing::Spent { .. }) => (Presented::Refused(Refused::Spent), None),
            Some(Pairing::Waiting { expires, .. }) if now >= *expires => {
                (Presented::Refused(Refused::Expired), None)
            }
            Some(Pairing::Waiting {
                salt,
                digest,
                expires,
                left,
            }) => {
                if code.mac(salt, actor).verify_slice(digest).is_ok() {
                    (Presented::Matched, Some(Pairing::Used { at: now }))
                } else if *left <= 1 {
                    (
                        Presented::Refused(Refused::Died),
                        Some(Pairing::Spent { at: now }),
                    )
                } else {
                    let left = left - 1;
                    (
                        Presented::Refused(Refused::Wrong { left }),
                        Some(Pairing::Waiting {
                            salt: salt.clone(),
                            digest: *digest,
                            expires: *expires,
                            left,
                        }),
                    )
                }
            }
        }
    }
}

/// WHERE THE BOX KEEPS ITS PAIRING: one value, replaced in one step against every other writer,
/// so two tries at once, or a try and a new code, never interleave.
pub trait Pairings: Send + Sync {
    /// Replace what is held by what `step` answers for it, `None` keeping it as it is, while no
    /// other writer reads or writes.
    ///
    /// # Errors
    /// The store's own; `step` then ran on nothing, or its answer is not kept.
    fn rewrite(
        &self,
        step: &mut dyn FnMut(Option<&Pairing>) -> Option<Pairing>,
    ) -> Result<(), StoreError>;
}

/// `code` typed by a device of `actor` at `now`, the try counted and a match used, in one step.
///
/// # Errors
/// The store's own.
pub fn present(
    held: &dyn Pairings,
    code: &Code,
    actor: &Actor,
    now: DateTime<Utc>,
) -> Result<Presented, StoreError> {
    let mut presented = Presented::Refused(Refused::NotAsked);
    held.rewrite(&mut |pairing| {
        let (answer, kept) = Pairing::presented(pairing, code, actor, now);
        presented = answer;
        kept
    })?;
    Ok(presented)
}

/// A pairing as text, its one serde shape: the bytes in hex.
#[derive(Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Spelled {
    Waiting {
        salt: String,
        digest: String,
        expires: DateTime<Utc>,
        left: u8,
    },
    Used {
        at: DateTime<Utc>,
    },
    Spent {
        at: DateTime<Utc>,
    },
}

impl Serialize for Pairing {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Pairing::Waiting {
                salt,
                digest,
                expires,
                left,
            } => Spelled::Waiting {
                salt: hex::encode(salt.0),
                digest: hex::encode(digest),
                expires: *expires,
                left: *left,
            },
            Pairing::Used { at } => Spelled::Used { at: *at },
            Pairing::Spent { at } => Spelled::Spent { at: *at },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Pairing {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let bytes = |text: &str| -> Result<[u8; 32], D::Error> {
            hex::decode(text)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| D::Error::custom("not 32 bytes in hex"))
        };
        Ok(match Spelled::deserialize(deserializer)? {
            Spelled::Waiting {
                salt,
                digest,
                expires,
                left,
            } => {
                if !(1..=TRIES).contains(&left) {
                    return Err(D::Error::custom(format!(
                        "{left} tries left is not 1 to {TRIES}"
                    )));
                }
                Pairing::Waiting {
                    salt: Salt(bytes(&salt)?),
                    digest: bytes(&digest)?,
                    expires,
                    left,
                }
            }
            Spelled::Used { at } => Pairing::Used { at },
            Spelled::Spent { at } => Pairing::Spent { at },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signatures::InMemoryPairings;

    fn at(minute: u32) -> DateTime<Utc> {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
            .and_then(|day| day.and_hms_opt(12, minute, 0))
            .expect("a real instant")
            .and_utc()
    }

    fn ada() -> Actor {
        Actor::new("ada").expect("a roster name")
    }

    fn code(text: &str) -> Code {
        Code::typed(text).expect("a code")
    }

    fn waiting(text: &str) -> InMemoryPairings {
        let store = InMemoryPairings::default();
        let minted = Pairing::minted(&code(text), Salt([7; 32]), &ada(), at(0));
        store.rewrite(&mut |_| Some(minted.clone())).expect("kept");
        store
    }

    fn held(store: &InMemoryPairings) -> Option<Pairing> {
        let mut read = None;
        store
            .rewrite(&mut |pairing| {
                read = pairing.cloned();
                None
            })
            .expect("read");
        read
    }

    #[test]
    fn a_drawn_code_is_eight_letters_of_the_alphabet_and_skips_unfair_bytes() {
        let drawn = Code::drawn(&[0, 255, 1, 240, 19, 20, 39, 239, 100, 7]).expect("enough bytes");
        assert_eq!(drawn.shown(), "BCZB-ZZBK");
        assert_eq!(
            Code::drawn(&[250; 32]),
            None,
            "bytes all unfair draw nothing"
        );
    }

    #[test]
    fn neither_debug_carries_a_letter() {
        let shown = format!("{:?}", code("BCDF-GHJK"));
        assert!(!shown.contains('B'), "{shown}");
        let pairing = Pairing::minted(&code("BCDF-GHJK"), Salt([1; 32]), &ada(), at(0));
        let shown = format!("{pairing:?}");
        assert!(!shown.contains("BCDF"), "{shown}");
    }

    #[test]
    fn the_right_code_matches_once_and_is_then_used() {
        let store = waiting("BCDF-GHJK");
        assert_eq!(
            present(&store, &code("bcdfghjk"), &ada(), at(9)).expect("read"),
            Presented::Matched
        );
        assert_eq!(
            present(&store, &code("BCDF-GHJK"), &ada(), at(9)).expect("read"),
            Presented::Refused(Refused::Used)
        );
    }

    #[test]
    fn a_code_typed_for_another_actor_does_not_match() {
        let store = waiting("BCDF-GHJK");
        let grace = Actor::new("grace").expect("a roster name");
        assert_eq!(
            present(&store, &code("BCDF-GHJK"), &grace, at(1)).expect("read"),
            Presented::Refused(Refused::Wrong { left: TRIES - 1 })
        );
    }

    #[test]
    fn five_wrong_tries_end_the_code_and_the_right_one_then_fails() {
        let store = waiting("BCDF-GHJK");
        for left in (1..TRIES).rev() {
            assert_eq!(
                present(&store, &code("ZZZZ-ZZZZ"), &ada(), at(1)).expect("read"),
                Presented::Refused(Refused::Wrong { left })
            );
        }
        assert_eq!(
            present(&store, &code("ZZZZ-ZZZZ"), &ada(), at(1)).expect("read"),
            Presented::Refused(Refused::Died)
        );
        assert_eq!(
            present(&store, &code("BCDF-GHJK"), &ada(), at(1)).expect("read"),
            Presented::Refused(Refused::Spent)
        );
    }

    #[test]
    fn a_code_works_for_ten_minutes_and_an_expired_try_is_not_counted() {
        let store = waiting("BCDF-GHJK");
        let before = held(&store);
        assert_eq!(
            present(&store, &code("ZZZZ-ZZZZ"), &ada(), at(10)).expect("read"),
            Presented::Refused(Refused::Expired)
        );
        assert_eq!(held(&store), before, "nothing counted");
        assert_eq!(
            present(&store, &code("BCDF-GHJK"), &ada(), at(10)).expect("read"),
            Presented::Refused(Refused::Expired)
        );
    }

    #[test]
    fn no_code_is_not_asked_and_every_refusal_says_what_to_run() {
        let store = InMemoryPairings::default();
        let presented = present(&store, &code("BCDF-GHJK"), &ada(), at(0)).expect("read");
        assert_eq!(presented, Presented::Refused(Refused::NotAsked));
        for refused in [
            Refused::NotAsked,
            Refused::Expired,
            Refused::Used,
            Refused::Spent,
            Refused::Died,
        ] {
            let words = refused.said();
            assert!(words.contains(RUN), "{words}");
        }
    }

    #[test]
    fn a_pairing_reads_back_as_it_was_kept_and_holds_no_code() {
        let kept = Pairing::minted(&code("BCDF-GHJK"), Salt([3; 32]), &ada(), at(0));
        let text = serde_json::to_string(&kept).expect("printed");
        assert!(!text.contains("BCDF"), "{text}");
        assert_eq!(serde_json::from_str::<Pairing>(&text).expect("read"), kept);
        let spoiled = text.replace("\"left\":5", "\"left\":9");
        assert!(
            serde_json::from_str::<Pairing>(&spoiled).is_err(),
            "{spoiled}"
        );
    }
}
