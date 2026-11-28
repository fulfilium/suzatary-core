//! One change to the sources a program listens to.

/// What a host does to its listeners so they equal a program's next [`Subs`](crate::Subs).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "host", derive(serde::Serialize, ts_rs::TS))]
#[cfg_attr(feature = "host", serde(tag = "type"))]
pub enum SubPatch<K, V> {
    /// A source for a slot. On a slot that is held, the host stops the earlier listener first.
    Start {
        slot: K,
        source: V,
    },
    Stop {
        slot: K,
    },
}
