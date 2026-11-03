use suzatary_core::wire::{Declarations, exported};
use ts_rs::TypeVisitor;
use viewtree::Runtime;

use suzatary_pages::runtime::{Answer, Context, Effect, Gate, Input};
use suzatary_pages::settings::{Act, Flags, Msg, Settings};

/// `web/src/wire/generated/pages.ts`: the runtime's effects and inputs, and what each page that
/// speaks them starts from and asks, declared once, here.
#[test]
fn generates_the_pages_boundary_types() {
    use suzatary_pages::{mail, message, runtime, settings, thread};
    let mut d = Declarations::default();
    d.visit::<runtime::Effect<runtime::Never, runtime::Never>>();
    d.visit::<runtime::Input<runtime::Never, runtime::Never>>();
    d.visit::<mail::Flags>();
    d.visit::<mail::Hold>();
    d.visit::<mail::Ask>();
    d.visit::<settings::Flags>();
    d.visit::<settings::Hold>();
    d.visit::<settings::Act>();
    d.visit::<settings::Device>();
    d.visit::<settings::Card>();
    d.visit::<message::At>();
    d.visit::<thread::At>();
    d.visit::<suzatary_pages::reading::Nothing>();
    d.visit::<suzatary_pages::calendar::At>();
    d.visit::<suzatary_pages::briefing::At>();
    let mut out = String::from(
        "// GENERATED FROM app/pages/src BY `cargo test -p suzatary-pages`. DO NOT EDIT.\n",
    );
    exported(&mut out, d.out);
    if let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") {
        std::fs::create_dir_all(&dir).expect("the export directory is writable");
        std::fs::write(std::path::Path::new(&dir).join("pages.ts"), out)
            .expect("pages.ts is writable");
    }
}

/// A double tap is two writes unless the act is running: the second tap of the same act asks
/// nothing until the first is answered, and its button is disabled meanwhile.
#[test]
fn an_act_runs_once_until_it_is_answered() {
    let (mut rt, _) = Runtime::<Settings>::start(Flags {
        context: Context {
            gate: Gate::Writes,
            offsets: Vec::new(),
        },
    });
    let pair = || Act::Pair {
        code: "BCDF-GHJK".to_owned(),
    };
    let (_, first) = rt.step(Msg::Run(pair()));
    let (_, second) = rt.step(Msg::Run(pair()));
    assert!(
        matches!(&first[..], [Effect::Post { ask, path, .. }] if *ask == pair() && path == "/api/act/pair"),
        "{first:?}"
    );
    assert_eq!(second, Vec::new());
    let answered = Input::Answered {
        ask: pair(),
        answer: Answer::Refused {
            status: 403,
            words: "That code is used.".to_owned(),
        },
    };
    let (_, said) = rt.step(Msg::Host(Box::new(answered)));
    assert_eq!(
        said,
        vec![Effect::Toast {
            words: "That code is used.".to_owned()
        }]
    );
    let (_, again) = rt.step(Msg::Run(pair()));
    assert_eq!(again.len(), 1, "a refused act may be tapped again");
}
