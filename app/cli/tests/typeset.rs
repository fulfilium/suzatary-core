//! The closed world's bound, against the real child: `suzatary typeset` as the box runs it.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use suzatary_core::capability::Typeset;
use suzatary_io::typeset::Closed;
use suzatary_io::typeset::closed::{BACKSTOP, CPU_SECONDS};

fn closed() -> Closed {
    crate::binary::typeset()
}

#[test]
fn the_child_answers_as_the_compile_does_in_process() {
    let closed = closed();
    for source in ["Renew *passport*", "#html.elem(\"b\")[x]", "a ] b"] {
        assert_eq!(
            closed.closed(source),
            suzatary_io::typeset::compile(source),
            "{source}"
        );
    }
}

/// A text that would compile for hours is refused by the child's CPU limit, long before the
/// wall-clock backstop.
#[test]
fn a_text_that_runs_forever_is_refused_by_its_cpu_limit() {
    let started = Instant::now();
    let refused = closed()
        .closed("#for i in range(100000) { for j in range(100000) { } }")
        .expect_err("it never finishes");
    assert!(
        refused
            .to_string()
            .contains(&format!("within {CPU_SECONDS} s of CPU time")),
        "{refused}"
    );
    assert!(started.elapsed() < BACKSTOP / 2, "{:?}", started.elapsed());
}

/// Processes that use every CPU until they are dropped.
struct Load(Vec<Child>);

impl Load {
    fn every_cpu_twice() -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
        Self(
            (0..2 * cpus)
                .map(|_| {
                    Command::new("sh")
                        .args(["-c", "while :; do :; done"])
                        .stdin(Stdio::null())
                        .spawn()
                        .expect("a busy loop")
                })
                .collect(),
        )
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A busy box slows a compile down without adding to its CPU time, so a short text compiles.
#[test]
fn a_short_text_compiles_while_the_box_is_busy() {
    let load = Load::every_cpu_twice();
    let closed = closed();
    for _ in 0..5 {
        assert_eq!(
            closed.closed("Renew *passport*"),
            suzatary_io::typeset::compile("Renew *passport*")
        );
    }
    drop(load);
}

/// A start that waits longer than the old five-second wall-clock limit, as a child of a box
/// building a deploy can, uses no CPU and is not counted against the text.
#[test]
fn a_slow_start_is_not_counted_against_the_text() {
    let dir = tempfile::tempdir().expect("scratch");
    let script = suzatary_testkit::fake_program(
        dir.path(),
        "slow-start",
        &format!(
            "#!/bin/sh\nsleep 6\nexec '{}' \"$@\"\n",
            crate::binary::path()
        ),
    );
    assert_eq!(
        Closed::new(&script, BACKSTOP).closed("Renew *passport*"),
        suzatary_io::typeset::compile("Renew *passport*")
    );
}

/// A child that waits without using CPU is stopped at the backstop and named as such.
#[test]
fn a_child_that_never_answers_is_stopped_at_the_backstop() {
    let dir = tempfile::tempdir().expect("scratch");
    let script = suzatary_testkit::fake_program(dir.path(), "stuck", "#!/bin/sh\nexec sleep 30\n");
    let started = Instant::now();
    let refused = Closed::new(&script, Duration::from_millis(500))
        .closed("Renew *passport*")
        .expect_err("it never answers");
    assert!(
        refused.to_string().contains("did not answer within 500ms"),
        "{refused}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

/// A text that allocates without end is stopped at the child's address-space cap, which the child
/// sets on itself before it reads the text: refused by the cap, not by the CPU limit.
#[test]
fn unbounded_memory_is_stopped_at_its_cap() {
    let started = Instant::now();
    let refused = closed()
        // One request for three gigabytes, so the cap is reached at once and not after seconds of
        // filling a range, which a busy box could spend past the CPU limit first.
        .closed("#(\"x\" * 3000000000).len()")
        .expect_err("two gigabytes is not enough for three");
    assert!(!refused.diagnostics.is_empty());
    assert!(
        refused.to_string().contains("the compile stopped"),
        "stopped by the address-space cap, not by the CPU limit or a typst error: {refused}"
    );
    assert!(started.elapsed() < Duration::from_secs(30), "{refused}");
}
