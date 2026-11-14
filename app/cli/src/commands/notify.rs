//! `suzatary notify`: push one of the core's [`Push`]es to the phone.
//!
//! The verb takes a notice and the day or conversation it names, never a title or a message.

use std::path::Path;

use anyhow::bail;
use suzatary_base::conversation_id::ConversationId;
use suzatary_core::connect::Channel;
use suzatary_core::nonempty::NonEmpty;
use suzatary_core::notify::{Notify as _, Push};
use suzatary_io::devices::Devices;
use suzatary_io::notify::{Channels, Configured, Waiting};
use suzatary_io::secrets::Resolves;
use suzatary_signatures::devices::Device;
use suzatary_store::store::Log;

use crate::instance::Instance;

/// What `suzatary notify` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Print what would be sent and send nothing.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Which notice.
    #[command(subcommand)]
    notice: Notice,
}

/// The notices a person's units send, each one [`Push`].
#[derive(Debug, Clone, PartialEq, Eq, clap::Subcommand)]
pub enum Notice {
    /// The briefing saved for this day, as a deep link into it.
    Briefing {
        /// The day, `YYYY-MM-DD`.
        day: chrono::NaiveDate,
    },
    /// A secretary conversation's brief, as a deep link into it (the id `secretary brief`
    /// prints).
    Brief {
        /// The conversation.
        conversation: String,
    },
    /// The morning brief was due and was not written.
    NoBrief,
}

impl Notice {
    /// The push this notice is.
    ///
    /// # Errors
    /// A conversation id that is not one.
    pub fn push(&self) -> anyhow::Result<Push> {
        Ok(match self {
            Notice::Briefing { day } => Push::Brief { day: *day },
            Notice::Brief { conversation } => Push::Answered {
                conversation: ConversationId::new(conversation.as_str())?,
                brief: true,
            },
            Notice::NoBrief => Push::NoBrief,
        })
    }
}

/// `suzatary notify`: compose the push and send it, or print it and send nothing.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = &Instance::open(path)?;
    let push = args.notice.push()?;
    if args.dry_run {
        let printed = push.printed(&instance.deployment.app_url);
        println!(
            "{}",
            serde_json::json!({
                "title": printed.title,
                "message": printed.body,
                "click": printed.click,
                "tag": printed.tag,
            })
        );
        return Ok(());
    }
    let delivered = deliver(instance, &push)?;
    println!("{}", serde_json::json!({ "delivered": delivered }));
    Ok(())
}

/// Push `push` through the deployment's channels; returns how many devices it reached.
///
/// # Errors
/// No channel named, the device store unreadable, the secret store not answering, every channel
/// refusing, or nothing delivered.
pub fn deliver(instance: &Instance, push: &Push) -> anyhow::Result<usize> {
    let Some(channels) = &instance.notify else {
        bail!(
            "notify: {} names no [notify] channel, so a push reaches nobody",
            instance.path.display()
        );
    };
    let devices = Devices::open(instance.paths.devices.clone());
    let known = suzatary_signatures::devices::everyone(&devices)
        .map_err(|error| anyhow::anyhow!("notify: reading the registered devices: {error}"))?
        .len();
    let delivered = self::push(
        Some(channels),
        instance.secrets(),
        "suzatary notify",
        &instance.deployment.app_url,
        devices,
    )
    .push(push)
    .map_err(|error| anyhow::anyhow!("{error}"))?;
    if delivered.devices() == 0 {
        bail!(
            "notify: nothing delivered ({known} actor(s) with devices on file) — no device \
             registered and no ntfy topic"
        );
    }
    Ok(delivered.devices())
}

/// The push effect over `channels`: none named is [`Configured::Absent`], asking no secret
/// store; a store that does not answer yet is asked again at each push. Generic over
/// [`Resolves`] so a test can prove nothing is resolved otherwise.
pub(crate) fn push<D: Log<String, Device> + Clone>(
    channels: Option<&NonEmpty<Channel>>,
    secrets: impl Resolves + Send + Sync + 'static,
    reason: &str,
    app: &str,
    devices: D,
) -> Configured<D> {
    let Some(channels) = channels else {
        tracing::info!("no [notify] channel — push is Absent, nothing resolved");
        return Configured::Absent;
    };
    tracing::info!(?channels, "[notify] configured — resolving push secrets");
    match Channels::configured(&secrets, reason, app, devices.clone()) {
        Ok(channels) => Configured::Push(Box::new(channels)),
        Err(unanswered) => {
            tracing::warn!("push waits for the secret store, asked again at a push: {unanswered}");
            Configured::Waiting(Box::new(Waiting::new(
                Box::new(secrets),
                reason,
                app,
                devices,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use suzatary_base::capability::Secret;
    use suzatary_base::refusal::Refusal;
    use suzatary_store::store::{Decide, Edit, StoreError};

    use super::*;

    // Panics when asked, proving nothing was resolved rather than resolved empty.
    struct PanicsIfAsked;
    impl Resolves for PanicsIfAsked {
        fn get(
            &self,
            name: &str,
            _reason: &str,
        ) -> Result<Option<Secret>, suzatary_io::secrets::SecretError> {
            panic!("push must not resolve {name} when no channel is named")
        }
    }

    #[derive(Clone)]
    struct NoDevices;
    impl Log<String, Device> for NoDevices {
        fn keys(&self) -> Result<Vec<String>, StoreError> {
            panic!("an Absent push must never read the device store")
        }
        fn lines(&self, _: &String) -> Result<Vec<Device>, StoreError> {
            panic!("an Absent push must never read the device store")
        }
        fn start(&self, _: &String, _: &Device) -> Result<(), StoreError> {
            unreachable!("not exercised by this test")
        }
        fn push(&self, _: &String, _: &Device) -> Result<(), StoreError> {
            unreachable!("not exercised by this test")
        }
        fn edit(&self, _: &String, _: &mut Decide<'_, Device>) -> Result<Edit<Device>, Refusal> {
            unreachable!("not exercised by this test")
        }
    }

    /// No channel resolves nothing and pushes to nobody.
    #[test]
    fn no_channel_resolves_nothing_and_is_absent() {
        let result = push(None, PanicsIfAsked, "test", "https://app", NoDevices);
        assert!(matches!(result, Configured::Absent));
    }
}
