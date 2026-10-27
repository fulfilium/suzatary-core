//! One connection: a template applied to what a person chose.

use std::fmt;

use prodrome_core::literal::{ProdromeError, Value};
use serde::{Deserialize, Serialize};

use super::{Group, Params, Refused, Seen, Standing, Step, Table, Template, TemplateId};
use prodrome_core::literal::codec::{Constructor, Fields, Literal, record};

/// The model connection's key name: a deployment has one model.
const MODEL_KEY: &str = "model";

/// A template applied to parameters of the shape it reads. [`Connection::new`] is its only
/// constructor, so its tables, steps and check are functions of the value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Connection {
    template: TemplateId,
    params: Params,
}

/// A connection's identity: its group and its name there. Two connections under one key are
/// one connection, as a mail account is one account by its address whichever strategy syncs it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    /// Its group.
    pub group: Group,
    /// Its name in the group: an address, a label, a folder, the model, a channel.
    pub name: String,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.group.as_str(), self.name)
    }
}

impl Connection {
    /// `template` applied to `params`.
    ///
    /// # Errors
    /// [`Refused::Params`] for parameters of another shape than the template reads.
    pub fn new(template: TemplateId, params: Params) -> Result<Self, Refused> {
        let shape = template.template().shape();
        if params.shape() != shape {
            return Err(Refused::Params {
                template: template.as_str(),
                wanted: shape.wanted(),
            });
        }
        Ok(Self { template, params })
    }

    /// A notification channel's connection, which reads no parameter.
    #[must_use]
    pub const fn of_channel(channel: super::Channel) -> Self {
        Self {
            template: channel.template(),
            params: Params::Nothing,
        }
    }

    /// A connection from its text form: the template's name and its parameters' texts.
    ///
    /// # Errors
    /// [`Refused`] naming the template or the parameter that did not parse.
    pub fn parse(template: &str, params: &[String]) -> Result<Self, Refused> {
        let id = TemplateId::parse(template)?;
        Self::new(
            id,
            Params::parse(id.template().shape(), id.as_str(), params)?,
        )
    }

    /// Which template.
    #[must_use]
    pub const fn id(&self) -> TemplateId {
        self.template
    }

