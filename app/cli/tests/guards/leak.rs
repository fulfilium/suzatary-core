//! The leak guard, `scripts/public-tree.sh --check`, and the public scan over another
//! repository's range, `scripts/public-scan.sh`: run as the real scripts over throwaway trees and
//! throwaway deployments, never the real ones. The lists live only in the private data repository
//! and are never copied here, so a failure prints each hit's file and line and nothing else.

use super::{Scratch, binary, repo, stderr};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn public_tree_script() -> PathBuf {
    repo().join("scripts/public-tree.sh")
}

/// A public tree is built from a checkout (`git archive HEAD`), and its link rule asks git which
/// private history the checkout holds; the deployed code root is a store path with no `.git`,
/// where the triage push runs this suite, so there these guards say so and pass.
fn a_checkout(guard: &str) -> bool {
    let checkout = repo().join(".git").exists();
    if !checkout {
        eprintln!("skipping {guard}: the code root is not a checkout");
    }
    checkout
}

/// `public-tree.sh` with `args`, reading the deployment at `instance` (the script's own
/// default, `<repo>/instance.toml`, when `None`).
fn public_tree(args: &[&Path], instance: Option<&Path>) -> Output {
    let mut command = Command::new(public_tree_script());
    command.args(args).env("SUZATARY_BIN", binary::path());
    if let Some(instance) = instance {
        command.env(suzatary_cli::instance::ENV, instance);
    } else if let Ok(set) = std::env::var(suzatary_cli::instance::ENV) {
        command.env(suzatary_cli::instance::ENV, set);
    }
    command.output().expect("public-tree.sh runs")
}

