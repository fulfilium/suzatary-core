//! Body text and links from notmuch's `show --format=json` body, a nested forest of MIME parts.

use std::sync::LazyLock;

use mail_parser::decoders::html::add_html_token;
use regex::Regex;
use serde_json::Value;
use suzatary_mail::mail::MailLink;

/// Every leaf whose `content-type` starts with `mimetype`. A multipart container is recursed
/// into whatever its own type, since a `multipart/mixed` part holds `text/plain` leaves.
fn typed_parts<'a>(part: &'a Value, mimetype: &str, out: &mut Vec<&'a str>) {
    match part {
        Value::Array(items) => {
            for item in items {
                typed_parts(item, mimetype, out);
            }
        }
        Value::Object(map) => {
            let content_type = map
                .get("content-type")
                .and_then(Value::as_str)
                .unwrap_or("");
            match map.get("content") {
                Some(Value::String(s)) if content_type.starts_with(mimetype) => out.push(s),
                Some(list @ Value::Array(_)) => typed_parts(list, mimetype, out),
                _ => {}
            }
        }
        _ => {}
    }
}

/// The readable body: `text/plain` when the message carries one, else `text/html` converted,
/// so an empty body means empty, never an html-only message unread.
#[must_use]
pub fn body_text(body: &Value) -> String {
    let mut plain_parts = Vec::new();
    typed_parts(body, "text/plain", &mut plain_parts);
    let plain = plain_parts.concat();
    let trimmed = plain.trim();
    if !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    let mut html_parts = Vec::new();
    typed_parts(body, "text/html", &mut html_parts);
    html_to_text(&html_parts.concat())
}

/// Every `&name;`, `&#n;` and `&#xh;` decoded by `mail-parser`'s table, which names every HTML
/// entity; a number that is no character becomes U+FFFD, as a browser draws it. An unknown name
/// or a `&` that opens no reference passes through unchanged.
#[must_use]
pub fn unescape_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let semi = tail[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .map_or(tail.len(), |at| at + 1);
        if tail[semi..].starts_with(';') {
            add_html_token(&mut out, &tail.as_bytes()[..=semi], false);
            rest = &tail[semi + 1..];
        } else {
            out.push('&');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

// Byte slicing in the detagger is safe: every delimiter searched for is ASCII, which is never a
// UTF-8 continuation byte.

/// Tags whose content is markup or styling, never prose — dropped whole.
const DROP: [&str; 4] = ["script", "style", "head", "title"];

/// Tags that end a line of prose on their closing tag (`<br>` is handled at the open).
const BREAK: [&str; 12] = [
    "p",
    "div",
    "tr",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "table",
];

/// Tags whose closing tag separates their words from the next cell's by a space; the row's `tr`
/// ends the line.
const CELL: [&str; 2] = ["td", "th"];

static SPACES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]+").expect("a literal pattern"));
static BLANK_RUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\n{3,}").expect("a literal pattern"));

fn finish_text(joined: &str) -> String {
    let normalized = joined.replace('\u{a0}', " ");
    let lines: Vec<String> = normalized
        .lines()
        .map(|line| SPACES.replace_all(line, " ").trim().to_owned())
        .collect();
    let combined = lines.join("\n");
    BLANK_RUN.replace_all(&combined, "\n\n").trim().to_owned()
}

/// HTML mail, walked into readable text; a tag's name ends at whitespace, `!` or `/`, so
/// `<!DOCTYPE …>` is the inert empty name.
///
/// The leak guard reads drawn markup through this (`door::drawing`), so the text keeps every
/// word a browser draws and two blocks' words never join; `mail-parser`'s `html_to_text` joins
/// `<li>` items, drops all after a comment closed by `--->` and keeps an attributed `<style>`.
#[must_use]
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut skip: u32 = 0;
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        if skip == 0 {
            out.push_str(&unescape_entities(&rest[..lt]));
        }
        rest = &rest[lt..];
        if let Some(after_bang) = rest.strip_prefix("<!--") {
            let Some(end) = after_bang.find("-->") else {
                rest = "";
                break;
            };
            rest = &after_bang[end + 3..];
            continue;
        }
        let Some(gt) = rest.find('>') else {
            rest = "";
            break;
        };
        let tag_body = &rest[1..gt];
        rest = &rest[gt + 1..];

        let is_end = tag_body.starts_with('/');
        let is_selfclose = tag_body.trim_end().ends_with('/');
        let name_part = tag_body
            .trim_start_matches('/')
            .trim_start_matches('!')
            .trim_end_matches('/');
        let name = name_part
            .split(char::is_whitespace)
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        if !is_end {
            if DROP.contains(&name.as_str()) {
                skip += 1;
            } else if name == "br" {
                out.push('\n');
            }
        }
        if is_end || is_selfclose {
            if DROP.contains(&name.as_str()) {
                skip = skip.saturating_sub(1);
            } else if BREAK.contains(&name.as_str()) {
                out.push('\n');
            } else if CELL.contains(&name.as_str()) {
                out.push(' ');
            }
        }
    }
    if skip == 0 {
        out.push_str(&unescape_entities(rest));
    }
    finish_text(&out)
}

