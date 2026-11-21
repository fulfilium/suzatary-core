//! Door 1 of a public store (decision `publictaskwords`), a security
//! boundary: a store whose charter says `public = true` takes no event that names a term of the
//! lists, a name of the deployment's own, a tailnet, a home path, an address, a telephone number
//! or a secret, and nothing at all while it cannot read the lists. The refusal names the field
//! and the rule's kind, never the words. From the box it takes no write by the in-app secretary,
//! and from anyone no signature object. A private store takes the same writes. The lists are
//! scratch ones written here, and every sample is cut in two so no line of this file holds one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use prodrome_core::event::{Envelope, canonical_envelope, mk_completed, seal_hash};
use suzatary_cli::commands::door::{Door, Opened};
use suzatary_cli::instance::Instance;
use suzatary_store::store::{Prodrome as _, Shut};

use crate::todos::{a_chain, a_signing_instance, scratch, signing};

const SEED: &str = "9999999999999999999999999999999999999999999999999999999999999999";

/// A box whose one chain, `life`, is chartered as `charter` says, with its lists in the box-local
/// tier and the code's secret rules at its code root.
fn a_box(name: &str, public: bool) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    a_chain(&root);
    if public {
        std::fs::write(root.join("events/charter.toml"), "public = true\n").expect("a charter");
    }
    std::fs::create_dir_all(root.join("local")).expect("the box-local tier");
    std::fs::write(
        root.join("local/personal-terms.txt"),
        "# a comment\nzzperson\n",
    )
    .expect("terms");
    std::fs::write(root.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    std::fs::write(root.join("FETCHER"), "zzfetch\n").expect("the box's name");
    std::fs::create_dir_all(root.join("scripts")).expect("scripts");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/secret-rules.toml"),
        root.join("scripts/secret-rules.toml"),
    )
    .expect("the secret rules");
    let key = prodrome_core::sign::Secret::new(SEED).expect("a seed");
    let instance = a_signing_instance(&root, &key.public());
    (root, instance)
}

/// Every object file the chain holds, by name.
fn held(root: &Path) -> BTreeSet<String> {
    std::fs::read_dir(root.join("events/objects"))
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

/// `todo add <id>` with this body, a markup field, and this rationale, a plain-text one.
fn add(instance: &Path, id: &str, body: &str, rationale: &str) -> std::process::Output {
    signing(
        instance,
        &[
            "todo",
            "add",
            id,
            "--body",
            body,
            "--rationale",
            rationale,
            "--category",
            "home",
            "--priority",
            "70",
        ],
        &[
            (suzatary_signatures::signatures::BOX_KEY, SEED.to_owned()),
            (suzatary_signatures::signatures::AGENT_KEY, "8".repeat(64)),
        ],
    )
}

/// Door 1 of the public box `name`, as the terminal opens it.
fn a_door(name: &str) -> Door {
    let (root, path) = a_box(name, true);
    let instance = Instance::read(&path, &suzatary_cli::paths::Env::under(&root, &root))
        .expect("the instance reads");
    let chain = instance.chain(None).expect("life");
    let suzatary_cli::instance::Charted::Public(lists) = &chain.charter else {
        panic!("the box's chain is chartered public");
    };
    Door::of(
        lists.clone(),
        &chain.policy,
        Opened::AtTerminal,
        std::sync::Arc::new(crate::binary::typeset()),
    )
}

/// A GitHub token's shape, from noise made here.
fn a_token() -> String {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let noise: String = (0..36)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(alphabet[usize::try_from(state % 62).expect("small")])
        })
        .collect();
    format!("{}{noise}", concat!("gh", "p_"))
}