/// No tracked file of this repository, its private history included, holds a term from the
/// data repository's lists; the script skips with one line where there is no instance.
#[test]
fn personal_terms_no_tracked_file_names_a_person() {
    let out = public_tree(&[Path::new("--check"), Path::new("--terms"), &repo()], None);
    eprint!("{}", stderr(&out));
    assert!(
        out.status.success(),
        "tracked files hold a term from the data repository's lists, at:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// The tree a public repository would start from holds nothing of this deployment: no address
/// outside the reserved domains, no tailnet name, no home path, none of this box's names and
/// no term of the data repository's lists, which skip with a line where there is no instance.
#[test]
fn public_tree_holds_nothing_of_this_deployment() {
    if !a_checkout("public_tree_holds_nothing_of_this_deployment") {
        return;
    }
    let scratch = Scratch::new("public-tree");
    let tree = scratch.join("tree");
    let made = public_tree(&[&tree], None);
    assert!(made.status.success(), "{}", stderr(&made));
    let out = public_tree(&[Path::new("--check"), &tree], None);
    eprint!("{}", stderr(&out));
    assert!(
        out.status.success(),
        "the public tree would leak, at:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// The public tree hosts itself: a commit of its own output, run through its own copy of the
/// script, writes that output again, byte for byte.
#[test]
fn public_tree_is_a_fixpoint_of_itself() {
    if !a_checkout("public_tree_is_a_fixpoint_of_itself") {
        return;
    }
    let scratch = Scratch::new("public-tree-fixpoint");
    let (first, second) = (scratch.join("first"), scratch.join("second"));
    let made = public_tree(&[&first], None);
    assert!(made.status.success(), "{}", stderr(&made));
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", "user.name=cast", "-c", "user.email=cast@example.test"])
            .args(args)
            .current_dir(&first)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "{}", stderr(&out));
    };
    git(&["init", "-q"]);
    // Forced: a published file is published even where a `.gitignore` names it.
    git(&["add", "-A", "--force"]);
    git(&["commit", "-q", "-m", "the public tree"]);
    let again = Command::new(first.join("scripts/public-tree.sh"))
        .arg(&second)
        .output()
        .expect("the public tree's own script runs");
    assert!(again.status.success(), "{}", stderr(&again));
    let diff = Command::new("diff")
        .args(["-r", "--no-dereference", "-x", ".git"])
        .arg(&first)
        .arg(&second)
        .output()
        .expect("diff runs");
    assert!(
        diff.status.success(),
        "the public tree's own script wrote another tree:\n{}",
        String::from_utf8_lossy(&diff.stdout)
    );
}

/// The matcher itself, over a throwaway tree and a throwaway deployment, never the real ones.
/// Every leak is spelled in pieces, so the tree this guard reads does not hold them.
#[test]
#[allow(clippy::too_many_lines)]
fn public_tree_guard_finds_each_leak_and_never_prints_it() {
    if !a_checkout("public_tree_guard_finds_each_leak_and_never_prints_it") {
        return;
    }
    let scratch = Scratch::new("public-tree-matcher");
    let (data, tree) = (scratch.join("data"), scratch.join("tree"));
    std::fs::create_dir_all(&data).expect("data dir");
    std::fs::create_dir_all(&tree).expect("tree dir");
    std::fs::write(
        data.join("instance.toml"),
        concat!(
            "repo = \"/ho",
            "me/zzuser/code\"\n[web]\napp_url = \"https://zzbox.zznet",
            ".ts.net\"\n"
        ),
    )
    .expect("instance writes");
    std::fs::write(data.join("FETCHER"), "zzfetch\n").expect("FETCHER writes");
    let local = data.join("local");
    std::fs::create_dir_all(&local).expect("the box-local tier");
    std::fs::write(
        local.join("personal-terms.txt"),
        "# a comment, not a term\n\nZzPerson\nzz  two words\n",
    )
    .expect("terms write");
    std::fs::write(local.join("context-terms.txt"), "zzcontext\nzzshort\n")
        .expect("context writes");

    std::fs::write(
        tree.join("clean.txt"),
        concat!(
            "her@example.invalid, a@b.test, ops@mail.example.com, svc@example.iam.x",
            "yz.com\n",
            "box.example",
            ".ts.net and peer.tailnet",
            ".ts.net\n",
            "/ho",
            "me/ada/Mail and /ho",
            "me/root/.ssh\n",
            "https://user@calendar.goo",
            "gle.com and https://u:pass@acme.cloud.databricks.com\n",
            "Co-Authored-By: Claude <noreply@anthrop",
            "ic.com>\n",
            "zzboxes is another word than the box's\n",
        ),
    )
    .expect("clean.txt writes");
    let private = repo().join(concat!("docs/", "CLAUDE-history.md"));
    std::fs::write(
        tree.join("leaks.txt"),
        concat!(
            "nothing here\n",
            "mail zz@realm",
            "ail.com\n",
            "the zzbox box\n",
            "the zzfetch box\n",
            "via peer.zznet",
            ".ts.net\n",
            "/ho",
            "me/zzother/x\n",
            "a ZZPERSON mention\n",
            "zzcontext\n",
            ".claude/projects/-home-zzuser/memory\n",
            "box.tailabc",
            ".ts.net\n",
            "write to mailto:zz@realm",
            "ail.com\n",
            "a wrapped zz two\n",
            "// words term\n",
            "zzshortcake and zzshort_x are other words\n",
            "a (zzshort) mention\n",
            "see docs/",
            "CLAUDE-history.md\n",
        ),
    )
    .expect("leaks.txt writes");
    std::fs::write(tree.join("zzbox-notes.txt"), "nothing here\n").expect("notes write");
    std::fs::write(tree.join("binary.bin"), b"\0zzperson\n").expect("binary writes");
    std::fs::write(tree.join("image.png"), b"\x89PNG\0\0tEXt\0zzperson\0").expect("image writes");
    std::os::unix::fs::symlink("clean.txt", tree.join("link")).expect("link writes");

    let set = |out: &Output| -> std::collections::BTreeSet<String> {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    };
    let named = |hits: &[&str]| -> std::collections::BTreeSet<String> {
        hits.iter().map(|&hit| hit.to_owned()).collect()
    };
    // The link rule names a file of the private history this checkout holds; a public tree
    // holds none, so there it has nothing to find.
    let link: &[&str] = if private.exists() {
        &["leaks.txt:16"]
    } else {
        &[]
    };

    let instance = data.join("instance.toml");
    let all = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    let printed = format!("{}{}", String::from_utf8_lossy(&all.stdout), stderr(&all));
    assert!(!all.status.success(), "leaks were found: {printed}");
    let mut expected = named(&[
        "leaks.txt:2",
        "leaks.txt:3",
        "leaks.txt:4",
        "leaks.txt:5",
        "leaks.txt:6",
        "leaks.txt:7",
        "leaks.txt:8",
        "leaks.txt:9",
        "leaks.txt:10",
        "leaks.txt:11",
        "leaks.txt:12",
        "leaks.txt:15",
        "zzbox-notes.txt:0",
        "image.png:0",
        "binary.bin:0",
        "link:0",
    ]);
    expected.extend(named(link));
    assert_eq!(set(&all), expected, "each leak, and only the leaks");
    for said in [
        "leaks.txt:7 names a term of the lists",
        "leaks.txt:3 names a name of this deployment",
        "leaks.txt:2 names an email address",
    ] {
        assert!(printed.contains(said), "{said}: {printed}");
    }
    for secret in [
        "zzperson",
        "zzcontext",
        "zzshort",
        "zz two",
        "zzfetch",
        "zzuser",
        "zzother",
    ] {
        assert!(
            !printed.to_lowercase().contains(secret),
            "the guard printed what it guards: {printed}"
        );
    }

    let lists = public_tree(
        &[Path::new("--check"), Path::new("--lists"), &tree],
        Some(&instance),
    );
    assert_eq!(
        set(&lists),
        named(&[
            "leaks.txt:7",
            "leaks.txt:8",
            "leaks.txt:12",
            "leaks.txt:15",
            "image.png:0",
            "binary.bin:0",
            "link:0"
        ]),
        "--lists reads the two term lists, the secrets and the admitted files, and no other rule"
    );

    let nowhere = scratch.join("no-instance.toml");
    let bare = public_tree(&[Path::new("--check"), &tree], Some(&nowhere));
    let mut expected = named(&[
        "leaks.txt:2",
        "leaks.txt:5",
        "leaks.txt:6",
        "leaks.txt:10",
        "leaks.txt:11",
        "link:0",
        // No manifest lists either binary.
        "image.png:0",
        "binary.bin:0",
    ]);
    expected.extend(named(link));
    assert_eq!(
        set(&bare),
        expected,
        "with no deployment, the rules that need none still hold"
    );
    assert!(stderr(&bare).contains("skipped"), "a skip says so");
}

/// The rules a reader's eye would catch and the old matcher did not, over a throwaway tree and a
/// throwaway deployment, each leak spelled in pieces: a tailnet node's address; a telephone
/// number; a term with an accent; a binary file the tree's manifest does not list with its
/// digest. The cast's tailnet block and a fictional number are not leaks.
#[test]
fn public_tree_guard_finds_what_a_reader_would() {
    let scratch = Scratch::new("public-tree-reader");
    let (data, tree) = (scratch.join("data"), scratch.join("tree"));
    std::fs::create_dir_all(&data).expect("data dir");
    std::fs::create_dir_all(&tree).expect("tree dir");
    std::fs::write(data.join("instance.toml"), "repo = \"/srv/code\"\n").expect("instance");
    std::fs::create_dir_all(data.join("local")).expect("the box-local tier");
    std::fs::write(data.join("local/personal-terms.txt"), "Zzperson\n").expect("terms");
    std::fs::write(data.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    let lines = [
        "nothing here".to_owned(),
        // A tailnet node and a telephone; the cast's block and a fictional number are not.
        concat!("host = \"100.", "101.102.103\"").to_owned(),
        concat!("call +961 ", "3 123 456").to_owned(),
        "host = \"100.64.0.9\", or (555) 555-0100".to_owned(),
        // A term with an accent the list does not write.
        "a note about zzpérson".to_owned(),
    ];
    std::fs::write(tree.join("leaks.txt"), lines.join("\n") + "\n").expect("leaks");
    // Two pictures: one the manifest lists with its digest, and one it lists by another's.
    std::fs::create_dir_all(tree.join("scripts")).expect("scripts dir");
    let (listed, changed): (&[u8], &[u8]) = (b"\x89PNG\0\0listed\0", b"\x89PNG\0\0changed\0");
    std::fs::write(tree.join("listed.png"), listed).expect("a picture");
    std::fs::write(tree.join("changed.png"), changed).expect("a picture");
    let digest = |bytes: &[u8]| {
        use sha2::Digest as _;
        hex::encode(sha2::Sha256::digest(bytes))
    };
    std::fs::write(
        tree.join("scripts/public-admitted.sha256"),
        format!(
            "# the manifest\n{}  listed.png\n{}  changed.png\n",
            digest(listed),
            digest(listed)
        ),
    )
    .expect("the manifest");
    let instance = data.join("instance.toml");
    let all = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    assert_eq!(
        String::from_utf8_lossy(&all.stdout)
            .lines()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "changed.png:0",
            "leaks.txt:2",
            "leaks.txt:3",
            "leaks.txt:5",
        ]),
        "each leak, and only the leaks"
    );
}

/// A matcher that fails is a check that did not run, never a clean tree: an older binary
/// without the verb, or one that fails part way (here on the files, after the paths), exits 2
/// and prints no verdict, whatever the tree holds.
#[test]
fn public_tree_guard_is_not_clean_when_its_matcher_fails() {
    let scratch = Scratch::new("public-tree-matcher-fails");
    let (data, tree) = (scratch.join("data"), scratch.join("tree"));
    std::fs::create_dir_all(&data).expect("data dir");
    std::fs::create_dir_all(&tree).expect("tree dir");
    std::fs::write(data.join("instance.toml"), "repo = \"/srv/code\"\n").expect("instance");
    std::fs::create_dir_all(data.join("local")).expect("the box-local tier");
    std::fs::write(data.join("local/personal-terms.txt"), "zzperson\n").expect("terms");
    std::fs::write(data.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    std::fs::write(tree.join("note.txt"), "a note about zzperson\n").expect("a leak");
    std::fs::write(tree.join("picture.png"), b"\x89PNG\0\0pixels\0").expect("a picture");
    let real = binary::path();
    for (name, body) in [
        (
            "old",
            "#!/bin/sh\necho \"error: unrecognized subcommand 'leaks'\" >&2\nexit 2\n".to_owned(),
        ),
        (
            "partway",
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in --paths|instance) exec {real} \"$@\";; esac; done\nexit 1\n"
            ),
        ),
    ] {
        let bin = suzatary_testkit::fake_program(&scratch, name, &body);
        let out = Command::new(public_tree_script())
            .args([Path::new("--check"), &tree])
            .env(suzatary_cli::instance::ENV, data.join("instance.toml"))
            .env("SUZATARY_BIN", &bin)
            .output()
            .expect("public-tree.sh runs");
        assert_eq!(out.status.code(), Some(2), "{name}: {}", stderr(&out));
        assert!(out.stdout.is_empty(), "{name}: no verdict is printed");
        assert!(
            stderr(&out).contains("did not run whole"),
            "{name}: {}",
            stderr(&out)
        );
    }
}

