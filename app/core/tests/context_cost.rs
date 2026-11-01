//! THE AGENT'S `context` AT THE LIVE SHAPE. One call reads the facts, builds the record from every
//! line of the conversations and the digest's history (`Recollection::record`), and fits the
//! memory into its answer (`agent::context`), which reads the digest's view about twenty times
//! while it searches for the largest view that fits. A view asks for every node of the tree how
//! its summary stands, and that answer finds the node by name, so a lookup that searched the
//! whole tree made the call cost the square of the store. Two laws bound the call's CPU time:
//! it grows with the store and not with its square, and at the live shape an optimised build
//! answers in under two seconds of one core.

use std::time::Duration;

use suzatary_core::secretary::live_shape::{self, Shape, Stores};

/// `lines` lines over `conversations` conversations, the secretary's answers about 3 kB.
fn store(lines: usize, conversations: usize) -> Stores {
    Shape {
        lines,
        conversations,
        answer_bytes: 3_000,
    }
    .build()
}

/// The CPU time this thread has run, which the kernel counts in nanoseconds: what other
/// processes and threads do on the machine does not add to it, as it adds to wall time.
fn cpu() -> Duration {
    let read = std::fs::read_to_string("/proc/thread-self/schedstat")
        .expect("Linux counts a thread's CPU time in /proc/thread-self/schedstat");
    let nanoseconds = read
        .split_whitespace()
        .next()
        .and_then(|first| first.parse().ok())
        .expect("its first field is the nanoseconds the thread ran");
    Duration::from_nanos(nanoseconds)
}

/// The least of three CPU times of one `context` call over `stores`, as the verb makes it.
fn context(stores: &Stores) -> Duration {
    (0..3)
        .map(|_| {
            let started = cpu();
            let recollection = stores.recollection(16 * 1024);
            let answer = live_shape::context(&recollection);
            let took = cpu().saturating_sub(started);
            assert!(
                answer["memory"]
                    .as_str()
                    .is_some_and(|memory| memory.contains("derived:")),
                "{answer}"
            );
            took
        })
        .min()
        .expect("three timings")
}

/// Four times the lines cost `context` less than eight times the CPU: about four and a half
/// times after the index, and fourteen times when each lookup searched the tree. The ratio of
/// two CPU times holds on any machine and under any load the gate puts beside it.
#[test]
fn the_context_costs_what_the_store_holds_and_not_its_square() {
    let small = context(&store(1_000, 8));
    let large = context(&store(4_000, 32));
    assert!(
        large < small * 8,
        "1,000 lines took {small:?} of CPU and 4,000 took {large:?}"
    );
}

/// The live shape, about 13,000 lines over 100 conversations under a summarised tree, costs
/// under two seconds of CPU in an optimised build.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "a bound on the optimised build: cargo test --release -p suzatary-core --test all context_cost"
)]
fn the_context_at_the_live_shape_answers_in_two_seconds() {
    let took = context(&store(13_000, 100));
    assert!(took < Duration::from_secs(2), "{took:?} of CPU");
}
