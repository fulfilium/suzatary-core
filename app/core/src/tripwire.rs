//! The tripwire over typst's HTML: one check, run by the box on every compile and cache read and by
//! the tab (through `suzatary-wasm`) on every page it compiles, so the two refuse the same strings.
//!
//! It admits what typst writes and nothing else, so a trick nobody listed is refused. It refuses,
//! never edits: a sanitiser that repaired markup would be a second reading of HTML.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// The elements typst writes (HTML, math, SVG frames, the app's pages), lowercased.
const ELEMENTS: &str = "\
    a aside blockquote body br button code dd details div dl dt em figcaption figure h1 h2 h3 \
    h4 h5 h6 head hr html img li mark meta nav ol p pre s section span strong style sub \
    summary sup table tbody td tfoot th thead title tr ul math mfrac mi mmultiscripts mn mo \
    mover mprescripts mroot mrow mspace msqrt msub msubsup msup mtable mtd mtext mtr munder \
    munderover clippath defs g image lineargradient path pattern radialgradient rect stop svg \
    symbol use";

/// The attributes typst writes, lowercased. Any `data-` or `aria-` attribute is inert too.
const ATTRIBUTES: &str = "\
    alt charset cite class colspan content height href id lang name open reversed role rowspan \
    src start style type value width accent accentunder columnspan display displaystyle fence \
    form largeop linethickness lspace mathvariant maxsize minsize movablelimits rspace \
    scriptlevel separator stretchy symmetric clip-path cx cy d fill fill-rule fr fx fy \
    gradienttransform gradientunits offset overflow patterntransform patternunits \
    preserveaspectratio r shape-rendering spreadmethod stop-color stroke stroke-dasharray \
    stroke-dashoffset stroke-linecap stroke-linejoin stroke-miterlimit stroke-width transform \
    viewbox x xlink:href xmlns xmlns:h5 xmlns:xlink y";

/// The attributes whose value a browser follows as a URL.
const URLS: [&str; 3] = ["href", "src", "xlink:href"];

/// The schemes a followed URL may name, as `web/src/view/href.ts` follows them (`todo:<id>` is the
/// app's own route), beside none and a [`crate::reference::Reference`]'s.
const SCHEMES: [&str; 5] = ["http", "https", "mailto", "tel", "todo"];

const ELEMENT: &str = "an element typst never writes";
const ATTRIBUTE_NAME: &str = "an attribute typst never writes";
const URL: &str = "a URL that does not navigate";

/// The whitespace a browser splits a tag on, and no other: the regex crate's `\s` is Unicode's,
/// so a no-break space or a vertical tab would end a name or a value here and not in the browser.
const SPACE: &str = r"\t\n\x0C\r ";

/// A tag's start and its name: `<` and a letter, as a browser opens one (typst writes `<` in text
/// as `&lt;`).
static TAG: LazyLock<Regex> = LazyLock::new(|| pattern(&format!("<([a-zA-Z][^{SPACE}/>]*)")));

/// One attribute at the head of what follows a tag's name: its name, then its value whole,
/// quoted or not, so nothing inside a value is ever read as a name.
static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
    pattern(&format!(
        r#"^[{SPACE}/]*([^{SPACE}/>][^{SPACE}/>=]*)(?:[{SPACE}]*=[{SPACE}]*(?:"([^"]*)"|'([^']*)'|([^{SPACE}>]*)))?"#
    ))
});

/// Whether `name` is one of the space-separated words of `list`.
fn listed(list: &str, name: &str) -> bool {
    list.split_ascii_whitespace().any(|word| word == name)
}

fn pattern(text: &str) -> Regex {
    Regex::new(text).expect("a fixed, tested pattern")
}

/// The first element, attribute or URL in this HTML that typst never writes, or `None`.
#[must_use]
pub fn suspicious(html: &str) -> Option<&'static str> {
    TAG.captures_iter(html).find_map(|tag| {
        let (whole, name) = (tag.get(0)?, tag.get(1)?.as_str());
        if !listed(ELEMENTS, &name.to_ascii_lowercase()) {
            return Some(ELEMENT);
        }
        let mut rest = &html[whole.end()..];
        std::iter::from_fn(|| {
            let found = ATTRIBUTE.captures(rest)?;
            rest = &rest[found.get(0)?.end()..];
            Some(found)
        })
        .find_map(|found| refused(&found))
    })
}

