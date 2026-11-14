//! The phones that asked to be woken: one `<actor>.tsv` per actor of `platform`, `token`,
//! `registered-at` lines, interpreted by [`Lines`].
//!
//! The format is fixed: a change would orphan every registered phone. A ragged line is dropped,
//! costing one phone a re-registration rather than the file.

use std::path::PathBuf;

use prodrome_core::fpl::{datetime_of, instant_of, iso, parse_iso};
use suzatary_signatures::devices::Device;

use crate::lines::{Codec, Layout, Lines};

/// The phones, one log per actor.
pub type Devices = Lines<String, Device>;

impl Devices {
    #[must_use]
    pub fn open(dir: PathBuf) -> Self {
        Lines::new(dir, &LAYOUT, &CODEC)
    }
}

/// An actor's file is named for it through the one door, so a stem is always its own key.
const LAYOUT: Layout<String> = Layout {
    what: "device log",
    store: "device store",
    extension: "tsv",
    stem: |actor| safe_actor(actor),
    key: |stem| Some(stem.to_owned()),
    lock: None,
    durability: crate::lines::Durability::Written,
};

/// A tab-separated line; a tab or newline inside a field would forge the separators.
const CODEC: Codec<Device> = Codec {
    what: "a device",
    print: |device| {
        if [&device.platform, &device.token]
            .iter()
            .any(|field| field.contains(['\t', '\n']))
        {
            return Err("a device token is one line".to_owned());
        }
        Ok(format!(
            "{}\t{}\t{}",
            device.platform,
            device.token,
            iso(instant_of(device.registered))
        ))
    },
    parse: |line| {
        let parts: Vec<&str> = line.split('\t').collect();
        let [platform, token, at] = parts.as_slice() else {
            return Ok(None);
        };
        Ok(parse_iso(at)
            .ok()
            .and_then(|instant| datetime_of(instant).ok())
            .map(|registered| Device {
                platform: (*platform).to_owned(),
                token: (*token).to_owned(),
                registered,
            }))
    },
};

/// An actor name made safe for a file name.
fn safe_actor(actor: &str) -> String {
    let mapped: String = actor
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if mapped.is_empty() {
        "_".to_string()
    } else {
        mapped
    }
}

#[cfg(test)]
mod tests {
    use suzatary_base::capability::Datetime;
    use suzatary_signatures::devices::{self, Phones};
    use suzatary_store::store::{Edit, Log as _};

    use super::*;

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn register(store: &Devices, actor: &str, platform: &str, token: &str, day: u32) {
        let line = Device {
            platform: platform.to_owned(),
            token: token.to_owned(),
            registered: at(day),
        };
        store
            .edit(&actor.to_owned(), &mut |_| Ok(Edit::Push(line.clone())))
            .expect("registers");
    }

    fn phones(store: &Devices, actor: &str) -> Vec<Device> {
        Phones::of(&store.lines_if_any(&actor.to_owned()).expect("reads")).0
    }

    #[test]
    fn register_then_read_round_trips() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Devices::open(dir.path().to_path_buf());
        register(&store, "ada", "android", "tok-1", 1);
        let read = phones(&store, "ada");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].platform, "android");
        assert_eq!(read[0].token, "tok-1");
        assert_eq!(read[0].registered, at(1));
    }

    #[test]
    fn a_ragged_line_costs_one_device_and_not_the_file() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Devices::open(dir.path().to_path_buf());
        register(&store, "ada", "android", "tok-1", 1);
        let path = store.path(&"ada".to_owned());
        let mut text = std::fs::read_to_string(&path).expect("reads");
        text.push_str("only\ttwo\n");
        text.push_str("android\ttok-2\tnot-a-date\n");
        std::fs::write(&path, text).expect("writes");
        let read = phones(&store, "ada");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].token, "tok-1");
    }

    #[test]
    fn a_tab_in_a_token_is_refused() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Devices::open(dir.path().to_path_buf());
        let line = |platform: &str, token: &str| Device {
            platform: platform.to_owned(),
            token: token.to_owned(),
            registered: at(1),
        };
        for bad in [line("android", "tok\t1"), line("android\t", "tok-1")] {
            assert!(
                store
                    .edit(&"ada".to_owned(), &mut |_| Ok(Edit::Push(bad.clone())))
                    .is_err()
            );
        }
    }

    #[test]
    fn everyone_finds_two_actors_by_their_file_names() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Devices::open(dir.path().to_path_buf());
        register(&store, "ada", "android", "tok-1", 1);
        register(&store, "tri age", "ios", "tok-2", 1);
        let all = devices::everyone(&store).expect("reads");
        assert_eq!(
            all.keys().cloned().collect::<Vec<_>>(),
            vec!["ada".to_owned(), "tri_age".to_owned()]
        );
    }
}
