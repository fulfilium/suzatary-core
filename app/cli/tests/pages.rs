//! `prodrome pages` against the readings it draws, through the real binary: the list page's rows
//! are `todos open`'s and each todo's page is `todo show`'s at the same instant, a reference to a
//! todo of the store leads to its page and any other stays text, and every word a page holds is
//! the store's or the application's. And the workflow `prodrome init --public` writes runs that
//! verb, holding no secret and asking only what Pages needs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use prodrome_core::fpl;
use prodrome_core::literal::Datetime;
use suzatary_record::record::{EventStore, MarkupSource, TodoEvent, mk_authored};

use crate::todos::scratch;

const IDS: [&str; 4] = ["permit", "taxes", "garden", "roof"];
const AT: &str = "2026-10-01T12:00:00";

type Lifecycle =
    fn(&str, Datetime, &str, &str) -> Result<TodoEvent, prodrome_core::literal::ProdromeError>;

/// One step of a generated history: a todo authored at a flat price, two records written by two
/// replicas that did not see each other (a conflict), or a lifecycle event with its note.
#[derive(Debug, Clone)]
enum Step {
    Authored(f64, String),
    Concurrent(String, String),
    Event(Lifecycle, String),
}

/// The todo every generated store begins with, never written again: its body escapes, refers to
/// itself spelled, inside a link after code, and by an id it only begins.
const ANCHOR: &str = "anchor";
const ANCHOR_BODY: &str = "Fees < 5 & \"quoted\" after todo:roadmap/anchor, not todo:roadmap/anchorTop, \
     nor #link(\"https://example.test\")[`c` then todo:roadmap/anchor]";

fn when(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 9, 0, 0, 0).expect("a real instant")
}

/// Words, and now and then a reference: linked or spelled, to a todo of this store or another's.
fn body() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::prelude::*;
    let reference = prop_oneof![
        Just(String::new()),
        prop::sample::select(IDS.to_vec())
            .prop_map(|id| format!(" see #link(\"todo:roadmap/{id}\")")),
        prop::sample::select(IDS.to_vec()).prop_map(|id| format!(" after todo:roadmap/{id}")),
        Just(" after todo:elsewhere/permit".to_owned()),
    ];
    ("[a-z]{1,10}( [a-z]{1,10}){0,3}", reference).prop_map(|(words, said)| words + &said)
}

fn step() -> impl proptest::strategy::Strategy<Value = (usize, &'static str, Step)> {
    use proptest::prelude::*;
    let lifecycle: [Lifecycle; 3] = [
        prodrome_core::event::mk_completed,
        prodrome_core::event::mk_cancelled,
        prodrome_core::event::mk_reopened,
    ];
    let step = prop_oneof![
        3 => (1u32..100, body()).prop_map(|(percent, body)| Step::Authored(f64::from(percent) / 100.0, body)),
        1 => (body(), body()).prop_map(|(one, other)| Step::Concurrent(one, other)),
        2 => (prop::sample::select(lifecycle.to_vec()), "([a-z]{1,8} ?){0,3}").prop_map(|(build, note)| Step::Event(build, note)),
    ];
    (
        0..IDS.len(),
        prop::sample::select(vec!["assistant", "ada"]),
        step,
    )
}

fn authored(todo: &str, day: u32, actor: &str, body: &str, value: f64) -> TodoEvent {
    mk_authored(
        todo,
        when(day),
        actor,
        "todo",
        when(day),
        MarkupSource::from_chain(body),
        fpl::mk_flat(value).expect("a term"),
        vec!["generated".to_owned()],
        "home",
        "",
        MarkupSource::empty(),
        None,
        Vec::new(),
        Vec::new(),
        "",
    )
    .expect("valid")
}

fn store(dir: &Path) -> EventStore {
    EventStore::new(dir, prodrome_core::policy::Untrusted::none())
}

fn append(dir: &Path, event: TodoEvent) -> prodrome_core::event::Hash {
    suzatary_store::store::append_to(&store(dir), suzatary_record::record::GENESIS, event)
        .expect("appends")
}