/// A path holding a newline is refused before anything reads it, named escaped: a reader of
/// lines would see two paths, and a term split across them would pass.
#[test]
fn public_tree_guard_refuses_a_path_holding_a_newline() {
    let scratch = Scratch::new("public-tree-newline-path");
    let (data, tree) = (scratch.join("data"), scratch.join("tree"));
    std::fs::create_dir_all(&data).expect("data dir");
    std::fs::create_dir_all(&tree).expect("tree dir");
    std::fs::write(data.join("instance.toml"), "repo = \"/srv/code\"\n").expect("instance");
    std::fs::create_dir_all(data.join("local")).expect("the box-local tier");
    std::fs::write(data.join("local/personal-terms.txt"), "zzfirst zzlast\n").expect("terms");
    std::fs::write(data.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    std::fs::write(tree.join("zzfirst\nzzlast.md"), "nothing here\n").expect("a file");
    let out = public_tree(
        &[Path::new("--check"), &tree],
        Some(&data.join("instance.toml")),
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(out.stdout.is_empty(), "no verdict is printed");
    assert!(
        stderr(&out).contains("control character") && stderr(&out).contains("zzfirst\\nzzlast.md"),
        "{}",
        stderr(&out)
    );
}

/// The public scan reads every file a range adds, a name git would quote included: `zoë.md`
/// holding a term is a hit, never a file skipped.
#[test]
fn public_scan_reads_a_file_whose_name_git_would_quote() {
    let scratch = Scratch::new("public-scan-quoted");
    let (data, scanned) = (scratch.join("data"), scratch.join("repo"));
    std::fs::create_dir_all(data.join("local")).expect("the box-local tier");
    std::fs::write(data.join("instance.toml"), "repo = \"/srv/code\"\n").expect("instance");
    std::fs::write(data.join("local/personal-terms.txt"), "zzperson\n").expect("terms");
    std::fs::write(data.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    std::fs::create_dir_all(&scanned).expect("repo dir");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&scanned)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", stderr(&out));
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(scanned.join("start.md"), "nothing\n").expect("a file");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "start"]);
    git(&["tag", "base"]);
    std::fs::write(scanned.join("zoë.md"), "a note about zzperson\n").expect("a file");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "a note"]);
    let out = Command::new(repo().join("scripts/public-scan.sh"))
        .arg(&scanned)
        .arg("base..main")
        .env(suzatary_cli::instance::ENV, data.join("instance.toml"))
        .env("SUZATARY_BIN", binary::path())
        .output()
        .expect("public-scan.sh runs");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("zoë.md:1"), "{}", stderr(&out));

    // A scan that could not run is a 2, never a hit's 1: no instance to find the lists by, and
    // a pull request it cannot read (this repository has no origin to name it by).
    let scan = |instance: &Path, pr: Option<&str>| {
        let mut command = Command::new(repo().join("scripts/public-scan.sh"));
        command.arg(&scanned).arg("base..main").args(pr);
        command
            .env(suzatary_cli::instance::ENV, instance)
            .env("SUZATARY_BIN", binary::path())
            .output()
            .expect("public-scan.sh runs")
    };
    for (what, out) in [
        ("no instance", scan(&scratch.join("nowhere.toml"), None)),
        (
            "an unread pull request",
            scan(&data.join("instance.toml"), Some("7")),
        ),
    ] {
        assert_eq!(out.status.code(), Some(2), "{what}: {}", stderr(&out));
    }
}

