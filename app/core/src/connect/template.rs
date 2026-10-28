//! A template: how one kind of connection is made, as data.

use super::{Check, Group, Params, Refused, Shape, StepShape};
use crate::mail::account::{Authority, MailAccount, MsImap, Proton, Sync, slug_of};
use crate::mail::account_email::AccountEmail;
use crate::mail::folders::Folders;
use crate::plugin::{Manifest, ManifestError, PluginName};

/// A template: its group, the table it writes, the steps a person takes and the check that
/// proves it. Every value that grants anything (an origin, a secret's name, a help link, a
/// table) is here or derived from a parameter its constructor parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    /// Which it is.
    pub id: TemplateId,
    /// Where the overview draws its connections.
    pub group: Group,
    /// What it connects, as a card names it.
    pub title: &'static str,
    /// The table it writes.
    pub table: Table,
    /// The person's steps, in order.
    pub steps: &'static [StepShape],
    /// How a connection of it proves it works.
    pub check: Check,
}

impl Template {
    /// What a connection of it reads.
    #[must_use]
    pub const fn shape(&self) -> Shape {
        self.table.shape()
    }
}

/// The table of the instance file a template writes, from a closed vocabulary: each arm is
/// interpreted once, and every template is data over them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// None: every deployment has exactly one, its data repository.
    Always,
    /// None: the deployment's model, which every deployment runs.
    Model,
    /// A `url` credential its label names on the fragment's plugin, whose manifest is declared
    /// from the fragment when the instance has none.
    Link(Fragment),
    /// A `[[mail.accounts]]` entry of this kind.
    Account(AccountKind),
    /// A folder `[secretary] drive_allow` lets the Drive be read under.
    Folder,
    /// A `[notify]` channel.
    Channel(Channel),
}

/// A channel that wakes the owner's phones: `[notify] channels` is read into these once, where
/// the instance file is read, and a name that is none of them stops the instance from loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    /// Firebase Cloud Messaging, by a service account.
    Fcm,
    /// ntfy, by a topic.
    Ntfy,
}

impl Channel {
    /// Its name in `[notify] channels`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Channel::Ntfy => "ntfy",
            Channel::Fcm => "fcm",
        }
    }

    /// Every channel, in the order the Settings page lists them.
    pub const ALL: [Channel; 2] = [Channel::Fcm, Channel::Ntfy];

    /// The channel `[notify] channels` names `name`, or `None` for a name none is.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|channel| channel.as_str() == name)
    }

    /// The template that connects it.
    #[must_use]
    pub const fn template(self) -> TemplateId {
        match self {
            Channel::Ntfy => TemplateId::Ntfy,
            Channel::Fcm => TemplateId::Fcm,
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl serde::Serialize for Channel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Channel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::parse(&name).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "{name:?} is no notification channel; the channels are {}",
                Self::ALL.map(Channel::as_str).join(", ")
            ))
        })
    }
}

impl Table {
    /// What a connection writing it reads.
    #[must_use]
    pub const fn shape(&self) -> Shape {
        match self {
            Table::Always | Table::Model | Table::Channel(_) => Shape::Nothing,
            Table::Link(_) => Shape::Label,
            Table::Account(AccountKind::Microsoft) => Shape::Microsoft,
            Table::Account(AccountKind::Gmail | AccountKind::Google | AccountKind::Proton) => {
                Shape::Address
            }
            Table::Folder => Shape::Folder,
        }
    }
}

/// The kinds of mail account the instance holds, one per sync strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    /// Gmail, through lieer and its own sign-in.
    Gmail,
    /// A Google account signed in from the box's card: its mail through lieer, under the grant
    /// the box keeps, and its calendars through Google's Calendar API.
    Google,
    /// Microsoft, over IMAP and XOAUTH2.
    Microsoft,
    /// Proton, through its Bridge.
    Proton,
}

impl AccountKind {
    /// The kind an account's strategy is.
    #[must_use]
    pub const fn of(sync: &Sync) -> Self {
        match sync {
            Sync::Gmail => AccountKind::Gmail,
            Sync::Google => AccountKind::Google,
            Sync::MsImap(_) => AccountKind::Microsoft,
            Sync::Proton(_) => AccountKind::Proton,
        }
    }

    /// The template that writes an account of this kind.
    #[must_use]
    pub const fn template(self) -> TemplateId {
        match self {
            AccountKind::Gmail => TemplateId::Gmail,
            AccountKind::Google => TemplateId::Google,
            AccountKind::Microsoft => TemplateId::MsImap,
            AccountKind::Proton => TemplateId::Proton,
        }
    }

