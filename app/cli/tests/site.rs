//! What `suzatary web` SAYS, read from the files it writes: the facts, hashes,
//! links and states of each page, never its markup, so the pages may change
//! how they are built and still be held to what they state.

mod common;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use common::{now, scratch, shift, six_todo_chain};
use prodrome_core::fpl::mk_flat;
use suzatary_core::cast::CAST;
use suzatary_core::record::{EventStore, mk_authored, mk_source};

/// Prose an attacker's mail could carry: a script element, a Typst
/// expression, a closing bracket and a quote.
const HOSTILE_BODY: &str = "<script>alert(1)</script> #eval(\"1\") ] \"quoted\"";
const HOSTILE_SUBJECT: &str = "Re: ]#html.elem(\"script\")[ <img src=x>";
const HOSTILE_ID: &str = "hostile";

fn a_hostile_todo(events: &EventStore) {
    let made = shift(now(), -20);
    suzatary_core::store::append_to(
        events,
        suzatary_core::record::GENESIS,
        mk_authored(
            HOSTILE_ID,
            made,
            "triage",
            "todo",
            made,
            suzatary_core::record::MarkupSource::from_chain(HOSTILE_BODY),
            mk_flat(0.9).expect("a term"),
            vec!["]#link(\"javascript:x\")[".to_owned()],
            "home",
            "",
            suzatary_core::record::MarkupSource::from_chain("#strong[bold] </pre>"),
            Some(mk_source(
                "mallory@example.test",
                HOSTILE_SUBJECT,
                made,
                "",
                "",
                "",
            )),
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("valid"),
    )
    .expect("appends");
}

fn an_instance(root: &Path) -> PathBuf {
    let path = root.join("instance.toml");
    std::fs::write(
        &path,
        format!(
            r#"
repo = "{repo}"

[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["{owner}", "triage"]
owner = "{owner}"

[web]
viewers = ["someone@example.com"]
app_url = "https://example.invalid"

[web.act_as]
"someone@example.com" = "{owner}"

[web.serving]
host = "127.0.0.1"
port = 8790
tls_dir = "tls"
dist = "dist"
briefings = "briefings"

[categories]
home = 0
work = 1
other = 2

[mail]
root = "mail"
state = "state.json"
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
heartbeat = "triage-heartbeat"

[[mail.accounts]]
email = "someone@example.com"
subdir = "someone"
sync = "gmail"
"#,
            repo = root.display(),
            owner = CAST.owner.actor,
        ),
    )
    .expect("an instance file");
    path
}

/// A page's text as a reader sees it: tags gone (a `<wbr>` is no text at
/// all), entities decoded, the head left out, whitespace collapsed.
fn said(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html.find("<body").map_or(html, |at| &html[at..]);
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let close = rest[open..]
            .find('>')
            .map_or(rest.len(), |at| open + at + 1);
        if !rest[open..close].starts_with("<wbr") {
            out.push(' ');
        }
        rest = &rest[close..];
    }
    out.push_str(rest);
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every `href` on a page, in order.
fn links(html: &str) -> Vec<String> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|tail| tail.split_once('"').map(|(href, _)| href.to_owned()))
        .collect()
}

struct Site {
    out: PathBuf,
    events: PathBuf,
}

impl Site {
    fn page(&self, path: &str) -> String {
        std::fs::read_to_string(self.out.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn files(&self) -> BTreeSet<String> {
        fn walk(dir: &Path, base: &Path, into: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(dir).expect("a directory") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(&path, base, into);
                } else {
                    let relative = path.strip_prefix(base).expect("under the base");
                    into.insert(relative.to_string_lossy().into_owned());
                }
            }
        }
        let mut files = BTreeSet::new();
        walk(&self.out, &self.out, &mut files);
        files
    }

    /// Every file that is not an exported object.
    fn pages(&self) -> BTreeSet<String> {
        self.files()
            .into_iter()
            .filter(|file| !file.starts_with("objects/"))
            .collect()
    }
}

