//! A message a reference names that this box no longer keeps; whatever cites it stays, and reads
//! it in these words.

/// What every reading of a dangling mail reference says, never an error of the reading's.
pub const SAID: &str = "this email is no longer stored";

/// A message no longer kept, with what a todo citing it recorded of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gone<'a> {
    pub subject: Option<&'a str>,
    pub sender: Option<&'a str>,
}

impl Gone<'_> {
    #[must_use]
    pub fn said(&self) -> String {
        match (known(self.subject), known(self.sender)) {
            (None, None) => SAID.to_owned(),
            (Some(subject), None) => format!("{SAID} (“{subject}”)"),
            (None, Some(sender)) => format!("{SAID} (from {sender})"),
            (Some(subject), Some(sender)) => format!("{SAID} (“{subject}” from {sender})"),
        }
    }
}

fn known(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|text| !text.is_empty())
}
