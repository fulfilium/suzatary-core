//! Typst's HTML export as the one renderer: a template called with a value ([`Called`]), compiled
//! by a [`Compiler`], narrowed to its body and past the tripwire ([`suspicious`]).
//!
//! A body and an answer evaluate a writer's or a model's markup in the closed world, so no
//! `html` but typst's own; the tripwire refuses the impossible case, never edits it. A page's
//! template reads every datum as data. A body and an answer are kept on disk; a page only in the
//! process, since its prices move with the clock.

use std::path::PathBuf;
use std::sync::Arc;

use prodrome_memo::Finding;
use suzatary_base::answer::Renderable;
use suzatary_base::capability::RenderError;
use suzatary_base::capability::{Rejected, Typeset};
use suzatary_base::escape::Escaped;
use suzatary_base::narrowing::narrowed;
use suzatary_base::tripwire::suspicious;
use suzatary_todo::render::{Html, Remembered, Render};
use suzatary_todo::rendering::{self, ANSWER, MARKUP, Markup};
use suzatary_todo::template::{Called, Job, Kept, Output, Unprintable};
use suzatary_topics::secretary::answers::{AnswerError, Answers};

use crate::rendered::Rendered;
use crate::typeset::Compiler;

const FRAGMENTS_KEPT: usize = 4096;

const PAGES_KEPT: usize = 256;

pub struct Typst {
    fragments: Rendered,
    pages: Rendered,
    compiler: Arc<dyn Compiler>,
    /// The closed world's answers: one compile per text, not one per poll.
    closed: Remembered<Texts>,
}

/// The compiler, asked for lone texts.
struct Texts(Arc<dyn Compiler>);

impl Typeset for Texts {
    fn closed(&self, source: &str) -> Result<String, Rejected> {
        self.0.compile(&Job::text(source))
    }
}

impl std::fmt::Debug for Typst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Typst")
            .field("fragments", &self.fragments)
            .finish_non_exhaustive()
    }
}

impl Typst {
    /// `cache` is where bodies and answers are stored by key; `compiler` compiles every call.
    #[must_use]
    pub fn new(cache: impl Into<PathBuf>, compiler: impl Compiler + 'static) -> Self {
        let compiler: Arc<dyn Compiler> = Arc::new(compiler);
        Self {
            fragments: Rendered::on_disk(cache, FRAGMENTS_KEPT, &rendering::kept()),
            pages: Rendered::in_memory(PAGES_KEPT),
            closed: Remembered::new(Texts(Arc::clone(&compiler)), PAGES_KEPT),
            compiler,
        }
    }

    fn table(&self, called: &Called) -> Option<&Rendered> {
        match called.kept() {
            Kept::Disk => Some(&self.fragments),
            Kept::Process => Some(&self.pages),
            Kept::Nowhere => None,
        }
    }

    /// `called`'s output: what its key keeps, or else compiled, narrowed and kept.
    fn rendered(&self, called: &Called) -> Result<Html, RenderError> {
        let table = self.table(called);
        if let Some(table) = table
            && let Some(kept) = table.recall(called.key()).map_err(|e| unread(&e))?
        {
            return Ok(Html::from_render(kept));
        }
        let output = drawn(&*self.compiler, called)?;
        if let Some(table) = table {
            // A finding is kept as evidence and answers nothing; this compile is still the answer.
            let _ = table.remember(called.key().clone(), &output);
        }
        Ok(Html::from_render(output))
    }

    fn answer(&self, block: &Renderable) -> Result<Html, RenderError> {
        self.rendered(&ANSWER.call(block).map_err(|e| unprintable(&e))?)
    }

    /// Every key whose tabulation holds two values: a compile that was not a function of its
    /// call, or a file on disk another writer left.
    #[must_use]
    pub fn findings(&self) -> Vec<Finding> {
        [
            self.fragments.findings(),
            self.pages.findings(),
            self.closed.findings(),
        ]
        .concat()
    }
}

/// `called` compiled and past the tripwire, kept nowhere.
pub fn drawn(compiler: &dyn Compiler, called: &Called) -> Result<String, RenderError> {
    let document = compiler
        .compile(called.job())
        .map_err(|rejected| RenderError::Refused(rejected.said(called.job().main_text())))?;
    match called.output() {
        Output::Body => narrowed(&document),
        Output::Document => match suspicious(&document) {
            Some(reason) => Err(RenderError::Suspicious(reason.to_string())),
            None => Ok(document),
        },
    }
}

