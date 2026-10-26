//! `claude setup-token`, driven from a card rather than a terminal.
//!
//! The CLI draws an interactive terminal program, so it runs in a pseudo-terminal this process
//! holds: what it draws is read as a [`Screen`], its sign-in URL is handed to the card, the code
//! the owner pastes into the card is typed into it, and the token it prints is captured as a
//! [`Secret`] that only the caller stores. Nothing it draws after the code is typed is kept or
//! said: an error past that point names what was waited for, never the screen.

mod screen;

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use rustix::pty::OpenptFlags;
use suzatary_core::capability::Secret;
use suzatary_core::secretary::sign_in::SignInUrl;

pub use screen::Screen;

/// The subcommand that mints a long-lived subscription token.
pub const SUBCOMMAND: &str = "setup-token";

/// Wide enough that the sign-in URL is drawn on one line, never wrapped.
const COLUMNS: u16 = 1000;

/// Between the code and the key that submits it, so a paste and an Enter arrive as two inputs.
const SETTLE: Duration = Duration::from_millis(150);

/// A sign-in in progress: the CLI running in a terminal this process holds, its URL drawn.
pub struct SignIn {
    running: Running,
    url: SignInUrl,
}

/// The CLI running in a terminal this process holds, and what it has drawn so far.
struct Running {
    child: Child,
    terminal: File,
    drawn: Receiver<Vec<u8>>,
    screen: Screen,
}

/// Why a sign-in did not give a token; never carries the code or the token.
#[derive(Debug, thiserror::Error)]
pub enum SignInError {
    /// The pseudo-terminal could not be opened.
    #[error("the terminal for the sign-in could not be opened: {0}")]
    Terminal(#[from] std::io::Error),
    /// The CLI could not be started.
    #[error("{program} could not be run: {source}")]
    Spawn {
        /// The program, as given.
        program: String,
        /// The operating system's reason.
        source: std::io::Error,
    },
    /// The CLI drew no sign-in URL in time.
    #[error("no sign-in link was drawn within {after:?}; the CLI last said: {said}")]
    NoUrl {
        /// How long it was given.
        after: Duration,
        /// The end of what it drew, which holds no code or token yet.
        said: String,
    },
    /// The CLI exited before what was waited for.
    #[error("the sign-in ended before {waiting}")]
    Ended {
        /// What was waited for.
        waiting: &'static str,
    },
    /// The CLI said the code was refused.
    #[error(
        "that code was not accepted: copy the whole code the sign-in page shows, or tap Connect again"
    )]
    Refused,
    /// The code went in and no token came out in time.
    #[error("no token was printed within {after:?}: the code may be wrong or expired")]
    NoToken {
        /// How long it was given.
        after: Duration,
    },
}

