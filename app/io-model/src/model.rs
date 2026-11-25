//! The `claude` CLI as a [`Model`].
//!
//! It is given no tools: `--tools ""`, `--strict-mcp-config` with no config and a deny-all
//! `--settings` [`Fence`] keep the account's own servers (a mail sender among them) and the
//! box's files out of reach of a prompt that carries other people's mail. All three are needed.

use crate::prompt::Reads;
pub mod failure;
pub mod sessions;
pub mod stream;
pub mod stream_event;
pub mod summary;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use suzatary_base::capability::{Clock, Secret};
use suzatary_conversations::secretary::model::{
    Conversation, Halt, Model, ModelError, Reply, TokenUsage, Turn,
};
use suzatary_held::for_model::ForModel;

use crate::blocks::{Blocks, Stores};
use crate::prompt::{Prompt, prompt_of};
use failure::{Heard, failed};
use stream::{Ended, Unreadable, delta_of, result_of};
use suzatary_io_access::run::{Cmd, RunError, run, watch, zero};
use suzatary_io_access::secrets::SecretError;
use suzatary_io_local::attachments::AttachmentStore;
use suzatary_io_local::blobs::Blobs;
use suzatary_io_local::clock::SystemClock;

/// The long-lived setup token an unattended run authenticates with.
pub const TOKEN: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// The reason secretspec's audit log records for reading [`TOKEN`].
pub const REASON: &str = "the secretary answers a conversation";

const DEFAULT_TIMEOUT: Duration = Duration::from_mins(5);