/// A front page's relative links and pictures name files of the tree: a missing one, or one
/// outside the tree, is a hit at its line, an external link or an anchor never is, and
/// `--lists` reads no link.
#[test]
fn public_tree_guard_finds_each_dangling_link() {
    let scratch = Scratch::new("public-tree-links");
    let tree = scratch.join("tree");
    std::fs::create_dir_all(tree.join("docs/images")).expect("tree dirs");
    std::fs::write(tree.join("docs/images/shot.png"), "a picture\n").expect("picture writes");
    std::fs::write(tree.join("SECURITY.md"), "# Security\n").expect("page writes");
    std::fs::write(
        tree.join("README.md"),
        concat!(
            "<img src=\"docs/images/shot.png\"> and ![a shot](docs/images/shot.png)\n",
            "<img src=\"docs/images/gone.png\">\n",
            "[outside](../elsewhere.md)\n",
            "[security](SECURITY.md#reporting), [web](https://example.com/x), [top](#top)\n",
        ),
    )
    .expect("README writes");
    std::fs::write(
        tree.join("docs/README.md"),
        "[back](../README.md) and [gone](GONE.md)\n",
    )
    .expect("docs README writes");

    let nowhere = scratch.join("no-instance.toml");
    let out = public_tree(&[Path::new("--check"), &tree], Some(&nowhere));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .collect::<Vec<_>>(),
        ["README.md:2", "README.md:3", "docs/README.md:1"],
        "each dangling link, and only those"
    );
    let lists = public_tree(
        &[Path::new("--check"), Path::new("--lists"), &tree],
        Some(&nowhere),
    );
    assert!(
        stderr(&out).contains("README.md:2 links to nothing"),
        "{}",
        stderr(&out)
    );
    assert!(lists.status.success(), "--lists reads no link");
}

/// The public tree is a function of a commit's tree: of a throwaway repository holding the
/// script, the tree of the commit named is written whatever later commits and the working tree
/// hold, its changelog `unreleased` where the changelog dates no release, never dated by the
/// clock or the commit; and `--strict`, the check a publication passes, fails where a rule would
/// skip.
#[test]
fn public_tree_is_a_function_of_its_commit() {
    let scratch = Scratch::new("public-tree-of");
    let source = scratch.join("source");
    let write = |rel: &str, content: &str| {
        let path = source.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(path, content).expect("a file writes");
    };
    std::fs::create_dir_all(source.join("scripts")).expect("scripts");
    for file in ["public-changelog.md", "secret-rules.toml"] {
        std::fs::copy(
            repo().join("scripts").join(file),
            source.join("scripts").join(file),
        )
        .expect("the file copies");
    }
    suzatary_testkit::copy_program(
        &public_tree_script(),
        &source.join("scripts/public-tree.sh"),
    );
    write(
        "app/Cargo.toml",
        "[workspace.package]\nversion = \"9.8.7\"\n",
    );
    write("CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n");
    write("private/notes.md", "the private history\n");
    write(
        ".gitattributes",
        "/private/** export-ignore\n/CHANGELOG.md export-ignore\n",
    );
    write("a.txt", "the first\n");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", "user.name=cast", "-c", "user.email=cast@example.test"])
            .args(args)
            .current_dir(&source)
            .env("GIT_COMMITTER_DATE", "2001-02-03T23:30:00+05:00")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "{}", stderr(&out));
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "the first"]);
    let first = git(&["rev-parse", "HEAD"]);
    write("a.txt", "the second\n");
    git(&["commit", "-q", "-am", "the second"]);
    write("a.txt", "not committed\n");

    let tree = scratch.join("tree");
    let made = Command::new(source.join("scripts/public-tree.sh"))
        .arg(&tree)
        .arg(&first)
        .output()
        .expect("public-tree.sh runs");
    assert!(made.status.success(), "{}", stderr(&made));
    let read = |rel: &str| std::fs::read_to_string(tree.join(rel)).unwrap_or_default();
    assert_eq!(read("a.txt"), "the first\n", "the commit named, not HEAD");
    assert!(!tree.join("private").exists(), "the private history stays");
    assert!(
        read("CHANGELOG.md").contains("## [9.8.7] - unreleased"),
        "dated by nothing outside the tree: {}",
        read("CHANGELOG.md")
    );

    let nowhere = scratch.join("no-instance.toml");
    let check = |strict: bool| {
        let mut command = Command::new(source.join("scripts/public-tree.sh"));
        command.arg("--check");
        if strict {
            command.arg("--strict");
        }
        command
            .arg(&tree)
            .env(suzatary_cli::instance::ENV, &nowhere)
            .env("SUZATARY_BIN", binary::path())
            .output()
            .expect("public-tree.sh runs")
    };
    assert!(check(false).status.success(), "{}", stderr(&check(false)));
    let strict = check(true);
    assert_eq!(
        strict.status.code(),
        Some(2),
        "a skipped rule is a check that did not run whole, never a hit"
    );
    assert!(stderr(&strict).contains("--strict"), "{}", stderr(&strict));
    std::fs::write(
        tree.join("b.txt"),
        format!("write to someone{}mail.invalid.org\n", '@'),
    )
    .expect("a leaking file");
    assert_eq!(
        check(false).status.code(),
        Some(1),
        "a hit, and only a hit, is 1"
    );
}

/// Bytes no test holds as text: `n` letters and digits of a xorshift from `seed`, so a key built
/// from them is a scanner's finding and never a key anyone issued.
fn noise(seed: u32, n: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut state = seed;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            char::from(ALPHABET[state as usize % ALPHABET.len()])
        })
        .collect()
}

