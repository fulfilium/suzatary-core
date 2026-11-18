//! What one compaction attempt spent: its calls to the model and the bytes it sent.
//!
//! A commutative monoid under addition, so what a window of attempts spent is the sum of
//! theirs whatever order the history holds them in: the compactor's budget is a fold over the
//! digest's own objects, kept nowhere else.

/// Calls made and bytes sent; the unit spends nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Spent {
    /// Calls to the model.
    pub calls: u32,
    /// Bytes sent to it, every call's whole prompt.
    pub sent: u64,
}

impl Spent {
    /// One call that sends `sent` bytes.
    #[must_use]
    pub fn call(sent: u64) -> Self {
        Self { calls: 1, sent }
    }

    /// What both spent. Saturating, which keeps the sum associative over counts that never
    /// reach the bound.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        Self {
            calls: self.calls.saturating_add(other.calls),
            sent: self.sent.saturating_add(other.sent),
        }
    }
}

impl std::iter::Sum for Spent {
    fn sum<I: Iterator<Item = Self>>(spent: I) -> Self {
        spent.fold(Self::default(), Self::join)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn spent() -> impl Strategy<Value = Spent> {
        (any::<u32>(), any::<u64>()).prop_map(|(calls, sent)| Spent { calls, sent })
    }

    proptest! {
        /// A commutative monoid: associative, commutative, the default its unit.
        #[test]
        fn spending_is_a_commutative_monoid(a in spent(), b in spent(), c in spent()) {
            prop_assert_eq!(a.join(b).join(c), a.join(b.join(c)));
            prop_assert_eq!(a.join(b), b.join(a));
            prop_assert_eq!(a.join(Spent::default()), a);
        }
    }
}
