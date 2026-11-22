//! What revoking a removed account's sign-ins came to; [`crate::mail::cache::Forget`] does it.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Revoked {
    NoneKept,
    /// Every grant it held (reading and sending) was deleted.
    Deleted,
}

impl Revoked {
    #[must_use]
    pub const fn sentence(&self) -> &'static str {
        match self {
            Revoked::NoneKept => "it held no sign-in on this box",
            Revoked::Deleted => "forgot its sign-in to read and send mail",
        }
    }
}
