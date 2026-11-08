//! Body text and links from notmuch's `show --format=json` body, a nested forest of MIME parts.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use suzatary_core::mail::MailLink;
use suzatary_core::spelling::shorten;

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

/// One named or numeric character reference, without its `&`/`;`. Deliberately narrow: a full
/// named-entity table would be a dependency this crate does not take.
fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            if let Some(hex) = entity
                .strip_prefix('#')
                .and_then(|e| e.strip_prefix('x').or_else(|| e.strip_prefix('X')))
            {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else {
                let dec = entity.strip_prefix('#')?;
                dec.parse::<u32>().ok().and_then(char::from_u32)
            }
        }
    }
}

/// Unescape the entities `decode_entity` knows; anything else, a bare `&` included, passes
/// through unchanged.
#[must_use]
pub fn unescape_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        // The longest reference decoded is `&#x10FFFF;`. The window counts chars, not bytes: a
        // byte cut would panic inside a multi-byte character.
        let window = shorten(tail, 12);
        if let Some(semi) = window.find(';') {
            let entity = &tail[1..semi];
            if let Some(decoded) = decode_entity(entity) {
                out.push(decoded);
                rest = &tail[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &tail[1..];
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

/// HTML mail, walked into readable text. A comment's content is dropped; a tag's name is read
/// up to its first whitespace, `!` or `/`, so `<!DOCTYPE …>` is the inert empty name.
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
}
