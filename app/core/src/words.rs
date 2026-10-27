//! A type with a finite set of values, each printed as a word: its literal and its serde form are
//! both read from one list of its values, so the two cannot spell a value differently.

/// `impl Literal`, `Serialize` and `Deserialize` for each type, printed by `$print` and read back
/// as the value of `$all` that prints as the text: one list of words for the stored form and the
/// wire.
macro_rules! words {
    ($($ty:ty: $all:expr => $print:expr),+ $(,)?) => {$(
        prodrome_core::word_literal!($ty: $all => $print);

        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let print: fn(&$ty) -> &str = $print;
                serializer.serialize_str(print(self))
            }
        }

        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <String as serde::Deserialize>::deserialize(deserializer)?;
                prodrome_core::literal::codec::word(
                    &prodrome_core::literal::Value::str(text),
                    $all,
                    $print,
                )
                .map_err(serde::de::Error::custom)
            }
        }
    )+};
}
pub(crate) use words;

/// A type whose serde form a derive spells (its `rename_all`, which a generated wire type reads)
/// prints every value of `all` as its literal's word, and reads each word back.
#[cfg(test)]
pub(crate) fn assert_serde_words<T>(all: &[T], print: fn(&T) -> &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    for value in all {
        let word = print(value);
        assert_eq!(
            serde_json::to_value(value).expect("a word serialises"),
            serde_json::Value::String(word.to_owned()),
            "{value:?}'s serde word is its literal's"
        );
        assert_eq!(
            &serde_json::from_value::<T>(serde_json::Value::String(word.to_owned()))
                .expect("its word reads back"),
            value
        );
    }
}