    /// The roster entry for `params`: its maildir folder named by the address, and this kind's
    /// defaults.
    ///
    /// # Errors
    /// Parameters of another shape.
    pub fn account(self, params: &Params) -> Result<MailAccount, Refused> {
        let (email, sync) = match (self, params) {
            (AccountKind::Gmail, Params::Address(email)) => (email, Sync::Gmail),
            (AccountKind::Google, Params::Address(email)) => (email, Sync::Google),
            (AccountKind::Microsoft, Params::Microsoft { email, tenant }) => (
                email,
                Sync::MsImap(MsImap {
                    authority: *tenant,
                    inbox_folders: Folders::inbox(),
                    sent_folder: match tenant {
                        Authority::Organizations => "Sent Items",
                        Authority::Consumers => "Sent",
                    }
                    .to_owned(),
                }),
            ),
            (AccountKind::Proton, Params::Address(email)) => (
                email,
                Sync::Proton(Proton {
                    channel: "proton".to_owned(),
                    smtp_user: email.as_str().to_owned(),
                    send_addresses: vec![email.as_str().to_owned()],
                    inbox_folders: Folders::inbox(),
                }),
            ),
            _ => {
                return Err(Refused::Params {
                    template: "a mail account",
                    wanted: Table::Account(self).shape().wanted(),
                });
            }
        };
        let email = AccountEmail::folded(email);
        Ok(MailAccount {
            subdir: slug_of(email.as_str()),
            email,
            sync,
        })
    }

    /// The parameters an account of this kind was written from.
    #[must_use]
    pub fn params(account: &MailAccount) -> Params {
        let email = account.email.address().clone();
        match &account.sync {
            Sync::Gmail | Sync::Google | Sync::Proton(_) => Params::Address(email),
            Sync::MsImap(ms) => Params::Microsoft {
                email,
                tenant: ms.authority,
            },
        }
    }
}

/// The manifest a calendar-like template declares when the instance has none of its plugin:
/// an app bundle and the origins it may reach from the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fragment {
    /// The plugin, and its credentials' namespace.
    pub plugin: &'static PluginName,
    /// Its component, under the app's bundles.
    pub component: &'static str,
    /// Its schema, beside the component.
    pub schema: &'static str,
    /// The origins it may reach before any grant.
    pub origins: &'static [&'static str],
}

impl Fragment {
    /// Its manifest, through the door the file's tables go through.
    ///
    /// # Errors
    /// The refusal the file's table would get.
    pub fn manifest(&self) -> Result<Manifest, ManifestError> {
        Manifest::declare(
            self.plugin.as_str(),
            self.component,
            self.schema,
            self.origins,
        )
    }
}

/// One template of the catalog, by its closed name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemplateId {
    /// The data repository, pushed to a git remote.
    GitRemote,
    /// A Proton Drive folder, through its command line.
    DriveCli,
    /// A calendar, by its secret iCal link.
    Ics,
    /// Gmail, through lieer.
    Gmail,
    /// A Google account: its mail and calendars, by one sign-in with Google.
    Google,
    /// Microsoft 365 or Outlook, by the device-code sign-in.
    MsImap,
    /// Proton Mail, through its Bridge.
    Proton,
    /// The deployment's model, signed in from a card.
    Model,
    /// Notifications through ntfy.
    Ntfy,
    /// Notifications through Firebase.
    Fcm,
}

impl TemplateId {
    /// Every template, in the catalog's order.
    pub const ALL: [TemplateId; 10] = [
        TemplateId::GitRemote,
        TemplateId::DriveCli,
        TemplateId::Ics,
        TemplateId::Gmail,
        TemplateId::Google,
        TemplateId::MsImap,
        TemplateId::Proton,
        TemplateId::Model,
        TemplateId::Ntfy,
        TemplateId::Fcm,
    ];

    /// Its name, as a stored act and a verb spell it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TemplateId::GitRemote => "git-remote",
            TemplateId::DriveCli => "drive-cli",
            TemplateId::Ics => "ics",
            TemplateId::Gmail => "lieer",
            TemplateId::Google => "google",
            TemplateId::MsImap => "imap-device-code",
            TemplateId::Proton => "bridge",
            TemplateId::Model => "model",
            TemplateId::Ntfy => "ntfy",
            TemplateId::Fcm => "fcm",
        }
    }

    /// The template its name spells.
    ///
    /// # Errors
    /// [`Refused::Template`] for a name no template has.
    pub fn parse(text: &str) -> Result<Self, Refused> {
        Self::ALL
            .into_iter()
            .find(|id| id.as_str() == text)
            .ok_or_else(|| Refused::Template(text.to_owned()))
    }

    /// Its data.
    #[must_use]
    pub const fn template(self) -> &'static Template {
        super::catalog::of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each channel reads back from its name, its template writes that channel, and a name no
    /// channel has is refused where the instance file is read, naming the channels there are.
    #[test]
    fn a_channel_reads_back_from_its_name_and_its_template_writes_it() {
        for channel in Channel::ALL {
            assert_eq!(Channel::parse(channel.as_str()), Some(channel));
            assert_eq!(channel.template().template().table, Table::Channel(channel));
            let read: Channel =
                serde_json::from_value(serde_json::json!(channel.as_str())).expect("a channel");
            assert_eq!(read, channel);
        }
        assert_eq!(Channel::parse("carrier-pigeon"), None);
        let refused = serde_json::from_value::<Channel>(serde_json::json!("carrier-pigeon"))
            .expect_err("no channel");
        assert!(refused.to_string().contains("fcm, ntfy"), "{refused}");
    }
}