/// The store the steps write at `dir`. A concurrent pair is one record appended here and one in a
/// copy of the store taken before it, which this store then adopts.
fn written(root: &Path, steps: &[(usize, &str, Step)]) -> std::path::PathBuf {
    let dir = root.join("store");
    std::fs::create_dir_all(&dir).expect("a directory");
    std::fs::write(dir.join("charter.toml"), "public = true\n").expect("chartered");
    append(&dir, authored(ANCHOR, 1, "ada", ANCHOR_BODY, 0.5));
    for (day, (id, actor, step)) in (2..).zip(steps) {
        let todo = IDS[*id];
        match step {
            Step::Authored(value, body) => {
                append(&dir, authored(todo, day, actor, body, *value));
            }
            Step::Concurrent(one, other) => {
                let replica = root.join(format!("replica-{day}"));
                copy(&dir, &replica);
                append(&dir, authored(todo, day, actor, one, 0.4));
                let theirs = append(&replica, authored(todo, day, "assistant", other, 0.6));
                store(&dir)
                    .adopt(
                        &store(&replica),
                        &theirs,
                        prodrome_core::store::Sender::Stranger,
                    )
                    .expect("adopts the replica's record");
            }
            Step::Event(build, note) => {
                append(&dir, build(todo, when(day), actor, note).expect("valid"));
            }
        }
    }
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("readable").flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).expect("copies");
        }
    }
}

/// The binary's stdout for `args`, or its stderr where it refused.
fn reads(args: &[&str]) -> Result<String, String> {
    let out = crate::binary::suzatary()
        .args(args)
        .output()
        .expect("the binary runs");
    let said = |bytes: Vec<u8>| String::from_utf8(bytes).expect("utf-8");
    if out.status.success() {
        Ok(said(out.stdout))
    } else {
        Err(said(out.stderr))
    }
}

fn unescaped(html: &str) -> String {
    html.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn captures(pattern: &str, html: &str) -> Vec<Vec<String>> {
    regex::Regex::new(pattern)
        .expect("a literal pattern")
        .captures_iter(html)
        .map(|found| {
            found
                .iter()
                .skip(1)
                .map(|group| unescaped(group.map_or("", |g| g.as_str())))
                .collect()
        })
        .collect()
}

/// The price as `todos open --json` prints it, from the page's `todos open` text spelling.
fn priority(drawn: &str) -> serde_json::Value {
    match drawn {
        "—" => serde_json::json!({ "type": "Unlinked" }),
        "∅" => serde_json::json!({ "type": "Absent" }),
        number => serde_json::json!({ "type": "Percent", "percent": number }),
    }
}

/// The list page read back: each row's id, price and words.
fn listed(html: &str) -> Vec<(String, serde_json::Value, String)> {
    captures(
        r#"<li class="trow"><a class="topen" href="todo/([a-z0-9_-]+)\.html"><span class="tmark"><span class="tpct">([^<]*)</span></span><span><span class="ttitle">([^<]*)</span>"#,
        html,
    )
    .into_iter()
    .map(|row| (row[0].clone(), priority(&row[1]), row[2].clone()))
    .collect()
}

/// One todo's page read back: its state, its price, each candidate's block, and the objects of its
/// history.
fn drawn_todo(html: &str) -> serde_json::Value {
    let (head, history) = html.split_once(">History</h").expect("a history");
    let line = &captures(r"<strong>([^<]*)</strong>, ([^,]*), as of ", head)[0];
    serde_json::json!({
        "state": line[0],
        "priority": priority(&line[1]),
        "blocks": captures(r"(?s)<pre><code>(.*?)</code></pre>", head)
            .into_iter()
            .map(|block| block[0].clone())
            .collect::<Vec<_>>(),
        "history": captures(r"<li>(?:<p>)?<code>([0-9a-f]{64})</code>", history)
            .into_iter()
            .map(|object| object[0].clone())
            .collect::<Vec<_>>(),
    })
}

/// `todo show`'s reading of the same parts.
fn shown_parts(shown: &serde_json::Value) -> serde_json::Value {
    let each = |list: &str, field: &str| -> Vec<serde_json::Value> {
        shown[list]
            .as_array()
            .expect("a list")
            .iter()
            .map(|item| item[field].clone())
            .collect()
    };
    serde_json::json!({
        "state": shown["state"],
        "priority": shown["priority"],
        "blocks": each("candidates", "block"),
        "history": each("history", "object"),
    })
}

/// Every word of `text`, lowercased: a run of letters and digits holding no digit, so a name, a
/// number or an instant is no word.
fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !word.chars().any(char::is_numeric))
        .map(str::to_lowercase)
        .collect()
}

