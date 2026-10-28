//! The last pairing code's state on disk, as [`Pairings`]: one private file per instance,
//! box-local and never pushed, read and replaced under a lock beside it, so `suzatary device
//! pair` and the server's tries never interleave. It holds the code's keyed hash, never the code.

use std::path::PathBuf;

use suzatary_core::signatures::{Code, Pairing, Pairings as Kept, Salt};
use suzatary_core::store::StoreError;

use crate::docs::{DocCodec, Docs, Locked};

/// The pairing file and its lock.
#[derive(Debug, Clone)]
pub struct Pairings(Locked<Pairing>);

impl Pairings {
    /// The file at `path`, its lock beside it.
    #[must_use]
    pub fn at(path: PathBuf) -> Self {
        let lock = path.with_extension("lock");
        let docs = Docs::new("pairing", move |()| path.clone(), &DocCodec::JSON).private();
        Self(Locked::new(docs, lock))
    }
}

impl Kept for Pairings {
    fn rewrite(
        &self,
        step: &mut dyn FnMut(Option<&Pairing>) -> Option<Pairing>,
    ) -> Result<(), StoreError> {
        self.0.rewrite(step).map(|_| ())
    }
}

/// A fresh code and the key of its hash, from the system's generator.
///
/// # Errors
/// The generator failing.
pub fn fresh() -> Result<(Code, Salt), String> {
    let random = || -> Result<[u8; 32], String> {
        ring::rand::generate(&ring::rand::SystemRandom::new())
            .map(ring::rand::Random::expose)
            .map_err(|_| "the system's random generator failed".to_owned())
    };
    // 32 bytes hold 8 fair ones but for a chance below 10^-20; drawing again costs nothing.
    let code = loop {
        if let Some(code) = Code::drawn(&random()?) {
            break code;
        }
    };
    Ok((code, Salt(random()?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ada() -> prodrome_core::event::Actor {
        prodrome_core::event::Actor::new("ada").expect("a roster name")
    }

    #[test]
    fn a_pairing_is_kept_private_and_holds_no_code() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("pairing").join("one.json");
        let store = Pairings::at(path.clone());
        let (code, salt) = fresh().expect("random bytes");
        let kept = Pairing::minted(&code, salt, &ada(), chrono::Utc::now());
        store.rewrite(&mut |_| Some(kept.clone())).expect("kept");
        let mut read = None;
        store
            .rewrite(&mut |held| {
                read = held.cloned();
                None
            })
            .expect("read");
        assert_eq!(read, Some(kept));
        let mode = std::fs::metadata(&path)
            .expect("there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let text = std::fs::read_to_string(&path).expect("there");
        assert!(!text.contains(&code.shown()), "the file holds no code");
    }

    /// A second `device pair` replaces the first code: the first then reads as a wrong try, and
    /// the second matches.
    #[test]
    fn a_new_code_replaces_the_last() {
        use suzatary_core::signatures::{Presented, Refused, TRIES, present};
        let dir = tempfile::tempdir().expect("a temp dir");
        let store = Pairings::at(dir.path().join("one.json"));
        let now = chrono::Utc::now();
        let minted = || {
            let (code, salt) = fresh().expect("random bytes");
            let pairing = Pairing::minted(&code, salt, &ada(), now);
            store.rewrite(&mut |_| Some(pairing.clone())).expect("kept");
            code
        };
        let (first, second) = (minted(), minted());
        assert_eq!(
            present(&store, &first, &ada(), now).expect("read"),
            Presented::Refused(Refused::Wrong { left: TRIES - 1 })
        );
        assert_eq!(
            present(&store, &second, &ada(), now).expect("read"),
            Presented::Matched
        );
    }

    /// Two writers on one file, as the terminal and the server are: of eight tries of the right
    /// code at once, one matches and every other finds it used.
    #[test]
    fn of_tries_at_once_through_two_handles_one_matches() {
        use suzatary_core::signatures::{Presented, Refused, present};
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("one.json");
        let (code, salt) = fresh().expect("random bytes");
        let now = chrono::Utc::now();
        let minted = Pairing::minted(&code, salt, &ada(), now);
        Pairings::at(path.clone())
            .rewrite(&mut |_| Some(minted.clone()))
            .expect("kept");
        let answers: Vec<Presented> = std::thread::scope(|scope| {
            let tries: Vec<_> = (0..8)
                .map(|_| {
                    let (path, code) = (path.clone(), code.clone());
                    scope.spawn(move || {
                        present(&Pairings::at(path), &code, &ada(), now).expect("read")
                    })
                })
                .collect();
            tries
                .into_iter()
                .map(|one| one.join().expect("no panic"))
                .collect()
        });
        let matched = answers.iter().filter(|a| **a == Presented::Matched).count();
        let used = answers
            .iter()
            .filter(|a| **a == Presented::Refused(Refused::Used))
            .count();
        assert_eq!((matched, used), (1, 7), "{answers:?}");
    }
}
