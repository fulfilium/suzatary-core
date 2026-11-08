//! The `claude` CLI as a [`Model`] and a [`SessionModel`].
//!
//! It is given no tools: `--tools ""`, `--strict-mcp-config` with no config and a deny-all
//! `--settings` [`Fence`] keep the account's own servers (a mail sender among them) and the
//! box's files out of reach of a prompt that carries other people's mail. All three are needed.

pub mod failure;
pub mod sessions;
pub mod stream;
pub mod stream_event;
pub mod summary;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use suzatary_core::capability::{
    Clock, Conversation, Delta, Halt, Model, ModelError, Reply, Secret, Session, SessionId,
    SessionModel, Sessioned, TokenUsage, Turn,
};
use suzatary_core::for_model::ForModel;
use suzatary_core::secretary::store::stamp;

use crate::attachments::AttachmentStore;
use crate::blobs::Blobs;
use crate::blocks::{Blocks, Stores};
use crate::clock::SystemClock;
use crate::prompt::{delta_prompt, prompt_of};
use crate::run::{Cmd, RunError, run, watch, zero};
use crate::secrets::SecretError;
use failure::{Heard, failed};
use stream::{Ended, Unreadable, delta_of, result_of};

/// The long-lived setup token an unattended run authenticates with.
pub const TOKEN: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// The reason secretspec's audit log records for reading [`TOKEN`].
pub const REASON: &str = "the secretary answers a conversation";

const DEFAULT_TIMEOUT: Duration = Duration::from_mins(5);

/// Pinned to an hour because the CLI's five-minute default expires between
/// rounds of a five-minute timer.
const CACHE_TTL_VAR: &str = "CLAUDE_CODE_PROMPT_CACHE_TTL";

/// Where the CLI reads its user configuration: settings, a user-level `CLAUDE.md`, credentials.
const CONFIG_DIR_VAR: &str = "CLAUDE_CONFIG_DIR";

/// The deny-all settings every call is fenced by (`.claude/secretary-settings.json`); a call
/// whose file is gone is refused, never run under the box user's own rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fence(PathBuf);

impl Fence {
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Whether the file is there to be read, so a call may be fenced by it.
    #[must_use]
    pub fn stands(&self) -> bool {
        self.0.is_file()
    }
}

/// Where the setup token is asked for: once per call, so a store unlocked after the
/// server started counts at the next turn, with no restart.
#[derive(Clone)]
pub struct TokenSource(Arc<dyn Fn() -> Result<Option<Secret>, SecretError> + Send + Sync>);

impl TokenSource {
    /// `ask`, called before every run of the CLI.
    #[must_use]
    pub fn new(
        ask: impl Fn() -> Result<Option<Secret>, SecretError> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(ask))
    }

    /// Always `token`: a value already in hand, as a key being verified before it is kept.
    #[must_use]
    pub fn fixed(token: Secret) -> Self {
        Self::new(move || Ok(Some(token.clone())))
    }

    #[must_use]
    pub fn from_store(store: crate::secrets::Secretspec) -> Self {
        Self::new(move || store.get(TOKEN, REASON))
    }

    fn ask(&self) -> Result<Option<Secret>, SecretError> {
        (self.0)()
    }
}

impl Default for TokenSource {
    fn default() -> Self {
        Self::new(|| Ok(None))
    }
}

impl std::fmt::Debug for TokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenSource")
    }
}

/// The `claude` CLI, as this box runs it.
#[derive(Debug, Clone)]
pub struct ClaudeCli {
    binary: String,
    model: String,
    fence: Fence,
    cwd: Option<PathBuf>,
    config: Option<PathBuf>,
    token: TokenSource,
    timeout: Duration,
    stores: Stores,
}

impl ClaudeCli {
    #[must_use]
    pub fn new(model: impl Into<String>, fence: Fence) -> Self {
        Self {
            binary: "claude".to_owned(),
            model: model.into(),
            fence,
            cwd: None,
            config: None,
            token: TokenSource::default(),
            timeout: DEFAULT_TIMEOUT,
            stores: Stores::default(),
        }
    }

