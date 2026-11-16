//! Door 1 of a public store (`docs/PUBLICTASKS-PLAN-2026-11-15.md` §3.1, stage 2), a security
//! boundary: a store whose charter says `public = true` takes no event that names a term of the
//! lists, a tailnet, a home path, an address, a telephone number or a secret, and nothing at all
//! while it cannot read the lists. The refusal names the field and the rule's kind, never the
//! words. From the box it takes no write by the in-app secretary, and from anyone no signature
//! object. A private store takes the same writes. The lists are scratch ones written here, and
//! every sample is cut in two so no line of this file holds one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use prodrome_core::event::{Envelope, canonical_envelope, mk_completed, seal_hash};
use suzatary_cli::commands::door::Opened;
use suzatary_cli::instance::Instance;
use suzatary_store::store::Prodrome as _;

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

/// `todo add <id>` with this rationale, a plain-text field, through the binary.
fn add(instance: &Path, id: &str, rationale: &str) -> std::process::Output {
    signing(
        instance,
        &[
            "todo",
            "add",
            id,
            "--body",
            "Water the garden",
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
    let hits: [(&str, String, &str, &str); 6] = [
        (
            "term",
            "Ask zzperson about it".to_owned(),
            "names a term of the lists",
            "zzperson",
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
        let refused = add(&public_instance, &format!("zz{id}"), text);
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

        let beside = add(&private_instance, &format!("zz{id}"), text);
        assert!(
            beside.status.success(),
            "{id} on a private store: {}",
            String::from_utf8_lossy(&beside.stderr)
        );
    }

    let before = held(&public);
    let clean = add(&public_instance, "zzclean", "A plain reason");
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    let new: Vec<String> = held(&public).difference(&before).cloned().collect();
    assert_eq!(new.len(), 1, "one unsigned change, no signature: {new:?}");
}

/// The lists are read at each write and only a deployment's machine holds them: where one cannot
/// be read, the public store takes nothing and says why; a private one never reads them.
#[test]
fn a_public_store_takes_nothing_while_it_cannot_read_the_lists() {
    for (name, public) in [("door-unlisted", true), ("door-unlisted-private", false)] {
        let (root, instance) = a_box(name, public);
        std::fs::remove_file(root.join("local/context-terms.txt")).expect("removed");
        let before = held(&root);
        let wrote = add(&instance, "zzplain", "A plain reason");
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
