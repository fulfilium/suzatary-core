//! What a removed mail account leaves for the box to delete.

/// What a removed account's card deletes after it answers: its sign-in whenever it holds one, its
/// mail on the card's wider yes; nothing to delete is no removal, so it has no arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    SignIn,
    Mail,
    Both,
}

impl Removal {
    /// For an account that holds a sign-in when `signs_in`, its mail too when `mail`; `None`
    /// when there is nothing to delete.
    #[must_use]
    pub fn of(signs_in: bool, mail: bool) -> Option<Self> {
        match (signs_in, mail) {
            (true, true) => Some(Self::Both),
            (true, false) => Some(Self::SignIn),
            (false, true) => Some(Self::Mail),
            (false, false) => None,
        }
    }

    #[must_use]
    pub fn next(self) -> &'static str {
        match self {
            Self::SignIn => "its sign-in is deleted next",
            Self::Mail => "its mail is deleted next",
            Self::Both => "its sign-in and its mail are deleted next",
        }
    }
}