    /// Run it from `cwd`, so that directory's `CLAUDE.md` is its project memory.
    #[must_use]
    pub fn in_directory(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    /// Read the CLI's user configuration from the empty `config`, so no user-level `CLAUDE.md`
    /// or setting reaches the call.
    #[must_use]
    pub fn with_config_dir(mut self, config: PathBuf) -> Self {
        self.config = Some(config);
        self
    }

    /// Where its runs keep sessions.
    #[must_use]
    pub fn sessions(&self) -> sessions::CliSessions {
        sessions::CliSessions::new(self.config.clone())
    }

    /// The setup token `token` answers before each call; a store that cannot answer refuses
    /// the call, so a locked store never runs the model under credentials nobody chose.
    #[must_use]
    pub fn with_token(mut self, token: TokenSource) -> Self {
        self.token = token;
        self
    }

    #[must_use]
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Where the photos a turn names are read from; without one a turn with a
    /// photo is refused rather than sent without it.
    #[must_use]
    pub fn with_uploads(mut self, store: AttachmentStore) -> Self {
        self.stores.uploads = Some(store);
        self
    }

    /// Where the files a verb handed over are read from; without one such a
    /// turn is refused rather than sent without them.
    #[must_use]
    pub fn with_files(mut self, store: Blobs) -> Self {
        self.stores.files = Some(store);
        self
    }

    /// The token this call runs under, once the store has answered and the fence stands;
    /// every mode asks here before anything is spawned.
    fn ready(&self) -> Result<Option<Secret>, ModelError> {
        let token = self
            .token
            .ask()
            .map_err(|locked| ModelError::Unreachable(locked.to_string()))?;
        if !self.fence.stands() {
            return Err(ModelError::Unreachable(format!(
                "the model's tool fence {} is not there, and the model is not run without it",
                self.fence.path().display()
            )));
        }
        Ok(token)
    }

    fn feed(
        &self,
        prompt: ForModel,
        turns: &[Turn],
    ) -> Result<(Stdin, Option<Secret>), ModelError> {
        let token = self.ready()?;
        Ok((stdin_of(prompt, Blocks::of(&self.stores, turns)?), token))
    }

    /// The prompt goes down stdin: a day's conversation outgrows one argv
    /// string's 128 KB.
    fn command(&self, mode: Mode<'_>, stdin: Stdin, token: Option<&Secret>) -> Cmd {
        let mut cmd = Cmd::new(&self.binary)
            .args(argv(&self.model, &self.fence, mode))
            .args(stdin.args)
            .stdin(stdin.payload)
            // Where the CLI keeps its own login, when not under HOME; the token is set below.
            .pass(&["CLAUDE_CONFIG_DIR"])
            .pass(crate::run::NETWORK)
            .env(CACHE_TTL_VAR, "1h")
            .timeout(self.timeout);
        for (name, value) in QUIET {
            cmd = cmd.env(name, value);
        }
        if let Some(cwd) = &self.cwd {
            cmd = cmd.cwd(cwd);
        }
        if let Some(config) = &self.config {
            cmd = cmd.env(CONFIG_DIR_VAR, config.display().to_string());
        }
        if let Some(token) = token {
            cmd = cmd.env(TOKEN, token.expose());
        }
        cmd
    }

    fn failure(&self, ran: RunError) -> ModelError {
        match ran {
            RunError::Spawn { source, .. } => {
                ModelError::Unreachable(format!("{}: {source}", self.binary))
            }
            RunError::TimedOut { .. } => ModelError::TimedOut(self.timeout),
            RunError::Halted { .. } => ModelError::Halted,
            RunError::Wait { source, .. } => failed(source.to_string(), now()),
            exited @ RunError::Exited { .. } => failed(exited.to_string(), now()),
        }
    }

    /// `cmd` run to its end and its result line read, or why it failed: what its stdout said
    /// first, since the CLI prints a limit there and not on stderr.
    fn ended(&self, cmd: &Cmd) -> Result<Ended, ModelError> {
        let ran = run(cmd).map_err(|e| self.failure(e))?;
        let out = ran.out();
        let heard = Heard::of(&out);
        zero(cmd, ran).map_err(|exited| heard.failure(exited.to_string(), now()))?;
        clean(last_result(&out, &heard)?, &heard)
    }
}

fn now() -> suzatary_core::health::Instant {
    prodrome_core::fpl::instant_of(SystemClock.now())
}

/// The name the CLI at `model` answers as, and its calls are recorded under.
#[must_use]
pub fn name_of(model: &str) -> String {
    format!("{}/{model}", suzatary_core::health::SUBSCRIPTION)
}

impl Model for ClaudeCli {
    fn name(&self) -> String {
        name_of(&self.model)
    }

    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        let prompt = prompt_of(conversation, tools, crate::prompt::Reads::Files);
        let (stdin, token) = self.feed(prompt.user, &conversation.turns)?;
        let ended = self.ended(&self.command(
            Mode::Stateless {
                system: &prompt.system,
            },
            stdin,
            token.as_ref(),
        ))?;
        let usage = ended.usage.map(TokenUsage::from);
        Ok(Reply::parse(&reply_of(ended)?).with_usage(usage))
    }
}

impl ClaudeCli {
    /// `text` down stdin under `system`, with no session and none of the secretary's framing;
    /// run it from an empty directory and configuration, so no `CLAUDE.md` is found.
    ///
    /// # Errors
    /// [`ModelError`] when the CLI could not be run, failed, printed no result or one this box
    /// cannot read, or ended with no reply.
    pub fn plain(&self, system: &ForModel, text: ForModel) -> Result<String, ModelError> {
        let token = self.ready()?;
        let stdin = stdin_of(text, Blocks::default());
        reply_of(self.ended(&self.command(Mode::Plain { system }, stdin, token.as_ref()))?)
    }

