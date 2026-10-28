//! The tables of the instance file connections are read from.

use std::collections::BTreeMap;

use super::{AccountKind, Channel, Connection, Key, Label, Params, Refused, Table, TemplateId};
use crate::drive::DrivePath;
use crate::mail::account::{MailAccount, Roster};
use crate::plugin::{Manifest, Origin, PluginName};

/// What the instance declares where a connection lives, each table parsed: the one value the
/// connections are read from and the file's editor must agree with, so adding a connection is
/// [`Declared::with`] here and a table written there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    /// `[[mail.accounts]]`, a roster by construction.
    pub accounts: Vec<MailAccount>,
    /// `[[plugins]]`, each name once.
    pub plugins: Vec<Manifest>,
    /// `[secretary] drive_allow`.
    pub drive: Vec<DrivePath>,
    /// `[notify] channels`.
    pub channels: Vec<Channel>,
}

impl Declared {
    /// Every connection these tables hold, by key: one per table entry a template writes, and
    /// the data repository, which every deployment has.
    #[must_use]
    pub fn connections(&self) -> BTreeMap<Key, Connection> {
        let made = |id: TemplateId, params: Params| Connection::new(id, params).ok();
        let mut connections = vec![made(TemplateId::GitRemote, Params::Nothing)];
        connections.extend(
            self.drive
                .iter()
                .map(|folder| made(TemplateId::DriveCli, Params::Folder(folder.clone()))),
        );
        connections.extend(self.accounts.iter().map(|account| {
            made(
                AccountKind::of(&account.sync).template(),
                AccountKind::params(account),
            )
        }));
        for id in TemplateId::ALL {
            match id.template().table {
                Table::Link(fragment) => {
                    connections.extend(
                        self.links(fragment.plugin)
                            .into_iter()
                            .map(|label| made(id, Params::Label(label))),
                    );
                }
                Table::Model => connections.push(made(id, Params::Nothing)),
                Table::Channel(channel) if self.channels.contains(&channel) => {
                    connections.push(made(id, Params::Nothing));
                }
                Table::Always | Table::Account(_) | Table::Folder | Table::Channel(_) => {}
            }
        }
        connections
            .into_iter()
            .flatten()
            .map(|connection| (connection.key(), connection))
            .collect()
    }

    /// The labels of every link `plugin`'s manifest declares.
    fn links(&self, plugin: &PluginName) -> Vec<Label> {
        let Some(manifest) = self.manifest(plugin) else {
            return Vec::new();
        };
        manifest
            .credentials()
            .filter_map(|name| Label::of_suffix(name.label()).ok())
            .collect()
    }

    fn manifest(&self, plugin: &PluginName) -> Option<&Manifest> {
        self.plugins
            .iter()
            .find(|manifest| manifest.name() == plugin)
    }

    /// These tables with `connection` written into them.
    ///
    /// # Errors
    /// [`Refused::Duplicate`] for a key already held, [`Refused::Unwritten`] for a template no
    /// table holds, and the refusal the file would get for the table it would leave.
    pub fn with(&self, connection: &Connection) -> Result<Self, Refused> {
        let key = connection.key();
        if self.connections().contains_key(&key) {
            return Err(Refused::Duplicate(connection.called()));
        }
        let mut next = self.clone();
        match (connection.template().table, connection.params()) {
            (Table::Link(fragment), Params::Label(_)) => {
                let name = link_name(connection)?;
                match next
                    .plugins
                    .iter_mut()
                    .find(|held| held.name() == fragment.plugin)
                {
                    Some(held) => *held = held.with_link(&name)?,
                    None => next.plugins.push(fragment.manifest()?.with_link(&name)?),
                }
            }
            (Table::Account(kind), params) => {
                next.accounts.push(kind.account(params)?);
                Roster::new(next.accounts.clone())
                    .map_err(|refused| Refused::Roster(refused.to_string()))?;
            }
            (Table::Folder, Params::Folder(folder)) => next.drive.push(folder.clone()),
            (Table::Channel(channel), _) => next.channels.push(channel),
            (table, _) => return Err(unwritten(connection, table)),
        }
        Ok(next)
    }

    /// These tables without `connection`. A plugin's manifest stays when its last link goes,
    /// since a table its template wrote cannot be told from one the person wrote alike: the
    /// plugin is then declared with nothing to fetch.
    ///
    /// # Errors
    /// [`Refused::Absent`] for a key not held, and [`Refused::Unwritten`] for a template no
    /// table holds.
    pub fn without(&self, connection: &Connection) -> Result<Self, Refused> {
        let key = connection.key();
        if !self.connections().contains_key(&key) {
            return Err(Refused::Absent(connection.called()));
        }
        let mut next = self.clone();
        match connection.template().table {
            Table::Link(fragment) => {
                let name = link_name(connection)?;
                for held in &mut next.plugins {
                    if held.name() == fragment.plugin {
                        *held = held.without_credential(&name);
                    }
                }
            }
            Table::Account(_) => next
                .accounts
                .retain(|account| account.email.as_str() != key.name),
            Table::Folder => next.drive.retain(|folder| folder.to_string() != key.name),
            Table::Channel(channel) => next.channels.retain(|held| *held != channel),
            table @ (Table::Always | Table::Model) => {
                return Err(unwritten(connection, table));
            }
        }
        Ok(next)
    }

    /// These tables with `plugin` allowed to reach `origin`.
    ///
    /// # Errors
    /// [`Refused::NoPlugin`] when no manifest is named `plugin`.
    pub fn with_origin(&self, plugin: &PluginName, origin: &Origin) -> Result<Self, Refused> {
        let held = self
            .manifest(plugin)
            .ok_or_else(|| Refused::NoPlugin(plugin.to_string()))?;
        let widened = held.with_origin(origin.clone());
        let mut next = self.clone();
        for manifest in &mut next.plugins {
            if manifest.name() == plugin {
                *manifest = widened.clone();
            }
        }
        Ok(next)
    }
}

/// The credential a link connection names.
fn link_name(connection: &Connection) -> Result<String, Refused> {
    connection
        .secrets()
        .into_iter()
        .next()
        .ok_or_else(|| Refused::Params {
            template: connection.id().as_str(),
            wanted: connection.template().shape().wanted(),
        })
}

/// Why a template with no table is neither added nor removed.
fn unwritten(connection: &Connection, table: Table) -> Refused {
    Refused::Unwritten {
        template: connection.id().as_str(),
        why: match table {
            Table::Always => "every deployment has exactly one data repository",
            Table::Model => "every deployment runs its model, and its sign-in is its own card",
            Table::Link(_) | Table::Account(_) | Table::Folder | Table::Channel(_) => {
                "its parameters are not of the shape its table reads"
            }
        },
    }
}
