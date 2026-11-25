//! A schema's constructors reuse no Prodrome envelope or term name; the parser reads those first.

use prodrome_data::event::ENVELOPE_SIGNATURES;
use prodrome_data::literal::Vocabulary as _;
use prodrome_data::literal::codec::Constructor;

/// Fails on the first of `constructors` that is an envelope's or a term's.
pub fn assert_own(constructors: &[Constructor]) {
    for (name, _) in constructors {
        assert!(
            prodrome_term::term::schema::signatures()
                .signature(name)
                .is_none(),
            "{name} is a term"
        );
        assert!(
            ENVELOPE_SIGNATURES
                .iter()
                .all(|(envelope, _)| envelope != name),
            "{name} is an envelope"
        );
    }
}