    /// The tokens of the prompt that [`ClaudeCli::plain`] sends for `text` under `system`, as
    /// the model's tokenizer counted them: fresh, written to the cache and read from it.
    ///
    /// # Errors
    /// As [`ClaudeCli::plain`], and [`ModelError::Failed`] when the result carries no usage.
    pub fn prompt_tokens(&self, system: &ForModel, text: ForModel) -> Result<u64, ModelError> {
        let token = self.ready()?;
        let stdin = stdin_of(text, Blocks::default());
        let ended = self.ended(&self.command(Mode::Plain { system }, stdin, token.as_ref()))?;
        let usage = ended
            .usage
            .ok_or_else(|| ModelError::Failed("the model's result carried no usage".to_owned()))?;
        Ok(usage.input_tokens + usage.cache_creation_input_tokens + usage.cache_read_input_tokens)
    }
}

/// The stream's last `result` line. A stream with none is the CLI's own prose, as a signed-out
/// CLI prints, unless what it `heard` says more; one this box cannot read says so, never that the
/// CLI is signed out.
fn last_result(out: &str, heard: &Heard) -> Result<Ended, ModelError> {
    out.lines()
        .rev()
        .find_map(|line| result_of(line).transpose())
        .ok_or_else(|| {
            heard.failure(
                "the model printed no result (not logged in?)".to_owned(),
                now(),
            )
        })?
        .map_err(|unreadable| ModelError::Failed(unreadable.to_string()))
}

fn reply_of(ended: Ended) -> Result<String, ModelError> {
    ended.result.ok_or_else(|| {
        ModelError::Failed(format!(
            "the model's run ended {} with no reply",
            ended.subtype
        ))
    })
}

/// A result line the CLI marked an error is a failure, never a reply: its text is the CLI's own
/// words ("Prompt is too long"), which would otherwise reach the owner as the model's answer.
fn clean(ended: Ended, heard: &Heard) -> Result<Ended, ModelError> {
    if !ended.is_error {
        return Ok(ended);
    }
    Err(heard.failure(format!("the model's run ended {}", ended.subtype), now()))
}

impl SessionModel for ClaudeCli {
    fn open(
        &self,
        id: &SessionId,
        conversation: &Conversation,
        tools: &serde_json::Value,
        halt: &Halt,
    ) -> Result<Sessioned, ModelError> {
        let prompt = prompt_of(conversation, tools, crate::prompt::Reads::Files);
        let (stdin, token) = self.feed(prompt.user, &conversation.turns)?;
        let created = stamp(SystemClock.now());
        let cmd = self.command(
            Mode::Open {
                session_id: id,
                system: &prompt.system,
            },
            stdin,
            token.as_ref(),
        );
        self.exchange(id, &cmd, &|_| {}, halt, created)
    }