fn a_site(name: &str) -> Site {
    let root = scratch(name);
    let store = six_todo_chain(&root);
    a_hostile_todo(&store);
    let instance = an_instance(&root);
    let out = root.join("site");
    let ran = crate::binary::keyed()
        .args(["--instance", &instance.to_string_lossy(), "web"])
        .arg(&out)
        .output()
        .expect("the binary runs");
    assert!(
        ran.status.success(),
        "suzatary web failed: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
    Site {
        out,
        events: root.join("events"),
    }
}

/// A page's text with the one thing that moves on every run, the footer's
/// moment, and the filter box's label and script, taken out.
fn stated(html: &str) -> String {
    let text = said(html);
    let text = text.rfind(" generated ").map_or(&*text, |at| &text[..at]);
    text.replace(" filter ", " ")
}

/// Every page's text and links, pinned in `tests/pins/site.pin`. The prices
/// read the wall clock, and the chain is dated so that every one of them has
/// settled: past its deadline, and resolved more than thirty days ago.
#[test]
fn every_page_says_what_it_said() {
    let site = a_site("pinned");
    let pages = site
        .pages()
        .into_iter()
        .fold(String::new(), |mut out, file| {
            let html = site.page(&file);
            let _ = writeln!(
                out,
                "=== {file}\n{}\nlinks: {}",
                stated(&html),
                links(&html).join(" ")
            );
            out
        });
    common::pinned("site", &pages);
}

/// The export is the store's own objects, byte for byte, and nothing else
/// beside the pages.
#[test]
fn the_objects_are_the_store_s_bytes() {
    let site = a_site("objects");
    let stored: BTreeSet<String> = std::fs::read_dir(site.events.join("objects"))
        .expect("the store's objects")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let exported: BTreeSet<String> = site
        .files()
        .into_iter()
        .filter_map(|file| file.strip_prefix("objects/").map(str::to_owned))
        .collect();
    assert_eq!(exported, stored);
    for name in &stored {
        let copy = std::fs::read(site.out.join("objects").join(name)).expect("the copy");
        let original = std::fs::read(site.events.join("objects").join(name)).expect("the object");
        assert!(copy == original, "objects/{name} is not the stored bytes");
    }
    let pages = site.pages();
    let todos = [
        "book-flight",
        "email-harrow",
        "file-permit",
        HOSTILE_ID,
        "pack-bags",
        "renew-card",
        "water-plants",
    ];
    let want: BTreeSet<String> = ["index.html".to_owned(), "chain.html".to_owned()]
        .into_iter()
        .chain(todos.iter().map(|id| format!("todo/{id}.html")))
        .collect();
    assert_eq!(pages, want);
}

/// Mail an attacker wrote prints as the characters it is: no element, no
/// Typst expression, no link.
#[test]
fn a_hostile_subject_and_body_print_as_text() {
    let site = a_site("hostile");
    let page = site.page(&format!("todo/{HOSTILE_ID}.html"));
    let text = said(&page);
    for hostile in [
        HOSTILE_BODY,
        HOSTILE_SUBJECT,
        "#strong[bold] </pre>",
        "]#link(\"javascript:x\")[",
    ] {
        assert!(
            text.contains(hostile),
            "{hostile:?} is not on the page as text"
        );
    }
    for file in site.pages() {
        let html = site.page(&file);
        let lower = html.to_lowercase();
        assert!(!lower.contains("<img"), "{file}: an element from the mail");
        assert!(
            !lower.contains("href=\"javascript"),
            "{file}: a link from the mail"
        );
        assert!(
            !lower.contains("<strong>bold"),
            "{file}: markup from the mail"
        );
    }
    assert!(
        !page.to_lowercase().contains("<script"),
        "a script element on the hostile page"
    );
}

/// A page whose HTML the tripwire refuses is not written; the page that
/// stands in its place says so, and the command names it. Text can trip it
/// only where the tripwire reads conservatively: a URL attribute is read
/// wherever it is spelled, text included.
#[test]
fn a_page_the_tripwire_refuses_says_so() {
    let root = scratch("refused");
    let events = EventStore::new(
        root.join("events"),
        prodrome_core::policy::Untrusted::none(),
    );
    let made = shift(now(), -20);
    suzatary_core::store::append_to(
        &events,
        suzatary_core::record::GENESIS,
        mk_authored(
            "tripped",
            made,
            "triage",
            "todo",
            made,
            suzatary_core::record::MarkupSource::escaped("click here href=javascript:steal()"),
            mk_flat(0.5).expect("a term"),
            Vec::new(),
            "home",
            "",
            suzatary_core::record::MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("valid"),
    )
    .expect("appends");
    let instance = an_instance(&root);
    let out = root.join("site");
    let ran = crate::binary::keyed()
        .args(["--instance", &instance.to_string_lossy(), "web"])
        .arg(&out)
        .output()
        .expect("the binary runs");
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert!(
        String::from_utf8_lossy(&ran.stderr).contains("todo/tripped.html refused"),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let page = std::fs::read_to_string(out.join("todo/tripped.html")).expect("the page");
    assert!(
        said(&page).contains("Refused: typst's HTML for this page carried an executable URL"),
        "{page}"
    );
    assert!(!page.contains("javascript"), "{page}");
    assert!(links(&page).contains(&"../index.html".to_owned()), "{page}");
}
