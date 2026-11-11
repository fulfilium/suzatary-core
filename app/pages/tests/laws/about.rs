use crate::common::*;

fn held_todo() -> impl Strategy<Value = Option<HeldTodo>> {
    let value = prop_oneof![
        Just(None),
        (0u8..=10).prop_map(|n| Some(Fulfillment::Number(f64::from(n) / 10.0))),
        Just(Some(Fulfillment::Absent(
            suzatary_core::wire::todo::Absent::Absent
        ))),
    ];
    proptest::option::of((value, words()).prop_map(|(value, title)| HeldTodo {
        row: Neighbour {
            todo: "t".to_owned(),
            state: EntryState::Open,
            value,
            unconfirmed: false,
        },
        title,
    }))
}

fn lot() -> impl Strategy<Value = Lot> {
    prop_oneof![Just(Lot::Open), Just(Lot::Cancelled), Just(Lot::Completed)]
}

fn releases_payload() -> impl Strategy<Value = Value> {
    let item =
        (0u8..4, held_todo(), lot(), any::<bool>()).prop_map(|(n, held, lot, now)| ReleaseItem {
            todo: format!("i{n}"),
            held,
            lot,
            named: if now { Named::Current } else { Named::Dropped },
        });
    let place = prop_oneof![Just(Place::Earlier), Just(Place::Next), Just(Place::Later)];
    let release = (
        0u8..3,
        held_todo(),
        lot(),
        place,
        proptest::collection::vec(item, 0..4),
    )
        .prop_map(|(n, held, lot, place, items)| ReleaseRow {
            todo: format!("r{n}"),
            held,
            version: "2.3".to_owned(),
            lot,
            place,
            items,
        });
    proptest::collection::vec(release, 0..3).prop_map(|releases| {
        printed(&ReleasesPayload {
            chain: "suzatary".to_owned(),
            reads: "2.2".to_owned(),
            releases,
        })
    })
}

fn about_payload() -> impl Strategy<Value = Value> {
    prop_oneof![Just("dev"), Just("abc1234")].prop_map(|build| {
        printed(&AboutPayload {
            version: "2.1.59".to_owned(),
            build: build.to_owned(),
            completes: Vec::new(),
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn about_laws(msgs in proptest::collection::vec(prop_oneof![
        held::<about::Hold, about::Ask, _>(about::Hold::About, about_payload()).prop_map(|i| about::Msg::Host(Box::new(i))),
        held::<about::Hold, about::Ask, _>(about::Hold::Releases, releases_payload()).prop_map(|i| about::Msg::Host(Box::new(i))),
        Just(about::Msg::Retry),
    ], 0..8)) {
        page_laws::<about::About>(|| opened(true, Nothing {}), &msgs);
    }
}
