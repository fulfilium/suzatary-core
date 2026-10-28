//! The default templates: what a deployment can connect with no code of its own.

use super::{
    AccountKind, Channel, Check, Fragment, Group, Run, SecretName, StepShape, Table, Template,
    TemplateId,
};
use crate::plugin::PluginName;
use crate::settings::{Page, Words};

/// The data repository, pushed to a remote.
const GIT_REMOTE: Template = Template {
    id: TemplateId::GitRemote,
    group: Group::Storage,
    title: "the data repository, pushed to its remote",
    table: Table::Always,
    steps: &[StepShape::Terminal {
        run: Run::Shell(
            "git -C \"$(dirname \"$SUZATARY_INSTANCE\")\" remote add origin <the remote's URL>",
        ),
        why: "the box pushes what it writes to the data repository's remote; with none, \
              nothing leaves this box",
    }],
    check: Check::Pushed,
};

/// A folder of the owner's Proton Drive the secretary may read without asking.
const DRIVE_CLI: Template = Template {
    id: TemplateId::DriveCli,
    group: Group::Drive,
    title: "a Proton Drive folder the secretary may read",
    table: Table::Folder,
    steps: &[StepShape::Terminal {
        run: Run::Shell("~/bin/proton-drive auth login"),
        why: "the Drive's command line signs in at a terminal, and again whenever its \
              session expires",
    }],
    check: Check::Listed,
};

/// The calendar's plugin.
static ICS_PLUGIN: PluginName = PluginName::known("ics");

/// A calendar, by the secret link its service publishes.
const ICS: Template = Template {
    id: TemplateId::Ics,
    group: Group::Calendar,
    title: "a calendar, by its secret iCal link",
    table: Table::Link(Fragment {
        plugin: &ICS_PLUGIN,
        component: "ics.wasm",
        schema: "ics.schema",
        origins: &["https://outlook.office365.com"],
    }),
    steps: &[StepShape::Secret {
        name: SecretName::Link,
        help: Words {
            label: "your {label} calendar's secret link",
            text: "In your calendar's settings, copy its secret address in iCal format (a link \
                   ending in .ics). A Google account's calendars need no link: connect the \
                   account itself, which reads them through Google's own API.",
            page: Page::Unknown,
        },
    }],
    check: Check::Synced,
};

/// Gmail, through lieer.
const GMAIL: Template = Template {
    id: TemplateId::Gmail,
    group: Group::Mail,
    title: "a Gmail account",
    table: Table::Account(AccountKind::Gmail),
    steps: &[StepShape::Terminal {
        run: Run::Script("mkdir -p Mail/{subdir} && cd Mail/{subdir} && gmi init {email}"),
        why: "lieer's sign-in, in a browser",
    }],
    check: Check::MailSynced,
};

/// A Google account, by one sign-in with Google that grants its mail and its calendars (view and
/// edit): first the box's Google Cloud client, made once for every Google account,
/// then the account's own consent, which Google sends back to the box.
const GOOGLE: Template = Template {
    id: TemplateId::Google,
    group: Group::Mail,
    title: "a Google account (its mail and calendars)",
    table: Table::Account(AccountKind::Google),
    steps: &[StepShape::GoogleClient, StepShape::AuthCode],
    check: Check::MailSynced,
};

/// Microsoft 365 or Outlook, read over IMAP once the box's device-code sign-in is tapped, and
/// sending from it once its own consent is tapped: a second grant, kept apart from reading's.
const MS_IMAP: Template = Template {
    id: TemplateId::MsImap,
    group: Group::Mail,
    title: "a Microsoft 365 or Outlook account",
    table: Table::Account(AccountKind::Microsoft),
    steps: &[StepShape::DeviceCode, StepShape::SendingConsent],
    check: Check::MailSynced,
};

/// Proton Mail, through its Bridge, whose password is set only at a terminal.
const PROTON: Template = Template {
    id: TemplateId::Proton,
    group: Group::Mail,
    title: "a Proton Mail account",
    table: Table::Account(AccountKind::Proton),
    steps: &[
        StepShape::Terminal {
            run: Run::Tool("protonmail-bridge --cli"),
            why: "`login`, then `info` for the Bridge password",
        },
        StepShape::Terminal {
            run: Run::Secret("PROTON_BRIDGE_PASSWORD"),
            // A mail transport's secret is set at a terminal, never from a card.
            why: "that password, typed at the prompt",
        },
        StepShape::Terminal {
            run: Run::Shell("$EDITOR mbsyncrc"),
            why: "a `proton` group pulling Inbox",
        },
    ],
    check: Check::MailSynced,
};