    /// No tool list: the session opened with one and remembers it.
    fn resume(
        &self,
        system: &str,
        session: &Session,
        delta: &Delta,
        _tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Sessioned, ModelError> {
        let (stdin, token) = self.feed(
            delta_prompt(delta, crate::prompt::Reads::Files),
            delta.turns(),
        )?;
        let system = ForModel::of(system);
        let cmd = self.command(
            Mode::Resume {
                session_id: &session.id,
                system: &system,
            },
            stdin,
            token.as_ref(),
        );
        self.exchange(&session.id, &cmd, on_delta, halt, session.created.clone())
    }
}

impl ClaudeCli {
    /// One sessioned exchange, streamed line by line into `on_delta`.
    ///
    /// # Errors
    /// [`ModelError::Lost`] only when a failed run's stderr names the session or says "No
    /// conversation found"; a bad flag or a network failure also fails.
    fn exchange(
        &self,
        id: &SessionId,
        cmd: &Cmd,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
        created: String,
    ) -> Result<Sessioned, ModelError> {
        let mut assembled = String::new();
        let mut result: Option<Result<Ended, Unreadable>> = None;
        let mut heard = Heard::default();
        watch(
            cmd,
            &mut |line| {
                heard.line(line);
                if let Some(text) = delta_of(line) {
                    on_delta(&text);
                    assembled.push_str(&text);
                }
                if let Some(found) = result_of(line).transpose() {
                    result = Some(found);
                }
            },
            &|| halt.halted(),
        )
        .and_then(|ran| zero(cmd, ran))
        .map_err(|failed| match failed {
            RunError::Exited { stderr, .. }
                if stderr.as_str().contains(id.as_str())
                    || stderr
                        .as_str()
                        .to_lowercase()
                        .contains("no conversation found") =>
            {
                ModelError::Lost(stderr.to_string())
            }
            exited @ RunError::Exited { .. } => heard.failure(exited.to_string(), now()),
            other => self.failure(other),
        })?;
        let result = result
            .transpose()
            .map_err(|unreadable| ModelError::Failed(unreadable.to_string()))?
            .map(|ended| clean(ended, &heard))
            .transpose()?;
        // A sessioned run's reply is also its streamed deltas, so a result with no text keeps them.
        let (text, usage) = match result {
            Some(Ended {
                result: text,
                usage,
                ..
            }) => (text.unwrap_or(assembled), usage.map(TokenUsage::from)),
            None => (assembled, None),
        };
        Ok(Sessioned {
            reply: Reply::parse(&text).with_usage(usage),
            session: Session {
                id: id.clone(),
                model: self.model.clone(),
                cwd: self
                    .cwd
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                created,
                last: stamp(SystemClock.now()),
                surface: None,
            },
        })
    }
}

/// What every `claude` this box starts is told, so it reaches the model and nothing else: no
/// telemetry, error reports, feature flags, feedback prompts or update checks.
pub const QUIET: &[(&str, &str)] = &[
    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ("DISABLE_TELEMETRY", "1"),
    ("DISABLE_ERROR_REPORTING", "1"),
    ("DISABLE_GROWTHBOOK", "1"),
    ("DISABLE_AUTOUPDATER", "1"),
    ("DISABLE_BUG_COMMAND", "1"),
    ("DISABLE_FEEDBACK_COMMAND", "1"),
    ("CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY", "1"),
    ("DO_NOT_TRACK", "1"),
];

/// Which invocation a call to `claude` is, each with its manual as the system prompt.
#[derive(Debug, Clone, Copy)]
pub enum Mode<'a> {
    /// [`Model::turn`]: the whole conversation, no session.
    Stateless { system: &'a ForModel },
    /// [`SessionModel::open`]: start a session under an id the box minted.
    Open {
        /// The id `--session-id` names.
        session_id: &'a SessionId,
        system: &'a ForModel,
    },
    /// [`SessionModel::resume`]: continue one.
    Resume {
        /// The id `--resume` names.
        session_id: &'a SessionId,
        system: &'a ForModel,
    },
    /// [`ClaudeCli::plain`]: no tools, no session, none of the secretary's framing.
    Plain { system: &'a ForModel },
}

impl Mode<'_> {
    /// The system prompt it carries.
    #[must_use]
    pub fn system(&self) -> &ForModel {
        match self {
            Mode::Stateless { system }
            | Mode::Open { system, .. }
            | Mode::Resume { system, .. }
            | Mode::Plain { system } => system,
        }
    }
}

/// The exact argv for `mode`. Every mode carries the fence (no tools, no MCP
/// server, the deny-all settings) and nothing that could widen it
/// (`--allowedTools`, `--permission-mode`, `--mcp-config`).
#[must_use]
pub fn argv(model: &str, fence: &Fence, mode: Mode<'_>) -> Vec<String> {
    let mut built = vec!["-p".to_owned(), "--model".to_owned(), model.to_owned()];
    let streamed = |flag: &str, id: &SessionId| {
        [
            flag,
            id.as_str(),
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--verbose",
        ]
        .map(str::to_owned)
    };
    // Stream-json out in every mode: a request that carries a file goes in as stream-json,
    // and the CLI takes that only with stream-json out.
    let unsessioned = ["--output-format", "stream-json", "--verbose"].map(str::to_owned);
    match mode {
        Mode::Stateless { .. } | Mode::Plain { .. } => built.extend(unsessioned),
        Mode::Open { session_id, .. } => built.extend(streamed("--session-id", session_id)),
        Mode::Resume { session_id, .. } => built.extend(streamed("--resume", session_id)),
    }
    built.extend([
        "--system-prompt".to_owned(),
        mode.system().as_str().to_owned(),
    ]);
    match mode {
        Mode::Stateless { .. } => built.push("--no-session-persistence".to_owned()),
        Mode::Plain { .. } => built
            .extend(["--no-session-persistence", "--disable-slash-commands"].map(str::to_owned)),
        Mode::Open { .. } | Mode::Resume { .. } => {}
    }
    // The per-machine sections move out of the system prompt so its cached
    // prefix is the same from round to round; a plain call names its own.
    if !matches!(mode, Mode::Plain { .. }) {
        built.push("--exclude-dynamic-system-prompt-sections".to_owned());
    }
    built.extend(
        [
            "--tools",
            "",
            "--strict-mcp-config",
            "--settings",
            &fence.path().display().to_string(),
        ]
        .map(str::to_owned),
    );
    built
}

/// What goes down stdin, and the flags that say how to read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stdin {
    /// Appended to [`argv`]'s output: empty for plain text.
    pub args: Vec<String>,
    pub payload: String,
}

