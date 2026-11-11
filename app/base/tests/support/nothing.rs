use crate::capability::{Clock, Datetime, Entropy, Manual, Manuals, Peer, Whois, WhoisError};

/// Every effect, inert: reads find nothing, writes are refused, no peer has a login.
/// A test fills a record from it and swaps in the effects it is about.
#[derive(Debug, Clone, Copy, Default)]
pub struct Nothing;

impl Manuals for Nothing {
    fn document(&self, _which: Manual) -> Option<String> {
        None
    }
}

/// A closed world that only parses: every text typst's parser accepts compiles to an empty
/// document. The real one is `suzatary_io::typeset`; a door's refusals of what parses and still
/// fails to compile are tested there and in `app/cli/tests`.
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
