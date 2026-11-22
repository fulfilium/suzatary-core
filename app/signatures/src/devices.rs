//! The phones that asked to be woken: a push token is a fact about a phone, not a chain act.

use std::collections::BTreeMap;

use suzatary_base::capability::Datetime;
use suzatary_base::refusal::Refusal;
use suzatary_store::store::{Edit, Log, StoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// `android` or `ios`.
    pub platform: String,
    /// The push service's name for this app on this phone: not a secret.
    pub token: String,
    pub registered: Datetime,
}

/// An actor's phones: each token once, where it was first registered, as its latest line says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Phones(pub Vec<Device>);

impl Phones {
    #[must_use]
    pub fn of(lines: &[Device]) -> Self {
        let mut phones: Vec<Device> = Vec::new();
        for line in lines {
            match phones.iter_mut().find(|phone| phone.token == line.token) {
                Some(phone) => phone.clone_from(line),
                None => phones.push(line.clone()),
            }
        }
        Self(phones)
    }
}

pub const MAX_TOKEN: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// Firebase.
    Android,
    Ios,
}

impl Platform {
    /// As a [`Device`] stores it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Platform::Android => "android",
            Platform::Ios => "ios",
        }
    }
}

/// `POST /api/act/device`: checks the token is one line and appends the registration.
pub fn register_device(
    devices: &dyn Log<String, Device>,
    clock: &dyn suzatary_base::capability::Clock,
    registering: &crate::wire::signatures::Registering,
    actor: &prodrome_core::event::Actor,
) -> Result<crate::wire::signatures::DevicePayload, Refusal> {
    let token = registering.token.trim().to_owned();
    if token.is_empty() || token.len() > MAX_TOKEN || token.chars().any(char::is_whitespace) {
        return Err(Refusal::bad_request("token must be one non-empty line"));
    }
    let actor = actor.as_str().to_owned();
    let line = Device {
        platform: registering.platform.as_str().to_owned(),
        token,
        registered: clock.now(),
    };
    devices.edit(&actor, &mut |_| Ok(Edit::Push(line.clone())))?;
    let count = Phones::of(&devices.lines_if_any(&actor)?).0.len();
    Ok(crate::wire::signatures::DevicePayload {
        ok: true,
        devices: u32::try_from(count).unwrap_or(u32::MAX),
    })
}

pub fn everyone(devices: &dyn Log<String, Device>) -> Result<BTreeMap<String, Phones>, StoreError> {
    devices
        .keys()?
        .into_iter()
        .map(|actor| {
            let phones = Phones::of(&devices.lines_if_any(&actor)?);
            Ok((actor, phones))
        })
        .collect()
}

/// Drops a token the push service called dead.
pub fn forget(devices: &dyn Log<String, Device>, actor: &str, token: &str) -> Result<(), Refusal> {
    devices.edit(&actor.to_owned(), &mut |lines| {
        let Phones(mut phones) = Phones::of(lines);
        phones.retain(|phone| phone.token != token);
        Ok(Edit::Replace(phones))
    })?;
    Ok(())
}
