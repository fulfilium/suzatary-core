//! A list that holds at least one item, so "none" is spelled once, as the `None` around it.

/// At least one `T`, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonEmpty<T>(Vec<T>);

impl<T> NonEmpty<T> {
    /// `first`, then `rest`.
    #[must_use]
    pub fn new(first: T, rest: Vec<T>) -> Self {
        let mut items = vec![first];
        items.extend(rest);
        Self(items)
    }

    /// The first item.
    #[must_use]
    pub fn first(&self) -> &T {
        &self.0[0]
    }

    /// Each item through `f`, in order.
    #[must_use]
    pub fn map<U>(&self, f: impl FnMut(&T) -> U) -> NonEmpty<U> {
        NonEmpty(self.0.iter().map(f).collect())
    }

    /// `items`, or `None` when there are none.
    #[must_use]
    pub fn of(items: Vec<T>) -> Option<Self> {
        (!items.is_empty()).then_some(Self(items))
    }

    /// Every item, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// The items of `held`, none for `None`.
    #[must_use]
    pub fn listed(held: Option<&Self>) -> &[T] {
        held.map_or(&[], Self::as_slice)
    }
}

/// A list on the wire, as its items.
impl<T: serde::Serialize> serde::Serialize for NonEmpty<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A list on the wire, refused when it is empty.
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for NonEmpty<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::of(Vec::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::invalid_length(0, &"at least one item"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_list_on_the_wire_is_refused() {
        assert!(serde_json::from_str::<NonEmpty<u8>>("[]").is_err());
        assert_eq!(
            serde_json::from_str::<NonEmpty<u8>>("[3]")
                .expect("one item")
                .as_slice(),
            [3]
        );
    }
}
