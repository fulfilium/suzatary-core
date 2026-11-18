//! Percent-encoding over RFC 3986's unreserved set: a space is `%20`, never
//! `+`, so reading an encoded value needs no rule but this one.

use std::fmt::Write as _;

/// `value` with every byte outside `A-Za-z0-9-._~` as `%XX`.
#[must_use]
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// `key=value&…`, each side [`percent_encode`]d.
#[must_use]
pub fn encode_form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_unreserved_set_passes_through() {
        assert_eq!(percent_encode("aZ09-._~"), "aZ09-._~");
        assert_eq!(percent_encode("a b+c/é"), "a%20b%2Bc%2F%C3%A9");
        assert_eq!(encode_form(&[("k", "v&w=x"), ("n", "")]), "k=v%26w%3Dx&n=");
    }
}
