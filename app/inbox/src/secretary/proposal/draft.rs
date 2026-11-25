//! A message as proposed.

use serde::{Deserialize, Serialize};

/// A message as proposed: exactly the fields `suzatary send` takes, with no Bcc because there is
/// none anywhere. An attachment is recorded by source and digest, never its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Draft {
    /// The address to send as, one of this deployment's; the roster picks the transport at tap
    /// time.
    pub account: String,
    /// The visible recipients.
    pub to: Vec<String>,
    /// The visible copies.
    pub cc: Vec<String>,
    pub subject: String,
    /// The body, as plain text, like every outgoing message this program builds.
    pub body: String,
    /// The files it carries; omitted from the serde form when none, as every draft proposed
    /// before attachments was stored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<suzatary_held::secretary::attached::Attached>,
}

pub const EMAIL: prodrome_data::literal::codec::Constructor = (
    "Email",
    &["account", "to", "cc", "subject", "body", "attachments"],
);

// By hand: its reads refuse as `Fields` does.
impl prodrome_data::literal::codec::Literal for Draft {
    fn to_value(&self) -> prodrome_data::literal::Value {
        use prodrome_data::literal::codec::record;
        record(
            EMAIL,
            vec![
                self.account.to_value(),
                self.to.to_value(),
                self.cc.to_value(),
                self.subject.to_value(),
                self.body.to_value(),
                self.attachments.to_value(),
            ],
        )
    }

    fn from_value(
        value: &prodrome_data::literal::Value,
    ) -> Result<Self, prodrome_data::literal::ProdromeError> {
        let fields = prodrome_data::literal::codec::Fields::of(value, EMAIL)?;
        Ok(Draft {
            account: fields.get("account")?,
            to: fields.get("to")?,
            cc: fields.get("cc")?,
            subject: fields.get("subject")?,
            body: fields.get("body")?,
            attachments: fields.get("attachments")?,
        })
    }
}