    /// Its template's data.
    #[must_use]
    pub const fn template(&self) -> &'static Template {
        self.template.template()
    }

    /// What was chosen.
    #[must_use]
    pub const fn params(&self) -> &Params {
        &self.params
    }

    /// Its identity.
    #[must_use]
    pub fn key(&self) -> Key {
        let template = self.template();
        let name = match (template.table, &self.params) {
            (Table::Always, _) => "data".to_owned(),
            (Table::Model, _) => MODEL_KEY.to_owned(),
            (Table::Channel(channel), _) => channel.as_str().to_owned(),
            (_, Params::Label(label)) => label.as_str().to_owned(),
            (_, Params::Address(email) | Params::Microsoft { email, .. }) => {
                email.as_str().to_owned()
            }
            (_, Params::Folder(folder)) => folder.to_string(),
            (_, Params::Nothing) => template.id.as_str().to_owned(),
        };
        Key {
            group: template.group,
            name,
        }
    }

    /// What the owner calls it, as a sentence begins with it: the label they gave a calendar
    /// (`your Work calendar`), an account's address, a folder's path.
    #[must_use]
    pub fn called(&self) -> String {
        let template = self.template();
        match (template.table, &self.params) {
            (Table::Link(_), Params::Label(label)) if template.group == Group::Calendar => {
                format!("your {} calendar", label.shown())
            }
            (Table::Link(_), Params::Label(label)) => format!("the {} link", label.shown()),
            (_, Params::Address(email) | Params::Microsoft { email, .. }) => email.to_string(),
            (_, Params::Folder(folder)) => format!("the Drive folder {folder}"),
            (Table::Always, _) => "the data repository".to_owned(),
            (
                Table::Model
                | Table::Channel(_)
                | Table::Link(_)
                | Table::Account(_)
                | Table::Folder,
                _,
            ) => template.title.to_owned(),
        }
    }

    /// The table it writes, in words a card says: the place in the instance file, never a
    /// secret's value, which no table holds.
    #[must_use]
    pub fn writes(&self) -> String {
        match (self.template().table, &self.params) {
            (Table::Link(fragment), _) => {
                format!("one link for {}", super::plugin_called(fragment.plugin))
            }
            (Table::Account(_), _) => format!("a [[mail.accounts]] entry for {}", self.key().name),
            (Table::Folder, _) => format!("{} in [secretary] drive_allow", self.key().name),
            (Table::Channel(channel), _) => {
                format!("the {} channel in [notify]", channel.as_str())
            }
            (Table::Always, _) => "nothing: every deployment has one".to_owned(),
            (Table::Model, _) => "nothing: every deployment runs its model".to_owned(),
        }
    }

    /// The person's steps, in the template's order, filled with its parameters.
    #[must_use]
    pub fn steps(&self) -> Vec<Step> {
        let template = self.template();
        let plugin = match template.table {
            Table::Link(fragment) => Some(fragment.plugin),
            _ => None,
        };
        template
            .steps
            .iter()
            .filter_map(|shape| shape.fill(plugin, &self.params))
            .collect()
    }

    /// The names of the secrets its steps ask for.
    #[must_use]
    pub fn secrets(&self) -> Vec<String> {
        self.steps()
            .into_iter()
            .filter_map(|step| match step {
                Step::Secret { name, .. } => Some(name),
                _ => None,
            })
            .collect()
    }

    /// The names of everything the box keeps in the secret store for it alone: its steps'
    /// secrets, and the grant a sign-in with Google keeps. What goes with it when it is removed.
    #[must_use]
    pub fn kept(&self) -> Vec<String> {
        self.steps()
            .into_iter()
            .filter_map(|step| match step {
                Step::Secret { name, .. } => Some(name),
                Step::AuthCode { account } => Some(crate::google::grant_name(&account)),
                Step::DeviceCode { .. }
                | Step::SendingConsent { .. }
                | Step::GoogleClient
                | Step::SignIn
                | Step::Terminal { .. } => None,
            })
            .collect()
    }

    /// Where its steps stand as `seen` reads them, joined; its check's results are the
    /// reader's to join, since only the reader knows where they are kept.
    #[must_use]
    pub fn standing(&self, seen: &Seen<'_>) -> Standing {
        self.steps()
            .iter()
            .enumerate()
            .map(|(at, step)| step.standing(seen).at_step(at))
            .fold(Standing::Kept, Standing::join)
    }

    /// Its steps `seen` does not read as taken, in the template's order.
    #[must_use]
    pub fn unmet(&self, seen: &Seen<'_>) -> Vec<Step> {
        self.steps()
            .into_iter()
            .filter(|step| step.standing(seen) != Standing::Kept)
            .collect()
    }
}

/// `Connection(template, params)`.
pub const CONNECTION: Constructor = ("Connection", &["template", "params"]);

impl Literal for Connection {
    fn to_value(&self) -> Value {
        record(
            CONNECTION,
            vec![
                Value::str(self.template.as_str()),
                self.params.texts().to_value(),
            ],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::of(value, CONNECTION)?;
        Self::parse(
            &fields.get::<String>("template")?,
            &fields.get::<Vec<String>>("params")?,
        )
        .map_err(|refused| ProdromeError::invalid(refused.to_string()))
    }
}

/// The text form a page and a verb read, `{template, params}`.
#[derive(Serialize, Deserialize)]
struct Spelled {
    template: String,
    params: Vec<String>,
}

impl Serialize for Connection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Spelled {
            template: self.template.as_str().to_owned(),
            params: self.params.texts(),
        }
        .serialize(serializer)
    }
}

// Read through `parse`, so a stored act names only a connection the constructor admits.
impl<'de> Deserialize<'de> for Connection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spelled = Spelled::deserialize(deserializer)?;
        Self::parse(&spelled.template, &spelled.params).map_err(serde::de::Error::custom)
    }
}
