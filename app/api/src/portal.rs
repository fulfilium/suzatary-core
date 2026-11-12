//! What the Android portal reads from the box it is pointed at, before it opens the app.

use suzatary_core::connect::Channel;
use suzatary_core::nonempty::NonEmpty;

/// The box's answer to a portal at pairing: how the box wakes the phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Portal {
    /// The box wakes no phone through Firebase, so the portal registers for no push.
    Quiet,
    /// The box wakes phones through Firebase, and the portal registers with this sender.
    Fcm(Sender),
}

impl Portal {
    /// The answer for a box that pushes through `channels` and declares `sender`: a sender
    /// the box does not push as is no answer, since a phone registered with it is never woken.
    #[must_use]
    pub fn of(channels: Option<&NonEmpty<Channel>>, sender: Option<Sender>) -> Self {
        let pushes = NonEmpty::listed(channels).contains(&Channel::Fcm);
        match sender {
            Some(sender) if pushes => Self::Fcm(sender),
            Some(_) | None => Self::Quiet,
        }
    }
}

/// A Firebase project's number, which Play Services registers a phone against. It names who may
/// send to the phone and is not a secret: only the service account's key can send.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sender(String);

impl Sender {
    /// The longest number accepted; Firebase's are twelve digits.
    const MOST: usize = 20;

    /// The number as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Sender {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        if (1..=Self::MOST).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit()) {
            Ok(Self(text))
        } else {
            Err(format!(
                "{} is no Firebase sender: a sender is the project's number, 1 to {} digits",
                suzatary_base::refusal::shown(&text),
                Self::MOST
            ))
        }
    }
}

impl From<Sender> for String {
    fn from(sender: Sender) -> Self {
        sender.0
    }
}
