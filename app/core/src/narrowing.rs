//! A compiled document narrowed to what is kept of it: its body, past the tripwire. The name of
//! every function that narrows is a hash of this file and the tripwire's
//! ([`crate::rendering`]), so a change to either is a new function.

use crate::capability::RenderError;
use crate::tripwire::suspicious;

/// The inside of `document`'s `<body>`, refused whole when the tripwire finds anything typst never
/// writes.
///
/// # Errors
///
/// [`RenderError::Refused`] when there is no body, [`RenderError::Suspicious`] when the
/// tripwire fires.
pub fn narrowed(document: &str) -> Result<String, RenderError> {
    let Some(body) = body_of(document) else {
        return Err(RenderError::Refused(
            "typst produced no <body> for the fragment".to_string(),
        ));
    };
    match suspicious(body) {
        Some(reason) => Err(RenderError::Suspicious(reason.to_string())),
        None => Ok(body.to_string()),
    }
}

/// From the first `<body>` to the last `</body>`, so a body whose text mentions `</body>` still
/// ends at the tag.
fn body_of(document: &str) -> Option<&str> {
    let (_, rest) = document.split_once("<body>")?;
    let end = rest.rfind("</body>")?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_of_finds_the_body_of_a_real_typst_document() {
        let document =
            "<!DOCTYPE html><html><head><title>x</title></head><body><p>hi</p></body></html>";
        assert_eq!(body_of(document), Some("<p>hi</p>"));
    }

    #[test]
    fn body_of_is_none_without_a_body() {
        assert_eq!(body_of("<p>hi</p>"), None);
        assert_eq!(body_of(""), None);
    }

    #[test]
    fn body_of_uses_the_last_close_tag_not_the_first() {
        let document = "<body>see the string </body> in the text and the rest</body>";
        assert_eq!(
            body_of(document),
            Some("see the string </body> in the text and the rest")
        );
    }

    #[test]
    fn a_suspicious_body_is_refused() {
        assert!(matches!(
            narrowed("<body><p onclick=\"x()\">hi</p></body>"),
            Err(RenderError::Suspicious(_))
        ));
    }
}