impl SignIn {
    /// Start `program setup-token` in a fresh terminal and wait up to `wait` for its URL.
    ///
    /// # Errors
    /// [`SignInError`] for a terminal that does not open, a program that does not start, or no
    /// URL in time.
    pub fn start(program: &str, wait: Duration) -> Result<Self, SignInError> {
        let terminal =
            rustix::pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)
                .map_err(std::io::Error::from)?;
        rustix::pty::grantpt(&terminal).map_err(std::io::Error::from)?;
        rustix::pty::unlockpt(&terminal).map_err(std::io::Error::from)?;
        rustix::termios::tcsetwinsize(
            &terminal,
            rustix::termios::Winsize {
                ws_row: 50,
                ws_col: COLUMNS,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
        )
        .map_err(std::io::Error::from)?;
        let peer = rustix::pty::ioctl_tiocgptpeer(
            &terminal,
            OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
        )
        .map_err(std::io::Error::from)?;
        // The CLI must mint a token of its own, not report the one this box already runs on, so
        // it is given `run`'s base, the proxy and its config directory and nothing else (no
        // token, no key); it opens no browser on the box (the link goes to the card), and runs
        // in a group of its own, so ending the sign-in ends whatever it started.
        let child = Command::new(program)
            .arg(SUBCOMMAND)
            .env_clear()
            .envs(crate::run::environment(
                crate::run::NETWORK
                    .iter()
                    .copied()
                    .chain(["CLAUDE_CONFIG_DIR"]),
            ))
            .envs(crate::model::QUIET.iter().copied())
            .env("TERM", "xterm-256color")
            .env("BROWSER", "true")
            .process_group(0)
            .stdin(Stdio::from(peer.try_clone()?))
            .stdout(Stdio::from(peer.try_clone()?))
            .stderr(Stdio::from(peer))
            .spawn()
            .map_err(|source| SignInError::Spawn {
                program: program.to_owned(),
                source,
            })?;
        let terminal = File::from(terminal);
        let mut reading = terminal.try_clone()?;
        let (sender, drawn) = channel();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            // A read past the CLI's exit fails (EIO on Linux): that is the end of what it drew.
            while let Ok(read @ 1..) = reading.read(&mut buffer) {
                if sender.send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut running = Running {
            child,
            terminal,
            drawn,
            screen: Screen::default(),
        };
        match running.until(wait, Screen::url) {
            Ok(Some(url)) => Ok(Self { running, url }),
            Ok(None) => Err(SignInError::NoUrl {
                after: wait,
                said: running.screen.tail(200),
            }),
            Err(()) => Err(SignInError::Ended {
                waiting: "it drew a sign-in link",
            }),
        }
    }

    /// The URL the owner opens to sign in.
    #[must_use]
    pub fn url(&self) -> &SignInUrl {
        &self.url
    }

    /// Type `code` and wait up to `wait` for the token it is exchanged for.
    ///
    /// # Errors
    /// [`SignInError`] for a code that cannot be typed, a CLI that exits first, or no token in
    /// time.
    pub fn finish(self, code: &Secret, wait: Duration) -> Result<Secret, SignInError> {
        let mut running = self.running;
        let code = code.expose().trim();
        let mark = running.screen.mark();
        running.terminal.write_all(code.as_bytes())?;
        running.terminal.flush()?;
        std::thread::sleep(SETTLE);
        running.terminal.write_all(b"\r")?;
        running.terminal.flush()?;
        let answer = |screen: &Screen| match screen.token() {
            Some(token) => Some(Ok(Secret::new(token))),
            None => screen.refused_after(mark, code).then_some(Err(())),
        };
        match running.until(wait, answer) {
            Ok(Some(Ok(token))) => Ok(token),
            Ok(Some(Err(()))) => Err(SignInError::Refused),
            Ok(None) => Err(SignInError::NoToken { after: wait }),
            Err(()) => Err(SignInError::Ended {
                waiting: "it printed a token",
            }),
        }
    }
}

impl Running {
    /// Read what is drawn until `found` answers, `wait` passes (`Ok(None)`), or the CLI is done
    /// drawing with nothing found (`Err`).
    fn until<A>(
        &mut self,
        wait: Duration,
        found: impl Fn(&Screen) -> Option<A>,
    ) -> Result<Option<A>, ()> {
        let deadline = Instant::now() + wait;
        loop {
            if let Some(answer) = found(&self.screen) {
                return Ok(Some(answer));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.drawn.recv_timeout(left) {
                Ok(bytes) => self.screen.feed(&bytes),
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => {
                    return found(&self.screen).map(Some).ok_or(());
                }
            }
        }
    }
}

impl Drop for Running {
    /// A sign-in left unfinished leaves no CLI behind, nor anything it started.
    fn drop(&mut self) {
        let group = i32::try_from(self.child.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw);
        if let Some(group) = group {
            let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "fake-code#fake-state";
    const URL: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=fake\
                       &response_type=code&state=s1";
    const TOKEN: &str = "sk-ant-oat01-FAKEtoken_for-tests-only";

    /// A `claude` that draws what the real `setup-token` (2.1.292) draws: each word placed at its
    /// column, the link in a hyperlink, the terminal queries after the prompt. It refuses to run
    /// off a terminal as Ink does, and prints a token for one code.
    fn fake(dir: &std::path::Path) -> String {
        suzatary_testkit::fake_program(
            dir,
            "claude",
            &format!(
                r#"#!/bin/sh
[ "$1" = "setup-token" ] || exit 2
if [ ! -t 0 ] || [ ! -t 1 ]; then
  echo "Error: Raw mode is not supported on the current process.stdin" >&2
  exit 1
fi
[ -z "$CLAUDE_CODE_OAUTH_TOKEN" ] || echo "Warning: You already have authentication configured via environment variable or API key helper."
printf '\033[2J\033[H\033[1mThis will guide you through long-lived (1-year) auth token setup for your Claude account. Claude subscription required.\033[22m\r\n\r\n'
printf "\033[2G\033[38;5;246mBrowser\033[10Gdidn't\033[17Gopen?\033[23GUse\033[27Gthe\033[31Gurl\033[35Gbelow\033[41Gto\033[44Gsign\033[49Gin\033[39m\r\r\n\r\r\n"
printf '\033]8;id=1;{URL}\007\033[38;5;246m{URL}\033[39m\033]8;;\007\r\r\n\r\r\n'
printf '\033[2GPaste\033[8Gcode\033[13Ghere\033[18Gif\033[21Gprompted\033[30G>\r\r\n\033[>0q\033[?u\033[c'
read -r code
while [ "$code" != "{CODE}" ]; do
  printf '\r\nOAuth error: Invalid code. Please make sure the full code was copied\r\n\033[2GPaste\033[8Gcode\033[13Ghere\033[18Gif\033[21Gprompted\033[30G>\r\r\n'
  read -r code
done
printf '\r\n\033[32m\342\234\223\033[39m Long-lived authentication token created successfully!\r\n\r\n'
printf 'Your OAuth token (valid for 1 year):\r\n\r\n{TOKEN}\r\n\r\n'
printf "Store this token securely. You won't be able to see it again.\r\n\r\n"
printf 'Use this token by setting: export CLAUDE_CODE_OAUTH_TOKEN=<token>\r\n'
"#
            ),
        )
        .display()
        .to_string()
    }

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn the_url_comes_out_the_code_goes_in_and_the_token_is_captured() {
        let dir = tempfile::tempdir().expect("a directory");
        let signing = SignIn::start(&fake(dir.path()), WAIT).expect("a URL");
        assert_eq!(signing.url().as_str(), URL);
        let token = signing
            .finish(&Secret::new(format!("{CODE}\n")), WAIT)
            .expect("a token");
        assert_eq!(token.expose(), TOKEN);
    }

    /// The real CLI asks again after a wrong code; the sign-in says so at once, not when its
    /// wait is up, and says nothing it drew.
    #[test]
    fn a_wrong_code_is_refused_at_once_and_says_nothing_it_drew() {
        let dir = tempfile::tempdir().expect("a directory");
        let signing = SignIn::start(&fake(dir.path()), WAIT).expect("a URL");
        let began = Instant::now();
        let refused = signing
            .finish(&Secret::new("not-the-code"), Duration::from_mins(1))
            .expect_err("no token for a wrong code");
        assert!(matches!(refused, SignInError::Refused), "{refused}");
        assert!(
            began.elapsed() < WAIT,
            "refused after {:?}",
            began.elapsed()
        );
        assert!(!refused.to_string().contains("not-the-code"));
    }

    /// Ending a sign-in ends what its CLI started too: the whole group goes.
    #[test]
    fn dropping_a_sign_in_ends_its_whole_group() {
        let dir = tempfile::tempdir().expect("a directory");
        let pidfile = dir.path().join("child.pid");
        let script = suzatary_testkit::fake_program(
            dir.path(),
            "claude",
            &format!(
                "#!/bin/sh\nsleep 300 &\necho $! > {}\nprintf '{URL}\\r\\n'\nwait\n",
                pidfile.display()
            ),
        );
        let signing = SignIn::start(&script.display().to_string(), WAIT).expect("a URL");
        let child = std::fs::read_to_string(&pidfile).expect("the child's pid");
        // A killed process nobody has reaped yet is a zombie: dead, though /proc still names it.
        let alive = || {
            std::fs::read_to_string(format!("/proc/{}/stat", child.trim())).is_ok_and(|stat| {
                stat.rsplit(')')
                    .next()
                    .is_some_and(|rest| !rest.trim_start().starts_with('Z'))
            })
        };
        assert!(alive());
        drop(signing);
        let deadline = Instant::now() + WAIT;
        while alive() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "the CLI's child outlived the sign-in");
    }
}
