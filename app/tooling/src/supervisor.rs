//! The one runner of the train's children: each in a process group of its own, signalled as a
//! group, and every reap, timer and signal received on one channel.

use rustix::event::{PollFd, PollFlags};
use rustix::process::{Pid, Signal, WaitOptions};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

/// What the runner tells its client, one at a time.
pub enum Event<K, A> {
    /// Every process of the child's group is gone: its leader's status, and what it printed
    /// where `Capture::Out` kept it.
    Exited {
        key: K,
        status: Status,
        out: String,
    },
    Signal(Signal),
    Woke(A),
}

/// A leader's exit status, as a shell gives it (128 plus the signal that killed it), read from
/// the kernel; or none, where no wait answered it (a child that never started, or one the kernel
/// reaped itself). No status is never a success.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Read(u8),
    Unread,
}

impl Status {
    /// The status after one answer of a wait on the group: the leader's own answer is read.
    #[must_use]
    pub fn reaped(self, of_leader: bool, code: u8) -> Status {
        if of_leader { Status::Read(code) } else { self }
    }
}

/// Keeps this process's children for the waits that read their status: a SIGCHLD inherited as
/// ignored lets the kernel reap them first, and with a handler of any kind it does not. `main`
/// calls it before anything runs a child, git included.
pub fn keep_children() -> std::io::Result<()> {
    signal_hook::flag::register(
        signal_hook::consts::SIGCHLD,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .map(|_| ())
}

/// The time the rest of a group has, after its leader ends or a stop, before KILL: above the
/// gate's own, so a gate told to stop has stopped its lanes before the train kills it.
pub const GRACE: Duration = Duration::from_secs(10);

/// Where a child's output goes.
pub enum Capture {
    /// Stdout kept for its `Exited`, stderr shown.
    Out,
    /// Stdout and stderr written to the file and shown on stderr, as `tee`.
    Tee(PathBuf),
    /// The train's own stdin, stdout and stderr, and its process group: a command the train
    /// runs on its own behalf (`train locked`), which the terminal's signals reach directly.
    Own,
}

enum Mail<K, A> {
    Event(Event<K, A>),
    Value(Box<dyn std::any::Any + Send>),
}

/// A child the runner started: a process group led by it, or a `Capture::Own` process alone.
#[derive(Clone, Copy, Debug)]
pub enum Child {
    Group(Pid),
    Process(Pid),
}

impl Child {
    /// Sends `signal` to every process of the group, or to the process; one already gone is no
    /// error.
    pub fn signal(self, signal: Signal) {
        let _ = match self {
            Child::Group(pid) => rustix::process::kill_process_group(pid, signal),
            Child::Process(pid) => rustix::process::kill_process(pid, signal),
        };
    }
}

pub struct Supervisor<K, A> {
    to: Sender<Mail<K, A>>,
    mail: Receiver<Mail<K, A>>,
    alarms: Vec<(Instant, A)>,
    /// Events that came while `until` waited, for `recv`.
    kept: VecDeque<Event<K, A>>,
}

impl<K: Send + 'static, A: Send + 'static> Supervisor<K, A> {
    /// The runner of this process, made once: HUP, INT and TERM become events, and the process
    /// is its children's subreaper, so a group's orphans are reaped here and a group is gone only
    /// when the last of them is.
    pub fn new() -> std::io::Result<Self> {
        rustix::process::set_child_subreaper(Some(rustix::process::getpid()))?;
        let (to, mail) = channel();
        let mut signals = signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
        ])?;
        let said = to.clone();
        std::thread::spawn(move || {
            for raw in signals.forever() {
                if let Some(signal) = Signal::from_named_raw(raw) {
                    let _ = said.send(Mail::Event(Event::Signal(signal)));
                }
            }
        });
        Ok(Supervisor {
            to,
            mail,
            alarms: Vec::new(),
            kept: VecDeque::new(),
        })
    }

    /// Starts `cmd` as `key`; its `Exited` comes once its whole group is gone. When the leader
    /// ends, the rest of its group is stopped (TERM, then KILL after `GRACE`), as the shell's
    /// `group_wait` did, and its output is read only until the group is gone, whatever else still
    /// holds the pipe. A command that cannot start ends at once, its status unread.
    pub fn start(&mut self, key: K, mut cmd: Command, capture: Capture) -> Option<Child> {
        let own = matches!(capture, Capture::Own);
        if !own {
            cmd.process_group(0).stdin(Stdio::null());
        }
        let spawned = (|| {
            let copy = match capture {
                Capture::Own => None,
                Capture::Out => {
                    cmd.stdout(Stdio::piped());
                    None
                }
                Capture::Tee(path) => {
                    let (read, write) = std::io::pipe()?;
                    cmd.stdout(write.try_clone()?).stderr(write);
                    Some((read, std::fs::File::create(path)?))
                }
            };
            // The wake-up is made before the child, so its failure is a start that failed.
            let wake = std::io::pipe()?;
            Ok::<_, std::io::Error>((cmd.spawn()?, copy, wake))
        })();
        let (child, copy, (woken, wake)) = match spawned {
            Ok(spawned) => spawned,
            Err(e) => {
                eprintln!("train: {} did not start: {e}", cmd.get_program().display());
                self.post(Event::Exited {
                    key,
                    status: Status::Unread,
                    out: String::new(),
                });
                return None;
            }
        };
        // The pipe's write ends the command holds go with it, so the reader sees the group's end.
        drop(cmd);
        let pid = Pid::from_child(&child);
        let stdout = child.stdout;
        let to = self.to.clone();
        std::thread::spawn(move || {
            let reader = std::thread::spawn(move || {
                let mut said = Vec::new();
                if let Some(out) = stdout {
                    drain(out, &woken, |bytes| said.extend_from_slice(bytes));
                }
                if let Some((read, mut log)) = copy {
                    drain(read, &woken, |bytes| {
                        let _ = log.write_all(bytes);
                        let _ = std::io::stderr().write_all(bytes);
                    });
                }
                String::from_utf8_lossy(&said).into_owned()
            });
            let status = if own { reap(pid) } else { reap_group(pid) };
            drop(wake);
            let out = reader.join().unwrap_or_default();
            let _ = to.send(Mail::Event(Event::Exited { key, status, out }));
        });
        Some(if own {
            Child::Process(pid)
        } else {
            Child::Group(pid)
        })
    }

    /// `alarm` comes as an event once `after` has passed.
    pub fn wake(&mut self, after: Duration, alarm: A) {
        self.alarms.push((Instant::now() + after, alarm));
    }

    /// An event as if the runner had seen it: a child that could not even be prepared.
    pub fn post(&self, event: Event<K, A>) {
        let _ = self.to.send(Mail::Event(event));
    }

    /// The next event; nothing else waits.
    pub fn recv(&mut self) -> Event<K, A> {
        if let Some(event) = self.kept.pop_front() {
            return event;
        }
        loop {
            let soonest = (0..self.alarms.len()).min_by_key(|&at| self.alarms[at].0);
            let mail = match soonest {
                None => self.mail.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(at) => self
                    .mail
                    .recv_timeout(self.alarms[at].0.saturating_duration_since(Instant::now())),
            };
            match (mail, soonest) {
                (Ok(Mail::Event(event)), _) => return event,
                (Err(RecvTimeoutError::Timeout), Some(at)) => {
                    return Event::Woke(self.alarms.swap_remove(at).1);
                }
                // The channel's senders live as long as the runner: neither happens.
                (Ok(Mail::Value(_)) | Err(_), _) => {}
            }
        }
    }

    /// `work`'s value, or the first signal that comes before it; events of children are kept
    /// for later. For a wait with no child, as the nightly's for the lock.
    pub fn until<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, Signal> {
        let to = self.to.clone();
        std::thread::spawn(move || to.send(Mail::Value(Box::new(work()))));
        loop {
            match self.mail.recv() {
                Ok(Mail::Value(value)) => {
                    if let Ok(value) = value.downcast::<T>() {
                        return Ok(*value);
                    }
                }
                Ok(Mail::Event(Event::Signal(signal))) => return Err(signal),
                Ok(Mail::Event(event)) => self.kept.push_back(event),
                Err(_) => {}
            }
        }
    }
}