/// Pinned to an hour because the CLI's five-minute default expires between rounds of a
/// five-minute timer.
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
    pub fn from_store(store: suzatary_io_access::secrets::Secretspec) -> Self {
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

    #[must_use]
    pub fn sessions(&self) -> sessions::CliSessions {
        sessions::CliSessions::new(self.config.clone())
    }

    /// A store that cannot answer refuses the call, so a locked store never runs the model under
    /// credentials nobody chose.
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

    /// Without one a turn with a photo is refused rather than sent without it.
    #[must_use]
    pub fn with_uploads(mut self, store: AttachmentStore) -> Self {
        self.stores.uploads = Some(store);
        self
    }

    /// Without one a turn with a verb's files is refused rather than sent without them.
    #[must_use]
    pub fn with_files(mut self, store: Blobs) -> Self {
        self.stores.files = Some(store);
        self
    }

    /// Every mode asks here, for the token and the fence, before anything is spawned.
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

    fn feed(&self, prompt: &Prompt, turns: &[Turn]) -> Result<(Stdin, Option<Secret>), ModelError> {
        let token = self.ready()?;
        Ok((stdin_for(prompt, &Blocks::of(&self.stores, turns)?), token))
    }

    /// The prompt goes down stdin: a day's conversation outgrows one argv string's 128 KB.
    fn command(&self, mode: Mode<'_>, stdin: Stdin, token: Option<&Secret>) -> Cmd {
        let mut cmd = Cmd::new(&self.binary)
            .args(argv(&self.model, &self.fence, mode))
            .args(stdin.args)
            .stdin(stdin.payload)
            // Where the CLI keeps its own login, when not under HOME; the token is set below.
            .pass(&["CLAUDE_CONFIG_DIR"])
            .pass(suzatary_io_access::run::NETWORK)
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

    /// A failure reads stdout first, since the CLI prints a limit there and not on stderr.
    fn ended(&self, cmd: &Cmd) -> Result<Ended, ModelError> {
        let ran = run(cmd).map_err(|e| self.failure(e))?;
        let out = ran.out();
        let heard = Heard::of(&out);
        zero(cmd, ran).map_err(|exited| heard.failure(exited.to_string(), now()))?;
        clean(last_result(&out, &heard)?, &heard)
    }
}

fn now() -> suzatary_health::health::Instant {
    prodrome_data::instant::instant_of(SystemClock.now())
}

/// The name the CLI at `model` answers as, and its calls are recorded under.
#[must_use]
pub fn name_of(model: &str) -> String {
    format!("{}/{model}", suzatary_health::health::SUBSCRIPTION)
}

impl Model for ClaudeCli {
    fn name(&self) -> String {
        name_of(&self.model)
    }

    fn streamed(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Reply, ModelError> {
        let prompt = prompt_of(conversation, tools, Reads::Files);
        let (stdin, token) = self.feed(&prompt, &conversation.turns)?;
        // The manual and the verb list outgrow one argument, so they go by a file the call holds.
        let system = system_file(&prompt.system)?;
        let cmd = self.command(
            Mode::Stateless {
                system: system.path(),
            },
            stdin,
            token.as_ref(),
        );
        self.exchange(&cmd, on_delta, halt)
    }
}

impl ClaudeCli {
    /// `text` under `system` with no session and none of the secretary's framing, run from an
    /// empty directory and configuration, so no `CLAUDE.md` is found.
    pub fn plain(&self, system: &ForModel, text: ForModel) -> Result<String, ModelError> {
        let token = self.ready()?;
        let stdin = stdin_of(text, Blocks::default());
        reply_of(self.ended(&self.command(Mode::Plain { system }, stdin, token.as_ref()))?)
    }

    /// The tokens of the prompt that [`ClaudeCli::plain`] sends for `text` under `system`, as
    /// the model's tokenizer counted them: fresh, written to the cache and read from it.
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

impl ClaudeCli {
    /// One run of `cmd`, each fragment of its reply to `on_delta` as the stream carries it, the
    /// process ended when `halt` is pulled; the reply is the result line's text, else the
    /// fragments.
    fn exchange(
        &self,
        cmd: &Cmd,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Reply, ModelError> {
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
            exited @ RunError::Exited { .. } => heard.failure(exited.to_string(), now()),
            other => self.failure(other),
        })?;
        let ended = result
            .ok_or_else(|| {
                heard.failure(
                    "the model printed no result (not logged in?)".to_owned(),
                    now(),
                )
            })?
            .map_err(|unreadable| ModelError::Failed(unreadable.to_string()))?;
        let ended = clean(ended, &heard)?;
        let usage = ended.usage.map(TokenUsage::from);
        let text = ended.result.unwrap_or(assembled);
        Ok(Reply::parse(&text).with_usage(usage))
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

/// Which invocation a call to `claude` is, each with its system text.
#[derive(Debug, Clone, Copy)]
pub enum Mode<'a> {
    /// [`Model::streamed`]: the whole conversation, no session; the manual and the verb list in
    /// the file at `system`.
    Stateless { system: &'a Path },
    /// [`ClaudeCli::plain`]: no tools, no session, none of the secretary's framing.
    Plain { system: &'a ForModel },
}

/// `system` in a file of its own, removed when the returned handle drops.
fn system_file(system: &ForModel) -> Result<tempfile::NamedTempFile, ModelError> {
    use std::io::Write as _;
    let unwritten = |failed: std::io::Error| {
        ModelError::Unreachable(format!("the system text could not be written: {failed}"))
    };
    let mut file = tempfile::Builder::new()
        .prefix("suzatary-system-")
        .tempfile()
        .map_err(unwritten)?;
    file.write_all(system.as_str().as_bytes())
        .map_err(unwritten)?;
    Ok(file)
}

/// Every mode carries the fence (no tools, no MCP server, the deny-all settings) and nothing
/// that could widen it (`--allowedTools`, `--permission-mode`, `--mcp-config`).
#[must_use]
pub fn argv(model: &str, fence: &Fence, mode: Mode<'_>) -> Vec<String> {
    let mut built = vec!["-p".to_owned(), "--model".to_owned(), model.to_owned()];
    // Stream-json out in every mode: a request whose blocks go in as stream-json is taken only
    // with stream-json out.
    built.extend(["--output-format", "stream-json"].map(str::to_owned));
    if matches!(mode, Mode::Stateless { .. }) {
        built.push("--include-partial-messages".to_owned());
    }
    built.push("--verbose".to_owned());
    built.extend(match mode {
        Mode::Stateless { system } => [
            "--system-prompt-file".to_owned(),
            system.display().to_string(),
        ],
        Mode::Plain { system } => ["--system-prompt".to_owned(), system.as_str().to_owned()],
    });
    match mode {
        Mode::Stateless { .. } => built.push("--no-session-persistence".to_owned()),
        Mode::Plain { .. } => built
            .extend(["--no-session-persistence", "--disable-slash-commands"].map(str::to_owned)),
    }
    // The per-machine sections move out so the cached prefix is the same from round to round.
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

/// Plain text when there are no blocks; otherwise one stream-json user message, the blocks
/// followed by the text. [`ClaudeCli::plain`]'s.
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

/// One stream-json user message: what the context tells before the turns, the transcript's head,
/// the earlier rounds' turns, the round's turns, each after the files it hands over, the newest
/// carrying the call's one cache mark, then what the context tells after it (the record's newest
/// parts and the topics) and the tail. The CLI places marks of its own (two in the system text,
/// one after or on the message's last block) and the API takes four a request. `files` are the
/// blocks of the round's turns, by turn.
#[must_use]
pub fn stdin_for(prompt: &Prompt, files: &Blocks) -> Stdin {
    let text = |text: &ForModel| serde_json::json!({ "type": "text", "text": text.as_str() });
    let mut content: Vec<serde_json::Value> = prompt.told.iter().map(text).collect();
    content.push(text(&prompt.head));
    content.extend(prompt.earlier.iter().map(text));
    for (at, turn) in prompt.round.iter().enumerate() {
        content.extend(files.of_turn(at));
        content.push(text(turn));
    }
    // The newest turn's files, and any of a turn the prompt holds no block for.
    content.extend(files.from_turn(prompt.round.len()));
    if let Some(newest) = &prompt.newest {
        let mut marked = text(newest);
        marked["cache_control"] = serde_json::json!({ "type": "ephemeral", "ttl": "1h" });
        content.push(marked);
    }
    content.extend(prompt.after.iter().map(text));
    content.push(text(&prompt.tail));
    let message = serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": content },
    });
    Stdin {
        args: vec!["--input-format".to_owned(), "stream-json".to_owned()],
        payload: format!("{message}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_conversations::secretary::model::Earlier;
    use suzatary_held::for_model::ForModel;

    use suzatary_held::attachment::Attachment;

    /// A CLI that met a limit exits 1 with nothing on stderr and says why on stdout.
    #[test]
    fn a_failed_run_names_its_failure_from_what_the_cli_printed() {
        use suzatary_health::health::{Limit, Limited};
        let dir = suzatary_testkit::scratch("model");
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
                context: suzatary_conversations::secretary::model::Context::default(),
                earlier: Vec::new(),
                turns: vec![person_with(Vec::new())],
            };
            let failures = [
                cli.turn(&conversation, &serde_json::json!([]))
                    .expect_err("exit 1"),
                cli.plain(&ForModel::of("s"), ForModel::of("hi"))
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
        use suzatary_held::enclosed::{Enclosed, Enclosure};
        let dir = suzatary_testkit::scratch("model");
        let files = Blobs::open(dir.path().to_path_buf());
        let pdf = b"%PDF-1.7\n%%EOF\n";
        let id = files.put(pdf).expect("kept");
        let turn = Turn::Tool {
            verb: "attachments".into(),
            answer: "{}".to_owned(),
            outcome: suzatary_conversations::secretary::outcome::Outcome::Answered,
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
            .feed(
                &Prompt {
                    system: ForModel::of(""),
                    told: Vec::new(),
                    head: ForModel::of("read it"),
                    earlier: Vec::new(),
                    round: Vec::new(),
                    newest: None,
                    after: Vec::new(),
                    tail: ForModel::of("answer"),
                },
                &[turn],
            )
            .expect("fed")
            .0;
        let content = &message_of(&fed)["message"]["content"];
        assert_eq!(
            content[0],
            serde_json::json!({"type": "text", "text": "read it"})
        );
        assert_eq!(
            content[1],
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
    }

    /// F8: a turn's files stand just before its own block: with a file on two turns of the round,
    /// each document block is followed by its own turn's text, the newest's by the marked block.
    #[test]
    fn a_turns_files_stand_just_before_its_own_block() {
        use suzatary_held::enclosed::{Enclosed, Enclosure};
        let dir = suzatary_testkit::scratch("model");
        let files = Blobs::open(dir.path().to_path_buf());
        let handed = |name: &str, bytes: &[u8]| Turn::Tool {
            verb: "attachments".into(),
            answer: "{}".to_owned(),
            outcome: suzatary_conversations::secretary::outcome::Outcome::Answered,
            files: vec![Enclosed {
                id: files.put(bytes).expect("kept"),
                name: name.to_owned(),
                media: Enclosure::Pdf,
                size: bytes.len() as u64,
                text: String::new(),
            }],
        };
        let turns = [
            handed("first.pdf", b"%PDF-1.7\n%first\n%%EOF\n"),
            handed("second.pdf", b"%PDF-1.7\n%second\n%%EOF\n"),
        ];
        let stores = Stores {
            files: Some(files.clone()),
            ..Stores::default()
        };
        let blocks = Blocks::of(&stores, &turns).expect("read");
        let prompt = Prompt {
            system: ForModel::of(""),
            told: Vec::new(),
            head: ForModel::of("head"),
            earlier: Vec::new(),
            round: vec![ForModel::of("first turn")],
            newest: Some(ForModel::of("second turn")),
            after: Vec::new(),
            tail: ForModel::of("tail"),
        };
        let content = message_of(&stdin_for(&prompt, &blocks))["message"]["content"].clone();
        let kinds: Vec<String> = content
            .as_array()
            .expect("blocks")
            .iter()
            .map(|block| match block["type"].as_str() {
                Some("document") => format!("file {}", block["title"].as_str().unwrap_or("")),
                _ => block["text"].as_str().unwrap_or("").to_owned(),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "head",
                "file first.pdf",
                "first turn",
                "file second.pdf",
                "second turn",
                "tail"
            ]
        );
        assert!(
            content[4].get("cache_control").is_some(),
            "the newest turn's block is marked"
        );
    }

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
        let system = ForModel::of("s");
        let file = Path::new("/run/system.md");
        for (mode, flag, said) in [
            (
                Mode::Stateless { system: file },
                "--system-prompt-file",
                "/run/system.md",
            ),
            (Mode::Plain { system: &system }, "--system-prompt", "s"),
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
            assert_eq!(after(flag).as_deref(), Some(said), "{built:?}");
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
                    context: suzatary_conversations::secretary::model::Context::default(),
                    earlier: Vec::new(),
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

    /// A call that names no session keeps none: nothing of it is written under the CLI's
    /// projects, where a transcript would outlive the answer.
    #[test]
    fn a_call_with_no_session_persists_none() {
        let system = ForModel::of("s");
        for mode in [
            Mode::Stateless {
                system: Path::new("/run/system.md"),
            },
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
        let dir = suzatary_testkit::scratch("model");
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
                context: suzatary_conversations::secretary::model::Context::default(),
                earlier: Vec::new(),
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
        let dir = suzatary_testkit::scratch("model");
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
                    context: suzatary_conversations::secretary::model::Context::default(),
                    earlier: Vec::new(),
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
                || suzatary_io_access::run::BASE
                    .iter()
                    .chain(suzatary_io_access::run::NETWORK)
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
                    context: suzatary_conversations::secretary::model::Context::default(),
                    earlier: Vec::new(),
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

    /// A turn's call moves the per-machine sections out of the system prompt and keeps its cache
    /// an hour, so the cached prefix is the same from round to round.
    #[test]
    fn a_turn_keeps_its_prefix_the_same_and_its_cache_an_hour() {
        let mode = Mode::Stateless {
            system: Path::new("/run/system.md"),
        };
        let built = argv("claude-haiku-4-5", &shipped(), mode);
        assert!(
            built.contains(&"--exclude-dynamic-system-prompt-sections".to_owned()),
            "{built:?}"
        );
        let cmd = ClaudeCli::new("claude-haiku-4-5", shipped()).command(
            mode,
            stdin_of(ForModel::of(""), Blocks::default()),
            None,
        );
        let ttl = cmd
            .env
            .iter()
            .find(|(name, _)| name == CACHE_TTL_VAR)
            .and_then(|(_, value)| value.clone());
        assert_eq!(ttl.as_deref(), Some("1h".as_ref()), "{:?}", cmd.args);
    }

    #[test]
    fn a_plain_call_sends_the_prompt_and_the_text_and_nothing_else() {
        use suzatary_digest::secretary::compact_ask::{Ask, PROMPT};
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

    /// THE CACHE LAYOUT: the facts and the record, the head, the earlier rounds, the round with
    /// the one mark on its newest turn, then the topics and the tail; the verbs are in the system
    /// text, and the same conversation is the same bytes.
    #[test]
    fn the_one_cache_mark_is_on_the_newest_turn() {
        let told = |earlier: Vec<Earlier>| Conversation {
            system: "s".to_owned(),
            context: suzatary_conversations::secretary::model::Context {
                cached: vec!["facts".to_owned(), "record 0+4".to_owned()],
                rest: vec!["topics".to_owned()],
            },
            earlier,
            turns: vec![person_with(Vec::new()), person_with(Vec::new())],
        };
        let content = |conversation: &Conversation| {
            let prompt = prompt_of(conversation, &serde_json::json!([]), Reads::Files);
            assert!(prompt.system.as_str().contains("# The verbs you may call"));
            message_of(&stdin_for(&prompt, &Blocks::default()))["message"]["content"].clone()
        };
        let marked = told(vec![Earlier::Said(person_with(Vec::new()))]);
        let blocks = content(&marked);
        let texts: Vec<&str> = blocks
            .as_array()
            .expect("blocks")
            .iter()
            .map(|block| block["text"].as_str().expect("text"))
            .collect();
        assert_eq!(texts[..2], ["facts", "record 0+4"]);
        assert!(texts[2].starts_with("# The conversation so far"));
        assert!(
            texts[3..6]
                .iter()
                .all(|text| text.starts_with("OWNER: look"))
        );
        assert_eq!(texts[6], "topics");
        assert!(
            texts[7].starts_with("# Your turn"),
            "the box's own words are last"
        );
        assert_eq!(texts.len(), 8);
        let marks: Vec<usize> = (0..texts.len())
            .filter(|at| blocks[*at].get("cache_control").is_some())
            .collect();
        assert_eq!(marks, [5], "the newest turn carries the one mark");
        assert_eq!(content(&marked), blocks, "a function of the conversation");
    }
}
