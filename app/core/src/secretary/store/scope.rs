//! Which turns an ask sends its model.

/// Which turns an ask sends its model: a window of the last live turns (a stateless model, or a
/// session opening and every delta it is resumed with since), or every stored turn (a job told no
/// memory, which is sent its whole transcript each time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// [`queue::for_model`](crate::secretary::queue::for_model)'s window, and every turn sent to a
    /// session kept since.
    #[default]
    Window,
    /// Every stored turn, set aside or not.
    Transcript,
}

impl Scope {
    /// Every scope.
    pub const ALL: [Scope; 2] = [Scope::Window, Scope::Transcript];

    /// The name a stored line prints.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Window => "window",
            Scope::Transcript => "transcript",
        }
    }

    /// The scope `text` names; `None` for anything but the two names.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Scope::ALL.into_iter().find(|scope| scope.as_str() == text)
    }
}

// Its literal and its serde form are its word, as a stored line prints it.
crate::words::words!(Scope: Scope::ALL => |scope| scope.as_str());
