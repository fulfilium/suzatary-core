//! A type with a finite set of values, each printed as a word: its literal and its serde form are
//! both read from one list of its values, so the two cannot spell a value differently.

/// `impl Literal`, `Serialize` and `Deserialize` for each type, all read from `$all` and `$print`.
#[macro_export]
macro_rules! words {
    ($($ty:ty: $all:expr => $print:expr),+ $(,)?) => {$(
        $crate::words::expansion::word_literal!($ty: $all => $print);

        impl $crate::words::expansion::serde::Serialize for $ty {
            fn serialize<S: $crate::words::expansion::serde::Serializer>(
                &self,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                let print: fn(&$ty) -> &str = $print;
                serializer.serialize_str(print(self))
            }
        }

        impl<'de> $crate::words::expansion::serde::Deserialize<'de> for $ty {
            fn deserialize<D: $crate::words::expansion::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                let text =
                    <String as $crate::words::expansion::serde::Deserialize>::deserialize(deserializer)?;
                $crate::words::expansion::word(
                    &$crate::words::expansion::Value::str(text),
                    $all,
                    $print,
                )
                .map_err($crate::words::expansion::serde::de::Error::custom)
            }
        }
    )+};
}
pub use words;

/// What [`words!`] expands to name, so its caller needs no crate of its own for it.
#[doc(hidden)]
pub mod expansion {
    pub use prodrome_data::literal::{Value, codec::word};
    pub use prodrome_data::word_literal;
    pub use serde;
}

/// The value of `all` that `print` spells as `text`.
pub fn of_word<T: Copy, const N: usize>(
    all: [T; N],
    print: impl Fn(T) -> &'static str,
    text: &str,
) -> Option<T> {
    all.into_iter().find(|value| print(*value) == text)
}

/// A type whose serde form a derive spells (its `rename_all`, which a generated wire type reads)
/// prints every value of `all` as its literal's word, and reads each word back.
#[cfg(any(test, feature = "nothing"))]
pub fn assert_serde_words<T>(all: &[T], print: fn(&T) -> &str)
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