fn markup(markup: &Escaped) -> Result<Called, RenderError> {
    MARKUP
        .call(&Markup {
            source: markup.clone(),
        })
        .map_err(|e| unprintable(&e))
}

impl Typeset for Typst {
    fn closed(&self, source: &str) -> Result<String, Rejected> {
        self.closed.closed(source)
    }
}

impl Render for Typst {
    fn html(&self, escaped: &Escaped) -> Result<Html, RenderError> {
        self.rendered(&markup(escaped)?)
    }

    fn cached(&self, escaped: &Escaped) -> Option<Html> {
        // Never compiles; an unreadable cache is reported by `html`, which reads it again.
        let key = MARKUP
            .key_of(&Markup {
                source: escaped.clone(),
            })
            .ok()?;
        self.fragments
            .recall(&key)
            .ok()
            .flatten()
            .map(Html::from_render)
    }

    fn warm(&self, markup: &[Escaped]) -> usize {
        markup
            .iter()
            .filter(|fragment| self.html(fragment).is_ok())
            .count()
    }

    fn page(&self, page: &Called) -> Result<Html, RenderError> {
        self.rendered(page)
    }

    fn findings(&self) -> Vec<Finding> {
        Typst::findings(self)
    }
}

impl suzatary_todo::render_cache::RenderCache for Typst {
    fn forget(&self, block: &Renderable) -> Result<(), String> {
        let key = ANSWER.key_of(block).map_err(|e| e.to_string())?;
        self.fragments
            .forget(&key)
            .map_err(|failed| failed.to_string())
    }
}

impl Answers for Typst {
    fn block(&self, block: &Renderable) -> Result<Html, AnswerError> {
        self.answer(block)
            .map_err(|refused| AnswerError(refused.to_string()))
    }
}

/// An unreadable cache is an error, so a broken disk is never a compile every request.
fn unread(failed: &std::io::Error) -> RenderError {
    RenderError::Refused(format!("the rendered cache could not be read: {failed}"))
}