/// The text a reader sees: the HTML without its tags.
fn text(html: &str) -> String {
    let tags = regex::Regex::new(r"<[^>]*>").expect("a literal pattern");
    unescaped(&tags.replace_all(html, " "))
}

/// What a page says, to a reader and in its attributes a browser shows or follows.
fn said(html: &str) -> String {
    let attributes = captures(r#"(?:href|title|alt)="([^"]*)""#, html)
        .into_iter()
        .map(|value| value[0].clone());
    std::iter::once(text(html))
        .chain(attributes)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The most anchors a page holds one inside another: one, where no anchor holds an anchor.
fn deepest_anchor(html: &str) -> usize {
    let tags = regex::Regex::new(r"<(/?)a[ >]").expect("a literal pattern");
    let mut depth = 0usize;
    let mut deepest = 0;
    for tag in tags.captures_iter(html) {
        if tag[1].is_empty() {
            depth += 1;
            deepest = deepest.max(depth);
        } else {
            depth = depth.saturating_sub(1);
        }
    }
    deepest
}

/// The words of every string the store's objects publish (`door::published`) and of every
/// object's own print, where its kinds and fields are named.
fn the_stores_words(store: &Path) -> BTreeSet<String> {
    let objects = suzatary_store::store::linearised(&self::store(store)).expect("reads");
    objects
        .iter()
        .flat_map(|(_, envelope)| {
            let published = envelope
                .event()
                .map(suzatary_cli::commands::door::published)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, text)| text);
            published.chain([prodrome_core::event::canonical_envelope(envelope)])
        })
        .flat_map(|text| words(&text))
        .collect()
}

