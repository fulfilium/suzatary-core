//! The containment check: a block is written only if its rendering reads back,
//! through typst's own parser, as the AST that went in. Fields can come from
//! attacker-controlled mail; escaping is a claim, this is a measurement, and a
//! field that altered the structure makes the two differ.
//!
//! It does not go through [`crate::render::Render`]: `Render::html` takes an
//! [`suzatary_base::escape::Escaped`], which only confirmed content yields, and the
//! agent's writes are provisional. The leading `//` run is verified too, so an
//! update cannot silently drop a note about the whole block.

use super::value::{Call, render_block};
use super::{DecorationError, read};
use suzatary_base::refusal::Refusal;

/// Render a block and prove it reads back to itself, through
/// `read::only_block` so nothing after the closing paren is tolerated.
///
/// # Errors
///
/// A [`Refusal`] when the rendering does not read back as the block that was
/// built; the write does not happen.
pub fn render_verified(call: &Call, leading: &[String]) -> Result<String, Refusal> {
    let rendered = render_block(call, leading);
    let read = read::only_block(&rendered, &call.callee).map_err(|DecorationError(message)| {
        Refusal::bad_request(format!("refusing to write unreadable typst: {message}"))
    })?;
    if &read.call != call || read.leading != leading {
        return Err(Refusal::bad_request(
            "refusing to write: the rendered block does not read back as the \
             block that was built, which means a field altered the structure",
        ));
    }
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::super::value::{Arg, Value};
    use super::*;

    fn a_call() -> Call {
        Call {
            callee: "todo".to_owned(),
            args: vec![
                Arg::named("id", Value::Str("filething".to_owned())),
                Arg::named("created", Value::Ident("note_day".to_owned())),
                Arg::explained(
                    "priority",
                    Value::Num("35.00%".to_owned()),
                    vec!["Missing it is not recoverable.".to_owned()],
                ),
                Arg::named("waitingOn", Value::Ident("none".to_owned())),
            ],
            trailing: vec![r"File the \*permit\* renewal".to_owned()],
            hashed: true,
            tail: Vec::new(),
        }
    }

    /// A bare `]` in a body would close the content block: escaped it round
    /// trips, raw it is refused.
    #[test]
    fn an_escaped_body_survives_and_a_raw_one_would_not() {
        let mut call = a_call();
        call.trailing = vec![
            suzatary_base::escape::escape_markup("close ] this #todo(x)[")
                .as_str()
                .to_owned(),
        ];
        let text = render_verified(&call, &[]).expect("verifies");
        assert_eq!(read::only_block(&text, "todo").expect("reads").call, call);

        let mut raw = a_call();
        raw.trailing = vec!["close ] this".to_owned()];
        assert!(render_verified(&raw, &[]).is_err());
    }

    /// A dropped leading run is a refusal, not a quiet deletion.
    #[test]
    fn the_leading_run_is_part_of_what_is_verified() {
        let text = render_verified(&a_call(), &["kept".to_owned()]).expect("verifies");
        assert!(text.starts_with("// kept\n"), "{text}");
        assert_eq!(
            read::only_block(&text, "todo").expect("reads").leading,
            vec!["kept".to_owned()]
        );
    }
}
