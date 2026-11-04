//! The box's epochs on disk (`suzatary_core::epoch`), one text file per store in the state
//! directory. Never pushed (a history rewrite must not take an epoch back) and never backed up
//! (a restored box must start its epochs as a fresh one does). Format: the epoch on the first
//! line, then `genesis <name>` and `head <name>` lines; any other line is refused.

use std::collections::BTreeSet;
use std::path::PathBuf;

use suzatary_core::epoch::{Epochs, Served};

use crate::docs::{DocCodec, Docs};

/// The epochs under `root`, one file per store, its name the store's with `/` spelled `-`.
#[must_use]
pub fn open(root: PathBuf) -> Epochs {
    Epochs::kept(Docs::new(
        "epochs",
        move |store: &String| root.join(store.replace('/', "-")),
        &CODEC,
    ))
}

const CODEC: DocCodec<String, Served> = DocCodec {
    print: |_, served| {
        let lines = served.geneses.iter().map(|g| format!("genesis {g}\n"));
        let lines = lines.chain(served.heads.iter().map(|h| format!("head {h}\n")));
        Ok(std::iter::once(format!("{}\n", served.epoch))
            .chain(lines)
            .collect::<String>()
            .into_bytes())
    },
    parse: |_, bytes| {
        let text = String::from_utf8(bytes).map_err(|failed| failed.to_string())?;
        let mut lines = text.lines();
        let epoch = lines
            .next()
            .ok_or("an empty epoch file")?
            .parse()
            .map_err(|failed: std::num::ParseIntError| failed.to_string())?;
        let mut served = Served {
            epoch,
            geneses: BTreeSet::new(),
            heads: BTreeSet::new(),
        };
        for line in lines {
            match line.split_once(' ') {
                Some(("genesis", name)) => served.geneses.insert(name.to_owned()),
                Some(("head", name)) => served.heads.insert(name.to_owned()),
                _ => return Err(format!("not an epoch line: {line:?}")),
            };
        }
        Ok(Some(served))
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::epoch::Seen;

    #[test]
    fn an_epoch_outlives_the_process_that_kept_it() {
        let dir = std::env::temp_dir().join(format!("suzatary-epochs-{}", std::process::id()));
        let now =
            suzatary_core::capability::Datetime::new(2026, 10, 7, 12, 0, 0, 0).expect("an instant");
        let one = |names: &[&str]| names.iter().map(|&n| n.to_owned()).collect::<BTreeSet<_>>();
        let seen = Seen {
            geneses: one(&["a"]),
            heads: one(&["x"]),
            held: std::sync::Arc::new(one(&["x"])),
        };
        let first = open(dir.clone())
            .served("store/inbox", now, || Ok((seen.clone(), ())))
            .expect("served")
            .0;
        assert_eq!(
            open(dir.clone())
                .served("store/inbox", now, || Ok((seen.clone(), ())))
                .expect("served")
                .0,
            first
        );
        let back = Seen {
            held: std::sync::Arc::default(),
            heads: BTreeSet::new(),
            ..seen.clone()
        };
        let moved = open(dir.clone())
            .served("store/inbox", now, || Ok((back.clone(), ())))
            .expect("served")
            .0;
        assert_eq!(
            moved,
            first + 1,
            "the head it answered last is gone: a rollback"
        );
        assert!(dir.join("store-inbox").exists());
        std::fs::remove_dir_all(dir).expect("cleaned");
    }

    #[test]
    fn a_file_of_the_first_format_is_refused() {
        let dir = std::env::temp_dir().join(format!("suzatary-epochs-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory");
        std::fs::write(dir.join("chain-life"), "7\nabc123\n").expect("an old file");
        let now =
            suzatary_core::capability::Datetime::new(2026, 10, 7, 12, 0, 0, 0).expect("an instant");
        assert!(
            open(dir.clone())
                .served("chain/life", now, || Ok((Seen::default(), ())))
                .is_err()
        );
        std::fs::remove_dir_all(dir).expect("cleaned");
    }
}
