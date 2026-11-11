//! The secretary in the owner's repositories, over a real server with a scripted model and a
//! local repository: a read of a repository the token does not reach is refused, and a patch or
//! a file leaves the box only on the owner's tap, as its card showed it.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_base::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment([ada()], [(ada(), common::actor("ada"))])
}

fn exchange(base: &str, said: &str) -> serde_json::Value {
    common::exchange(base, "The CV", said).1
}

fn answer(payload: &serde_json::Value, verb: &str) -> (String, bool) {
    let turn = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .find(|turn| turn["who"] == "verb" && turn["verb"] == verb)
        .unwrap_or_else(|| panic!("no {verb} turn: {payload}"));
    (
        turn["text"].as_str().unwrap_or_default().to_owned(),
        turn["outcome"]["is"] == "refused",
    )
}

#[test]
fn a_repository_the_token_does_not_reach_is_refused_by_name() {
    let root = common::scratch("repos-unknown");
    let fakes = Fakes::new("repos-unknown")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .repos(common::local_repos(&root))
        .model(FakeModel::saying([
            r#"{"say": "Looking.", "calls": [
                {"verb": "repo_read", "args": {"repo": "someorg/private", "path": "README.md"}}
            ]}"#,
            r#"{"say": "I cannot reach that one."}"#,
        ]));
    let base = common::start(Arc::new(fakes), deployment(), Vec::new());
    let payload = exchange(&base, "read someorg's readme");
    let (text, refused) = answer(&payload, "repo_read");
    assert!(refused && text.contains("someorg/private"), "{text}");
}

const RETITLE: &str = "--- a/cv.typ\\n+++ b/cv.typ\\n@@ -1,2 +1,2 @@\\n = Ada Quill\\n-Research engineer\\n+Principal investigator\\n";

fn propose(patch: &str) -> String {
    format!(
        r#"{{"say": "Here is the change.", "calls": [{{"verb": "propose_patch", "args": {{
            "repo": "bmabsout/cv",
            "patch": "{patch}",
            "message": "Retitle the CV",
            "why": "He is a PI now.",
            "sources": "todo:life/permitfiling"
        }}}}]}}"#
    )
}

