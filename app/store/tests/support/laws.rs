//! The laws an order and a join state, written once (the Prodrome's
//! `docs/design-register-types.md` §7, law 2): each schema's tests call them over values its own
//! histories reach, so a law is stated in one place and checked for every order the app defines.

use std::fmt::Debug;

use prodrome_core::fold::Order;
use proptest::prelude::*;

/// `≤` is reflexive, antisymmetric and transitive over every triple of `values`.
///
/// # Errors
/// The first law that fails, at the values it fails at.
pub fn partial_order<V: Order + PartialEq + Debug>(values: &[V]) -> Result<(), TestCaseError> {
    for a in values {
        prop_assert!(a.le(a), "reflexive at {:?}", a);
        for b in values {
            prop_assert!(
                !(a.le(b) && b.le(a)) || a == b,
                "antisymmetric at {:?} and {:?}",
                a,
                b
            );
            for c in values {
                prop_assert!(
                    !(a.le(b) && b.le(c)) || a.le(c),
                    "transitive at {:?} ≤ {:?} ≤ {:?}",
                    a,
                    b,
                    c
                );
            }
        }
    }
    Ok(())
}

/// `join` is the least upper bound of what each reading holds, over every pair and triple of
/// `readings`: what `a ⊔ b` holds is above what each holds and below every upper bound among
/// them, and the join is idempotent, commutative and associative in what it holds.
///
/// # Errors
/// The first law that fails, at the readings it fails at.
pub fn join<T: Clone + Debug, V: Order + PartialEq + Debug>(
    readings: &[T],
    held: impl Fn(&T) -> V,
    join: impl Fn(T, T) -> T,
) -> Result<(), TestCaseError> {
    let joined = |a: &T, b: &T| join(a.clone(), b.clone());
    for a in readings {
        prop_assert_eq!(held(&joined(a, a)), held(a), "idempotent at {:?}", a);
        for b in readings {
            let ab = joined(a, b);
            prop_assert!(
                held(a).le(&held(&ab)) && held(b).le(&held(&ab)),
                "an upper bound of {:?} and {:?}",
                a,
                b
            );
            prop_assert_eq!(held(&ab), held(&joined(b, a)), "commutative");
            for c in readings {
                if held(a).le(&held(c)) && held(b).le(&held(c)) {
                    prop_assert!(
                        held(&ab).le(&held(c)),
                        "the least upper bound, below {:?}",
                        c
                    );
                }
                prop_assert_eq!(
                    held(&joined(&ab, c)),
                    held(&joined(a, &joined(b, c))),
                    "associative"
                );
            }
        }
    }
    Ok(())
}
