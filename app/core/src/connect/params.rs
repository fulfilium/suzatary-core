//! What a person chooses for one connection.

use super::{Label, Refused};
use crate::drive::DrivePath;
use crate::mail::account::Authority;
use crate::mail::address::EmailAddress;

/// What a template reads, a function of the table it writes, so a template cannot read one
/// shape and write another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Nothing to choose.
    Nothing,
    /// A label.
    Label,
    /// An email address.
    Address,
    /// A Microsoft account's address and tenant.
    Microsoft,
    /// A Drive folder.
    Folder,
}

impl Shape {
    /// What it reads, in words a refusal says.
    #[must_use]
    pub const fn wanted(self) -> &'static str {
        match self {
            Shape::Nothing => "nothing",
            Shape::Label => "a label",
            Shape::Address => "an email address",
            Shape::Microsoft => "an email address and a tenant (organizations or consumers)",
            Shape::Folder => "a Drive folder",
        }
    }

    /// What to reply to choose it, as the setup conversation asks.
    #[must_use]
    pub const fn asked(self) -> &'static str {
        match self {
            Shape::Nothing => "reply to go ahead, and the card that connects it comes next",
            Shape::Label => {
                "reply with a name for it, such as Work, and the card that connects it comes next"
            }
            Shape::Address => {
                "reply with its email address, and the card that connects it comes next"
            }
            Shape::Microsoft => {
                "reply with its email address and whether it is a work or school account or a \
                 personal one, and the card that connects it comes next"
            }
            Shape::Folder => {
                "reply with the folder's path in your Drive, such as /Documents/Taxes, and the \
                 card that lets the secretary read it comes next"
            }
        }
    }
}

/// The parameters of one connection, each parsed by its own constructor: a person or a model
/// chooses these and nothing else, so no value that grants anything is typed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Params {
    /// Nothing chosen.
    Nothing,
    /// A calendar's label.
    Label(Label),
    /// A mail account's address.
    Address(EmailAddress),
    /// A Microsoft account's address and the tenant it signs in with.
    Microsoft {
        /// The address.
        email: EmailAddress,
        /// The tenant.
        tenant: Authority,
    },
    /// A folder of the Drive.
    Folder(DrivePath),
}

impl Params {
    /// Its shape.
    #[must_use]
    pub const fn shape(&self) -> Shape {
        match self {
            Params::Nothing => Shape::Nothing,
            Params::Label(_) => Shape::Label,
            Params::Address(_) => Shape::Address,
            Params::Microsoft { .. } => Shape::Microsoft,
            Params::Folder(_) => Shape::Folder,
        }
    }

    /// Its text form, one string per parameter in order: what a stored act and a verb's
    /// arguments carry, read back by [`Params::parse`].
    #[must_use]
    pub fn texts(&self) -> Vec<String> {
        match self {
            Params::Nothing => Vec::new(),
            Params::Label(label) => vec![label.shown()],
            Params::Address(email) => vec![email.as_str().to_owned()],
            Params::Microsoft { email, tenant } => {
                vec![email.as_str().to_owned(), tenant.as_str().to_owned()]
            }
            Params::Folder(folder) => vec![folder.to_string()],
        }
    }

    /// Read `texts` as parameters of `shape`, each through its constructor.
    ///
    /// # Errors
    /// [`Refused::Param`] naming the parameter that did not parse, or a count of texts the
    /// shape does not take.
    pub fn parse(shape: Shape, template: &'static str, texts: &[String]) -> Result<Self, Refused> {
        let email = |text: &str| {
            EmailAddress::new(text).map_err(|refused| Refused::Param {
                param: "email",
                said: refused.message().to_owned(),
            })
        };
        let wrong = || Refused::Params {
            template,
            wanted: shape.wanted(),
        };
        Ok(match (shape, texts) {
            (Shape::Nothing, []) => Params::Nothing,
            (Shape::Label, [label]) => Params::Label(Label::parse(label)?),
            (Shape::Address, [address]) => Params::Address(email(address)?),
            (Shape::Microsoft, [address, tenant]) => Params::Microsoft {
                email: email(address)?,
                tenant: Authority::parse(tenant).map_err(|refused| Refused::Param {
                    param: "tenant",
                    said: refused.to_string(),
                })?,
            },
            (Shape::Folder, [folder]) => Params::Folder(folder.parse().map_err(
                |refused: crate::drive::DrivePathError| Refused::Param {
                    param: "folder",
                    said: refused.to_string(),
                },
            )?),
            _ => return Err(wrong()),
        })
    }
}
