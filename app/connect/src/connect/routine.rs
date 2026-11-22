//! `[coding]` in the instance file: the routine a tap fires and the name its token lives under.

use suzatary_base::repo_ids::RepoName;

/// The secretspec name the routine's bearer token lives under; never read by this crate.
pub const ROUTINE_TOKEN: &str = "SUZATARY_ROUTINE_TOKEN";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coding {
    pub routine: RoutineId,
    /// The repository the routine's saved prompt names; the poll reads its branches.
    pub repo: RepoName,
}

/// Declares a newtype over `String` that parses through `$check`, refused as `$refused(text)`.
#[macro_export]
macro_rules! parsed {
    ($(#[$doc:meta])* $name:ident, $error:ty, $refused:expr, $check:expr) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = $error;
            fn try_from(text: String) -> Result<Self, Self::Error> {
                let check: fn(&str) -> bool = $check;
                if check(&text) { Ok(Self(text)) } else { Err($refused(text)) }
            }
        }

        impl std::str::FromStr for $name {
            type Err = $error;
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::try_from(text.to_owned())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a routine id is letters, digits, '_' and '-', got {0:?}")]
pub struct NotARoutine(String);

parsed!(
    /// The routine's id; it goes into the fire URL's path, so it is only `[A-Za-z0-9_-]`.
    RoutineId,
    NotARoutine,
    NotARoutine,
    |text| {
        !text.is_empty()
            && text.len() <= 128
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    }
);

impl RoutineId {
    /// The routine's own page, where its API trigger's token is generated.
    #[must_use]
    pub fn page(&self) -> String {
        format!("https://claude.ai/code/routines/{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::RoutineId;

    #[test]
    fn a_routine_id_is_a_path_segment() {
        assert!("trig_01XyZ-2".parse::<RoutineId>().is_ok());
        assert!("../fire".parse::<RoutineId>().is_err());
    }
}
