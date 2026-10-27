//! An event schema's constructors are its own: none is one of the Prodrome's envelopes, which the
//! parser looks up before a schema's, or one of its terms, which a schema that holds a price reads
//! beside its own.

use prodrome_core::event::ENVELOPE_SIGNATURES;
use prodrome_core::literal::Vocabulary as _;
use prodrome_core::literal::codec::Constructor;

/// Fails on the first of `constructors` that is an envelope's or a term's.
pub(crate) fn assert_own(constructors: &[Constructor]) {
    for (name, _) in constructors {
        assert!(
            prodrome_core::term::schema::signatures()
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