/// The deployment's model, signed in from a card.
const MODEL: Template = Template {
    id: TemplateId::Model,
    group: Group::Models,
    title: "the model",
    table: Table::Model,
    steps: &[StepShape::SignIn],
    check: Check::Answered,
};

/// Notifications through ntfy.
const NTFY: Template = Template {
    id: TemplateId::Ntfy,
    group: Group::Notifications,
    title: "notifications through ntfy",
    table: Table::Channel(Channel::Ntfy),
    steps: &[StepShape::Secret {
        name: SecretName::Fixed("NTFY_TOPIC"),
        help: Words {
            label: "your notifications topic",
            text: "Choose a topic name only you know, and subscribe to it in the ntfy app.",
            page: Page::At("https://docs.ntfy.sh/subscribe/phone/"),
        },
    }],
    check: Check::Delivered,
};

/// Notifications through Firebase.
const FCM: Template = Template {
    id: TemplateId::Fcm,
    group: Group::Notifications,
    title: "notifications through Firebase",
    table: Table::Channel(Channel::Fcm),
    steps: &[StepShape::Secret {
        name: SecretName::Fixed("FCM_SERVICE_ACCOUNT_B64"),
        help: Words {
            label: "the Firebase service account",
            text: "In the Firebase console, project settings, Service accounts: generate a \
                       private key, and paste the file base64-encoded.",
            page: Page::At("https://console.firebase.google.com/"),
        },
    }],
    check: Check::Delivered,
};

/// Every default template, in [`TemplateId::ALL`]'s order.
pub const CATALOG: [Template; 10] = [
    GIT_REMOTE, DRIVE_CLI, ICS, GMAIL, GOOGLE, MS_IMAP, PROTON, MODEL, NTFY, FCM,
];

/// The template `id` names.
pub(super) const fn of(id: TemplateId) -> &'static Template {
    match id {
        TemplateId::GitRemote => &GIT_REMOTE,
        TemplateId::DriveCli => &DRIVE_CLI,
        TemplateId::Ics => &ICS,
        TemplateId::Gmail => &GMAIL,
        TemplateId::Google => &GOOGLE,
        TemplateId::MsImap => &MS_IMAP,
        TemplateId::Proton => &PROTON,
        TemplateId::Model => &MODEL,
        TemplateId::Ntfy => &NTFY,
        TemplateId::Fcm => &FCM,
    }
}

/// The words for the secret named `name`, from the template whose step asks for it.
#[must_use]
pub fn secret_words(name: &str) -> Option<Words> {
    CATALOG.iter().find_map(|template| {
        template.steps.iter().find_map(|step| match *step {
            StepShape::Secret {
                name: SecretName::Fixed(fixed),
                help,
            } if fixed == name => Some(help),
            StepShape::Secret {
                name: SecretName::Link,
                help,
            } => match template.table {
                Table::Link(fragment)
                    if name
                        .strip_prefix("PLUGIN_")
                        .and_then(|rest| {
                            rest.strip_prefix(&*fragment.plugin.as_str().to_ascii_uppercase())
                        })
                        .is_some_and(|rest| rest.starts_with('_')) =>
                {
                    Some(help)
                }
                _ => None,
            },
            _ => None,
        })
    })
}

/// What the owner calls the plugin `plugin`: the calendar for the plugin the calendar template
/// declares, and the plugin by its name otherwise.
#[must_use]
pub fn plugin_called(plugin: &str) -> String {
    CATALOG
        .iter()
        .find(
            |template| matches!(template.table, Table::Link(fragment) if fragment.plugin.as_str() == plugin),
        )
        .map_or_else(
            || format!("the {plugin} plugin"),
            |template| match template.group {
                Group::Calendar => "the calendar".to_owned(),
                Group::Storage
                | Group::Mail
                | Group::Drive
                | Group::Models
                | Group::Notifications
                | Group::Coding
                | Group::Plugins
                | Group::Box => format!("the {plugin} plugin"),
            },
        )
}