/// A throwaway deployment's instance file, its lists holding one term each.
fn throwaway_deployment(scratch: &Scratch) -> PathBuf {
    let data = scratch.join("data");
    std::fs::create_dir_all(data.join("local")).expect("the box-local tier");
    std::fs::write(data.join("instance.toml"), "repo = \"/srv/code\"\n").expect("instance");
    std::fs::write(data.join("local/personal-terms.txt"), "zzperson\n").expect("terms");
    std::fs::write(data.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    data.join("instance.toml")
}

/// The secret scanner is gitleaks with its maintained rules: a GitHub token, an AWS key pair, a
/// PEM private key and a token wrapped in base64 are each a hit at their line, printed by file
/// and line alone. A finding the tree's `.gitleaksignore` names by its fingerprint passes, at
/// that path and no other, once the manifest admits the file's bytes too.
#[test]
fn public_tree_guard_finds_secrets_with_gitleaks() {
    use base64::Engine as _;
    let scratch = Scratch::new("public-tree-gitleaks");
    let instance = throwaway_deployment(&scratch);
    let tree = scratch.join("tree");
    for dir in ["fixtures", "elsewhere", "scripts"] {
        std::fs::create_dir_all(tree.join(dir)).expect("tree dirs");
    }
    let token = format!("{}{}", concat!("gh", "p_"), noise(0x2545_F491, 36));
    std::fs::write(tree.join("token.txt"), format!("token = {token}\n")).expect("a token");
    std::fs::write(
        tree.join("aws.txt"),
        format!(
            "aws_access_key_id = {}\naws_secret_access_key = {}\n",
            // gitleaks's own test data.
            concat!("AKIA", "LALEMEL33243OLIA"),
            noise(0x1234_5678, 40)
        ),
    )
    .expect("an AWS key pair");
    std::fs::write(
        tree.join("wrapped.txt"),
        format!(
            "payload: {}\n",
            base64::engine::general_purpose::STANDARD.encode(format!("token = {token}"))
        ),
    )
    .expect("a wrapped token");
    let key = std::fs::read(repo().join("app/io/tests/fixtures/bridge-key.pem")).expect("a key");
    std::fs::write(tree.join("fixtures/key.pem"), &key).expect("the admitted key");
    std::fs::write(tree.join("fixtures/unlisted.pem"), &key).expect("a key not listed");
    std::fs::write(tree.join("elsewhere/key.pem"), &key).expect("the same key elsewhere");
    std::fs::write(
        tree.join(".gitleaksignore"),
        "# vouched for\nfixtures/key.pem:private-key:1\nfixtures/unlisted.pem:private-key:1\n",
    )
    .expect("the admission");
    let digest = {
        use sha2::Digest as _;
        hex::encode(sha2::Sha256::digest(&key))
    };
    std::fs::write(
        tree.join("scripts/public-admitted.sha256"),
        format!("{digest}  fixtures/key.pem\n"),
    )
    .expect("the manifest");

    let out = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "aws.txt:1",
            "aws.txt:2",
            "elsewhere/key.pem:0",
            "elsewhere/key.pem:1",
            "fixtures/unlisted.pem:0",
            "token.txt:1",
            "wrapped.txt:1",
        ]),
        "each secret, the admitted key at its own path alone (elsewhere its PEM body is opaque \
         too), and its bytes admitted too"
    );
    let printed = format!("{}{}", String::from_utf8_lossy(&out.stdout), stderr(&out));
    assert!(!printed.contains(&token[4..]), "the guard printed a token");
}

/// No path hides a secret from the scanner: gitleaks's own default allowlist passes over a
/// picture's text, lock files, `node_modules/`, a PDF's and a `.bin`'s text and any path naming
/// its config, so the guard hands it every file under a neutral name. A token in each is a hit.
#[test]
fn public_tree_guard_finds_a_secret_whatever_its_path() {
    let scratch = Scratch::new("public-tree-gitleaks-paths");
    let instance = throwaway_deployment(&scratch);
    let tree = scratch.join("tree");
    let token = format!("{}{}", concat!("gh", "p_"), noise(0x00C0_FFEE, 36));
    let paths = [
        "logo.svg",
        "web/static/icon.svg",
        "yarn.lock",
        "package-lock.json",
        "go.sum",
        "go.mod",
        "node_modules/x/i.js",
        "vendor/x/i.go",
        "scripts/secret-rules.toml",
        "scripts/gitleaks.toml",
        "notes.pdf",
        "blob.bin",
    ];
    for path in paths {
        let file = tree.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
        std::fs::write(file, format!("token = {token}\n")).expect("a token");
    }
    let out = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let hits = String::from_utf8_lossy(&out.stdout);
    for path in paths {
        assert!(hits.contains(&format!("{path}:1")), "{path}: {hits}");
    }
    assert!(
        stderr(&out).contains("gitleaks rule github-pat"),
        "{}",
        stderr(&out)
    );
}

/// A `.gitleaksignore` line with a space at either end is a check that did not run: gitleaks
/// trims it and would admit the finding, while the manifest's rule would name no file.
#[test]
fn public_tree_guard_refuses_an_untrimmed_admission() {
    let scratch = Scratch::new("public-tree-untrimmed");
    let instance = throwaway_deployment(&scratch);
    let tree = scratch.join("tree");
    std::fs::create_dir_all(&tree).expect("tree dir");
    let token = format!("{}{}", concat!("gh", "p_"), noise(0x5EED_0001, 36));
    std::fs::write(tree.join("a.txt"), format!("nothing\ntoken = {token}\n")).expect("a token");
    std::fs::write(tree.join(".gitleaksignore"), "  a.txt:github-pat:2\n").expect("an admission");
    let out = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(out.stdout.is_empty(), "no verdict is printed");
}