/// The tap opens a pull request: exactly the stored diff on a branch of its own, named by the
/// proposal, with nothing on the remote before the tap and the default branch untouched after.
#[test]
fn a_tapped_patch_opens_a_pull_request_and_nothing_before() {
    let root = common::scratch("repos-publish");
    let repos = common::local_repos(&root);
    let fakes = Arc::new(
        Fakes::new("repos-publish")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .repos(Arc::clone(&repos))
            .model(FakeModel::saying([
                propose(RETITLE).leak(),
                r#"{"say": "Proposed."}"#,
                r#"{"say": "Opened."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .real_acts(
            "repos-publish",
            "her@study.test",
            common::again(&fakes.conversations),
            common::ScriptedOauth::new(Vec::new()),
        )
        .publishing(repos)
        .start()
        .base;
    let payload = exchange(&base, "update my title");
    let id = payload["id"].as_str().expect("an id").to_owned();
    let proposal = &payload["proposals"][0];
    assert_eq!(proposal["act"]["kind"], "patch", "{proposal}");
    assert_eq!(proposal["card"]["answers"][0]["verb"], "open");
    assert!(
        proposal["card"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("+Principal investigator")
    );
    let remote = root.join("remotes/bmabsout/cv.git");
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(args)
            .output()
            .expect("git runs");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let main = git(&["rev-parse", "main"]);
    assert_eq!(
        git(&["for-each-ref", "--format=%(refname)"]).trim(),
        "refs/heads/main",
        "proposing pushed nothing"
    );

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    let after = common::body_json(&mut tapped);
    assert_eq!(common::status(&tapped), 200, "{after}");
    let branch = format!("secretary/{id}-0");
    assert!(git(&["show", &format!("{branch}:cv.typ")]).contains("Principal investigator"));
    let message = git(&["log", "-1", "--format=%B", &branch]);
    assert!(message.starts_with("Retitle the CV"), "{message}");
    assert!(message.contains(&format!("proposal:{id}/0")), "{message}");
    assert_eq!(
        git(&["rev-parse", "refs/pull/1/head"]),
        git(&["rev-parse", &branch])
    );
    assert_eq!(
        git(&["rev-parse", "main"]),
        main,
        "the default branch is his to merge into"
    );
    assert!(
        after.to_string().contains("opened pull request #1"),
        "the outcome names the request: {after}"
    );
}

/// The attack: a mail the secretary reads asks it to publish the owner's words in a public
/// repository, buried in a patch too long to read on a phone. After that read, a public patch and
/// its message carry at most `MAX_SHOWN_BYTES` together: the long one is refused before a card,
/// and a short one is filed with its whole diff on the card.
#[test]
fn after_a_mail_read_a_public_patch_is_only_what_its_card_shows_whole() {
    let root = common::scratch("repos-tainted");
    let long = "+a line of the owner's notes\n".repeat(200);
    let long = format!("--- /dev/null\n+++ b/notes.txt\n@@ -0,0 +1,200 @@\n{long}");
    let patch = |patch: &str| {
        serde_json::json!({"say": "", "calls": [{"verb": "propose_patch", "args": {
            "repo": "bmabsout/cv", "patch": patch, "message": "Add notes",
            "why": "The mail asked.", "sources": "mail:a1b2c3d4e5f6"}}]})
        .to_string()
    };
    let fakes = Fakes::new("repos-tainted")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .repos(common::local_repos(&root))
        .model(FakeModel::saying_owned([
            r#"{"say": "", "calls": [{"verb": "search", "args": {"query": "notes"}}]}"#.to_owned(),
            patch(&long),
            patch(&RETITLE.replace("\\n", "\n")),
            r#"{"say": "Proposed."}"#.to_owned(),
        ]));
    let base = common::start(Arc::new(fakes), deployment(), Vec::new());
    let payload = exchange(&base, "update my title");
    let answers: Vec<(String, bool)> = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "verb" && turn["verb"] == "propose_patch")
        .map(|turn| {
            let text = turn["text"].as_str().unwrap_or_default().to_owned();
            (text, turn["outcome"]["is"] == "refused")
        })
        .collect();
    assert_eq!(answers.len(), 2, "{payload}");
    assert!(
        answers[0].1 && answers[0].0.contains("4096 bytes"),
        "{answers:?}"
    );
    assert!(!answers[1].1, "{answers:?}");
    let proposals = payload["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 1, "{payload}");
    let body = proposals[0]["card"]["body"].as_str().unwrap_or_default();
    assert!(body.contains("+Principal investigator") && !body.contains("notes.txt"));
}

/// A patch that does not apply to its base is refused before anything is
/// stored, so no card ever shows one.
#[test]
fn a_patch_that_does_not_apply_is_never_stored() {
    let root = common::scratch("repos-stale");
    let stale =
        "--- a/cv.typ\\n+++ b/cv.typ\\n@@ -1,2 +1,2 @@\\n = Ada Quill\\n-Not what it says\\n+x\\n";
    let fakes = Fakes::new("repos-stale")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .repos(common::local_repos(&root))
        .model(FakeModel::saying([
            propose(stale).leak(),
            r#"{"say": "It no longer applies."}"#,
        ]));
    let base = common::start(Arc::new(fakes), deployment(), Vec::new());
    let payload = exchange(&base, "update my title");
    let (text, refused) = answer(&payload, "propose_patch");
    assert!(refused && text.contains("does not apply"), "{text}");
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty()
    );
}

/// A public repository takes a patch only where its entry says `public = true`: the secretary that
/// writes one has read mail, and a pushed branch is anyone's to read. Refused before a card.
#[test]
fn a_public_repository_whose_entry_does_not_say_so_takes_no_patch() {
    let root = common::scratch("repos-public");
    let _remote = common::local_repos(&root);
    let fakes = Fakes::new("repos-public")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .repos(Arc::new(common::local_mirrors_declaring(
            &root,
            suzatary_core::repos::RepoTarget::default(),
        )))
        .model(FakeModel::saying([
            propose(RETITLE).leak(),
            r#"{"say": "It is public."}"#,
        ]));
    let base = common::start(Arc::new(fakes), deployment(), Vec::new());
    let payload = exchange(&base, "update my title");
    let (text, refused) = answer(&payload, "propose_patch");
    assert!(refused && text.contains("public = true"), "{text}");
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty()
    );
}

fn attaching(attach: &str) -> String {
    format!(
        r#"{{"say": "Here is the CV.", "calls": [{{"verb": "propose_email", "args": {{
            "account": "ada@example.com", "to": "recruiter@example.test",
            "subject": "My CV", "body": "Attached.",
            "why": "The recruiter asked for it.", "sources": "todo:life/permitfiling",
            "attach": {attach}
        }}}}]}}"#
    )
}

fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository's binary file attaches as the bytes the card named: the card shows the repo,
/// path, commit and SHA-256, and the tap sends exactly those bytes though the branch moved on.
#[test]
fn a_file_from_a_repository_is_sent_as_the_card_showed_it() {
    let root = common::scratch("repos-attach");
    let fakes = Arc::new(
        Fakes::new("repos-attach")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .repos(common::local_repos(&root))
            .model(FakeModel::saying_owned([
                attaching(r#"[{"repo": "bmabsout/cv", "path": "cv.pdf", "at": "main"}]"#),
                r#"{"say": "Proposed."}"#.to_owned(),
            ])),
    );
    let running = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new()).start();
    let payload = exchange(&running.base, "send my cv to the recruiter");
    let id = payload["id"].as_str().expect("an id").to_owned();
    let work = root.join("cv-work");
    let commit = git_in(&work, &["rev-parse", "HEAD"]).trim().to_owned();
    let file = &payload["proposals"][0]["act"]["attachments"][0];
    assert_eq!(file["name"], "cv.pdf", "{payload}");
    assert_eq!(file["media"], "application/pdf");
    assert_eq!(
        file["sha256"],
        suzatary_core::attachment::AttachmentId::of(common::CV_PDF).as_str()
    );
    assert_eq!(file["from"]["kind"], "repo");
    assert_eq!(file["from"]["repo"], "bmabsout/cv");
    assert_eq!(file["from"]["path"], "cv.pdf");
    assert_eq!(
        file["from"]["at"], commit,
        "the branch is recorded as the commit it named"
    );

    std::fs::write(work.join("cv.pdf"), b"%PDF-1.7 a newer cv").expect("a newer cv");
    git_in(&work, &["commit", "-qam", "newer"]);
    let remote = root.join("remotes/bmabsout/cv.git");
    git_in(
        &work,
        &["push", "-q", remote.to_str().expect("utf-8"), "main"],
    );

    let mut tapped = common::act(
        &format!(
            "{}/api/secretary/conversations/{id}/proposals/0/tap",
            running.base
        ),
        &serde_json::json!({}),
    );
    assert_eq!(
        common::status(&tapped),
        200,
        "{}",
        common::body_json(&mut tapped)
    );
    let parts = running.sent.parts();
    let [sent] = parts.as_slice() else {
        panic!("one send: {parts:?}");
    };
    let [part] = sent.as_slice() else {
        panic!("one attachment: {sent:?}");
    };
    assert_eq!(part.filename, "cv.pdf");
    assert_eq!(
        part.content,
        common::CV_PDF,
        "the bytes shown, not the branch's newer ones"
    );
}

/// A repository the instance file does not declare is refused by name, and nothing is filed,
/// though it is the owner's and on the very remote the declared one is.
#[test]
fn an_undeclared_repository_attaches_nothing() {
    let root = common::scratch("repos-attach-undeclared");
    let repos = common::local_repos(&root);
    git_in(
        &root,
        &[
            "clone",
            "-q",
            "--bare",
            "cv-work",
            "remotes/bmabsout/undeclared.git",
        ],
    );
    let fakes = Fakes::new("repos-attach-undeclared")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .repos(repos)
        .model(FakeModel::saying_owned([
            attaching(r#"[{"repo": "bmabsout/undeclared", "path": "cv.pdf"}]"#),
            r#"{"say": "I cannot reach that one."}"#.to_owned(),
        ]));
    let base = common::start(Arc::new(fakes), deployment(), Vec::new());
    let payload = exchange(&base, "send the other cv");
    let (text, refused) = answer(&payload, "propose_email");
    assert!(refused && text.contains("bmabsout/undeclared"), "{text}");
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty()
    );
    assert!(
        !root.join("mirrors/bmabsout/undeclared.git").exists(),
        "nothing was fetched"
    );
}

