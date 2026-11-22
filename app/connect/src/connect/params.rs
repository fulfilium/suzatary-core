//! What a person chooses for one connection.

use suzatary_base::drive_path::DrivePath;
use suzatary_health::health::{Label, Refused};
use suzatary_mail::mail::account::Authority;
use suzatary_mail::mail::address::EmailAddress;

/// What a template reads, derived from the table it writes, so a template cannot read one shape
/// and write another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Nothing,
    Label,
    Address,
    /// A Microsoft account's address and tenant.
    Microsoft,
    Folder,
}

impl Shape {
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

/// The parameters of one connection: a person or a model chooses these, so none of them holds a
/// value that grants access.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Params {
    Nothing,
    Label(Label),
    Address(EmailAddress),
    Microsoft {
        email: EmailAddress,
        tenant: Authority,
    },
    Folder(DrivePath),
}

impl Params {
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

    /// One string per parameter, as a stored act carries them; [`Params::parse`] reads them back.
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

    /// # Errors
    /// [`Refused::Param`] naming a parameter that did not parse, or [`Refused::Params`] when
    /// `shape` does not take that many texts.
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
                |refused: suzatary_base::drive_path::DrivePathError| Refused::Param {
                    param: "folder",
                    said: refused.to_string(),
                },
            )?),
            _ => return Err(wrong()),
        })
    }
}
