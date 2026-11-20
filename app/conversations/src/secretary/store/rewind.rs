//! What asking again keeps of the last round.

/// What asking again keeps of the last round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum Rewind {
    /// The question stands and is pending again; the trace and answer go.
    Regenerate,
    /// The question goes too; the new one is appended after the rewind.
    Edit,
}

impl Rewind {
    pub const ALL: [Rewind; 2] = [Rewind::Regenerate, Rewind::Edit];

    /// Its printed name, as a stored object spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Rewind::Regenerate => "regenerate",
            Rewind::Edit => "edit",
        }
    }
}

prodrome_core::word_literal!(Rewind: Rewind::ALL => |how| how.as_str());
