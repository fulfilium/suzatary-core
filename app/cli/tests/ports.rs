//! A port for a server a test starts as another process, chosen below the kernel's ephemeral
//! range because every `bind(127.0.0.1:0)` of this binary draws from that range and could take
//! the port between the check and `serve`'s bind.

use std::sync::atomic::{AtomicU16, Ordering};

/// The lowest port no well-known service takes.
const LOWEST: u16 = 10_000;

/// A loopback port below the ephemeral range that nothing holds now, a different one for each
/// call of this process.
pub(crate) fn outside_ephemeral() -> u16 {
    static CALLS: AtomicU16 = AtomicU16::new(0);
    let range = std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range")
        .expect("the kernel's ephemeral range reads");
    let low: u16 = range
        .split_whitespace()
        .next()
        .and_then(|low| low.parse().ok())
        .expect("the range starts at a port");
    assert!(
        low > LOWEST,
        "the ephemeral range starts at {low}, at or under {LOWEST}"
    );
    let span = low - LOWEST;
    // Two test processes on one machine start apart, and one process's calls apart.
    let start = std::process::id()
        .wrapping_mul(7_919)
        .wrapping_add(u32::from(CALLS.fetch_add(97, Ordering::Relaxed)));
    (0..span)
        .map(|step| {
            let offset = start.wrapping_add(u32::from(step)) % u32::from(span);
            LOWEST + u16::try_from(offset).expect("under the span")
        })
        .find(|port| std::net::TcpListener::bind(("127.0.0.1", *port)).is_ok())
        .expect("a free port below the ephemeral range")
}