/// A labelled anchor, `href="…">…</a>`: the label may span lines, and the tag is
/// case-insensitive.
static ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\b[^>]*?href=["']([^"']+)["'][^>]*>(.*?)</a>"#).expect("a literal pattern")
});
/// A bare `http(s)://` URL, stopping at whitespace or a character that is almost never part of
/// a URL but often follows one in prose or markup.
static BARE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s"'<>\)\]]+"#).expect("a literal pattern"));
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("a literal pattern"));

/// Every link from every part: labelled anchors from the html parts, then bare URLs from html
/// and plain, de-duplicated by URL. Every part, because a link may be only in the html.
#[must_use]
pub fn extract_links(body: &Value) -> Vec<MailLink> {
    let mut html_parts = Vec::new();
    typed_parts(body, "text/html", &mut html_parts);
    let html = html_parts.concat();

    let mut seen = std::collections::HashSet::new();
    let mut links = Vec::new();

    for cap in ANCHOR.captures_iter(&html) {
        let url = unescape_entities(cap[1].trim());
        if !url.starts_with("http") || seen.contains(&url) {
            continue;
        }
        let stripped = TAGS.replace_all(&cap[2], " ");
        let label = unescape_entities(&stripped);
        let label: String = label
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(120)
            .collect();
        seen.insert(url.clone());
        links.push(MailLink { url, label });
    }

    let mut plain_parts = Vec::new();
    typed_parts(body, "text/plain", &mut plain_parts);
    let plain = plain_parts.concat();

    for text in [html.as_str(), plain.as_str()] {
        let unescaped = unescape_entities(text);
        for found in BARE_URL.find_iter(&unescaped) {
            let url = found.as_str().to_owned();
            if seen.insert(url.clone()) {
                links.push(MailLink {
                    url,
                    label: String::new(),
                });
            }
        }
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unresolved `&` before a multi-byte character straddling byte 12 does not panic and
    /// passes through unchanged.
    #[test]
    fn a_display_name_whose_cjk_character_crosses_the_old_byte_cut_does_not_panic() {
        let unresolved = "&Nora Vale 诺拉) joins the call";
        assert_eq!(unescape_entities(unresolved), unresolved);
    }

    /// Every named entity decodes, the longest name included.
    #[test]
    fn a_named_entity_beyond_the_six_decodes() {
        assert_eq!(
            html_to_text(
                "<p>don&rsquo;t &mdash; caf&eacute; &CounterClockwiseContourIntegral;</p>"
            ),
            "don’t — café ∳"
        );
    }

    /// The leak guard's reading: a word split by an inline element and ended by a block stays
    /// one word apart from the next block, and a comment closed by `--->` hides nothing after it.
    #[test]
    fn the_guards_reading_keeps_blocks_apart_and_reads_past_an_odd_comment_close() {
        assert_eq!(html_to_text("<li>A<b>da</b></li><li>x</li>"), "Ada\nx");
        assert_eq!(html_to_text("a<!-- hidden --->Ada"), "aAda");
    }

    /// The leak guard's reading of a table: a two-word term set in two cells of a row reads as
    /// its two words, as a browser draws the cells apart, and each row is its own line.
    #[test]
    fn the_guards_reading_keeps_table_cells_apart() {
        assert_eq!(
            html_to_text(
                "<table><tr><th>Ada</th><td>Love<b>lace</b></td></tr><tr><td>x</td></tr></table>"
            ),
            "Ada Lovelace\nx"
        );
    }
}