/// Each kind of hit is refused before the store holds it: the store's objects are the same
/// files after. The refusal names the field and the kind, and holds no word it matched. The
/// same write to a private store is taken.
#[test]
fn a_public_store_refuses_each_kind_of_hit_before_it_holds_it() {
    let token = a_token();
    let hits: [(&str, String, &str, &str); 7] = [
        (
            "term",
            "Ask zzperson about it".to_owned(),
            "names a term of the lists",
            "zzperson",
        ),
        (
            "name",
            "Ask the zzfetch box".to_owned(),
            "names a name of this deployment",
            "zzfetch",
        ),
        (
            "tailnet",
            concat!("Reach box.zztail.ts", ".net").to_owned(),
            "names a tailnet",
            "zztail",
        ),
        (
            "home",
            concat!("Notes in /ho", "me/zzuser/notes").to_owned(),
            "names a home path",
            "zzuser",
        ),
        (
            "address",
            concat!("Write to zz@", "zzdomain.org").to_owned(),
            "names an email address",
            "zzdomain",
        ),
        (
            "phone",
            concat!("Call +961 ", "3 123 456").to_owned(),
            "names a telephone number",
            "123 456",
        ),
        (
            "secret",
            format!("token = {token}"),
            "holds a secret",
            &token[4..],
        ),
    ];
    let (public, public_instance) = a_box("door-public", true);
    let (_, private_instance) = a_box("door-private", false);
    for (id, text, kind, word) in &hits {
        let before = held(&public);
        let refused = add(
            &public_instance,
            &format!("zz{id}"),
            "Water the garden",
            text,
        );
        let said = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{id} was taken");
        assert!(
            said.contains("field rationale") && said.contains(kind),
            "{id}: {said}"
        );
        assert!(
            !said.contains(word),
            "{id}: the refusal holds the word: {said}"
        );
        assert_eq!(held(&public), before, "{id}: the store moved");

        let beside = add(
            &private_instance,
            &format!("zz{id}"),
            "Water the garden",
            text,
        );
        assert!(
            beside.status.success(),
            "{id} on a private store: {}",
            String::from_utf8_lossy(&beside.stderr)
        );
    }

    let before = held(&public);
    let clean = add(
        &public_instance,
        "zzclean",
        "Water the garden",
        "A plain reason",
    );
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    let new: Vec<String> = held(&public).difference(&before).cloned().collect();
    assert_eq!(new.len(), 1, "one unsigned change, no signature: {new:?}");
}

/// The package's own gitleaks and rules judge a write to a public store, with nothing on `PATH`
/// and no rules under the code root: a binary installed as the flake installs it
/// (`suzatary_cli::stamp`) takes a clean write and refuses a secret.
#[test]
fn a_packaged_binary_judges_a_public_write_with_its_own_gitleaks() {
    let (root, instance) = a_box("door-packaged", true);
    std::fs::remove_file(root.join("scripts/secret-rules.toml")).expect("removed");
    let out = root.join("package");
    let exe = out.join("bin/suzatary");
    std::fs::create_dir_all(exe.parent().expect("bin")).expect("bin");
    suzatary_testkit::copy_program(Path::new(crate::binary::path()), &exe);
    let rules = out.join(suzatary_cli::stamp::SECRET_RULES);
    std::fs::create_dir_all(rules.parent().expect("share")).expect("share");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/secret-rules.toml"),
        &rules,
    )
    .expect("the rules");
    let gitleaks = out.join(suzatary_cli::stamp::GITLEAKS);
    std::fs::create_dir_all(gitleaks.parent().expect("libexec")).expect("libexec");
    let found = std::env::split_paths(&std::env::var_os("PATH").expect("a PATH"))
        .map(|dir| dir.join("gitleaks"))
        .find(|candidate| candidate.is_file())
        .expect("gitleaks on the suite's PATH");
    std::os::unix::fs::symlink(found, &gitleaks).expect("the package's gitleaks");

    let packaged = |id: &str, rationale: &str| {
        crate::binary::run(&exe)
            .env("PATH", root.join("nothing"))
            .env("HOME", root.join("home"))
            .env(suzatary_signatures::signatures::BOX_KEY, SEED)
            .env(suzatary_signatures::signatures::AGENT_KEY, "8".repeat(64))
            .args(["--instance", &instance.to_string_lossy(), "todo", "add", id])
            .args(["--body", "Water the garden", "--rationale", rationale])
            .args(["--category", "home", "--priority", "70"])
            .output()
            .expect("the package's binary runs")
    };
    let before = held(&root);
    let taken = packaged("zzclean", "A plain reason");
    assert!(
        taken.status.success(),
        "{}",
        String::from_utf8_lossy(&taken.stderr)
    );
    assert_eq!(held(&root).len(), before.len() + 1, "one change");
    let refused = packaged("zzsecret", &format!("token = {}", a_token()));
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success() && said.contains("holds a secret"),
        "{said}"
    );
}

