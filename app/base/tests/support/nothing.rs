use crate::capability::{Clock, Datetime, Entropy, Manual, Manuals, Peer, Whois, WhoisError};

/// Every effect, inert: reads find nothing, writes are refused, no peer has a login.
#[derive(Debug, Clone, Copy, Default)]
pub struct Nothing;

impl Manuals for Nothing {
    fn document(&self, _which: Manual) -> Option<String> {
        None
    }
}

/// A closed world that only parses: every text typst's parser accepts compiles to an empty
#[derive(Debug, Clone, Copy, Default)]
pub struct Parses;

impl crate::capability::Typeset for Parses {
    fn closed(&self, source: &str) -> Result<String, crate::capability::Rejected> {
        crate::capability::parsed(source)
            .map(|()| "<!DOCTYPE html><html><body></body></html>".to_owned())
    }
}

impl crate::capability::Typeset for Nothing {
    fn closed(&self, source: &str) -> Result<String, crate::capability::Rejected> {
        crate::capability::parsed(source)
            .map(|()| "<!DOCTYPE html><html><body></body></html>".to_owned())
    }
}

impl Clock for Nothing {
    fn now(&self) -> Datetime {
        Datetime::new(2026, 9, 7, 0, 0, 0, 0).expect("a real date")
    }
}

/// Zeros: nothing a salt or a key guards is kept here.
impl Entropy for Nothing {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(0);
        Ok(())
    }
}

impl Whois for Nothing {
    fn peer(&self, _address: std::net::IpAddr) -> Result<Peer, WhoisError> {
        Ok(Peer::Stranger)
    }
}

impl crate::call_log::Journal for Nothing {
    fn record(
        &self,
        _call: crate::call_log::Call<'_>,
        _model: &str,
        _usage: Option<crate::call_log::TokenUsage>,
        _bytes: u64,
    ) {
    }
}

/// What a journal was told of one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub conversation: Option<String>,
    pub step: Option<usize>,
    pub model: String,
    pub usage: Option<crate::call_log::TokenUsage>,
    pub bytes: u64,
}

/// A journal that keeps every call it is told of, from whichever thread.
#[derive(Debug, Default)]
pub struct Recording(std::sync::Mutex<Vec<Recorded>>);

impl Recording {
    #[must_use]
    pub fn calls(&self) -> Vec<Recorded> {
        self.0.lock().expect("not poisoned").clone()
    }
}

impl crate::call_log::Journal for Recording {
    fn record(
        &self,
        call: crate::call_log::Call<'_>,
        model: &str,
        usage: Option<crate::call_log::TokenUsage>,
        bytes: u64,
    ) {
        let (conversation, step) = match call {
            crate::call_log::Call::Round { conversation, step } => {
                (Some(conversation.as_str().to_owned()), Some(step))
            }
            crate::call_log::Call::Compaction => (None, None),
        };
        self.0.lock().expect("not poisoned").push(Recorded {
            conversation,
            step,
            model: model.to_owned(),
            usage,
            bytes,
        });
    }
}
