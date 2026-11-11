//! The phones that asked to be woken: a log of registrations per actor, since a push token is a
//! fact about a phone and not an act on the chain.

use std::collections::BTreeMap;

use crate::capability::Datetime;
use crate::refusal::Refusal;
use crate::store::{Edit, Log, StoreError};

/// One registration of a phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// `android` or `ios`.
    pub platform: String,
    /// The push service's name for this app on this phone: state, not a secret.
    pub token: String,
    /// When it was registered.
    pub registered: Datetime,
}

/// An actor's phones: each token once, where it was first registered, as its latest line says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Phones(pub Vec<Device>);

impl Phones {
    /// The phones a log's lines come to, read in the order they were written.
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

/// The longest push token this will write down.
pub const MAX_TOKEN: usize = 4096;

/// The push service a phone's token is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// Firebase, on Android.
    Android,
    /// Apple's, on iOS.
    Ios,
}

impl Platform {
    /// Its name, as a [`Device`] stores it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Platform::Android => "android",
            Platform::Ios => "ios",
        }
    }
}

/// Check the token `POST /api/act/device` carries and write the registration down, answering
/// how many devices this actor now has.
///
/// # Errors
/// A token that is not one line, and the store's own.
pub fn register_device(
    devices: &dyn Log<String, Device>,
    clock: &dyn crate::capability::Clock,
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

/// Every actor's phones, for the sender.
///
/// # Errors
/// The store's own.
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

/// Drop a token the push service called dead, leaving the actor's other phones as they fold.
///
/// # Errors
/// The store's own.
pub fn forget(devices: &dyn Log<String, Device>, actor: &str, token: &str) -> Result<(), Refusal> {
    devices.edit(&actor.to_owned(), &mut |lines| {
        let Phones(mut phones) = Phones::of(lines);
        phones.retain(|phone| phone.token != token);
        Ok(Edit::Replace(phones))
    })?;
    Ok(())
}
