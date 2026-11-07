//! Waking the phone ([`Notify`]) over FCM and ntfy.
//!
//! A message is a [`Push`], printed by the core: a closed sum of counts, a day and ids the box
//! minted, so the words a relay carries in the clear are the core's own, never stored text.

mod fcm;
mod ntfy;

use std::sync::OnceLock;

use suzatary_core::capability::{Delivered, Notify, NotifyError, Printed, Push};
use suzatary_core::devices::Device;
use suzatary_core::store::Log;

pub use fcm::{API as FCM, ServiceAccount, credentials};
pub use ntfy::DEFAULT_BASE as NTFY;

use crate::http::{Http, Ureq};
use crate::secrets::{Resolves, SecretError};

/// One deployment's channels: FCM to every registered phone, and ntfy when a topic is configured.
pub struct Channels<D: Log<String, Device>> {
    http: Box<dyn Http>,
    /// The app a tap lands in.
    app: String,
    fcm: Option<fcm::Sender>,
    /// `(base URL, topic)`: the topic is ntfy's whole access control.
    ntfy: Option<(String, String)>,
    devices: D,
    clock: fn() -> u64,
}

impl<D: Log<String, Device>> Channels<D> {
    #[must_use]
    pub fn new(
        http: Box<dyn Http>,
        app: &str,
        fcm: Option<ServiceAccount>,
        ntfy: Option<(String, String)>,
        devices: D,
    ) -> Self {
        Self {
            http,
            app: app.to_owned(),
            fcm: fcm.map(fcm::Sender::new),
            ntfy,
            devices,
            clock: crate::tokens::unix_now,
        }
    }

    /// The channels whose secrets resolve through `secrets`.
    ///
    /// # Errors
    /// [`SecretError`] when the store could not say whether a channel is set.
    pub fn configured(
        secrets: &dyn Resolves,
        reason: &str,
        app: &str,
        devices: D,
    ) -> Result<Self, SecretError> {
        let fcm = credentials(secrets, reason)?;
        let ntfy = secrets
            .get("NTFY_TOPIC", reason)?
            .map(|topic| (ntfy::DEFAULT_BASE.to_owned(), topic.expose().to_owned()));
        Ok(Self::new(Box::new(Ureq::new()), app, fcm, ntfy, devices))
    }

    #[cfg(test)]
    fn with_clock(mut self, clock: fn() -> u64) -> Self {
        self.clock = clock;
        self
    }
}

/// Whether this deployment notifies at all.
pub enum Configured<D: Log<String, Device>> {
    /// At least one channel is named.
    Push(Box<Channels<D>>),
    /// Channels are named and the store gave no answer for their secrets.
    Waiting(Box<Waiting<D>>),
    /// None is: a push reaches nobody.
    Absent,
}

impl<D: Log<String, Device> + Clone + Send + Sync> Notify for Configured<D> {
    fn push(&self, message: &Push) -> Result<Delivered, NotifyError> {
        match self {
            Configured::Push(channels) => channels.push(message),
            Configured::Waiting(waiting) => waiting.channels()?.push(message),
            Configured::Absent => Ok(Delivered::to_devices(0)),
        }
    }
}

/// Channels whose secrets the store has not yet answered for: every push asks it again and keeps
/// the first answer.
pub struct Waiting<D: Log<String, Device>> {
    secrets: Box<dyn Resolves + Send + Sync>,
    reason: String,
    app: String,
    devices: D,
    answered: OnceLock<Channels<D>>,
}

impl<D: Log<String, Device> + Clone> Waiting<D> {
    #[must_use]
    pub fn new(
        secrets: Box<dyn Resolves + Send + Sync>,
        reason: &str,
        app: &str,
        devices: D,
    ) -> Self {
        Self {
            secrets,
            reason: reason.to_owned(),
            app: app.to_owned(),
            devices,
            answered: OnceLock::new(),
        }
    }

    fn channels(&self) -> Result<&Channels<D>, NotifyError> {
        if let Some(channels) = self.answered.get() {
            return Ok(channels);
        }
        let channels = Channels::configured(
            self.secrets.as_ref(),
            &self.reason,
            &self.app,
            self.devices.clone(),
        )
        .map_err(|failed| {
            NotifyError::Unreachable(format!(
                "the secret store gave no answer for the push channels: {failed}"
            ))
        })?;
        Ok(self.answered.get_or_init(|| channels))
    }
}

impl<D: Log<String, Device>> Notify for Channels<D> {
    fn push(&self, message: &Push) -> Result<Delivered, NotifyError> {
        let message: &Printed = &message.printed(&self.app);
        let now = (self.clock)();
        let mut accepted = 0usize;
        let mut failed = std::collections::BTreeSet::new();

        if let Some(sender) = &self.fcm {
            let everyone = suzatary_core::devices::everyone(&self.devices)
                .map_err(|error| NotifyError::Unreachable(error.to_string()))?;
            for (actor, phones) in &everyone {
                for device in &phones.0 {
                    let sent = match sender.send(self.http.as_ref(), now, &device.token, message) {
                        fcm::Sent::Accepted => {
                            accepted += 1;
                            Ok(())
                        }
                        fcm::Sent::Gone => {
                            suzatary_core::devices::forget(&self.devices, actor, &device.token)
                                .map_err(|error| format!("forgetting a gone device: {error}"))
                        }
                        fcm::Sent::Failed(why) => Err(why),
                    };
                    failed.extend(sent.err());
                }
            }
        }

        if let Some((base, topic)) = &self.ntfy {
            let sent = ntfy::send(self.http.as_ref(), base, topic, message).map(|()| accepted += 1);
            failed.extend(sent.err());
        }

        if accepted == 0 && !failed.is_empty() {
            return Err(NotifyError::Unreachable(
                failed.into_iter().collect::<Vec<_>>().join("; "),
            ));
        }
        Ok(Delivered::to_devices(accepted))
    }
}

