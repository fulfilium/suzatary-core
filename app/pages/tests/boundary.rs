use suzatary_core::wire::{Declarations, exported};
use ts_rs::TypeVisitor;
use viewtree::Runtime;

use suzatary_pages::runtime::{Answer, Context, Effect, Gate, Input, Reach};
use suzatary_pages::settings::{Act, Ask, Flags, Msg, Settings};

/// `web/src/wire/generated/pages.ts` is declared here, once.
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
    d.visit::<suzatary_pages::inbox::Flags>();
    d.visit::<suzatary_pages::inbox::Hold>();
    d.visit::<suzatary_pages::card::Asking>();
    d.visit::<settings::Ask>();
    d.visit::<runtime::Outcome>();
    d.visit::<suzatary_pages::intents::Intents>();
    d.visit::<suzatary_pages::waiting::InboxRead>();
    d.visit::<suzatary_pages::secretary::Flags>();
    d.visit::<suzatary_pages::secretary::Hold>();
    d.visit::<suzatary_pages::secretary::Ask>();
    d.visit::<suzatary_pages::secretary::StreamRead>();
    d.visit::<suzatary_pages::conversation::Flags>();
    d.visit::<suzatary_pages::conversation::Hold>();
    d.visit::<suzatary_pages::conversation::Ask>();
    d.visit::<suzatary_pages::conversation::Stream>();
    d.visit::<suzatary_pages::conversation::Windowed>();
    d.visit::<suzatary_pages::composer::Read>();
    d.visit::<suzatary_pages::draft::Flags>();
    d.visit::<suzatary_pages::draft::Hold>();
    d.visit::<suzatary_pages::draft::Ask>();
    d.visit::<suzatary_pages::needsyou::Flags>();
    d.visit::<suzatary_pages::needsyou::Hold>();
    d.visit::<suzatary_pages::needsyou::Route>();
    d.visit::<runtime::Scrolled>();
    d.visit::<runtime::LocalAnswers>();
    d.visit::<message::At>();
    d.visit::<thread::At>();
    d.visit::<suzatary_pages::reading::Nothing>();
    d.visit::<suzatary_pages::calendar::At>();
    d.visit::<suzatary_pages::briefing::At>();
    d.visit::<suzatary_pages::todo::Flags>();
    d.visit::<suzatary_pages::todos::Flags>();
    d.visit::<runtime::Keyed>();
    d.visit::<suzatary_pages::typstlist::Flags>();
    let mut out = String::from(
        "// GENERATED FROM app/pages/src BY `cargo test -p suzatary-pages`. DO NOT EDIT.\n",
    );
    out.push_str("\nexport ");
    out.push_str(suzatary_core::wire::SPEC_NODE);
    out.push('\n');
    exported(&mut out, d.out);
    out.push_str(&viewtree::patch::typescript());
    if let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") {
        std::fs::create_dir_all(&dir).expect("the export directory is writable");
        std::fs::write(std::path::Path::new(&dir).join("pages.ts"), out)
            .expect("pages.ts is writable");
    }
}

/// The second tap of a running act asks nothing, and its button is disabled meanwhile.
#[test]
fn an_act_runs_once_until_it_is_answered() {
    let (mut rt, _) = Runtime::<Settings>::start(Flags {
        context: Context {
            gate: Gate::Writes,
            reach: Reach::Writes,
            offsets: Vec::new(),
            wide: false,
            categories: vec!["home".to_owned(), "work".to_owned()],
        },
    });
    let pair = || Act::Pair {
        code: "BCDF-GHJK".to_owned(),
    };
    let (_, first) = rt.step(Msg::Run(pair()));
    let (_, second) = rt.step(Msg::Run(pair()));
    assert!(
        matches!(&first[..], [Effect::Post { ask: Ask::Run(ask), path, .. }] if *ask == pair() && path == "/api/act/pair"),
        "{first:?}"
    );
    assert_eq!(second, Vec::new());
    let answered = Input::Answered {
        ask: Ask::Run(pair()),
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
