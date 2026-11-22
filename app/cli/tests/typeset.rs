//! The closed world's bound, against the real child: `suzatary typeset` as the box runs it.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use suzatary_base::capability::Typeset;
use suzatary_io::typeset::Closed;
use suzatary_io::typeset::closed::{BACKSTOP, CPU_SECONDS};

fn closed() -> Closed {
    crate::binary::typeset()
}

#[test]
fn the_child_answers_as_the_compile_does_in_process() {
    use suzatary_io::typeset::Compiler as _;
    let closed = closed();
    for source in ["Renew *passport*", "#html.elem(\"b\")[x]", "a ] b"] {
        assert_eq!(
            closed.closed(source),
            suzatary_io::typeset::compile(&suzatary_todo::template::Job::text(source)),
            "{source}"
        );
    }
    for template in suzatary_todo::rendering::all() {
        for job in template.examples() {
            assert_eq!(
                closed.compile(&job),
                suzatary_io::typeset::compile(&job),
                "{}",
                template.main()
            );
        }
    }
}

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
            suzatary_io::typeset::compile(&suzatary_todo::template::Job::text("Renew *passport*"))
        );
    }
    drop(load);
}

/// A box building a deploy can start a child slowly, and that wait uses no CPU.
#[test]
fn a_slow_start_is_not_counted_against_the_text() {
    let dir = suzatary_testkit::scratch("typeset");
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
        suzatary_io::typeset::compile(&suzatary_todo::template::Job::text("Renew *passport*"))
    );
}

#[test]
fn a_child_that_never_answers_is_stopped_at_the_backstop() {
    let dir = suzatary_testkit::scratch("typeset");
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

/// The child sets its address-space cap on itself before it reads the text.
#[test]
fn unbounded_memory_is_stopped_at_its_cap() {
    let started = Instant::now();
    let refused = closed()
        // One request for three gigabytes reaches the cap at once, before a busy box spends the
        // CPU limit filling a range.
        .closed("#(\"x\" * 3000000000).len()")
        .expect_err("two gigabytes is not enough for three");
    assert!(!refused.diagnostics.is_empty());
    assert!(
        refused.to_string().contains("the compile stopped"),
        "stopped by the address-space cap, not by the CPU limit or a typst error: {refused}"
    );
    assert!(started.elapsed() < Duration::from_secs(30), "{refused}");
}
