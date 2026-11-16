//! `suzatary prodrome carry`, a migration (AGENTS.md §8): over a fixture upkeep chain with open
//! todos, completed releases and the open release (with a completed and a cancelled item), and a
//! scratch carried list, the dry run's verdicts are what `--apply` does; the destination verifies,
//! holds no signature or key object, shows every carried todo, reads the same version and prices
//! the open release the same; every carried source is cancelled naming its successor; a refused
//! carry, one whose list a completion has made stale, or one whose edited list would write a
//! record the destination holds differently, leaves both stores as they were; and a second
//! `--apply` writes nothing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use prodrome_core::event::{Envelope, TodoEvent};
use prodrome_core::fpl;
use prodrome_core::literal::Datetime;
use prodrome_core::store::EventStore;
use suzatary_record::record::{MarkupSource, mk_authored};

use crate::todos::{scratch, signing};

const SEED: &str = "7777777777777777777777777777777777777777777777777777777777777777";

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 10, day, 9, 0, 0, 0).expect("an instant")
}

/// A todo's record on the upkeep chain, priced by `spec`.
fn record(
    id: &str,
    day: u32,
    body: &str,
    spec: prodrome_core::term::Term,
    detail: &str,
) -> TodoEvent<suzatary_record::record::Record> {
    mk_authored(
        id,
        at(day),
        "assistant",
        "todo",
        at(day),
        MarkupSource::escaped(body),
        spec,
        vec!["Work on the project itself.".to_owned()],
        "home",
        "",
        MarkupSource::escaped(detail),
        None,
        Vec::new(),
        Vec::new(),
        "",
    )
    .expect("a record")
}

fn refs(ids: &[&str]) -> prodrome_core::term::Term {
    let members = ids
        .iter()
        .map(|id| fpl::mk_ref((*id).to_owned()).expect("a reference"))
        .collect();
    fpl::mk_conj(members, -4.0).expect("a conjunction")
}

/// A box whose upkeep chain is [`an_upkeep_chain`], and the destination begun public beside it,
/// empty.
fn a_box(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = scratch(name);
    an_upkeep_chain(&root);
    std::fs::create_dir_all(root.join("events")).expect("the life chain");
    crate::todos::a_chain(&root);
    std::fs::create_dir_all(root.join("local")).expect("the box-local tier");
    std::fs::write(root.join("local/personal-terms.txt"), "zzperson\n").expect("terms");
    std::fs::write(root.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    std::fs::create_dir_all(root.join("scripts")).expect("scripts");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/secret-rules.toml"),
        root.join("scripts/secret-rules.toml"),
    )
    .expect("the secret rules");
    let instance = an_instance(&root);
    let destination = root.join("roadmap");
    let begun = run(
        &instance,
        &[
            "prodrome",
            "init",
            "--public",
            "--store",
            &destination.to_string_lossy(),
        ],
    );
    assert!(
        begun.status.success(),
        "{}",
        String::from_utf8_lossy(&begun.stderr)
    );
    (root, instance, destination)
}