/// Reaps every process of the group `pid` leads, orphans included; the leader's status. Once the
/// leader ends, the rest of the group is sent TERM, and KILL if it is not gone after `GRACE`.
fn reap_group(pid: Pid) -> Status {
    let mut status = Status::Unread;
    let mut gone = None;
    loop {
        match rustix::process::waitpgid(pid, WaitOptions::empty()) {
            Ok(Some((reaped, st))) => {
                status = status.reaped(reaped == pid, code(st));
                if reaped == pid {
                    let _ = rustix::process::kill_process_group(pid, Signal::TERM);
                    let (told, waiting) = channel::<()>();
                    gone = Some(told);
                    std::thread::spawn(move || {
                        if waiting.recv_timeout(GRACE) == Err(RecvTimeoutError::Timeout) {
                            let _ = rustix::process::kill_process_group(pid, Signal::KILL);
                        }
                    });
                }
            }
            Err(rustix::io::Errno::INTR) | Ok(None) => {}
            Err(_) => break,
        }
    }
    drop(gone);
    status
}

fn reap(pid: Pid) -> Status {
    loop {
        match rustix::process::waitpid(Some(pid), WaitOptions::empty()) {
            Ok(Some((_, st))) => return Status::Read(code(st)),
            Err(rustix::io::Errno::INTR) | Ok(None) => {}
            Err(_) => return Status::Unread,
        }
    }
}

/// Copies what `from` gives to `sink` until its end, or, once `woken` is readable (the group is
/// gone), what is waiting in it and no more.
fn drain(mut from: impl Read + AsFd, woken: &std::io::PipeReader, mut sink: impl FnMut(&[u8])) {
    let mut buf = [0; 8192];
    loop {
        let mut fds = [
            PollFd::new(&from, PollFlags::IN),
            PollFd::new(woken, PollFlags::IN),
        ];
        match rustix::event::poll(&mut fds, None) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return,
        }
        let (data, woke) = (!fds[0].revents().is_empty(), !fds[1].revents().is_empty());
        if woke {
            let _ = rustix::io::ioctl_fionbio(&from, true);
            while let Ok(n @ 1..) = from.read(&mut buf) {
                sink(&buf[..n]);
            }
            return;
        }
        if data {
            match from.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => sink(&buf[..n]),
            }
        }
    }
}

fn code(st: rustix::process::WaitStatus) -> u8 {
    let raw = st
        .exit_status()
        .or_else(|| st.terminating_signal().map(|signal| 128 + signal))
        .unwrap_or(1);
    u8::try_from(raw).unwrap_or(1)
}