/// The application's own words: the two templates, the verb that writes the shell, and the block
/// grammar a record is printed in.
fn the_applications_words() -> BTreeSet<String> {
    [
        include_str!("../../todo/src/rendering/todos.typ"),
        include_str!("../../todo/src/rendering/todo.typ"),
        include_str!("../../todo/src/rendering/priority.typ"),
        include_str!("../src/commands/prodrome/pages.rs"),
        include_str!("../../record/src/decoration/block.rs"),
    ]
    .iter()
    .flat_map(|text| words(text))
    .collect()
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(8))]

    /// The pages show their readings: the list page's rows, prices and words are `todos open`'s,
    /// in its order; a todo has a page exactly where `todo show` answers for it, and the page's
    /// state, price, candidates (a conflict whole) and history are `todo show`'s; a reference to
    /// a todo of the store, spelled whole and outside a link and code, is a link to its page, and
    /// no anchor holds another; every link leads to a page of the site; and no page holds a word,
    /// in its text or its attributes, that is neither in an object of the store nor the
    /// application's.
    #[test]
    fn the_pages_show_what_todos_open_and_todo_show_read(
        steps in proptest::collection::vec(step(), 0..10)
    ) {
        let root = scratch("pages");
        let store = written(&root, &steps);
        let site = root.join("site");
        let (s, o) = (store.to_string_lossy(), site.to_string_lossy());
        let wrote = reads(&["prodrome", "pages", "--store", &s, "--at", AT, "--into", &o]);
        proptest::prop_assert!(wrote.is_ok(), "{wrote:?}");

        let open: serde_json::Value = serde_json::from_str(
            &reads(&["todos", "open", "--store", &s, "--at", AT, "--json"]).expect("reads"),
        )
        .expect("JSON");
        let rows: Vec<(String, serde_json::Value, String)> = open["open"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| {
                (
                    row["id"].as_str().expect("an id").to_owned(),
                    row["priority"].clone(),
                    row["body"].as_str().expect("words").to_owned(),
                )
            })
            .collect();
        let index = std::fs::read_to_string(site.join("index.html")).expect("the list");
        proptest::prop_assert_eq!(listed(&index), rows);

        let mut pages = vec![index];
        for id in IDS.into_iter().chain([ANCHOR]) {
            let page = std::fs::read_to_string(site.join(format!("todo/{id}.html"))).ok();
            let shown = reads(&["todo", "show", "--store", &s, "--at", AT, id]).ok();
            match (page, shown) {
                (Some(page), Some(shown)) => {
                    let shown: serde_json::Value = serde_json::from_str(&shown).expect("JSON");
                    proptest::prop_assert_eq!(drawn_todo(&page), shown_parts(&shown));
                    pages.push(page);
                }
                (None, None) => {}
                (page, shown) => proptest::prop_assert!(
                    false,
                    "{id}: a page {} and todo show {}",
                    page.is_some(),
                    shown.is_some()
                ),
            }
        }

        let anchor = std::fs::read_to_string(site.join(format!("todo/{ANCHOR}.html")))
            .expect("the anchor's page");
        proptest::prop_assert_eq!(
            anchor.matches(r#"<a href="anchor.html">todo:roadmap/anchor</a>"#).count(),
            1,
            "the spelled reference alone links: {}",
            anchor
        );
        proptest::prop_assert!(!anchor.contains(">todo:roadmap/anchorTop</a>"));
        for page in &pages {
            proptest::prop_assert_eq!(deepest_anchor(page), 1);
        }

        // A link is a file of the site, relative to the page, or the one outbound URL a body wrote.
        let exists = |href: &str, from: &Path| from.join(href).is_file();
        for page in &pages[1..] {
            for href in captures(r#"href="([^"]*)""#, page).into_iter().map(|h| h[0].clone()) {
                proptest::prop_assert!(
                    href == "https://example.test" || exists(&href, &site.join("todo")),
                    "{href} leads nowhere"
                );
            }
        }

        let known: BTreeSet<String> = the_stores_words(&store)
            .union(&the_applications_words())
            .cloned()
            .collect();
        for page in &pages {
            let unknown: Vec<String> = words(&said(page)).difference(&known).cloned().collect();
            proptest::prop_assert!(unknown.is_empty(), "words from nowhere: {unknown:?}");
        }
    }
}

/// The site of an empty public store is its list, saying nothing is open, and the stylesheets.
#[test]
fn an_empty_store_is_a_list_of_nothing() {
    let root = scratch("pages-empty");
    let store = root.join("store");
    assert!(
        reads(&[
            "prodrome",
            "init",
            "--public",
            "--store",
            &store.to_string_lossy()
        ])
        .is_ok()
    );
    let site = root.join("site");
    assert!(
        reads(&[
            "prodrome",
            "pages",
            "--store",
            &store.to_string_lossy(),
            "--at",
            AT,
            "--into",
            &site.to_string_lossy(),
        ])
        .is_ok()
    );
    let mut files: Vec<String> = std::fs::read_dir(&site)
        .expect("written")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, ["index.html", "style.css", "tokens.css"]);
    let index = std::fs::read_to_string(site.join("index.html")).expect("the list");
    assert!(text(&index).contains("0 open, priced as of 2026-10-01 12:00 UTC."));
}

/// A store begun private gets no Pages workflow and no ignored `_site`, and `prodrome pages`
/// refuses it, and refuses a directory that is not empty, both before it draws a page.
#[test]
fn a_private_store_is_not_published() {
    let root = scratch("pages-private");
    let store = root.join("store");
    let s = store.to_string_lossy();
    assert!(reads(&["prodrome", "init", "--store", &s]).is_ok());
    assert!(!store.join(".github/workflows/pages.yml").exists());
    let ignored = std::fs::read_to_string(store.join(".gitignore")).expect("written");
    assert!(!ignored.contains("_site"), "{ignored}");
    let site = root.join("site").to_string_lossy().into_owned();
    let refused = reads(&[
        "prodrome", "pages", "--store", &s, "--at", AT, "--into", &site,
    ]);
    assert!(
        refused
            .as_ref()
            .is_err_and(|said| said.contains("not chartered public")),
        "{refused:?}"
    );

    let public = root.join("public");
    let p = public.to_string_lossy();
    assert!(reads(&["prodrome", "init", "--public", "--store", &p]).is_ok());
    let ignored = std::fs::read_to_string(public.join(".gitignore")).expect("written");
    assert!(ignored.lines().any(|line| line == "/_site"), "{ignored}");
    std::fs::create_dir_all(root.join("full")).expect("a directory");
    std::fs::write(root.join("full/kept.html"), "").expect("a file");
    let full = root.join("full").to_string_lossy().into_owned();
    let refused = reads(&[
        "prodrome", "pages", "--store", &p, "--at", AT, "--into", &full,
    ]);
    assert!(
        refused
            .as_ref()
            .is_err_and(|said| said.contains("not empty")),
        "{refused:?}"
    );
}

