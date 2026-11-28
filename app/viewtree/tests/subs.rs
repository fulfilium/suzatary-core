//! The subscription laws, over a program whose model is its own subscriptions and a host kept as
//! data (`Subs::apply`), which takes a frame's patches the way `web/src/pages/subscribed.ts` does:
//!
//! 1. The listeners a host keeps by applying each frame equal the model's subscriptions.
//! 2. A step that moves no subscription patches nothing.
//! 3. A source that is equal is never started again: a slot's starts are the steps its source moved.
//! 4. A diff is the stops in slot order, then the starts in slot order, and applied it equals the
//!    subscriptions it was asked for.

use proptest::prelude::*;
use viewtree::node::text;
use viewtree::{Node, Program, Runtime, SubPatch, Subs};

type Listeners = Subs<u8, u8>;

#[derive(Debug, Clone)]
enum Msg {
    Listen(u8, u8),
    Stop(u8),
    Idle,
}

struct Listening;

impl Program for Listening {
    type Model = Listeners;
    type Msg = Msg;
    type Effect = ();
    type Flags = Listeners;
    type Slot = u8;
    type Sub = u8;

    fn init(flags: Listeners) -> (Listeners, Vec<()>) {
        (flags, Vec::new())
    }

    fn update(model: &mut Listeners, msg: Msg) -> Vec<()> {
        match msg {
            Msg::Listen(slot, source) => *model = std::mem::take(model).with(slot, source),
            Msg::Stop(slot) => {
                *model = model
                    .iter()
                    .filter(|(k, _)| **k != slot)
                    .map(|(k, v)| (*k, *v))
                    .collect();
            }
            Msg::Idle => {}
        }
        Vec::new()
    }

    fn view(model: &Listeners) -> Node<Msg> {
        text(format!("{}", model.iter().count()))
    }

    fn subscriptions(model: &Listeners) -> Listeners {
        model.clone()
    }
}

fn listeners() -> impl Strategy<Value = Listeners> {
    proptest::collection::vec((0u8..5, 0u8..3), 0..6).prop_map(|slots| slots.into_iter().collect())
}

fn msgs() -> impl Strategy<Value = Vec<Msg>> {
    proptest::collection::vec(
        prop_oneof![
            (0u8..5, 0u8..3).prop_map(|(k, v)| Msg::Listen(k, v)),
            (0u8..5).prop_map(Msg::Stop),
            Just(Msg::Idle),
        ],
        0..24,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn the_active_listeners_are_the_subscriptions(first in listeners(), run in msgs()) {
        let (mut rt, frame) = Runtime::<Listening>::start(first);
        let mut host = Listeners::new();
        host.apply(&frame.subs);
        prop_assert_eq!(&host, rt.active());
        for msg in run {
            host.apply(&rt.send(msg).subs);
            prop_assert_eq!(&host, &Listening::subscriptions(rt.model()));
            prop_assert_eq!(&host, rt.active());
        }
    }

    #[test]
    fn a_step_that_moves_no_subscription_patches_nothing(first in listeners(), run in msgs()) {
        let (mut rt, _) = Runtime::<Listening>::start(first);
        for msg in run {
            let before = rt.active().clone();
            let frame = rt.send(msg);
            prop_assert_eq!(frame.subs.is_empty(), &before == rt.active());
        }
    }

    #[test]
    fn an_unchanged_source_is_never_restarted(first in listeners(), run in msgs()) {
        let (mut rt, frame) = Runtime::<Listening>::start(first);
        let mut starts = [0usize; 5];
        let mut moved = [0usize; 5];
        let mut count = |before: &Listeners, now: &Listeners, patches: &[SubPatch<u8, u8>]| {
            for slot in 0u8..5 {
                let i = usize::from(slot);
                moved[i] += usize::from(now.get(&slot).is_some() && now.get(&slot) != before.get(&slot));
                starts[i] += patches.iter().filter(|p| matches!(p, SubPatch::Start { slot: s, .. } if *s == slot)).count();
            }
        };
        count(&Listeners::new(), rt.active(), &frame.subs);
        for msg in run {
            let before = rt.active().clone();
            let frame = rt.send(msg);
            count(&before, rt.active(), &frame.subs);
        }
        prop_assert_eq!(starts, moved);
    }

    #[test]
    fn a_diff_is_stops_then_starts_in_slot_order_and_reaches_the_next(a in listeners(), b in listeners()) {
        let patches = a.diff(&b);
        let mut reached = a.clone();
        reached.apply(&patches);
        prop_assert_eq!(&reached, &b);
        let stopped = patches.iter().take_while(|p| matches!(p, SubPatch::Stop { .. })).count();
        let starts = patches[stopped..].iter().all(|p| matches!(p, SubPatch::Start { .. }));
        prop_assert!(starts);
        let slot = |p: &SubPatch<u8, u8>| match p {
            SubPatch::Start { slot, .. } | SubPatch::Stop { slot } => *slot,
        };
        for run in [&patches[..stopped], &patches[stopped..]] {
            prop_assert!(run.windows(2).all(|w| slot(&w[0]) < slot(&w[1])));
        }
        prop_assert_eq!(a.diff(&a), Vec::new());
    }
}
