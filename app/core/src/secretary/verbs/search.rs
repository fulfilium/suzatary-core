use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::{answered, refused};
use crate::capability::Query;
use crate::refusal::Refusal;
use crate::secretary::file_query::FileQuery;
use crate::secretary::file_search::{self, FileHit, MAX_FILE_HITS};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Search the mail and the files the owner sent in a chat, with one query.
pub struct Search;

#[derive(Deserialize, JsonSchema)]
pub struct SearchArgs {
    query: String,
    limit: Option<usize>,
}

/// How many files a search answers when asked for no number.
const FILES_DEFAULT: usize = 10;

/// The answer: the files or why they were not searched, and the mail rows. An answer prints its
/// keys in alphabetical order, so `files` leads; there are at most [`MAX_FILE_HITS`], so a line
/// that stores only the head of a long answer keeps every file and cuts mail rows instead.
#[derive(Serialize)]
struct Answer {
    files: Files,
    mail: Vec<crate::mail::verbs::Summary>,
}

/// What the file side of a search answers.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Files {
    /// Every file that matched, newest first.
    Found(Vec<FileHit>),
    /// Why no file was searched, so "no file matched" is never said for a search not made.
    NotSearched(String),
}

impl Verb for Search {
    type Surface = SecretarySurface;
    type Args = SearchArgs;
    type Answer = Value;
    const NAME: &'static str = "search";
    const DESCRIPTION: &'static str = "Any notmuch query over the mail corpus AND the photos and \
        PDFs the owner sent in a chat, newest first (e.g. `from:counsel@example.com`, \
        `lease unit 4`). Use it to find the thread or the document a question is about before \
        you answer it. Answers `files` first: under `found`, each file whose name or text holds \
        every word of the query, with the conversation and turn it came in on, what he said \
        there, and the words around the match; `upload_read` opens one, and `propose_email`'s \
        `{\"upload\": <file>}` attaches it. Then `mail`: the rows `show` opens, each with \
        `held`, the kinds of held value its message names (`ssn`, `passport`, `a_number`, ...), \
        never a value: `show` gives each as a placeholder. A file is \
        matched on plain words, `word*` prefixes and quoted phrases, every word required and \
        matched whole, never stemmed, so try `permit*` where mail would find `permits`. A query \
        with a field such as `from:` or an operator such as `OR` searches the mail alone, and \
        `files` says why under `not_searched`. A photo is found by its name only, and a PDF the \
        box holds back for naming an identity number is never found.";

    fn run(call: &Call<'_>, args: SearchArgs) -> Result<Value, Refusal> {
        let query = Query::new(&args.query).map_err(|e| Refusal::bad_request(e.to_string()))?;
        let mail = call
            .caps
            .corpus
            .search(&query, args.limit)
            .map_err(|e| refused(&e))?;
        let files = match FileQuery::of(&query) {
            Ok(words) => files(
                call,
                &words,
                args.limit.unwrap_or(FILES_DEFAULT).min(MAX_FILE_HITS),
            ),
            Err(unanswerable) => Files::NotSearched(unanswerable.to_string()),
        };
        answered(&Answer { files, mail })
    }
}

/// The files that match, or why the index could not answer: the mail is answered either way.
fn files(call: &Call<'_>, query: &FileQuery, limit: usize) -> Files {
    let conversations = match crate::secretary::list(call.caps.conversations) {
        Ok(conversations) => conversations,
        Err(failed) => return Files::NotSearched(failed.message().to_owned()),
    };
    match call
        .caps
        .upload_index
        .refresh(&file_search::named(&conversations))
    {
        Ok(standings) => Files::Found(file_search::find(&conversations, &standings, query, limit)),
        Err(failed) => Files::NotSearched(failed.to_string()),
    }
}