/// A go.sum line is allowed only in its own shape: a module, a version of digits and words, an
/// `h1:` checksum. A key standing where the version stands is a secret like any other.
#[test]
fn public_tree_guard_takes_a_go_sum_line_only_in_its_shape() {
    use base64::Engine as _;
    use sha2::Digest as _;
    let scratch = Scratch::new("public-tree-go-sum");
    let instance = throwaway_deployment(&scratch);
    let tree = scratch.join("tree");
    std::fs::create_dir_all(&tree).expect("tree dir");
    let checksum =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(b"a module"));
    std::fs::write(
        tree.join("go.sum"),
        format!(
            "example.com/mod v1.2.3+incompatible/go.mod h1:{checksum}\n\
             example.com/mod v{}={} h1:{checksum}\n",
            concat!("api", "_key"),
            noise(0x00D0_0D1E, 32)
        ),
    )
    .expect("a go.sum");
    let out = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "go.sum:2");
}

/// A sealed slot passes both checks in its exact shape: the matcher reads its box as
/// ciphertext, and the scanner reads the line with the slot a placeholder. A key on the same
/// line as a slot, or on another, is still found, and so is a slot of a near-miss shape.
#[test]
fn public_tree_guard_reads_a_sealed_slot_as_ciphertext() {
    use base64::Engine as _;
    use sha2::Digest as _;
    let scratch = Scratch::new("public-tree-sealed");
    let instance = throwaway_deployment(&scratch);
    let tree = scratch.join("tree");
    std::fs::create_dir_all(&tree).expect("tree dir");
    let random = |n: usize| -> Vec<u8> {
        (0u32..)
            .flat_map(|i| sha2::Sha256::digest(i.to_le_bytes()).to_vec())
            .take(n)
            .collect()
    };
    let name = hex::encode(sha2::Sha256::digest(b"a key's name"));
    let slot = |name: &str, bytes: &[u8]| {
        format!(
            "Locked(key='{name}', box='{}')",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    };
    let object = |slot: &str| format!("Change(deps=(), event=Opened(words=({slot},)))");
    let sealed = object(&slot(&name, &random(256 + 16)));
    let key = format!(
        "{} = '{}'",
        concat!("secret", "_key"),
        noise(0x5EA1_ED00, 40)
    );
    let files = [
        ("object.py", sealed.clone()),
        (
            "vectors.json",
            format!("{{\n  \"text\": \"{sealed}\"\n}}\n"),
        ),
        ("config.py", format!("{key}\n")),
        ("same-line.py", format!("{sealed}, {key}\n")),
        (
            "near.py",
            format!(
                "{}\n",
                object(&slot(&name.to_uppercase(), &random(256 + 16)))
            ),
        ),
    ];
    for (path, content) in &files {
        std::fs::write(tree.join(path), content).expect("a file");
    }
    let out = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let hits = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    for hit in ["config.py:1", "same-line.py:1", "near.py:1"] {
        assert!(hits.contains(hit), "{hit}: {hits:?}");
    }
    assert!(
        hits.iter()
            .all(|hit| !hit.starts_with("object.py") && !hit.starts_with("vectors.json")),
        "{hits:?}"
    );
}

/// A secret scanner that could not run is a check that did not run: a gitleaks that fails, here
/// with 1 as its fatal errors do, makes the guard exit 2 with no verdict.
#[test]
fn public_tree_guard_is_not_clean_when_gitleaks_fails() {
    let scratch = Scratch::new("public-tree-gitleaks-fails");
    let instance = throwaway_deployment(&scratch);
    let tree = scratch.join("tree");
    std::fs::create_dir_all(&tree).expect("tree dir");
    std::fs::write(tree.join("note.txt"), "nothing here\n").expect("a file");
    let stub = scratch.join("bin");
    std::fs::create_dir_all(&stub).expect("a stub dir");
    let _ = suzatary_testkit::fake_program(
        &stub,
        "gitleaks",
        "#!/bin/sh\necho 'FTL broken' >&2\nexit 1\n",
    );
    let path = format!(
        "{}:{}",
        stub.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(public_tree_script())
        .args([Path::new("--check"), &tree])
        .env(suzatary_cli::instance::ENV, &instance)
        .env("SUZATARY_BIN", binary::path())
        .env("PATH", path)
        .output()
        .expect("public-tree.sh runs");
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(out.stdout.is_empty(), "no verdict is printed");
}

/// Opaque content is admitted, not decoded: a file the matcher cannot read as words (a binary,
/// a run of base64) is a hit until the manifest lists its path with the digest of its bytes,
/// at that path alone; a listed file whose bytes changed is a hit again; what an admitted file
/// holds encoded is not read, and its plain lines still are.
#[test]
fn public_tree_guard_admits_opaque_files_by_digest() {
    use base64::Engine as _;
    let scratch = Scratch::new("public-tree-opaque");
    let instance = throwaway_deployment(&scratch);
    let tree = scratch.join("tree");
    std::fs::create_dir_all(tree.join("scripts")).expect("tree dirs");
    let blob = |text: &str| {
        format!(
            "a note\npayload: {}\n",
            base64::engine::general_purpose::STANDARD.encode(text.repeat(12))
        )
    };
    let listed = blob("a note about zzperson, ");
    let plain = format!("{listed}and zzcontext in the clear\n");
    for (name, bytes) in [
        ("listed.txt", listed.as_bytes()),
        ("plain.txt", plain.as_bytes()),
        ("moved.txt", listed.as_bytes()),
        ("fresh.txt", listed.as_bytes()),
        ("picture.png", b"\x89PNG\0\0zzperson\0".as_slice()),
    ] {
        std::fs::write(tree.join(name), bytes).expect("a file");
    }
    let digest = |bytes: &[u8]| {
        use sha2::Digest as _;
        hex::encode(sha2::Sha256::digest(bytes))
    };
    std::fs::write(
        tree.join("scripts/public-admitted.sha256"),
        format!(
            "# admitted\n{0}  listed.txt\n{1}  plain.txt\n{2}  moved.txt\n{3}  picture.png\n",
            digest(listed.as_bytes()),
            digest(plain.as_bytes()),
            digest(blob("other bytes, ").as_bytes()),
            digest(b"\x89PNG\0\0zzperson\0"),
        ),
    )
    .expect("the manifest");
    let out = public_tree(&[Path::new("--check"), &tree], Some(&instance));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["fresh.txt:0", "moved.txt:0", "plain.txt:3"]),
        "the unlisted blob, the one whose bytes moved, and the admitted file's plain line"
    );
    for said in [
        "fresh.txt holds base64 or hex; once you have looked at these bytes",
        "sha256sum fresh.txt >> scripts/public-admitted.sha256",
        "scripts/train.sh publish --admit fresh.txt",
        "plain.txt:3 names a term of the lists",
    ] {
        assert!(stderr(&out).contains(said), "{said}: {}", stderr(&out));
    }
}

