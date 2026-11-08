//! A grant a card's effect needs and the box does not hold: why a tap wrote or sent nothing.
//! Stored on the try it answers, as one text literal: `<grant>:<account>`, or `google-client`.

use std::fmt;
use std::str::FromStr;

use super::address::EmailAddress;

/// What the box lacks to perform a card's effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lacking {
    /// `account` holds no `grant`.
    Grant {
        /// The account the effect acts as.
        account: EmailAddress,
        /// What it may not do yet.
        grant: Grant,
    },
    /// The box keeps no Google client, so it can ask Google for no account's grant.
    GoogleClient,
}

/// One grant an account gives the box, by what it lets the box do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// Write the Google account's calendar: a sign-in with Google, which an account the box reads
    /// through lieer alone has not given.
    GoogleCalendar,
    /// Send as the Google account: a sign-in with Google.
    GoogleSend,
    /// Send as the Microsoft account: its send sign-in, kept apart from its mail's.
    MicrosoftSend,
}

impl Grant {
    /// Every grant.
    pub const ALL: [Grant; 3] = [
        Grant::GoogleCalendar,
        Grant::GoogleSend,
        Grant::MicrosoftSend,
    ];

    /// The word its literal begins with.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Grant::GoogleCalendar => "google-calendar",
            Grant::GoogleSend => "google-send",
            Grant::MicrosoftSend => "microsoft-send",
        }
    }

    /// What the grant lets the box do, as a sentence ends with it.
    const fn lets(self) -> &'static str {
        match self {
            Grant::GoogleCalendar => "write its calendar",
            Grant::GoogleSend | Grant::MicrosoftSend => "send its mail",
        }
    }
}

/// The literal of the client's lack.
const GOOGLE_CLIENT: &str = "google-client";

impl Lacking {
    /// What the card says: that nothing happened, what to connect and where, and that a tap
    /// tries again once it is connected.
    #[must_use]
    pub fn sentence(&self) -> String {
        let step = match self {
            Lacking::Grant {
                account,
                grant: grant @ (Grant::GoogleCalendar | Grant::GoogleSend),
            } => format!(
                "connect {account} as a Google account in Settings to let the box {}",
                grant.lets()
            ),
            Lacking::Grant {
                account,
                grant: grant @ Grant::MicrosoftSend,
            } => format!(
                "sign {account} in again in Settings to let the box {}",
                grant.lets()
            ),
            Lacking::GoogleClient => {
                "set up Google in Settings: the box keeps no Google client to ask for the grant"
                    .to_owned()
            }
        };
        format!("Nothing was done: {step}. Then tap Try again.")
    }

    /// What the button that begins the fix says.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Lacking::Grant { account, .. } => format!("Connect {account}"),
            Lacking::GoogleClient => "Set up Google".to_owned(),
        }
    }
}

impl fmt::Display for Lacking {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lacking::Grant { account, grant } => write!(f, "{}:{account}", grant.as_str()),
            Lacking::GoogleClient => f.write_str(GOOGLE_CLIENT),
        }
    }
}

/// A stored lack that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a lacked grant is <grant>:<account> or {GOOGLE_CLIENT}, got {0:?}")]
pub struct NotALack(pub String);

impl FromStr for Lacking {
    type Err = NotALack;

    fn from_str(text: &str) -> Result<Self, NotALack> {
        if text == GOOGLE_CLIENT {
            return Ok(Lacking::GoogleClient);
        }
        let refused = || NotALack(text.to_owned());
        let (word, account) = text.split_once(':').ok_or_else(refused)?;
        let grant = crate::words::of_word(Grant::ALL, Grant::as_str, word).ok_or_else(refused)?;
        let account = EmailAddress::new(account).map_err(|_| refused())?;
        Ok(Lacking::Grant { account, grant })
    }
}

impl TryFrom<String> for Lacking {
    type Error = NotALack;

    fn try_from(text: String) -> Result<Self, NotALack> {
        text.parse()
    }
}

prodrome_core::text_literal!(
    Lacking => ToString::to_string,
);

#[cfg(test)]
mod tests {
    use super::*;

    /// Every lack reads back from the literal it prints, and a literal of no lack is refused.
    #[test]
    fn a_lack_reads_back_from_its_literal() {
        let account = EmailAddress::new("ada@example.test").expect("an address");
        let every = Grant::ALL
            .into_iter()
            .map(|grant| Lacking::Grant {
                account: account.clone(),
                grant,
            })
            .chain([Lacking::GoogleClient]);
        for lack in every {
            assert_eq!(
                lack.to_string().parse::<Lacking>(),
                Ok(lack.clone()),
                "{lack}"
            );
        }
        for bad in [
            "",
            "google-calendar",
            "google-calendar:",
            "google-calendar:not an address",
            "calendar:ada@example.test",
            "google-client:ada@example.test",
        ] {
            assert!(bad.parse::<Lacking>().is_err(), "{bad:?}");
        }
    }
}
