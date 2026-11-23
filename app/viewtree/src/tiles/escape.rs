//! A leaf's address inside the tree's print: every byte but a letter, a digit, `_` and `.` as
//! `%XX`, and a leading digit too, so that no leaf reads as a share or a mark of the grammar. The
//! empty address is `:`, which no escaped address holds, so it never reads as a root child's home.

pub(crate) fn escape(s: &str) -> String {
    if s.is_empty() {
        return ":".to_owned();
    }
    let hex = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for (i, b) in s.bytes().enumerate() {
        let plain =
            b.is_ascii_alphabetic() || b == b'_' || b == b'.' || (b.is_ascii_digit() && i > 0);
        if plain {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(hex[usize::from(b >> 4)]));
            out.push(char::from(hex[usize::from(b & 15)]));
        }
    }
    out
}

/// `None` for a `%` without two hex digits after it, or bytes that are not UTF-8.
pub(crate) fn unescape(s: &str) -> Option<String> {
    if s == ":" {
        return Some(String::new());
    }
    let mut bytes = Vec::with_capacity(s.len());
    let mut it = s.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let hi = char::from(it.next()?).to_digit(16)?;
            let lo = char::from(it.next()?).to_digit(16)?;
            bytes.push(u8::try_from(hi * 16 + lo).ok()?);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}