/// The Prodrome's scan is the public tree's guard over a range: a secret gitleaks finds and an
/// opaque file no one admitted are hits; `--admit` from a script is refused, and at a terminal
/// it admits the file's bytes at the tip for that scan.
#[test]
fn public_scan_shares_the_guard_and_admits_at_a_terminal() {
    use base64::Engine as _;
    let scratch = Scratch::new("public-scan-guard");
    let instance = throwaway_deployment(&scratch);
    let scanned = scratch.join("repo");
    std::fs::create_dir_all(&scanned).expect("repo dir");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&scanned)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", stderr(&out));
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(scanned.join("start.md"), "nothing\n").expect("a file");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "start"]);
    git(&["tag", "base"]);
    let blob = base64::engine::general_purpose::STANDARD.encode("a vector of bytes, ".repeat(16));
    std::fs::write(scanned.join("vector.txt"), format!("{blob}\n")).expect("a blob");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "a vector"]);
    git(&["tag", "blob"]);
    std::fs::write(
        scanned.join("token.txt"),
        format!(
            "token = {}{}\n",
            concat!("gh", "p_"),
            noise(0x0BAD_5EED, 36)
        ),
    )
    .expect("a token");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "a token"]);
    let scan = |range: &str, admit: &[&str]| {
        let mut command = Command::new(repo().join("scripts/public-scan.sh"));
        for path in admit {
            command.args(["--admit", path]);
        }
        command
            .arg(&scanned)
            .arg(range)
            .env(suzatary_cli::instance::ENV, &instance)
            .env("SUZATARY_BIN", binary::path())
            .stdin(std::process::Stdio::null());
        command
    };
    let out = scan("base..main", &[])
        .output()
        .expect("public-scan.sh runs");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    for hit in ["vector.txt:0", "token.txt:1"] {
        assert!(stderr(&out).contains(hit), "{hit}: {}", stderr(&out));
    }
    let scripted = scan("base..blob", &["vector.txt"])
        .output()
        .expect("public-scan.sh runs");
    assert_eq!(scripted.status.code(), Some(2), "{}", stderr(&scripted));
    assert!(
        stderr(&scripted).contains("at a terminal"),
        "{}",
        stderr(&scripted)
    );
    let command = scan("base..blob", &["vector.txt"]);
    let line = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|word| format!("'{}'", word.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    let at_a_terminal = Command::new("script")
        .args(["-qec", &line, "/dev/null"])
        .env(suzatary_cli::instance::ENV, &instance)
        .env("SUZATARY_BIN", binary::path())
        .output()
        .expect("script runs");
    assert!(
        at_a_terminal.status.success(),
        "{}{}",
        String::from_utf8_lossy(&at_a_terminal.stdout),
        stderr(&at_a_terminal)
    );
}

/// A push publishes every commit of a range, so the scan reads each one: a term one commit adds
/// and the next removes is a hit, though the tip holds none; and a commit's own file is never
/// overwritten by the scan's copy of its message.
#[test]
fn public_scan_reads_every_commit_of_the_range() {
    let scratch = Scratch::new("public-scan-every-commit");
    let instance = throwaway_deployment(&scratch);
    let scanned = scratch.join("repo");
    std::fs::create_dir_all(&scanned).expect("repo dir");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&scanned)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", stderr(&out));
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(scanned.join("note.md"), "nothing\n").expect("a file");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "start"]);
    git(&["tag", "base"]);
    std::fs::write(scanned.join("note.md"), "a note about zzperson\n").expect("a term");
    git(&["commit", "-q", "-am", "a note"]);
    std::fs::write(scanned.join("note.md"), "nothing again\n").expect("no term");
    git(&["commit", "-q", "-am", "and gone"]);
    // A file at the path the scan once wrote a commit's message to is read as the file it is.
    std::fs::create_dir_all(scanned.join(".public-scan")).expect("a dir");
    std::fs::write(
        scanned.join(".public-scan/message"),
        "another note about zzperson\n",
    )
    .expect("a term");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "a quiet message"]);
    let out = Command::new(repo().join("scripts/public-scan.sh"))
        .arg(&scanned)
        .arg("base..main")
        .env(suzatary_cli::instance::ENV, &instance)
        .env("SUZATARY_BIN", binary::path())
        .output()
        .expect("public-scan.sh runs");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("note.md:1"), "{}", stderr(&out));
    assert!(
        stderr(&out).contains(".public-scan/message:1"),
        "{}",
        stderr(&out)
    );
}