#[cfg(test)]
mod tests {
    use suzatary_core::capability::Datetime;
    use suzatary_core::refusal::Refusal;
    use suzatary_core::store::{Decide, Edit, StoreError};

    use super::fcm::tests::test_account;
    use super::*;
    use crate::http::{Resp, Scripted};

    const APP: &str = "https://box.example.ts.net";

    /// The token endpoint, and a send that is refused for `dead-token`.
    fn fake_http() -> Box<dyn Http> {
        Box::new(Scripted::new(|req| {
            Ok(if req.url.ends_with("/token") {
                Resp::new(200, r#"{"access_token":"fake-bearer","expires_in":3600}"#)
            } else if req.url.contains("messages:send") {
                if String::from_utf8_lossy(&req.body).contains("dead-token") {
                    Resp::new(404, r#"{"error":{"status":"UNREGISTERED"}}"#)
                } else {
                    Resp::new(200, "")
                }
            } else {
                Resp::new(404, "unexpected URL in the fake")
            })
        }))
    }

    #[derive(Clone)]
    struct PanicsIfAsked;

    impl Log<String, Device> for PanicsIfAsked {
        fn keys(&self) -> Result<Vec<String>, StoreError> {
            panic!("the device store must not be read")
        }
        fn lines(&self, _: &String) -> Result<Vec<Device>, StoreError> {
            panic!("the device store must not be read")
        }
        fn start(&self, _: &String, _: &Device) -> Result<(), StoreError> {
            unreachable!()
        }
        fn push(&self, _: &String, _: &Device) -> Result<(), StoreError> {
            unreachable!()
        }
        fn edit(&self, _: &String, _: &mut Decide<'_, Device>) -> Result<Edit<Device>, Refusal> {
            unreachable!()
        }
    }

    #[test]
    fn the_fanout_forgets_a_dead_token_and_counts_only_the_live_one() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let devices = crate::devices::Devices::open(dir.path().to_path_buf());
        let at = Datetime::new(2026, 9, 7, 12, 0, 0, 0).expect("a real instant");
        let phone = |token: &str| Device {
            platform: "android".to_owned(),
            token: token.to_owned(),
            registered: at,
        };
        devices
            .edit(&"ada".to_owned(), &mut |_| {
                Ok(Edit::Replace(vec![
                    phone("live-token"),
                    phone("dead-token"),
                ]))
            })
            .expect("seeded");
        let channels = Channels::new(fake_http(), APP, Some(test_account()), None, devices)
            .with_clock(|| 1_757_000_000);

        let push = Push::Agent {
            authored: 2,
            changes: 0,
        };
        let delivered = channels
            .push(&push)
            .expect("the fan-out never errors on a per-device failure");

        assert_eq!(delivered.devices(), 1);
        let left = suzatary_core::devices::everyone(&channels.devices).expect("reads");
        assert_eq!(left["ada"].0, vec![phone("live-token")]);
    }

    /// A store locked until `open` is set, counting what it is asked.
    #[derive(Default)]
    struct Unlockable {
        open: std::sync::atomic::AtomicBool,
        asked: std::sync::atomic::AtomicUsize,
    }

    impl Resolves for std::sync::Arc<Unlockable> {
        fn get(
            &self,
            name: &str,
            _: &str,
        ) -> Result<Option<suzatary_core::capability::Secret>, SecretError> {
            self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.open.load(std::sync::atomic::Ordering::SeqCst) {
                Ok(None)
            } else {
                Err(SecretError::Timeout {
                    name: name.to_owned(),
                })
            }
        }
    }

    /// A store locked when the box started is asked again at each push, and the first push
    /// after it is unlocked is delivered; the answer is then kept.
    #[test]
    fn the_push_right_after_an_unlock_is_delivered() {
        let store = std::sync::Arc::new(Unlockable::default());
        let waiting = Waiting::new(
            Box::new(std::sync::Arc::clone(&store)),
            "test",
            APP,
            PanicsIfAsked,
        );
        let configured = Configured::Waiting(Box::new(waiting));
        let push = Push::Agent {
            authored: 2,
            changes: 0,
        };
        let asked = || store.asked.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            configured.push(&push).is_err(),
            "a locked store is an error, never a zero"
        );
        store.open.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(configured.push(&push).expect("delivered").devices(), 0);
        let answered = asked();
        assert_eq!(configured.push(&push).expect("kept").devices(), 0);
        assert_eq!(asked(), answered, "the answer is kept");
    }
}