/// Two completed releases (the first naming a completed item), the open release naming two open
/// todos, a completed item, a cancelled one and the second completed release, and four more open
/// todos.
fn an_upkeep_chain(root: &Path) {
    let flat = |value: f64| fpl::mk_flat(value).expect("a term");
    let store = EventStore::new(
        root.join("events-suzatary"),
        prodrome_core::policy::Untrusted::none(),
    );
    let append = |event| {
        suzatary_store::store::append_to(&store, suzatary_record::record::GENESIS, event)
            .expect("appends");
    };
    append(record(
        "shippeditem",
        1,
        "Ship the first item",
        flat(0.6),
        "",
    ));
    append(
        prodrome_core::event::mk_completed("shippeditem", at(2), "assistant", "").expect("done"),
    );
    append(record(
        "suzatarytwo",
        1,
        "Release two",
        refs(&["shippeditem"]),
        "",
    ));
    append(
        prodrome_core::event::mk_completed("suzatarytwo", at(3), "assistant", "").expect("done"),
    );
    append(record(
        "suzatarytwoone",
        4,
        "Release two point one",
        flat(0.6),
        "",
    ));
    append(
        prodrome_core::event::mk_completed("suzatarytwoone", at(5), "assistant", "").expect("done"),
    );
    append(record(
        "coldstart",
        6,
        "A cold start reads every object",
        flat(0.55),
        "",
    ));
    append(record("zzperson", 6, "Ask about the notes", flat(0.7), ""));
    append(record("itemdone", 6, "Ship the second item", flat(0.6), ""));
    append(
        prodrome_core::event::mk_completed("itemdone", at(7), "assistant", "landed").expect("done"),
    );
    append(record(
        "itemdropped",
        6,
        "Drop the third item",
        flat(0.6),
        "",
    ));
    append(
        prodrome_core::event::mk_cancelled("itemdropped", at(7), "assistant", "moved out")
            .expect("dropped"),
    );
    append(record(
        "suzatarytwotwo",
        7,
        "Release two point two",
        refs(&[
            "coldstart",
            "zzperson",
            "itemdone",
            "itemdropped",
            "suzatarytwoone",
        ]),
        "",
    ));
    append(record("boxdisk", 8, "Watch the disk", flat(0.5), ""));
    append(record(
        "foldrun",
        8,
        "Keep the fold's reading",
        flat(0.6),
        "Measured at commit 9c1e4f0b7a2d6e8f13579bdf2468ace013579bdf.",
    ));
    append(record("restated", 9, "Old words", flat(0.65), "Old detail"));
}

/// The box's instance file: `life`, and the upkeep chain `suzatary` the release plan names.
fn an_instance(root: &Path) -> PathBuf {
    let key = prodrome_core::sign::Secret::new(SEED).expect("a seed");
    let instance = crate::todos::a_signing_instance(root, &key.public());
    let mut text = std::fs::read_to_string(&instance).expect("the instance");
    text.push_str(
        "\n[[chains]]\nname = \"suzatary\"\nevents = \"events-suzatary\"\n\
         untrusted = [\"triage\"]\nactors = [\"assistant\", \"ada\", \"triage\"]\n\
         owner = \"ada\"\n\n[upkeep]\nchain = \"suzatary\"\n",
    );
    std::fs::write(&instance, text).expect("the instance");
    instance
}

/// Both writers' public names: `assistant` as itself, and the owner `ada`, who restates, as
/// `owner`.
const ACTORS: &str = "assistant = \"assistant\"\nada = \"owner\"";

/// The carried list that carries every open todo but `boxdisk`, renames `zzperson`, whose id
/// names a listed term, and restates `restated`; `boxdisk`'s table as `boxdisk` says.
fn a_list(root: &Path, actors: &str, boxdisk: &str) -> PathBuf {
    let path = root.join("scratch-list.toml");
    std::fs::write(
        &path,
        format!(
            r#"
[actors]
{actors}

[todos.coldstart]
carry = "Any store starts slowly."

[todos.zzperson]
carry = "The notes are the project's."
as = "notesask"
body = "Ask about the notes"

[todos.suzatarytwotwo]
carry = "The open release."

[todos.foldrun]
carry = "The fold is the project's."

[todos.restated]
carry = "Said again for the public."
body = "New words"
detail = "New detail"
waiting_on = "the upstream fix"

[todos.itemdropped]
carry = "A dropped item of the open release."
as = "itemgone"
body = "Drop the third item, said again"

[todos.boxdisk]
{boxdisk}
"#
        ),
    )
    .expect("a list");
    path
}

fn run(instance: &Path, args: &[&str]) -> std::process::Output {
    signing(
        instance,
        args,
        &[
            (suzatary_signatures::signatures::BOX_KEY, SEED.to_owned()),
            (suzatary_signatures::signatures::AGENT_KEY, "8".repeat(64)),
        ],
    )
}

fn carry(instance: &Path, destination: &Path, list: &Path, apply: bool) -> std::process::Output {
    let (to, list) = (destination.to_string_lossy(), list.to_string_lossy());
    let mut args = vec!["prodrome", "carry", "--to", &to, "--list", &list];
    if apply {
        args.push("--apply");
    }
    run(instance, &args)
}

