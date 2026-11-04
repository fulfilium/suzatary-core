//! A message file's held values: every text the readers decode of it, in one fixed order, so the
//! mask that writes a handle and the reveal that reads one count the same values.

use mail_parser::{Address, HeaderValue, Message, MessageParser, MimeHeaders as _, PartType};
use suzatary_core::held_value::Values;

/// The values of `raw` and then of `shown`, the texts a reader built of it (its header values by
/// name, its body); a file that does not parse is read as text.
#[must_use]
pub fn values(raw: &[u8], shown: &[&str]) -> Values {
    let mut texts = match MessageParser::default().parse(raw) {
        Some(message) => decoded(&message),
        None => vec![String::from_utf8_lossy(raw).into_owned()],
    };
    texts.extend(shown.iter().map(|text| (*text).to_owned()));
    Values::of(texts.iter().map(String::as_str))
}

/// Every header value and every text part of `message`, forwarded messages included, in the
/// file's order.
fn decoded(message: &Message<'_>) -> Vec<String> {
    let mut texts = Vec::new();
    for part in &message.parts {
        for header in &part.headers {
            texts.push(value_text(&header.value).join(" "));
        }
        match &part.body {
            PartType::Text(text) | PartType::Html(text) => texts.push(text.to_string()),
            PartType::Binary(bytes) | PartType::InlineBinary(bytes) if textual(part) => {
                texts.push(String::from_utf8_lossy(bytes).into_owned());
            }
            PartType::Message(inner) => texts.extend(decoded(inner)),
            PartType::Binary(_) | PartType::InlineBinary(_) | PartType::Multipart(_) => {}
        }
    }
    texts
}

/// A `text/*` or `message/*` part the parser kept as bytes, its charset unknown to it.
fn textual(part: &mail_parser::MessagePart<'_>) -> bool {
    part.content_type().is_some_and(|ct| {
        ct.c_type.eq_ignore_ascii_case("text") || ct.c_type.eq_ignore_ascii_case("message")
    })
}

fn value_text(value: &HeaderValue<'_>) -> Vec<String> {
    match value {
        HeaderValue::Text(text) => vec![text.to_string()],
        HeaderValue::TextList(list) => list.iter().map(ToString::to_string).collect(),
        HeaderValue::Address(address) => {
            let addr = |addr: &mail_parser::Addr<'_>| {
                addr.name
                    .iter()
                    .chain(addr.address.iter())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            };
            match address {
                Address::List(list) => list.iter().flat_map(addr).collect(),
                Address::Group(groups) => groups
                    .iter()
                    .flat_map(|group| {
                        group
                            .name
                            .iter()
                            .map(ToString::to_string)
                            .chain(group.addresses.iter().flat_map(addr))
                    })
                    .collect(),
            }
        }
        HeaderValue::ContentType(ct) => std::iter::once(ct.c_type.to_string())
            .chain(ct.c_subtype.iter().map(ToString::to_string))
            .chain(
                ct.attributes
                    .iter()
                    .flatten()
                    .map(|attribute| attribute.value.to_string()),
            )
            .collect(),
        HeaderValue::DateTime(_) | HeaderValue::Received(_) | HeaderValue::Empty => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::redact::Kind;

    /// Synthetic numbers no person holds.
    const SSN: &str = "000-12-3456";
    const ITIN: &str = "912-70-1234";

    /// Header values come first, then the parts, then what a reader built; each value once.
    #[test]
    fn a_message_s_values_are_read_in_the_file_s_order() {
        let raw = format!(
            "Subject: ITIN {ITIN}\nContent-Type: multipart/mixed; boundary=XX\n\n--XX\n\
             Content-Type: text/plain\n\nSSN {SSN} and again 000 12 3456\n--XX--\n"
        );
        let values = values(raw.as_bytes(), &[&format!("SSN {SSN}")]);
        assert_eq!(values.get(0), Some((Kind::Itin, ITIN)));
        assert_eq!(values.get(1), Some((Kind::Ssn, SSN)));
        assert_eq!(values.get(2), None);
    }

    #[test]
    fn a_clean_message_holds_no_value() {
        let raw = "Subject: invoice 123456789\n\nOrder 555 0100.\n";
        assert!(values(raw.as_bytes(), &[]).is_empty());
    }
}
