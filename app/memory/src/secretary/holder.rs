//! The registry of every store that holds a copy of a conversation's words.

/// A store holding a copy of a conversation's words. For each slot it kills,
/// `suzatary_lines::secretary::transcript::sweep` empties each [`Fate::Deleted`] holder in order; a
/// copy it could not delete keeps the key for the next sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// The words opened in memory.
    Opened,
    /// The pages the box drew from the model's answers.
    Renders,
    /// The model CLI's session files, by the session ids the lines record.
    ModelSessions,
    /// The keys that open the sealed words, destroyed last.
    Keys,
    /// The keys in the vault's daily snapshots, which open the data repository's ciphertext.
    Vault,
    /// The proposals and briefings a conversation led to, pushed in the clear.
    DataHistory,
    /// The lines a device sealed and has not handed over, with their keys, on that device.
    DeviceLines,
}

/// What an erasure leaves in a holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// The sweep deletes the copy before it destroys the key.
    Deleted,
    /// Out of the box's reach, for as long as this says.
    Remains(&'static str),
}

impl Holder {
    /// In the order the sweep empties them.
    pub const ALL: [Holder; 7] = [
        Holder::Opened,
        Holder::Renders,
        Holder::ModelSessions,
        Holder::Keys,
        Holder::Vault,
        Holder::DataHistory,
        Holder::DeviceLines,
    ];

    /// As the erase question names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Holder::Opened => "the words in memory",
            Holder::Renders => "the pages drawn from them",
            Holder::ModelSessions => "the model's session files",
            Holder::Keys => "the keys that open them",
            Holder::Vault => "the keys in the vault's encrypted snapshots on Drive",
            Holder::DataHistory => {
                "the proposals and briefings it led to, in the data repository's history"
            }
            Holder::DeviceLines => "the lines a device wrote and has not handed over yet",
        }
    }

    #[must_use]
    pub const fn fate(self) -> Fate {
        match self {
            Holder::Opened | Holder::Renders | Holder::ModelSessions | Holder::Keys => {
                Fate::Deleted
            }
            // `scripts/vault-backup.sh` keeps seven daily snapshots.
            Holder::Vault => Fate::Remains("for up to seven days"),
            Holder::DataHistory => Fate::Remains("until that history is rewritten"),
            // `web/src/prodrome/ownlines.ts`: kept until the box answers them; a revoked
            // device's wipe deletes them with every database of the origin.
            Holder::DeviceLines => {
                Fate::Remains("on that device until it hands them over, or its wipe")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::secretary::erasure::Erasure;

    /// Law: the erase question names every holder, and how long each copy out of the box's reach
    /// stays.
    #[test]
    fn the_erase_question_names_every_holder_and_what_remains() {
        let asked = Erasure::for_good();
        for holder in super::Holder::ALL {
            let named = match holder.fate() {
                super::Fate::Deleted => holder.name().to_owned(),
                super::Fate::Remains(until) => format!("{}, {until}", holder.name()),
            };
            assert!(asked.contains(&named), "{named} is not in: {asked}");
        }
    }
}
