use suzatary_core::wire::{Declarations, exported};
use ts_rs::TypeVisitor;
use viewtree::Runtime;

use suzatary_pages::settings::{Act, Effect, Input, Msg, Outcome, Settings, Shown};

/// `web/src/wire/generated/settings.ts`, written where `TS_RS_EXPORT_DIR` says: what the host
/// hands the page and what the page asks of it, declared once, here.
#[test]
fn generates_the_settings_boundary_types() {
    let mut d = Declarations::default();
    d.visit::<Input>();
    d.visit::<Effect>();
    let mut out = String::from(
        "// GENERATED FROM app/pages/src/settings.rs BY `cargo test -p suzatary-pages`. DO NOT EDIT.\n",
    );
    exported(&mut out, d.out);
    if let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") {
        std::fs::create_dir_all(&dir).expect("the export directory is writable");
        std::fs::write(std::path::Path::new(&dir).join("settings.ts"), out)
            .expect("settings.ts is writable");
    }
}

/// `web/src/wire/generated/pages.ts`: the runtime's effects and inputs, and what each page that
/// speaks them starts from and asks, declared once, here.
#[test]
fn generates_the_pages_boundary_types() {
    use suzatary_pages::{mail, message, runtime, thread};
    let mut d = Declarations::default();
    d.visit::<runtime::Effect<runtime::Never, runtime::Never>>();
    d.visit::<runtime::Input<runtime::Never, runtime::Never>>();
    d.visit::<mail::Flags>();
    d.visit::<mail::Hold>();
    d.visit::<mail::Ask>();
    d.visit::<message::Flags>();
    d.visit::<thread::Flags>();
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

fn shown() -> Shown {
    serde_json::from_value(serde_json::json!({
        "overview": { "groups": [], "ask": "default", "offers": [] },
        "rows": { "type": "Val", "v": [] },
        "gate": "writes",
        "ask": "#/secretary/default?focus=1",
        "device": {
            "persist": { "type": "Granted" },
            "usage": { "bytes": 0.0, "count": 0 },
            "budget": 1.0,
            "signing": { "type": "Keyed", "key": "fixture-device", "standing": "untrusted" }
        },
        "offsets": [{ "from": 0, "minutes": 0 }]
    }))
    .expect("a reading")
}

/// A double tap is two writes unless the act is running: the second tap of the same act asks
/// nothing until the first is answered, and its button is disabled meanwhile.
#[test]
fn an_act_runs_once_until_it_is_answered() {
    let (mut rt, _) = Runtime::<Settings>::start(shown());
    let pair = || {
        Msg::Run(Act::Pair {
            code: "BCDF-GHJK".to_owned(),
        })
    };
    let (_, first) = rt.step(pair());
    let (_, second) = rt.step(pair());
    assert_eq!(
        first,
        vec![Effect::Run {
            act: Act::Pair {
                code: "BCDF-GHJK".to_owned()
            }
        }]
    );
    assert_eq!(second, Vec::new());
    let answered = Input::Answered {
        act: Act::Pair {
            code: "BCDF-GHJK".to_owned(),
        },
        outcome: Outcome::Refused {
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
    let (_, again) = rt.step(pair());
    assert_eq!(again.len(), 1, "a refused act may be tapped again");
}