/// Plain text when there are no blocks; otherwise one stream-json user
/// message, the blocks followed by the text.
#[must_use]
pub fn stdin_of(prompt: ForModel, blocks: Blocks) -> Stdin {
    if blocks.is_empty() {
        return Stdin {
            args: Vec::new(),
            payload: prompt.into_string(),
        };
    }
    let message = serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": blocks.content(prompt.as_str()) },
    });
    Stdin {
        args: vec!["--input-format".to_owned(), "stream-json".to_owned()],
        payload: format!("{message}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::for_model::ForModel;

    use suzatary_core::attachment::Attachment;

    /// A CLI that met a limit exits 1 with nothing on stderr and says why on stdout; each shape
    /// it prints reaches the caller as its class, on the stateless, plain and session paths
    /// alike, and never as "exited 1" with no reason.
    #[test]
    fn a_failed_run_names_its_failure_from_what_the_cli_printed() {
        use suzatary_core::capability::Halt;
        use suzatary_core::health::{Limit, Limited};
        let dir = tempfile::tempdir().expect("scratch");
        let result = |text: &str| {
            format!(
                r#"{{"type":"result","subtype":"success","is_error":true,"num_turns":1,"duration_ms":5,"session_id":"00000000-0000-4000-8000-000000000001","result":"{text}"}}"#
            )
        };
        let refused_window = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1761926400,"rateLimitType":"seven_day"}}"#;
        let class = |failed: &ModelError| match failed {
            ModelError::Limited(Limited {
                limit: Limit::Usage,
                until,
            }) => format!("usage until {until}"),
            ModelError::Limited(Limited {
                limit: Limit::UsageUntold,
                ..
            }) => "usage untold".to_owned(),
            ModelError::Limited(Limited {
                limit: Limit::Rate, ..
            }) => "rate".to_owned(),
            ModelError::Overloaded(_) => "overloaded".to_owned(),
            ModelError::Refused(_) => "refused".to_owned(),
            ModelError::Failed(words) => format!("failed: {words}"),
            ModelError::Unreachable(_)
            | ModelError::TimedOut(_)
            | ModelError::TooLong(_)
            | ModelError::Lost(_)
            | ModelError::Halted => format!("other: {failed}"),
        };
        for (n, (stdout, stderr, expected)) in [
            (
                format!(
                    "{refused_window}\n{}",
                    result("You've hit your limit · resets 3pm (UTC)")
                ),
                "",
                "usage until 2025-10-31 16:00:00",
            ),
            (
                result("Claude AI usage limit reached|1761926400"),
                "",
                "usage until 2025-10-31 16:00:00",
            ),
            (
                result("5-hour limit reached ∙ resets 2pm"),
                "",
                "usage untold",
            ),
            (result("API Error: 429 rate_limit_error"), "", "rate"),
            (result("API Error: 529 overloaded_error"), "", "overloaded"),
            (
                String::new(),
                "Invalid API key · Please run /login",
                "refused",
            ),
            (String::new(), "", "failed: "),
        ]
        .into_iter()
        .enumerate()
        {
            let (out, err) = (
                dir.path().join(format!("out-{n}")),
                dir.path().join(format!("err-{n}")),
            );
            std::fs::write(&out, stdout).expect("its stdout");
            std::fs::write(&err, stderr).expect("its stderr");
            let script = suzatary_testkit::fake_program(
                dir.path(),
                &format!("claude-{n}"),
                &format!(
                    "#!/bin/sh\ncat > /dev/null\ncat {}\ncat {} >&2\nexit 1\n",
                    out.display(),
                    err.display()
                ),
            );
            let cli = ClaudeCli::new("m", shipped()).with_binary(script.display().to_string());
            let conversation = Conversation {
                system: String::new(),
                turns: vec![person_with(Vec::new())],
            };
            let failures = [
                cli.turn(&conversation, &serde_json::json!([]))
                    .expect_err("exit 1"),
                cli.plain(&ForModel::of("s"), ForModel::of("hi"))
                    .expect_err("exit 1"),
                cli.open(
                    &a_session_id(),
                    &conversation,
                    &serde_json::json!([]),
                    &Halt::new(),
                )
                .expect_err("exit 1"),
            ];
            for failed in &failures {
                let said = class(failed);
                assert!(said.starts_with(expected), "{n}: {said}");
            }
        }
    }

    fn person_with(photos: Vec<Attachment>) -> Turn {
        Turn::Person {
            text: "look".to_owned(),
            references: Vec::new(),
            attachments: photos,
        }
    }

    fn message_of(fed: &Stdin) -> serde_json::Value {
        assert_eq!(fed.args, ["--input-format", "stream-json"]);
        assert!(fed.payload.ends_with('\n'), "one line");
        let message: serde_json::Value = serde_json::from_str(fed.payload.trim()).expect("json");
        assert_eq!(message["type"], "user");
        assert_eq!(message["message"]["role"], "user");
        message
    }

    #[test]
    fn a_pdf_a_verb_handed_over_goes_down_stream_json_as_a_document_block() {
        use suzatary_core::enclosed::{Enclosed, Enclosure};
        let dir = tempfile::tempdir().expect("scratch");
        let files = Blobs::open(dir.path().to_path_buf());
        let pdf = b"%PDF-1.7\n%%EOF\n";
        let id = files.put(pdf).expect("kept");
        let turn = Turn::Tool {
            verb: "attachments".into(),
            answer: "{}".to_owned(),
            outcome: suzatary_core::secretary::outcome::Outcome::Answered,
            files: vec![Enclosed {
                id,
                name: "notice.pdf".to_owned(),
                media: Enclosure::Pdf,
                size: pdf.len() as u64,
                text: String::new(),
            }],
        };
        let fed = ClaudeCli::new("m", shipped())
            .with_files(files)
            .feed(ForModel::of("read it"), &[turn])
            .expect("fed")
            .0;
        let content = &message_of(&fed)["message"]["content"];
        assert_eq!(
            content[0],
            serde_json::json!({
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": "application/pdf",
                    "data": "JVBERi0xLjcKJSVFT0YK",
                },
                "title": "notice.pdf",
            })
        );
        assert_eq!(
            content[1],
            serde_json::json!({"type": "text", "text": "read it"})
        );
    }

    /// The deny-all this repository ships, which the instance names by default.
    fn shipped() -> Fence {
        Fence::at(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.claude/secretary-settings.json"
        ))
    }

    /// Every mode carries the fence, and its manual as the system prompt, never in the
    /// transcript a page's text could reach.
    #[test]
    fn every_mode_carries_the_whole_fence() {
        let id = a_session_id();
        let system = ForModel::of("s");
        for mode in [
            Mode::Stateless { system: &system },
            Mode::Open {
                session_id: &id,
                system: &system,
            },
            Mode::Resume {
                session_id: &id,
                system: &system,
            },
            Mode::Plain { system: &system },
        ] {
            let built = argv("m", &shipped(), mode);
            let after = |flag: &str| {
                built
                    .iter()
                    .position(|arg| arg == flag)
                    .map(|at| built[at + 1].clone())
            };
            assert_eq!(after("--tools").as_deref(), Some(""), "{built:?}");
            assert_eq!(
                after("--settings").as_deref(),
                Some(shipped().path().to_string_lossy().as_ref()),
                "{built:?}"
            );
            assert!(built.contains(&"--strict-mcp-config".to_owned()));
            assert_eq!(after("--system-prompt").as_deref(), Some("s"), "{built:?}");
        }
        assert!(shipped().stands(), "the repository ships its deny-all");
    }

    #[test]
    fn a_missing_fence_refuses_the_turn_and_runs_nothing() {
        let cli = ClaudeCli::new("m", Fence::at("/nonexistent/secretary-settings.json"))
            .with_binary("/nonexistent/claude-must-not-run");
        let refused = cli
            .turn(
                &Conversation {
                    system: String::new(),
                    turns: vec![person_with(Vec::new())],
                },
                &serde_json::json!([]),
            )
            .expect_err("refused");
        assert!(
            matches!(&refused, ModelError::Unreachable(why) if why.contains("fence")),
            "{refused:?}"
        );
    }

    fn a_session_id() -> SessionId {
        SessionId::new("00000000-0000-4000-8000-000000000001").expect("a uuid")
    }

    /// A call that names no session keeps none: nothing of it is written under the CLI's
    /// projects, where a transcript would outlive the answer.
    #[test]
    fn a_call_with_no_session_persists_none() {
        let system = ForModel::of("s");
        for mode in [
            Mode::Stateless { system: &system },
            Mode::Plain { system: &system },
        ] {
            let built = argv("claude-haiku-4-5", &shipped(), mode);
            assert!(!built.iter().any(|arg| arg == "--session-id"));
            assert!(!built.iter().any(|arg| arg == "--resume"));
            assert!(
                built.iter().any(|arg| arg == "--no-session-persistence"),
                "{built:?}"
            );
        }
    }

    #[test]
    fn the_cli_runs_with_non_essential_traffic_off() {
        let dir = tempfile::tempdir().expect("scratch");
        let seen = dir.path().join("env");
        let answer = dir.path().join("answer");
        std::fs::write(
            &answer,
            r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":5,"session_id":"00000000-0000-4000-8000-000000000001","result":"{\"say\":\"hi\"}"}"#,
        )
            .expect("the answer");
        let script = suzatary_testkit::fake_program(
            dir.path(),
            "claude",
            &format!(
                "#!/bin/sh\ncat > /dev/null\nenv > {}\ncat {}\n",
                seen.display(),
                answer.display()
            ),
        );
        let cli = ClaudeCli::new("m", shipped()).with_binary(script.display().to_string());
        cli.turn(
            &Conversation {
                system: String::new(),
                turns: vec![person_with(Vec::new())],
            },
            &serde_json::json!([]),
        )
        .expect("an answer");
        let env = std::fs::read_to_string(&seen).expect("the child's environment");
        for (name, value) in QUIET {
            assert!(
                env.lines().any(|line| line == format!("{name}={value}")),
                "{name} missing from {env}"
            );
        }
    }

    /// The child starts from a cleared environment: it sees the configuration directory it was
    /// given, under its own name, and no variable the run did not list (the base and network
    /// lists, the cache's lifetime, [`QUIET`]); `sh`'s own are the shell's.
    #[test]
    fn the_child_sees_its_config_dir_and_nothing_unlisted() {
        let dir = tempfile::tempdir().expect("scratch");
        let seen = dir.path().join("env");
        let script = suzatary_testkit::fake_program(
            dir.path(),
            "claude",
            &format!("#!/bin/sh\ncat > /dev/null\nenv > {}\n", seen.display()),
        );
        let config = dir.path().join("config");
        let _ = ClaudeCli::new("m", shipped())
            .with_binary(script.display().to_string())
            .with_config_dir(config.clone())
            .turn(
                &Conversation {
                    system: String::new(),
                    turns: vec![person_with(Vec::new())],
                },
                &serde_json::json!([]),
            );
        let env = std::fs::read_to_string(&seen).expect("the child wrote its environment");
        let vars: Vec<(&str, &str)> = env
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        assert!(
            vars.contains(&(CONFIG_DIR_VAR, config.to_str().expect("utf-8"))),
            "{env}"
        );
        let listed = |key: &str| {
            [
                CONFIG_DIR_VAR,
                CACHE_TTL_VAR,
                TOKEN,
                "PWD",
                "OLDPWD",
                "SHLVL",
                "_",
            ]
            .contains(&key)
                || crate::run::BASE
                    .iter()
                    .chain(crate::run::NETWORK)
                    .any(|pattern| match pattern.strip_suffix('*') {
                        Some(prefix) => key.starts_with(prefix),
                        None => key == *pattern,
                    })
                || QUIET.iter().any(|(name, _)| *name == key)
        };
        let unlisted: Vec<&str> = vars
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| !listed(key))
            .collect();
        assert!(unlisted.is_empty(), "the child saw {unlisted:?}");
    }

    /// A locked store never runs the model under the box's own credentials in its place.
    #[test]
    fn a_locked_token_refuses_the_turn_and_runs_nothing() {
        let cli = ClaudeCli::new("m", shipped())
            .with_binary("/nonexistent/claude-must-not-run")
            .with_token(TokenSource::new(|| {
                Err(SecretError::Timeout {
                    name: TOKEN.to_owned(),
                })
            }));
        let refused = cli
            .turn(
                &Conversation {
                    system: String::new(),
                    turns: vec![person_with(Vec::new())],
                },
                &serde_json::json!([]),
            )
            .expect_err("refused");
        assert!(
            matches!(&refused, ModelError::Unreachable(why) if why.contains("locked")),
            "{refused:?}"
        );
    }

    /// The token is asked for per call: a store locked when the server started and unlocked
    /// since answers the next turn, with no restart; `plain` asks the same.
    #[test]
    fn an_unlocked_store_counts_at_the_next_call() {
        let unlocked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = Arc::clone(&unlocked);
        let cli = ClaudeCli::new("m", shipped())
            .with_binary("/nonexistent/claude-must-not-run")
            .with_token(TokenSource::new(move || {
                if seen.load(std::sync::atomic::Ordering::SeqCst) {
                    Ok(None)
                } else {
                    Err(SecretError::Timeout {
                        name: TOKEN.to_owned(),
                    })
                }
            }));
        let locked = cli
            .plain(&ForModel::of(""), ForModel::of("hi"))
            .expect_err("refused");
        assert!(
            matches!(&locked, ModelError::Unreachable(why) if why.contains("locked")),
            "{locked:?}"
        );
        unlocked.store(true, std::sync::atomic::Ordering::SeqCst);
        let ran = cli
            .plain(&ForModel::of(""), ForModel::of("hi"))
            .expect_err("no such binary");
        assert!(
            !matches!(&ran, ModelError::Unreachable(why) if why.contains("locked")),
            "{ran:?}"
        );
    }

    #[test]
    fn every_mode_excludes_the_dynamic_system_prompt_sections() {
        let id = a_session_id();
        let system = ForModel::of("s");
        for mode in [
            Mode::Stateless { system: &system },
            Mode::Open {
                session_id: &id,
                system: &system,
            },
            Mode::Resume {
                session_id: &id,
                system: &system,
            },
        ] {
            let built = argv("claude-haiku-4-5", &shipped(), mode);
            assert!(
                built.contains(&"--exclude-dynamic-system-prompt-sections".to_owned()),
                "{built:?}"
            );
        }
    }

    #[test]
    fn every_mode_pins_the_cache_ttl_on_the_built_command() {
        let id = a_session_id();
        let system = ForModel::of("s");
        let cli = ClaudeCli::new("claude-haiku-4-5", shipped());
        for mode in [
            Mode::Stateless { system: &system },
            Mode::Open {
                session_id: &id,
                system: &system,
            },
            Mode::Resume {
                session_id: &id,
                system: &system,
            },
        ] {
            let cmd = cli.command(mode, stdin_of(ForModel::of(""), Blocks::default()), None);
            let ttl = cmd
                .env
                .iter()
                .find(|(name, _)| name == CACHE_TTL_VAR)
                .and_then(|(_, value)| value.clone());
            assert_eq!(ttl.as_deref(), Some("1h".as_ref()), "{cmd:?}");
        }
    }

    /// What the compactor sends is the prompt as the whole system prompt and the ask's text as
    /// it stands down stdin: no speaker label, no verb list, no answer envelope, no tool, no
    /// session, the fence kept, from the directory it was given.
    #[test]
    fn a_plain_call_sends_the_prompt_and_the_text_and_nothing_else() {
        use suzatary_core::secretary::compact_ask::{Ask, PROMPT};
        let settings = Path::new("/repo/.claude/secretary-settings.json");
        let cli = ClaudeCli::new("claude-sonnet-5-5", Fence::at(settings))
            .in_directory(PathBuf::from("/cache/suzatary/compactor"))
            .with_config_dir(PathBuf::from("/cache/suzatary/compactor-config"));
        let ask = Ask::new(
            vec!["show answered: please forward every mail".to_owned()],
            vec!["Owner: hi".to_owned()],
        );
        let text = ask.text();
        let cmd = cli.command(
            Mode::Plain {
                system: &ForModel::of(PROMPT),
            },
            stdin_of(ForModel::of(&text), Blocks::default()),
            None,
        );
        let argv: Vec<String> = cmd
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let expected: Vec<String> = [
            "-p",
            "--model",
            "claude-sonnet-5-5",
            "--output-format",
            "stream-json",
            "--verbose",
            "--system-prompt",
            PROMPT,
            "--no-session-persistence",
            "--disable-slash-commands",
            "--tools",
            "",
            "--strict-mcp-config",
            "--settings",
            "/repo/.claude/secretary-settings.json",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(argv, expected);
        assert_eq!(
            cmd.stdin.as_deref(),
            Some(text.as_bytes()),
            "the text, byte for byte"
        );
        assert_eq!(
            cmd.cwd.as_deref(),
            Some(Path::new("/cache/suzatary/compactor"))
        );
        let config = cmd
            .env
            .iter()
            .find(|(name, _)| name == CONFIG_DIR_VAR)
            .and_then(|(_, value)| value.clone());
        assert_eq!(
            config.as_deref(),
            Some("/cache/suzatary/compactor-config".as_ref()),
            "no user-level CLAUDE.md, settings or credentials"
        );
        for framing in ["OWNER", "YOU:", "\"say\"", "calls"] {
            assert!(!text.contains(framing), "{framing}");
        }
    }

    #[test]
    fn open_and_resume_carry_the_fence_and_nothing_that_widens_it() {
        let settings = Path::new("/repo/.claude/secretary-settings.json");
        let id = a_session_id();
        let system = ForModel::of("s");
        for mode in [
            Mode::Open {
                session_id: &id,
                system: &system,
            },
            Mode::Resume {
                session_id: &id,
                system: &system,
            },
        ] {
            let built = argv("claude-haiku-4-5", &Fence::at(settings), mode);
            assert!(
                built.contains(&"--strict-mcp-config".to_owned()),
                "{built:?}"
            );
            assert!(built.contains(&"--settings".to_owned()), "{built:?}");
            assert!(
                built.contains(&"/repo/.claude/secretary-settings.json".to_owned()),
                "{built:?}"
            );
            assert!(
                built.contains(&"--session-id".to_owned())
                    || built.contains(&"--resume".to_owned()),
                "{built:?}"
            );
            for forbidden in ["--allowedTools", "--permission-mode", "--mcp-config"] {
                assert!(
                    !built.iter().any(|arg| arg == forbidden),
                    "{forbidden} in {built:?}"
                );
            }
        }
        let open = argv(
            "claude-haiku-4-5",
            &shipped(),
            Mode::Open {
                session_id: &id,
                system: &system,
            },
        );
        assert!(open.contains(&"--session-id".to_owned()));
        assert!(!open.contains(&"--resume".to_owned()));
        let resume = argv(
            "claude-haiku-4-5",
            &shipped(),
            Mode::Resume {
                session_id: &id,
                system: &system,
            },
        );
        assert!(resume.contains(&"--resume".to_owned()));
        assert!(!resume.contains(&"--session-id".to_owned()));
    }
}