/// A commit message is words a person wrote, never admitted: an opaque one is a hit, and
/// `--admit message` names no file of the range, even at a terminal.
#[test]
fn public_scan_never_admits_a_message() {
    use base64::Engine as _;
    let scratch = Scratch::new("public-scan-message");
    let instance = throwaway_deployment(&scratch);
    let scanned = scratch.join("repo");
    std::fs::create_dir_all(&scanned).expect("repo dir");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&scanned)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", stderr(&out));
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(scanned.join("note.md"), "nothing\n").expect("a file");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "start"]);
    git(&["tag", "base"]);
    std::fs::write(scanned.join("note.md"), "nothing new\n").expect("a file");
    let blob = base64::engine::general_purpose::STANDARD.encode("a message, encoded. ".repeat(16));
    git(&["commit", "-q", "-am", &blob]);
    let command = |admit: bool| {
        let mut command = Command::new(repo().join("scripts/public-scan.sh"));
        if admit {
            command.args(["--admit", "message"]);
        }
        command
            .arg(&scanned)
            .arg("base..main")
            .env(suzatary_cli::instance::ENV, &instance)
            .env("SUZATARY_BIN", binary::path());
        command
    };
    let out = command(false).output().expect("public-scan.sh runs");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("message:0"), "{}", stderr(&out));
    assert!(stderr(&out).contains("never admitted"), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains("--admit message"),
        "{}",
        stderr(&out)
    );
    let admit = command(true);
    let line = std::iter::once(admit.get_program())
        .chain(admit.get_args())
        .map(|word| format!("'{}'", word.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    let at_a_terminal = Command::new("script")
        .args(["-qec", &line, "/dev/null"])
        .env(suzatary_cli::instance::ENV, &instance)
        .env("SUZATARY_BIN", binary::path())
        .output()
        .expect("script runs");
    assert!(
        String::from_utf8_lossy(&at_a_terminal.stdout).contains("--admit message names no file"),
        "{}",
        String::from_utf8_lossy(&at_a_terminal.stdout)
    );
}

/// The law of the guard's passes: a check that reads only the files whose bytes or rules moved
/// since they passed gives the full check's verdict on the same tree, after a file changes,
/// after a list gains a term, after the manifest changes and after the matcher changes; a second
/// run reads no file that passed; and a hit is never kept as a pass.
#[test]
#[allow(clippy::too_many_lines)]
fn public_tree_guard_incremental_and_full_checks_agree() {
    let scratch = Scratch::new("public-tree-passes");
    let (data, tree, passes) = (
        scratch.join("data"),
        scratch.join("tree"),
        scratch.join("passes"),
    );
    std::fs::create_dir_all(data.join("local")).expect("the box-local tier");
    std::fs::create_dir_all(tree.join("scripts")).expect("tree dir");
    std::fs::write(data.join("instance.toml"), "repo = \"/srv/code\"\n").expect("instance");
    std::fs::write(data.join("local/personal-terms.txt"), "zzperson\n").expect("terms");
    std::fs::write(data.join("local/context-terms.txt"), "zzcontext\n").expect("context");
    for n in 0..20 {
        std::fs::write(
            tree.join(format!("clean{n}.txt")),
            format!("line {n} of zzplain\n"),
        )
        .expect("a clean file");
    }
    std::fs::write(tree.join("note.txt"), "a note\n").expect("a note");
    let picture: &[u8] = b"\x89PNG\0\0pixels\0";
    std::fs::write(tree.join("picture.png"), picture).expect("a picture");
    let digest = |bytes: &[u8]| {
        use sha2::Digest as _;
        hex::encode(sha2::Sha256::digest(bytes))
    };
    let manifest = tree.join("scripts/public-admitted.sha256");
    std::fs::write(&manifest, format!("{}  picture.png\n", digest(picture))).expect("manifest");
    let instance = data.join("instance.toml");
    let check = |full: bool, bin: &Path| {
        let mut args = vec![Path::new("--check")];
        if full {
            args.push(Path::new("--full"));
        }
        args.push(&tree);
        let out = Command::new(public_tree_script())
            .args(&args)
            .env(suzatary_cli::instance::ENV, &instance)
            .env("SUZATARY_BIN", bin)
            .env("SUZATARY_GUARD_PASSES", &passes)
            // A full gate sets it for every check it runs; this test chooses each check's mode.
            .env_remove("SUZATARY_GUARD_FULL")
            .output()
            .expect("public-tree.sh runs");
        let mut hits: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .collect();
        hits.sort();
        (out.status.code(), hits, stderr(&out))
    };
    let real = PathBuf::from(binary::path());
    let agree = |why: &str, hits: &[&str]| {
        let incremental = check(false, &real);
        let full = check(true, &real);
        assert_eq!(
            (incremental.0, &incremental.1),
            (full.0, &full.1),
            "{why}: the incremental check and the full one disagree: {}",
            incremental.2
        );
        assert_eq!(incremental.1, hits, "{why}");
        incremental.2
    };

    let first = agree("a first check", &[]);
    assert!(first.contains("reading 23 files; 0 passed"), "{first}");
    let second = check(false, &real);
    assert_eq!(second.0, Some(0));
    assert!(
        second.2.contains("reading 0 files; 23 passed"),
        "{}",
        second.2
    );

    std::fs::write(tree.join("note.txt"), "a note about zzperson\n").expect("a leak");
    agree("a file changes", &["note.txt:1"]);
    let again = check(false, &real);
    assert_eq!(again.1, ["note.txt:1"], "a hit is never kept as a pass");
    assert!(
        again.2.contains("reading 1 files; 22 passed"),
        "{}",
        again.2
    );
    std::fs::write(tree.join("note.txt"), "a note\n").expect("clean again");

    std::fs::write(data.join("local/personal-terms.txt"), "zzperson\nzzplain\n").expect("terms");
    let hits: Vec<String> = (0..20).map(|n| format!("clean{n}.txt:1")).collect();
    let mut sorted: Vec<&str> = hits.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    agree("a list gains a term an unchanged file holds", &sorted);
    std::fs::write(data.join("local/personal-terms.txt"), "zzperson\n").expect("terms");

    std::fs::write(&manifest, format!("{}  picture.png\n", digest(b"other"))).expect("manifest");
    agree(
        "the manifest no longer admits the picture",
        &["picture.png:0"],
    );

    // Another matcher is another rule: a program that answers as the real one, with other bytes.
    let other = scratch.join("matcher");
    suzatary_testkit::write_program(
        &other,
        format!(
            "#!/bin/sh\n# another matcher\nexec {} \"$@\"\n",
            real.display()
        )
        .as_bytes(),
    );
    let moved = check(false, &other);
    assert_eq!(
        (moved.0, moved.1),
        (Some(1), vec!["picture.png:0".to_owned()])
    );
    assert!(
        moved.2.contains("reading 23 files; 0 passed"),
        "{}",
        moved.2
    );
}
