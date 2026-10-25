//! The door an item comes through: a plugin's bytes, parsed into its declared schema, printed
//! back, and held to speak only as the plugin that emitted it.

use prodrome_core::declared::{Declaration, Declared};
use prodrome_core::event::{canonical, parse_event};
use prodrome_core::schema::Schema;

use super::{PluginName, Said};

/// The most bytes one item may be.
pub const MAX_ITEM: usize = 64 * 1024;

/// Why an item a plugin emitted was not admitted. It names a schema, an actor or what the parse
/// refused, as [`Said`], never more of the item than that.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ItemRefusal {
    /// Past the size an item may be.
    #[error("an item of {0} bytes is larger than this host reads")]
    TooLarge(usize),
    /// Not one event of the schema: not text, not its grammar, or a field its type refuses.
    #[error("not an event of this plugin's schema: {0}")]
    NotAnEvent(Said),
    /// It parses, and does not print back to the bytes it came as: a second spelling of one
    /// event would be a second object.
    #[error("the item does not print back to the bytes it came as")]
    NotCanonical,
    /// It says another actor wrote it; a plugin writes as itself.
    #[error("the item says {0:?} wrote it, and this plugin writes as itself")]
    NotItsOwn(Said),
}

/// Admit one item `plugin` emitted into `schema`, the plugin's admitted declaration: the event
/// it is.
///
/// # Errors
/// Every [`ItemRefusal`]: the size, the parse, the reprint, and the actor.
pub fn admit(
    schema: &Declaration,
    plugin: &PluginName,
    bytes: &[u8],
) -> Result<Declared, ItemRefusal> {
    if bytes.len() > MAX_ITEM {
        return Err(ItemRefusal::TooLarge(bytes.len()));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ItemRefusal::NotAnEvent(Said::clip("the bytes are not UTF-8")))?;
    let event: Declared = parse_event(schema, text)
        .map_err(|refused| ItemRefusal::NotAnEvent(Said::clip(refused.to_string())))?;
    if canonical(&event) != text {
        return Err(ItemRefusal::NotCanonical);
    }
    if event.actor().as_str() != plugin.as_str() {
        return Err(ItemRefusal::NotItsOwn(Said::clip(event.actor().as_str())));
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = "Schema(key='line', events=(Event(name='Said', fields=(Field(name='line', \
        type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), \
        Field(name='words', type=Text()))),), registers=(Register(name='words', order=Discrete(), \
        inflationary=False, writes=(Write(event='Said', field='words'),)),))";

    const ITEM: &str =
        "Said(line='a', at=datetime(2026, 10, 5, 10, 0, 0), actor='probe', words='hello')";

    fn probe() -> PluginName {
        PluginName::parse("probe").expect("a name")
    }

    fn schema() -> Declaration {
        Declaration::admit(SCHEMA).expect("a lawful schema")
    }

    #[test]
    fn every_way_an_item_can_be_wrong_is_refused_by_name() {
        let schema = schema();
        let refused = |bytes: &str| admit(&schema, &probe(), bytes.as_bytes());
        assert!(matches!(
            refused("not an event"),
            Err(ItemRefusal::NotAnEvent(_))
        ));
        assert!(matches!(
            refused(&ITEM.replace("words='hello'", "words=3")),
            Err(ItemRefusal::NotAnEvent(_))
        ));
        assert!(matches!(
            refused(&ITEM.replace("Said(", "Sealed(")),
            Err(ItemRefusal::NotAnEvent(_))
        ));
        assert_eq!(
            refused(&ITEM.replace("words='hello'", "words=\"hello\"")),
            Err(ItemRefusal::NotCanonical)
        );
        assert_eq!(
            refused(&ITEM.replace("actor='probe'", "actor='ics'")),
            Err(ItemRefusal::NotItsOwn(Said::clip("ics")))
        );
        assert!(matches!(
            refused(&ITEM.replace("hello", &"x".repeat(MAX_ITEM))),
            Err(ItemRefusal::TooLarge(_))
        ));
    }

    proptest::proptest! {
        /// Print then admit is the identity: whatever the words, an event's one spelling is
        /// admitted as that event.
        #[test]
        fn an_event_prints_and_is_admitted_back(words in "\\PC{0,60}", line in "[^\\p{Cc}]{1,20}") {
            let schema = schema();
            let text = format!(
                "Said(line={}, at=datetime(2026, 10, 5, 10, 0, 0), actor='probe', words={})",
                prodrome_core::literal::print_str(&line),
                prodrome_core::literal::print_str(&words),
            );
            let event = admit(&schema, &probe(), text.as_bytes());
            proptest::prop_assert_eq!(event.map(|event| canonical(&event)), Ok(text));
        }
    }
}