fn refused(found: &Captures<'_>) -> Option<&'static str> {
    let name = found.get(1)?.as_str().to_ascii_lowercase();
    let inert = name.starts_with("data-") || name.starts_with("aria-");
    if !inert && !listed(ATTRIBUTES, &name) {
        return Some(ATTRIBUTE_NAME);
    }
    let value = (2..=4)
        .find_map(|group| found.get(group))
        .map_or("", |value| value.as_str());
    (URLS.contains(&name.as_str()) && !navigates(value)).then_some(URL)
}

/// A character reference's character: numeric ones, with or without their `;`, and the named ones
/// a scheme can be spelled with.
static REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)&(?:#x([0-9a-f]+);?|#([0-9]+);?|(tab|newline|colon|amp|quot|apos|lt|gt);)")
});

/// The named references [`REFERENCE`] reads.
fn named(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "tab" => "\t",
        "newline" => "\n",
        "colon" => ":",
        "amp" => "&",
        "quot" => "\"",
        "apos" => "'",
        "lt" => "<",
        _ => ">",
    }
}

/// Does this URL navigate? Read as a browser reads it: references decoded, every control and
/// space taken out (typst writes tabs raw inside an attribute), then the scheme before `:`.
fn navigates(value: &str) -> bool {
    let decoded = REFERENCE.replace_all(value, |found: &Captures<'_>| {
        // A number naming no character reads as U+FFFD, as a browser reads it.
        let numeric = |group, radix| {
            found.get(group).map(|digits| {
                u32::from_str_radix(digits.as_str(), radix)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
            })
        };
        numeric(1, 16)
            .or_else(|| numeric(2, 10))
            .map_or_else(|| named(&found[3]).to_owned(), String::from)
    });
    let bare: String = decoded
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    let scheme = bare
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| {
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
    scheme.is_none_or(|scheme| SCHEMES.contains(&scheme))
        || decoded
            .trim()
            .parse::<crate::reference::Reference>()
            .is_ok()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The attack: markup that runs, loads or navigates where it should not, through an element or
    /// an attribute typst never writes. Each is refused, whatever spelling a browser still reads,
    /// SVG animation that sets a link after parsing included.
    #[test]
    fn what_typst_never_writes_is_refused() {
        for (html, reason) in [
            ("<ScRiPt>alert(1)</script>", ELEMENT),
            (
                "<svg><a><animate attributeName=\"href\" values=\"javascript:alert(1)\"/>",
                ELEMENT,
            ),
            (
                "<svg><set attributeName=\"href\" to=\"javascript:x\"/>",
                ELEMENT,
            ),
            ("<base href=\"https://example.test/\">", ELEMENT),
            (
                "<form action=\"https://example.test\"><input></form>",
                ELEMENT,
            ),
            ("<iframe src=\"https://example.test\">", ELEMENT),
            (
                "<svg><foreignObject><p>x</p></foreignObject></svg>",
                ELEMENT,
            ),
            ("<img src=\"x\"onerror=\"y()\">", ATTRIBUTE_NAME),
            ("<svg/onload=\"x()\">", ATTRIBUTE_NAME),
            ("<img src=a\u{a0}x=\" onerror=alert(1)//\">", ATTRIBUTE_NAME),
            (
                "<meta http-equiv=\"refresh\" content=\"0;url=https://example.test\">",
                ATTRIBUTE_NAME,
            ),
            ("<a href=\"java&Tab;script&colon;alert(1)\">go</a>", URL),
            ("<a href=\"&#106&#97vascript:alert(1)\">go</a>", URL),
            ("<svg><a xlink:href=\"javascript:x\">", URL),
            ("<img src=\"data:image/svg+xml,x\">", URL),
            ("<a href=\"ftp://example.test/\">go</a>", URL),
        ] {
            assert_eq!(suspicious(html), Some(reason), "{html:?}");
        }
    }

    #[test]
    fn ordinary_typst_html_is_not_suspicious() {
        for html in [
            "<p>a <strong>thing</strong></p>",
            r#"<a href="https://example.com">a link</a>"#,
            r#"<a href="mailto:a@example.com?subject=x&amp;body=y">a</a>"#,
            r##"<a href="#loc-1">a fragment</a>"##,
            r#"<a href="todo:book-flight">the flight</a>"#,
            "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" \
             content=\"width=device-width\"><title>x</title></head><body><p>hi</p></body></html>",
            "<p>a base of data, a form of words</p>",
            r#"<a href="https://x.test/a/onboarding=1">x</a>"#,
            "<p>a/one = b</p>",
        ] {
            assert_eq!(suspicious(html), None, "{html}");
        }
        assert_eq!(
            suspicious(
                "<math display=\"block\"><mrow><mi>x</mi><mo stretchy=\"false\">=</mo>\
                 <mfrac><mn>1</mn><mn>2</mn></mfrac></mrow></math>"
            ),
            None
        );
        assert_eq!(
            suspicious(
                "<svg class=\"typst-frame\" viewBox=\"0 0 10 10\" xmlns=\"http://www.w3.org/2000/svg\" \
                 xmlns:xlink=\"http://www.w3.org/1999/xlink\"><g transform=\"matrix(1 0 0 1 0 0)\">\
                 <use xlink:href=\"#g1\" x=\"0\"/><path d=\"M0 0\" fill=\"#000\"/></g></svg>"
            ),
            None
        );
        assert_eq!(
            suspicious(
                "<button type=\"button\" class=\"b\" data-intent=\"x\" aria-label=\"y\">z</button>"
            ),
            None
        );
    }

    #[test]
    fn escaped_text_naming_a_script_is_not_suspicious() {
        assert_eq!(suspicious("<p>&lt;script>alert(1)&lt;/script></p>"), None);
    }

    /// A character a browser takes out of a scheme, or that marks no scheme at all: every C0
    /// control, the space, and the other whitespace, raw or as a character reference.
    fn stray() -> impl Strategy<Value = String> {
        let raw = prop_oneof![
            (0u32..0x20).prop_map(|c| char::from_u32(c).expect("C0")),
            Just(' '),
            Just('\u{a0}'),
            Just('\u{2028}'),
            Just('\u{3000}'),
        ];
        (raw, 0u8..3).prop_map(|(c, spelling)| match spelling {
            0 => c.to_string(),
            1 => format!("&#x{:x};", u32::from(c)),
            _ => format!("&#{};", u32::from(c)),
        })
    }

    const REASONS: [&str; 3] = [ELEMENT, ATTRIBUTE_NAME, URL];

    proptest! {
        #[test]
        fn the_tripwire_answers_for_any_text(text in ".*") {
            if let Some(reason) = suspicious(&text) {
                prop_assert!(REASONS.contains(&reason), "the tripwire invented a reason");
            }
        }

        /// No element or attribute name outside the vocabulary is admitted, in any case.
        #[test]
        fn a_name_outside_the_vocabulary_is_refused(name in "[a-zA-Z][a-zA-Z0-9:-]{0,12}") {
            let lower = name.to_ascii_lowercase();
            if !listed(ELEMENTS, &lower) {
                prop_assert_eq!(suspicious(&format!("<{name}>")), Some(ELEMENT));
            }
            if !listed(ATTRIBUTES, &lower)
                && !lower.starts_with("data-")
                && !lower.starts_with("aria-")
            {
                prop_assert_eq!(suspicious(&format!("<p {name}=\"x\">")), Some(ATTRIBUTE_NAME));
            }
        }

        /// Every control and space, anywhere in a scheme that does not navigate, in any case, in
        /// any URL attribute and any quoting, is refused.
        #[test]
        fn a_scheme_salted_with_controls_and_spaces_is_still_refused(
            scheme in prop::sample::select(&["javascript:", "vbscript:", "data:"][..]),
            salt in prop::collection::vec((0usize..16, stray()), 0..4),
            upper in prop::collection::vec(any::<bool>(), 16),
            attribute in prop::sample::select(&URLS[..]),
            quote in prop::sample::select(&["\"", "'"][..]),
        ) {
            let mut spelled: Vec<String> = scheme
                .chars()
                .zip(&upper)
                .map(|(c, &up)| if up { c.to_ascii_uppercase() } else { c }.to_string())
                .collect();
            for (at, stray) in salt {
                let at = at % (spelled.len() + 1);
                spelled.insert(at, stray);
            }
            let html = format!("<a {attribute}={quote}{}x{quote}>go</a>", spelled.concat());
            prop_assert_eq!(suspicious(&html), Some(URL), "{:?}", html);
        }
    }
}