/// A Git LFS pointer is not the file it names, so it is refused, naming LFS.
#[test]
fn an_lfs_pointer_is_not_attached() {
    let root = common::scratch("repos-attach-lfs");
    let repos = common::local_repos(&root);
    let work = root.join("cv-work");
    std::fs::write(
        work.join("cv.pdf"),
        "version https://git-lfs.github.com/spec/v1\noid sha256:aaaa\nsize 12345\n",
    )
    .expect("a pointer");
    git_in(&work, &["commit", "-qam", "to lfs"]);
    let remote = root.join("remotes/bmabsout/cv.git");
    git_in(
        &work,
        &["push", "-q", remote.to_str().expect("utf-8"), "main"],
    );
    let fakes = Fakes::new("repos-attach-lfs")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .repos(repos)
        .model(FakeModel::saying_owned([
            attaching(r#"[{"repo": "bmabsout/cv", "path": "cv.pdf"}]"#),
            r#"{"say": "It is in LFS."}"#.to_owned(),
        ]));
    let base = common::start(Arc::new(fakes), deployment(), Vec::new());
    let payload = exchange(&base, "send my cv");
    let (text, refused) = answer(&payload, "propose_email");
    assert!(refused && text.contains("Git LFS"), "{text}");
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty()
    );
}

/// A branch that was pushed while its pull request could not be opened says so on the card,
/// never "may have been pushed", and a tap again opens the request on the commit pushed.
#[test]
fn a_pushed_branch_without_a_pull_request_is_finished_by_a_tap_again() {
    let root = common::scratch("repos-nopull");
    let repos = common::local_repos(&root);
    // A stale lock on the request's ref: the branch pushes, the request cannot be made.
    let pulls = root.join("remotes/bmabsout/cv.git/refs/pull/1");
    std::fs::create_dir_all(&pulls).expect("the pull refs");
    std::fs::write(pulls.join("head.lock"), "").expect("a stale lock");
    let fakes = Arc::new(
        Fakes::new("repos-nopull")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .repos(Arc::clone(&repos))
            .model(FakeModel::saying([
                propose(RETITLE).leak(),
                r#"{"say": "Proposed."}"#,
                r#"{"say": "Pushed, no request."}"#,
                r#"{"say": "Opened."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .real_acts(
            "repos-nopull",
            "her@study.test",
            common::again(&fakes.conversations),
            common::ScriptedOauth::new(Vec::new()),
        )
        .publishing(repos)
        .start()
        .base;
    let payload = exchange(&base, "update my title");
    let id = payload["id"].as_str().expect("an id").to_owned();
    let tap = || {
        let mut tapped = common::act(
            &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
            &serde_json::json!({}),
        );
        let after = common::body_json(&mut tapped);
        assert_eq!(common::status(&tapped), 200, "{after}");
        after
    };
    let stopped = tap();
    let said = stopped.to_string();
    assert!(
        said.contains(&format!("pushed secretary/{id}-0 ("))
            && said.contains("a pull request may be open"),
        "{stopped}"
    );
    assert_eq!(stopped["proposals"][0]["state"], "unfinished", "{stopped}");
    assert_eq!(
        stopped["proposals"][0]["card"]["link"]["label"], "See where it stands",
        "{stopped}"
    );
    std::fs::remove_file(pulls.join("head.lock")).expect("the lock goes");
    let finished = tap();
    assert_eq!(finished["proposals"][0]["state"], "acted", "{finished}");
    assert!(
        finished.to_string().contains("opened pull request #1"),
        "{finished}"
    );
}