fn held(store: &Path) -> BTreeSet<String> {
    std::fs::read_dir(store.join("objects"))
        .expect("the objects")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn envelopes(store: &Path) -> Vec<Envelope<TodoEvent<suzatary_record::record::Record>>> {
    use suzatary_store::store::Reads as _;
    suzatary_io::objects::Objects::chain(EventStore::new(
        store,
        prodrome_core::policy::Untrusted::none(),
    ))
    .objects()
    .expect("the store reads")
    .into_iter()
    .map(|(_, object)| object)
    .collect()
}

fn named_objects(
    store: &Path,
) -> Vec<(String, Envelope<TodoEvent<suzatary_record::record::Record>>)> {
    use suzatary_store::store::Reads as _;
    suzatary_io::objects::Objects::chain(EventStore::new(
        store,
        prodrome_core::policy::Untrusted::none(),
    ))
    .objects()
    .expect("the store reads")
    .into_iter()
    .map(|(name, object)| (name.as_str().to_owned(), object))
    .collect()
}

fn events(store: &Path) -> Vec<TodoEvent<suzatary_record::record::Record>> {
    envelopes(store)
        .into_iter()
        .filter_map(|envelope| envelope.event().cloned())
        .collect()
}

/// The open release's price as `todos open --json` prints it, reading the chain or store `at`.
fn price(instance: &Path, at: &[&str]) -> String {
    let mut args = vec!["todos"];
    args.extend_from_slice(at);
    args.extend(["open", "--json"]);
    let out = run(instance, &args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    rows["open"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["id"] == "suzatarytwotwo")
        .map(|row| row["priority"].to_string())
        .expect("the open release is open")
}

/// The lots of the open release's items, by the release reading over `events`.
fn lots(events: &[TodoEvent<suzatary_record::record::Record>]) -> Vec<suzatary_core::release::Lot> {
    let plan = suzatary_core::release::Plan::declared().expect("the plan");
    let reading = suzatary_core::release::Reading::of(
        &plan,
        events,
        at(28),
        &std::collections::BTreeMap::new(),
    );
    let mut lots: Vec<_> = reading
        .releases
        .into_iter()
        .find(|release| release.todo.as_str() == "suzatarytwotwo")
        .expect("the open release")
        .items
        .into_iter()
        .map(|item| item.lot)
        .collect();
    lots.sort_unstable();
    lots
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The migration's law, end to end.
#[test]
#[allow(clippy::too_many_lines)]
fn the_carry_does_what_its_dry_run_says_and_once() {
    let (root, instance, destination) = a_box("carry");
    let source = root.join("events-suzatary");
    let before = (held(&source), held(&destination));

    // A refused carry: door 1 refuses boxdisk's restated body, so nothing moves.
    let refusing = a_list(
        &root,
        ACTORS,
        "carry = \"Refused.\"\nbody = \"Ask zzcontext about the disk\"",
    );
    let dry = carry(&instance, &destination, &refusing, false);
    assert!(!dry.status.success(), "a refused dry run succeeded");
    let printed = stdout(&dry);
    let refusal = printed
        .lines()
        .find(|line| line.contains("door 1: refused: "))
        .unwrap_or_default();
    assert!(
        refusal.ends_with("field body names a term of the lists"),
        "{printed}"
    );
    assert!(
        !refusal.contains("zzcontext"),
        "the refusal holds the word: {refusal}"
    );
    assert_eq!(
        printed.lines().last(),
        Some("carried 9, staying 0, refused 1"),
        "{printed}"
    );
    let applied = carry(&instance, &destination, &refusing, true);
    assert!(!applied.status.success(), "a refused carry was applied");
    assert_eq!(
        (held(&source), held(&destination)),
        before,
        "a refused carry moved a store"
    );

    // An actor the list gives no public name refuses each row it would write.
    let unnamed = a_list(
        &root,
        "assistant = \"assistant\"",
        "stay = \"The box's own disk.\"",
    );
    let printed = stdout(&carry(&instance, &destination, &unnamed, false));
    assert_eq!(
        printed.lines().last(),
        Some("carried 6, staying 1, refused 3"),
        "{printed}"
    );
    assert!(
        printed.contains("refused: its actor ada has no public name"),
        "{printed}"
    );

    // The dry run of the list the owner reads.
    let list = a_list(&root, ACTORS, "stay = \"The box's own disk.\"");
    let dry = carry(&instance, &destination, &list, false);
    let printed = stdout(&dry);
    assert!(
        dry.status.success(),
        "{printed}{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert_eq!(
        (held(&source), held(&destination)),
        before,
        "the dry run wrote"
    );
    assert_eq!(
        printed.lines().last(),
        Some("carried 9, staying 1, refused 0"),
        "{printed}"
    );
    assert!(printed.contains("zzperson → notesask"), "{printed}");
    assert!(
        printed.contains("suzatarytwo → suzatarytwo (release 2.0)  completed"),
        "{printed}"
    );
    assert!(printed.contains("    actor = \"owner\"\n"), "{printed}");
    let verdicts: Vec<&str> = printed
        .lines()
        .filter(|line| line.contains(", door 1: "))
        .collect();
    assert!(
        verdicts.iter().all(|line| line.ends_with(": taken")),
        "{printed}"
    );
    assert!(
        printed.contains(
            "itemdropped → itemgone  closed: a carried price names it, so it is carried as it \
             closed\n"
        ),
        "{printed}"
    );
    assert!(printed.contains("  Cancelled at "), "{printed}");
    let open_release = price(&instance, &["--chain", "suzatary"]);
    let source_before = events(&source);

    // The apply writes exactly the events the dry run judged.
    let applied = carry(&instance, &destination, &list, true);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let new: BTreeSet<String> = held(&destination).difference(&before.1).cloned().collect();
    assert_eq!(
        new.len(),
        verdicts.len(),
        "one object for each event the dry run judged"
    );
    // The dry run printed every string each written event publishes, and nothing else.
    let mut shown: Vec<&str> = printed
        .lines()
        .filter(|line| line.starts_with("    "))
        .collect();
    let written: Vec<String> = events(&destination)
        .iter()
        .flat_map(suzatary_cli::commands::door::published)
        .map(|(field, text)| format!("    {field} = {text:?}"))
        .collect();
    let mut written: Vec<&str> = written.iter().map(String::as_str).collect();
    shown.sort_unstable();
    written.sort_unstable();
    assert_eq!(shown, written);
    assert!(
        written.contains(&"    waiting_on = \"the upstream fix\""),
        "{written:?}"
    );
    let carried: BTreeSet<String> = events(&destination)
        .iter()
        .map(|event| event.todo().as_str().to_owned())
        .collect();
    let expected: BTreeSet<String> = [
        "coldstart",
        "notesask",
        "suzatarytwotwo",
        "foldrun",
        "restated",
        "suzatarytwo",
        "suzatarytwoone",
        "itemdone",
        "itemgone",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(carried, expected);
    assert!(
        envelopes(&destination)
            .iter()
            .all(|envelope| matches!(envelope, Envelope::Genesis(_) | Envelope::Change(_))),
        "the destination holds a signature or a key object"
    );
    let verified = run(
        &instance,
        &["verify", "--store", &destination.to_string_lossy()],
    );
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    for todo in &expected {
        let shown = run(
            &instance,
            &[
                "todo",
                "show",
                "--store",
                &destination.to_string_lossy(),
                todo,
            ],
        );
        assert!(
            shown.status.success(),
            "{todo}: {}",
            String::from_utf8_lossy(&shown.stderr)
        );
    }

    // The open release names its items by their new ids, and the version reads the same.
    let renamed = events(&destination)
        .into_iter()
        .find_map(|event| match event {
            TodoEvent::Authored(record) if record.todo.as_str() == "suzatarytwotwo" => {
                Some(fpl::print_term(&record.payload.spec))
            }
            _ => None,
        });
    assert!(
        renamed
            .as_deref()
            .is_some_and(|spec| spec.contains("notesask")),
        "{renamed:?}"
    );
    let version = |store: &Path| {
        let events = events(store);
        let plan = suzatary_core::release::Plan::declared().expect("the plan");
        suzatary_core::release::Reading::of(
            &plan,
            &events,
            at(28),
            &std::collections::BTreeMap::new(),
        )
        .counted
    };
    assert_eq!(version(&destination), version(&source));
    // The open release prices the same, and its items read the same lots: a closed item is carried
    // as it closed.
    assert_eq!(
        price(&instance, &["--store", &destination.to_string_lossy()]),
        open_release
    );
    assert_eq!(lots(&events(&destination)), lots(&source_before));

    // A stop between a shipped release's two events: the next run writes the one it lacks.
    let completion = named_objects(&destination)
        .into_iter()
        .find_map(|(name, envelope)| match envelope.event() {
            Some(TodoEvent::Completed(done)) if done.todo.as_str() == "suzatarytwoone" => {
                Some(name)
            }
            _ => None,
        })
        .expect("the release's completion");
    std::fs::remove_file(destination.join("objects").join(format!("{completion}.py")))
        .expect("the completion removed");
    let heads = destination.join(".heads");
    if heads.is_dir() {
        std::fs::remove_dir_all(&heads).expect("the heads' record removed");
    } else if heads.exists() {
        std::fs::remove_file(&heads).expect("the heads' record removed");
    }
    assert_ne!(version(&destination), version(&source), "the stop took");
    let stopped = held(&destination);
    let resumed = carry(&instance, &destination, &list, true);
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert_eq!(held(&destination).difference(&stopped).count(), 1);
    assert_eq!(version(&destination), version(&source));

    // Each carried source is cancelled, naming its successor; the staying one is not.
    let notes: BTreeSet<(String, String)> = events(&source)
        .into_iter()
        .filter_map(|event| match event {
            TodoEvent::Cancelled(cancelled) if cancelled.note.starts_with("carried to ") => {
                Some((cancelled.todo.as_str().to_owned(), cancelled.note))
            }
            _ => None,
        })
        .collect();
    let named: BTreeSet<(String, String)> = [
        ("coldstart", "coldstart"),
        ("zzperson", "notesask"),
        ("suzatarytwotwo", "suzatarytwotwo"),
        ("foldrun", "foldrun"),
        ("restated", "restated"),
    ]
    .into_iter()
    .map(|(from, to)| (from.to_owned(), format!("carried to todo:roadmap/{to}")))
    .collect();
    assert_eq!(notes, named);

    // Again: nothing to write.
    let after = (held(&source), held(&destination));
    let again = carry(&instance, &destination, &list, true);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(
        (held(&source), held(&destination)),
        after,
        "a second apply wrote"
    );
}

/// A todo completed between the dry run and the apply, which no carried price names, makes the
/// list name a todo that is not open: the apply refuses it whole and writes nothing to either
/// store.
#[test]
fn a_list_a_completion_made_stale_is_refused_whole() {
    let (root, instance, destination) = a_box("carry-stale");
    let list = a_list(&root, ACTORS, "stay = \"The box's own disk.\"");
    assert!(
        carry(&instance, &destination, &list, false)
            .status
            .success()
    );
    let done = run(
        &instance,
        &["todo", "--chain", "suzatary", "done", "foldrun"],
    );
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let source = root.join("events-suzatary");
    let before = (held(&source), held(&destination));
    let applied = carry(&instance, &destination, &list, true);
    let said = String::from_utf8_lossy(&applied.stderr);
    assert!(
        !applied.status.success()
            && said.contains("carries foldrun, which is not open and no carried price names"),
        "{said}"
    );
    assert_eq!((held(&source), held(&destination)), before);
}

/// A run stopped after the destination's write and before the source's cancellations, then a list
/// edited before the next run: the edited row's record differs from the one the destination
/// holds, so the row is refused, counted as a refusal, and neither store moves. With the list as
/// it was, the next run writes the cancellation the stop left out.
#[test]
fn a_record_the_destination_holds_differently_is_refused() {
    let (root, instance, destination) = a_box("carry-differs");
    let list = a_list(&root, ACTORS, "stay = \"The box's own disk.\"");
    let applied = carry(&instance, &destination, &list, true);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let source = root.join("events-suzatary");
    let cancellation = named_objects(&source)
        .into_iter()
        .find_map(|(name, envelope)| match envelope.event() {
            Some(TodoEvent::Cancelled(cancelled)) if cancelled.todo.as_str() == "restated" => {
                Some(name)
            }
            _ => None,
        })
        .expect("the carry's cancellation of restated");
    std::fs::remove_file(source.join("objects").join(format!("{cancellation}.py")))
        .expect("the cancellation removed");
    let heads = source.join(".heads");
    if heads.is_dir() {
        std::fs::remove_dir_all(&heads).expect("the heads' record removed");
    } else if heads.exists() {
        std::fs::remove_file(&heads).expect("the heads' record removed");
    }

    let text = std::fs::read_to_string(&list).expect("the list");
    let edited = root.join("edited-list.toml");
    std::fs::write(
        &edited,
        text.replace("body = \"New words\"", "body = \"Newer words\""),
    )
    .expect("the edited list");
    let before = (held(&source), held(&destination));
    let dry = carry(&instance, &destination, &edited, false);
    let printed = stdout(&dry);
    assert!(
        !dry.status.success(),
        "{printed}{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(
        printed.contains(
            "restated → restated  carry: Said again for the public.\n  refused: the \
             destination holds a different record of it\n"
        ),
        "{printed}"
    );
    assert_eq!(
        printed.lines().last(),
        Some("carried 8, staying 1, refused 1"),
        "{printed}"
    );
    assert!(
        !carry(&instance, &destination, &edited, true)
            .status
            .success()
    );
    assert_eq!((held(&source), held(&destination)), before);

    let resumed = carry(&instance, &destination, &list, true);
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let carried = events(&source)
        .into_iter()
        .filter(|event| {
            matches!(event, TodoEvent::Cancelled(cancelled)
                if cancelled.todo.as_str() == "restated"
                    && cancelled.note == "carried to todo:roadmap/restated")
        })
        .count();
    assert_eq!(
        carried, 1,
        "the resumed run wrote the cancellation the stop left out"
    );
    assert_eq!(held(&destination), before.1);
}

/// A closed todo that only an earlier price of a carried todo named stays behind: the carry reads
/// the price it carries. Here the open release's revised price drops its cancelled item, which the
/// list then need not name.
#[test]
fn a_closed_item_no_carried_price_names_stays_behind() {
    let (root, instance, destination) = a_box("carry-revised");
    let store = EventStore::new(
        root.join("events-suzatary"),
        prodrome_core::policy::Untrusted::none(),
    );
    suzatary_store::store::append_to(
        &store,
        suzatary_record::record::GENESIS,
        prodrome_core::event::mk_spec_revised::<suzatary_record::record::Record>(
            "suzatarytwotwo",
            at(10),
            "assistant",
            refs(&["coldstart", "zzperson", "itemdone", "suzatarytwoone"]),
            "",
        )
        .expect("a revision"),
    )
    .expect("appends");
    let list = a_list(&root, ACTORS, "stay = \"The box's own disk.\"");
    let text = std::fs::read_to_string(&list).expect("the list");
    let (before, after) = text
        .split_once("[todos.itemdropped]")
        .expect("the item's table");
    let (_, rest) = after.split_once("\n\n").expect("the next table");
    std::fs::write(&list, format!("{before}{rest}")).expect("the list without the item");
    let dry = carry(&instance, &destination, &list, false);
    let printed = stdout(&dry);
    assert!(
        dry.status.success(),
        "{printed}{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(!printed.contains("itemdropped"), "{printed}");
    assert_eq!(
        printed.lines().last(),
        Some("carried 8, staying 1, refused 0"),
        "{printed}"
    );
}
