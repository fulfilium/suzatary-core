//! A changelog fragment (`suzatary_core::release::Fragment`) prints as what it parses from, and
//! folding fragments is a monoid, so a release's section is a function of the set of fragments.

use proptest::prelude::*;
use suzatary_core::release::{Fragment, FragmentError, Kind};

/// A fragment's text: kinds in any order, each over entries of a first line and continuations.
fn text() -> impl Strategy<Value = String> {
    let entry = (
        "[a-z][a-z `*.]{0,24}",
        prop::collection::vec("[a-z][a-z .]{0,24}", 0..3),
    )
        .prop_map(|(first, more)| {
            std::iter::once(format!("- {first}"))
                .chain(more.into_iter().map(|line| format!("  {line}")))
                .collect::<Vec<_>>()
                .join("\n")
        });
    let kind = prop::sample::select(Kind::ALL.to_vec());
    prop::collection::vec((kind, prop::collection::vec(entry, 1..4)), 1..5).prop_map(|kinds| {
        kinds
            .into_iter()
            .map(|(kind, entries)| format!("### {}\n{}\n", kind.word(), entries.join("\n")))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn fragment() -> impl Strategy<Value = Fragment> {
    text().prop_map(|t| Fragment::parse(&t).expect("a generated fragment parses"))
}

proptest! {
    #[test]
    fn a_printed_fragment_parses_as_itself(f in fragment()) {
        prop_assert_eq!(Fragment::parse(&f.print()), Ok(f));
    }

    #[test]
    fn folding_is_associative_with_the_empty_fragment_its_unit(
        a in fragment(), b in fragment(), c in fragment(),
    ) {
        prop_assert_eq!(a.clone().append(b.clone()).append(c.clone()), a.clone().append(b.append(c)));
        prop_assert_eq!(Fragment::default().append(a.clone()), a.clone());
        prop_assert_eq!(a.clone().append(Fragment::default()), a);
    }

    #[test]
    fn a_fold_prints_each_kind_once_in_a_sections_order(fs in prop::collection::vec(fragment(), 1..5)) {
        let printed = fs.into_iter().fold(Fragment::default(), Fragment::append).print();
        let headings: Vec<&str> = printed.lines().filter(|l| l.starts_with("### ")).collect();
        let order: Vec<String> = Kind::ALL.iter().map(|k| format!("### {}", k.word())).collect();
        let ranks: Vec<usize> = headings
            .iter()
            .map(|h| order.iter().position(|o| o == h).expect("a kind"))
            .collect();
        prop_assert!(ranks.windows(2).all(|w| w[0] < w[1]), "{}", printed);
    }
}

#[test]
fn a_fragment_holds_kinded_entries_and_nothing_else() {
    for (text, refused) in [
        ("- no heading\n", FragmentError::Unheaded(1)),
        (
            "### Improved\n- a word\n",
            FragmentError::UnknownKind(1, "Improved".to_owned()),
        ),
        ("## [Unreleased]\n", FragmentError::Heading(1)),
        (
            "### Added\n\n### Fixed\n- a fix\n",
            FragmentError::EmptyKind(1, "Added".to_owned()),
        ),
        (
            "### Fixed\nprose before the entry\n",
            FragmentError::Unheaded(2),
        ),
        ("\n\n", FragmentError::Empty),
    ] {
        assert_eq!(Fragment::parse(text), Err(refused), "{text:?}");
    }
}