/// `git` in `root`, as one writer at the instant the workflow test pushes.
fn git(root: &Path, args: &[&str]) {
    let done = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=someone",
            "-c",
            "user.email=someone@example.test",
        ])
        .args(args)
        .env("GIT_COMMITTER_DATE", "2026-10-01T12:00:00Z")
        .env("GIT_AUTHOR_DATE", "2026-10-01T12:00:00Z")
        .status()
        .expect("git runs");
    assert!(done.success(), "git {args:?}");
}

/// The workflow's text read as the template lays it out: two spaces a level, each item of a
/// sequence a `- ` at its key's depth plus two, a literal block after `key: |`, and comments only
/// on lines of their own or after a value. That the text is GitHub's syntax is
/// `gate_workflows_pass_actionlint`'s to say, over the template the verb writes.
struct Outline(Vec<String>);

fn depth(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

impl Outline {
    fn of(text: &str) -> Self {
        Self(
            text.lines()
                .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
                .map(str::to_owned)
                .collect(),
        )
    }

    fn top(&self) -> impl Iterator<Item = &str> {
        let top = self.0.iter().map(|line| depth(line)).min().unwrap_or(0);
        self.0
            .iter()
            .filter(move |line| depth(line) == top)
            .map(|line| line.trim_start())
    }

    /// The lines after the top line `head`, up to the next line no deeper than it.
    fn after(&self, head: &str) -> Self {
        let top = self.0.iter().map(|line| depth(line)).min().unwrap_or(0);
        let at = self
            .0
            .iter()
            .position(|line| depth(line) == top && line.trim_start() == head)
            .unwrap_or_else(|| panic!("a line `{head}`"));
        Self(
            self.0[at + 1..]
                .iter()
                .take_while(|line| depth(line) > top)
                .cloned()
                .collect(),
        )
    }

    /// The block under `key`.
    fn under(&self, key: &str) -> Self {
        self.after(&format!("{key}:"))
    }

    /// The keys of its top lines, in order.
    fn keys(&self) -> Vec<&str> {
        self.top()
            .map(|line| line.split(':').next().expect("a key"))
            .collect()
    }

    /// What its top line `key:` holds, less a comment after it.
    fn value(&self, key: &str) -> Option<&str> {
        let key = format!("{key}: ");
        self.top()
            .find_map(|line| line.strip_prefix(key.as_str()))
            .map(|value| value.split(" #").next().expect("a value").trim_end())
    }

    /// The literal block `key: |` as its text.
    fn literal(&self, key: &str) -> String {
        let block = self.after(&format!("{key}: |"));
        let indent = block.0.iter().map(|line| depth(line)).min().unwrap_or(0);
        block
            .0
            .iter()
            .fold(String::new(), |text, line| text + &line[indent..] + "\n")
    }

    /// The items of the sequence it is, each with its `- ` written as two spaces, so that its keys
    /// share one depth.
    fn items(&self) -> Vec<Self> {
        let top = self.0.iter().map(|line| depth(line)).min().unwrap_or(0);
        let mut items: Vec<Self> = Vec::new();
        for line in &self.0 {
            match line[top..].strip_prefix("- ") {
                Some(first) if depth(line) == top => {
                    items.push(Self(vec![format!("{}  {first}", &line[..top])]));
                }
                _ => items
                    .last_mut()
                    .expect("a sequence starts with an item")
                    .0
                    .push(line.clone()),
            }
        }
        items
    }
}

/// The permissions `job` asks, by name.
fn asked(jobs: &Outline, job: &str) -> BTreeMap<String, String> {
    let permissions = jobs.under(job).under("permissions");
    permissions
        .keys()
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                permissions.value(name).expect("a level").to_owned(),
            )
        })
        .collect()
}