/// Door 1 judges a batch as it judges each event alone: the lists read once and gitleaks run
/// once give each event the verdict the store's per-event rule gives it, whatever the batch
/// holds around it.
#[test]
fn door_1_judges_a_batch_as_each_event_alone() {
    use proptest::prelude::*;
    let door = a_door("door-batch");
    let token = a_token();
    let texts = [
        "A plain reason".to_owned(),
        "Ask zzperson about it".to_owned(),
        "Ask the zzfetch box".to_owned(),
        concat!("Notes in /ho", "me/zzuser/notes").to_owned(),
        format!("token = {token}"),
        format!("first line\ntoken = {token}"),
        concat!("Reach box.zztail.ts", ".net").to_owned(),
    ];
    let event = |(actor, text): (usize, usize)| {
        suzatary_record::record::mk_authored(
            "zzbatch",
            prodrome_core::literal::Datetime::new(2026, 11, 15, 9, 0, 0, 0).expect("an instant"),
            ["ada", "assistant", "triage"][actor],
            "todo",
            prodrome_core::literal::Datetime::new(2026, 11, 15, 0, 0, 0, 0).expect("a day"),
            suzatary_record::record::MarkupSource::escaped("Water the garden"),
            prodrome_core::fpl::mk_flat(0.7).expect("a term"),
            vec![texts[text].clone()],
            "home",
            "",
            suzatary_record::record::MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("an event")
    };
    let mut runner = proptest::test_runner::TestRunner::new(ProptestConfig {
        cases: 12,
        ..ProptestConfig::default()
    });
    runner
        .run(
            &proptest::collection::vec((0..3usize, 0..texts.len()), 1..6),
            |picked| {
                let events: Vec<_> = picked.into_iter().map(event).collect();
                let all: Vec<&_> = events.iter().collect();
                let mut batch: Vec<_> = all.iter().map(|_| Ok(())).collect();
                door.judge(&all, &mut batch);
                let alone: Vec<_> = all
                    .iter()
                    .map(|one| {
                        let mut verdict = [Ok(())];
                        door.judge(&[one], &mut verdict);
                        let [verdict] = verdict;
                        verdict
                    })
                    .collect();
                prop_assert_eq!(batch, alone);
                Ok(())
            },
        )
        .expect("the law holds");
}

/// Door 1 reads what a markup field draws, in both forms a renderer publishes: a body that builds
/// a listed word from parts, in its text or in an attribute, is refused, naming the field and
/// saying it is the drawn text, never the word; the same write to a private store is taken. A
/// body whose source names the word is refused once, for its source.
#[test]
fn a_public_store_reads_what_a_markup_field_draws() {
    let (public, public_instance) = a_box("door-drawn", true);
    let (_, private_instance) = a_box("door-drawn-private", false);
    for (id, body) in [
        ("zzbuilt", r#"Ask #("zz" + "per" + "son") about it"#),
        ("zzjoined", "Ask *zz*_person_ about it"),
        (
            "zzlinked",
            r#"Ask #link("https://zz" + "person.test")[them]"#,
        ),
    ] {
        let before = held(&public);
        let refused = add(&public_instance, id, body, "A plain reason");
        let said = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{id} was taken");
        assert!(
            said.contains("field body, as drawn, names a term of the lists"),
            "{id}: {said}"
        );
        assert!(
            !said.contains("zzperson"),
            "{id}: the refusal holds the word: {said}"
        );
        assert_eq!(held(&public), before, "{id}: the store moved");
        let beside = add(&private_instance, id, body, "A plain reason");
        assert!(
            beside.status.success(),
            "{id} on a private store: {}",
            String::from_utf8_lossy(&beside.stderr)
        );
    }

    let refused = add(
        &public_instance,
        "zzsource",
        "Ask zzperson about it",
        "A plain reason",
    );
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "taken");
    assert_eq!(
        said.matches("a public store holds no event").count(),
        1,
        "one finding: {said}"
    );
    assert!(
        said.contains("field body names a term of the lists") && !said.contains("as drawn"),
        "{said}"
    );
    assert!(
        !said.contains("zzperson"),
        "the refusal holds the word: {said}"
    );

    let clean = add(
        &public_instance,
        "zzclean",
        "Water the *garden*",
        "A plain reason",
    );
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
}

/// A renderer of a public store publishes of a markup field what door 1 read of it, and nothing
/// the field draws only where it can see the page: each body (and detail) below builds a listed
/// word from parts when it sees a heading, the heading counter, its place, the page's links, or a
/// state or counter the other field sets. Door 1 reads each field alone, where it draws no such
/// word, and takes it; `prodrome pages` then publishes no page that holds the word. (The page
/// numbers no heading, so typst steps no heading counter there today; that case guards a page that
/// would.)
#[test]
fn a_page_publishes_of_a_field_only_what_door_1_read() {
    let word = r#"#("zz" + "per" + "son")"#;
    let (root, instance) = a_box("door-page", true);
    let seen = |condition: &str| format!("Sort the shed #context if {condition} [{word}]");
    let fields: [(String, String); 6] = [
        (seen("query(heading).len() > 0"), String::new()),
        (seen("counter(heading).get().first() > 0"), String::new()),
        (
            seen("query(selector(heading).before(here())).len() > 0"),
            String::new(),
        ),
        (seen("query(link).len() > 0"), String::new()),
        (
            format!(r#"Sort the shed #state("zzshared").update([{word}])"#),
            r#"#context state("zzshared", none).get()"#.to_owned(),
        ),
        (
            r#"Sort the shed #counter("zzshared").step()"#.to_owned(),
            format!(r#"#context if counter("zzshared").get().first() > 0 [{word}]"#),
        ),
    ];
    for (n, (body, detail)) in fields.iter().enumerate() {
        let id = format!("zzshed{n}");
        let wrote = signing(
            &instance,
            &[
                "todo",
                "add",
                &id,
                "--body",
                body,
                "--detail",
                detail,
                "--rationale",
                "A plain reason",
                "--category",
                "home",
                "--priority",
                "70",
            ],
            &[
                (suzatary_signatures::signatures::BOX_KEY, SEED.to_owned()),
                (suzatary_signatures::signatures::AGENT_KEY, "8".repeat(64)),
            ],
        );
        assert!(
            wrote.status.success(),
            "{id}: door 1 reads the field alone, where it draws no listed word: {}",
            String::from_utf8_lossy(&wrote.stderr)
        );
    }
    let site = root.join("site");
    let paged = crate::binary::suzatary()
        .args(["prodrome", "pages", "--store"])
        .arg(root.join("events"))
        .args(["--at", "2030-01-01", "--into"])
        .arg(&site)
        .output()
        .expect("the binary runs");
    assert!(
        paged.status.success(),
        "{}",
        String::from_utf8_lossy(&paged.stderr)
    );
    let leaked: Vec<usize> = (0..fields.len())
        .filter(|n| {
            let page = std::fs::read_to_string(site.join(format!("todo/zzshed{n}.html")))
                .expect("the todo's page");
            assert!(page.contains("Sort the shed"), "zzshed{n}: {page}");
            page.contains("zzperson")
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "these publish a word door 1 never read: {leaked:?}"
    );
}

/// A public store holds no markup a renderer cannot draw: a body that does not compile in the
/// closed world, or loops past its CPU limit, is refused with typst's message and the field; a
/// message that would print a listed word is withheld and named as such.
#[test]
fn a_public_store_refuses_a_markup_field_that_does_not_draw() {
    let door = a_door("door-undrawn");
    let at = prodrome_core::literal::Datetime::new(2026, 11, 15, 9, 0, 0, 0).expect("an instant");
    let event = |body: &str, detail: &str| {
        suzatary_record::record::mk_authored(
            "zzundrawn",
            at,
            "ada",
            "todo",
            at,
            suzatary_record::record::MarkupSource::from_chain(body),
            prodrome_core::fpl::mk_flat(0.7).expect("a term"),
            Vec::new(),
            "home",
            "",
            suzatary_record::record::MarkupSource::from_chain(detail),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("an event")
    };
    let judged = |body: &str, detail: &str| {
        let mut verdict = [Ok(())];
        door.judge(&[&event(body, detail)], &mut verdict);
        let [verdict] = verdict;
        verdict
    };
    let refusal = |body: &str, detail: &str| match judged(body, detail) {
        Err(Shut::Refused(said)) => said,
        other => panic!("{body}: {other:?}"),
    };
    for (body, detail, field, says) in [
        (
            "Water the garden",
            "#nosuchthing",
            "detail",
            "unknown variable: nosuchthing",
        ),
        ("Water the #garden", "", "body", "unknown variable: garden"),
        (
            r#"Water #link("javascript:void(0)")[it]"#,
            "",
            "body",
            "its render carries a URL that does not navigate",
        ),
        (
            "#for i in range(100000) { for j in range(100000) {} }",
            "",
            "body",
            "did not finish compiling",
        ),
    ] {
        let said = refusal(body, detail);
        assert!(
            said.contains(&format!("field {field} does not draw in the closed world"))
                && said.contains(says),
            "{body}: {said}"
        );
    }
    let refused = refusal(r#"#panic("zz" + "person")"#, "");
    assert!(
        refused.contains(
            "field body does not draw, and typst's message about it names a term of the lists"
        ),
        "{refused}"
    );
    assert!(
        !refused.contains("zzperson"),
        "the refusal holds the word: {refused}"
    );
    assert_eq!(judged("Water the *garden*", "- one\n- two"), Ok(()));
}

/// The lists are read at each write and only a deployment's machine holds them: where one cannot
/// be read, the public store takes nothing and says why; a private one never reads them.
#[test]
fn a_public_store_takes_nothing_while_it_cannot_read_the_lists() {
    for (name, public) in [("door-unlisted", true), ("door-unlisted-private", false)] {
        let (root, instance) = a_box(name, public);
        std::fs::remove_file(root.join("local/context-terms.txt")).expect("removed");
        let before = held(&root);
        let wrote = add(&instance, "zzplain", "Water the garden", "A plain reason");
        let said = String::from_utf8_lossy(&wrote.stderr);
        if public {
            assert!(!wrote.status.success(), "taken unread");
            assert!(said.contains("context-terms.txt cannot be read"), "{said}");
            assert_eq!(held(&root), before, "the store moved");
        } else {
            assert!(wrote.status.success(), "{said}");
        }
    }
}

/// From the box, `assistant` is the in-app secretary, and a public store refuses its write; at
/// the terminal it is the main session, whose write is taken. No signature object is held,
/// whoever offers it.
#[test]
fn a_public_store_refuses_the_secretary_and_every_signature() {
    let (root, path) = a_box("door-writers", true);
    let instance = Instance::read(&path, &suzatary_cli::paths::Env::under(&root, &root))
        .expect("the instance reads");
    let chain = instance.chain(None).expect("life");
    let signers = instance.signers();
    let as_owner = prodrome_core::event::Actor::new("ada").expect("an actor");
    let at = prodrome_core::literal::Datetime::new(2026, 11, 15, 9, 0, 0, 0).expect("an instant");
    let done = || mk_completed("parkingpermit", at, "assistant", "").expect("an event");

    let before = held(&root);
    let by_the_box = Instance::writer_of(chain, &signers, &as_owner, Opened::ByTheBox);
    let refused = by_the_box
        .append(done())
        .expect_err("the secretary's write was taken");
    assert!(refused.to_string().contains("refuses it"), "{refused}");
    assert_eq!(held(&root), before, "the store moved");

    let key = prodrome_core::sign::Secret::new(SEED).expect("a seed");
    let first = before.iter().next().expect("a begun chain");
    let signed = Envelope::<suzatary_record::record::TodoEvent>::Signed(
        key.sign(&prodrome_core::event::Hash::new(first.trim_end_matches(".py")).expect("a name")),
    );
    let refused = by_the_box
        .accept(&[(seal_hash(&signed), canonical_envelope(&signed))])
        .expect_err("a signature was taken");
    assert!(refused.to_string().contains("holds no Signed"), "{refused}");
    assert_eq!(held(&root), before, "the store moved");

    Instance::writer_of(chain, &signers, &as_owner, Opened::AtTerminal)
        .append(done())
        .expect("the main session's write is taken");
    assert_eq!(held(&root).len(), before.len() + 1, "one unsigned change");
}

/// `todo add <id> --waiting-on <on>` on the box's first chain.
fn waiting(instance: &Path, id: &str, on: &str) -> std::process::Output {
    signing(
        instance,
        &[
            "todo",
            "add",
            id,
            "--body",
            "Mow",
            "--rationale",
            "A plain reason",
            "--category",
            "home",
            "--priority",
            "70",
            "--waiting-on",
            on,
        ],
        &[
            (suzatary_signatures::signatures::BOX_KEY, SEED.to_owned()),
            (suzatary_signatures::signatures::AGENT_KEY, "8".repeat(64)),
        ],
    )
}

/// The private chain `home`, holding `hometask`, added to the box at `root`.
fn a_private_chain(root: &Path, instance: &Path) {
    let private = suzatary_record::record::EventStore::new(
        root.join("events-home"),
        prodrome_core::policy::Untrusted::none(),
    );
    suzatary_store::store::append_to(
        &private,
        suzatary_record::record::GENESIS,
        crate::follows::task("hometask", 0.5, ""),
    )
    .expect("appends");
    let text = std::fs::read_to_string(instance).expect("the instance");
    std::fs::write(
        instance,
        format!(
            "{text}\n[[chains]]\nname = \"home\"\nevents = \"events-home\"\n\
             untrusted = []\nactors = [\"ada\"]\nowner = \"ada\"\n"
        ),
    )
    .expect("the instance");
}

/// A public store names a todo by its store's genesis, never by a chain name of this deployment
/// and never a task of a private chain, by either name. A chain name of a store anyone reads is
/// refused with the whole name by genesis to write instead, which `todo show` prints and the store
/// then takes; one of a private chain is refused with no name to write, and so is a private
/// chain's genesis. A private store takes every one of them.
#[test]
fn a_public_store_refuses_a_chain_name_and_says_the_genesis_form() {
    for (name, public) in [("door-named", true), ("door-named-private", false)] {
        let (root, instance) = a_box(name, public);
        a_private_chain(&root, &instance);
        let first = add(&instance, "zzfirst", "Water the garden", "A plain reason");
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        let named = |reference: &str| {
            let shown = signing(&instance, &["todo", "show", reference], &[]);
            assert!(
                shown.status.success(),
                "{}",
                String::from_utf8_lossy(&shown.stderr)
            );
            let shown: serde_json::Value =
                serde_json::from_slice(&shown.stdout).expect("todo show prints JSON");
            shown["name"].as_str().expect("its name").to_owned()
        };
        let life = named("todo:life/zzfirst");
        assert_eq!(named(&life), life, "shown by its name, it names itself");
        let home = named("todo:home/hometask");
        let before = held(&root);
        let private = "names no task of a private chain".to_owned();
        for (id, on, said) in [
            (
                "zzlocal",
                "todo:life/zzfirst".to_owned(),
                format!("write {life} instead"),
            ),
            ("zzhome", "todo:home/hometask".to_owned(), private.clone()),
            ("zzhomeg", home.clone(), private.clone()),
            (
                "zzhomes",
                format!("todo:+{}/hometask", &home[6..14]),
                private.clone(),
            ),
        ] {
            let wrote = waiting(&instance, id, &on);
            let stderr = String::from_utf8_lossy(&wrote.stderr);
            if public {
                assert!(!wrote.status.success(), "{on} was published");
                assert!(stderr.contains(&said), "{stderr}");
                assert!(stderr.contains("rewrite the field"), "{stderr}");
                // A refusal may repeat what the writer wrote, never say what they did not.
                assert!(
                    on.contains(&home[6..18]) || !stderr.contains(&home[6..18]),
                    "a private genesis was said: {stderr}"
                );
                assert_eq!(held(&root), before, "the store moved");
            } else {
                assert!(wrote.status.success(), "{stderr}");
            }
        }
        let global = waiting(&instance, "zzglobal", &life);
        assert!(
            global.status.success(),
            "{}",
            String::from_utf8_lossy(&global.stderr)
        );
    }
}

/// Where a chain of the deployment does not read, door 1 cannot tell whether a todo a write names
/// is one of it, so it takes no write that names a todo, a rightful name by genesis included,
/// and says which chain did not read; a write that names no todo is still taken.
#[test]
fn a_public_store_takes_no_name_while_a_chain_of_the_deployment_does_not_read() {
    let (root, instance) = a_box("door-unread-chain", true);
    a_private_chain(&root, &instance);
    let first = add(&instance, "zzfirst", "Water the garden", "A plain reason");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let shown = signing(&instance, &["todo", "show", "todo:life/zzfirst"], &[]);
    let shown: serde_json::Value = serde_json::from_slice(&shown.stdout).expect("JSON");
    let life = shown["name"].as_str().expect("its name").to_owned();
    // A second genesis: a chain is one prodrome, so it no longer reads.
    let second: Envelope<suzatary_record::record::TodoEvent> = Envelope::Genesis(
        prodrome_core::genesis::mk_genesis("todos", &"1".repeat(32)).expect("a genesis"),
    );
    std::fs::write(
        root.join("events-home/objects")
            .join(format!("{}.py", seal_hash(&second).as_str())),
        canonical_envelope(&second),
    )
    .expect("a second genesis");
    let before = held(&root);
    let named = waiting(&instance, "zznamed", &life);
    let said = String::from_utf8_lossy(&named.stderr);
    assert!(!named.status.success(), "taken while a chain did not read");
    assert!(said.contains("the chain home does not read"), "{said}");
    assert_eq!(held(&root), before, "the store moved");
    let plain = waiting(&instance, "zzplain", "Ada");
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
}