fn unprintable(failed: &Unprintable) -> RenderError {
    RenderError::Refused(failed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_todo::reading::CalendarPage;
    use suzatary_todo::rendering::CALENDAR;

    fn scratch_cache(name: &str) -> PathBuf {
        let root = suzatary_testkit::this_test().join(format!("render-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn typst_at(name: &str) -> (Typst, PathBuf) {
        let cache = scratch_cache(name);
        (Typst::new(&cache, crate::typeset::InProcess), cache)
    }

    /// Counts its compiles, answering as the in-process compiler does.
    struct Counted(Arc<std::sync::atomic::AtomicUsize>);
    impl Compiler for Counted {
        fn compile(&self, job: &Job) -> Result<String, Rejected> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            crate::typeset::compile(job)
        }
    }

    #[test]
    fn a_text_is_compiled_once_however_often_it_is_asked_about() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let typst = Typst::new(scratch_cache("counted"), Counted(count.clone()));
        for _ in 0..3 {
            assert!(typst.closed("Renew *it*").is_ok());
            assert!(typst.closed("a ] b").is_err());
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn cached_reads_a_fragment_another_process_kept_on_the_same_disk() {
        let cache = scratch_cache("read-from-disk");
        let markup = Escaped::from_chain("= Kept\n");
        let key = MARKUP
            .key_of(&Markup {
                source: markup.clone(),
            })
            .expect("prints");
        Rendered::on_disk(&cache, 8, &rendering::kept())
            .remember(key, "<p>kept</p>")
            .expect("no finding");
        let typst = Typst::new(&cache, crate::typeset::InProcess);
        assert_eq!(
            typst.cached(&markup).map(|html| html.as_str().to_string()),
            Some("<p>kept</p>".to_string())
        );
    }

    /// A detail with a link into the app (`todo:<id>`, as `rich.ts` routes it) and one out of it
    /// renders, past the tripwire: the fixture's checklist detail, which a refusal served as source.
    #[test]
    fn a_todo_link_and_an_outbound_link_render() {
        let (typst, _) = typst_at("links");
        let html = typst
            .html(&Escaped::from_chain(
                "After #link(\"todo:book-flight\")[the flight] is booked. Weights: \
                 #link(\"https://example.invalid/baggage\")[the airline's page].",
            ))
            .expect("renders");
        assert!(
            html.as_str().contains("href=\"todo:book-flight\""),
            "{}",
            html.as_str()
        );
        assert!(
            html.as_str()
                .contains("href=\"https://example.invalid/baggage\"")
        );
    }

    /// Every fixture todo's markup renders past the tripwire, as the fixture server warms it.
    #[test]
    fn every_cast_todo_renders() {
        let (typst, _) = typst_at("cast");
        for todo in suzatary_connect::cast::CAST.todos.all() {
            for text in [todo.body, todo.detail]
                .into_iter()
                .chain(todo.rationale.iter().copied())
            {
                let rendered = typst.html(&Escaped::from_chain(text));
                assert!(rendered.is_ok(), "{}: {text:?}: {rendered:?}", todo.id);
            }
        }
    }

    #[test]
    fn an_answer_block_is_html_with_mathml_in_the_closed_world() {
        let (typst, _) = typst_at("answer-blocks");
        let answer = |source: &str| typst.answer(&Renderable::of(source));
        let block = |source: &str| answer(source).expect("the block compiles");
        assert!(block("$ x^2 $").as_str().contains("<math"), "MathML");
        assert!(
            block("#panel[*Due* Friday]")
                .as_str()
                .contains("<blockquote><strong>Due</strong> Friday</blockquote>"),
            "{}",
            block("#panel[*Due* Friday]").as_str()
        );
        assert!(
            answer("#nosuchthing()").is_err(),
            "a block that does not compile is text"
        );
        for reaching in [
            "#html.elem(\"script\")[x]",
            "#std.html.div[x]",
            "#eval(\"html.elem(\\\"b\\\")[x]\")",
        ] {
            assert!(answer(reaching).is_err(), "{reaching}");
        }
    }

    /// A page's every datum is data: a stranger's title is drawn as text.
    #[test]
    fn a_page_is_classed_html_and_its_data_stays_text() {
        let (typst, _) = typst_at("todo-page");
        let stranger = "Review\"] #eval(\"x\") [<script>";
        let calendar = CalendarPage {
            days: vec![suzatary_todo::reading::CalendarDay {
                heading: "Monday 5 October".to_owned(),
                entries: vec![suzatary_todo::reading::CalendarRow {
                    time: "09:00–10:00".to_owned(),
                    title: stranger.to_owned(),
                    place: Some("Room #4".to_owned()),
                    calendar: "WORK".to_owned(),
                    status: "cancelled".to_owned(),
                    notes: vec!["cancelled".to_owned()],
                }],
            }],
        };
        let html = typst
            .page(&CALENDAR.call(&calendar).expect("prints"))
            .expect("the calendar compiles");
        let html = html.as_str();
        assert!(
            html.contains("Review\"] #eval(\"x\") [&lt;script>"),
            "{html}"
        );
        assert!(
            html.contains("<li class=\"calendarentry cancelled\">"),
            "{html}"
        );
    }

    #[test]
    fn a_page_read_at_two_instants_writes_nothing_under_the_cache() {
        let (typst, cache) = typst_at("pages-stay-in-memory");
        let at = |heading: &str| CalendarPage {
            days: vec![suzatary_todo::reading::CalendarDay {
                heading: heading.to_owned(),
                entries: Vec::new(),
            }],
        };
        let (morning, evening) = (at("Monday 5 October"), at("Tuesday 6 October"));
        assert_ne!(morning, evening, "two days are two values");
        for page in [&morning, &evening, &morning] {
            typst
                .page(&CALENDAR.call(page).expect("prints"))
                .expect("the page compiles");
        }
        let files = |dir: &std::path::Path| std::fs::read_dir(dir).map_or(0, Iterator::count);
        assert_eq!(files(&cache), 0, "a page wrote to the disk cache");
        typst
            .html(&Escaped::from_chain("a body"))
            .expect("the body compiles");
        assert_eq!(files(&cache), 1, "a body is kept on disk");
    }
}