/// The one step that uses the action `named` (`owner/repo@`).
fn using<'a>(steps: &'a [Outline], named: &str) -> &'a Outline {
    steps
        .iter()
        .find(|step| step.value("uses").is_some_and(|u| u.starts_with(named)))
        .unwrap_or_else(|| panic!("a step uses {named}"))
}

/// The workflow `prodrome init --public` writes: it is the template actionlint passes, it runs on
/// a push alone and its build on the default branch alone, its token asks nothing at the top and
/// each job only what Pages needs, it names no secret, every action is pinned by its commit, and
/// the command it runs is `prodrome pages` over the checkout, whose directory is the one it
/// publishes.
#[test]
fn the_pages_workflow_runs_the_verb_holding_no_secret() {
    let root = scratch("pages-workflow");
    git(&root, &["init", "-q"]);
    assert!(
        reads(&[
            "prodrome",
            "init",
            "--public",
            "--store",
            &root.to_string_lossy()
        ])
        .is_ok()
    );
    let yaml = std::fs::read_to_string(root.join(".github/workflows/pages.yml")).expect("written");
    assert_eq!(
        yaml,
        include_str!("../src/commands/prodrome/roadmap.pages.yml"),
        "the template actionlint passes"
    );
    let workflow = Outline::of(&yaml);

    assert_eq!(workflow.under("on").keys(), ["push"], "a push alone");
    assert!(!yaml.contains("pull_request"), "no pull request event");
    assert!(!yaml.contains("secrets."), "no secret");
    assert_eq!(
        workflow.value("permissions"),
        Some("{}"),
        "the token asks nothing outside a job"
    );
    let jobs = workflow.under("jobs");
    assert_eq!(jobs.keys(), ["build", "deploy"]);
    assert_eq!(
        asked(&jobs, "build"),
        BTreeMap::from([("contents".into(), "read".into())])
    );
    assert_eq!(
        asked(&jobs, "deploy"),
        BTreeMap::from([
            ("id-token".into(), "write".into()),
            ("pages".into(), "write".into())
        ])
    );
    assert_eq!(
        jobs.under("build").value("if"),
        Some("github.ref_name == github.event.repository.default_branch")
    );

    let steps: Vec<Outline> = jobs
        .keys()
        .into_iter()
        .flat_map(|job| jobs.under(job).under("steps").items())
        .collect();
    let pin = regex::Regex::new(r"\A[a-z0-9-]+/[a-z0-9-]+@[0-9a-f]{40}\z").expect("a pattern");
    for uses in steps.iter().filter_map(|step| step.value("uses")) {
        assert!(pin.is_match(uses), "pinned by commit: {uses}");
    }
    let checkout = using(&steps, "actions/checkout@");
    assert_eq!(
        checkout.under("with").value("persist-credentials"),
        Some("false")
    );

    let run = steps
        .iter()
        .filter(|step| step.keys().contains(&"run"))
        .map(|step| step.literal("run"))
        .find(|run| run.contains("prodrome pages"))
        .expect("a step runs the verb");
    let upload = using(&steps, "actions/upload-pages-artifact@").under("with");
    let published = upload.value("path").expect("the directory published");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "began"]);
    let binary = crate::binary::path();
    assert_eq!(
        workflow.under("concurrency").value("group"),
        Some("pages-${{ github.ref }}"),
        "each ref waits in its own group"
    );
    let script = run.replace("nix run --no-update-lock-file . --", binary);
    assert_ne!(script, run, "the verb runs as the flake's application");
    let ran = crate::binary::run(Path::new("bash"))
        .args(["-euo", "pipefail", "-c", &script])
        .current_dir(&root)
        .output()
        .expect("bash runs");
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let index = std::fs::read_to_string(root.join(published).join("index.html")).expect("the list");
    assert!(
        text(&index).contains("priced as of 2026-10-01 12:00 UTC."),
        "the instant is the pushed commit's"
    );
}
