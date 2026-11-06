# Contributing to Suzatary

Thank you for your interest in Suzatary. The project follows one principle: **no important
rule depends on a person's memory or discipline.** A type, a test, a gate or a lock enforces
each rule. Reviewers judge a contribution first by whether it keeps this principle true.

## Before you start

- Read [`CLAUDE.md`](CLAUDE.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).
  `CLAUDE.md` is the project's constitution. [`docs/README.md`](docs/README.md) is the index
  of all other documents.
- If you change `web/`, also read [`docs/TS-STYLE.md`](docs/TS-STYLE.md) and
  [`docs/WEB-SPEC.md`](docs/WEB-SPEC.md).
- If your change is larger than a fix, open an issue first. Say what you want to change and
  why. A design for it often exists already in a dated plan. Or the project rejected the idea
  before, and the reason is in `ROADMAP.md`.
- Do not report a security problem in an issue. Follow [`SECURITY.md`](SECURITY.md) instead.

## Setting up

The flake pins every tool. You need only [Nix](https://nixos.org) with flakes enabled.

```console
$ nix develop .#triage
$ git config core.hooksPath scripts/hooks
$ git config blame.ignoreRevsFile .git-blame-ignore-revs
```

The commit hook checks that a commit message names only files that the commit changes.

## The gate

A change reaches `master` only after the merge gate passes on it:

```console
$ nix develop .#triage -c scripts/merge-gate.sh --no-live
```

The gate runs every step, even after a step fails. It prints the result and the time of each
step. The steps are:

- `cargo fmt`;
- Clippy at `pedantic -D warnings`, with and without the fixture feature;
- `cargo doc`;
- the workspace's tests and the plugin fences;
- the web app's pure tests, its type check and its build;
- the whole browser suite against real fixture servers;
- the Nix build of the frontend.

`scripts/merge-gate.sh --list` prints every step, its lane and the files that it reads. The
gate records each passing step with a hash of the files that the step reads. If those files
have not changed, the next run in a clean checkout skips the step. In a checkout with
uncommitted changes, the gate runs every step and records nothing. If you delete `.git/gate/`,
the next run takes longer, but its results do not change.

The other half of the gate, `--only-live`, runs tests over the data of a real deployment. The
maintainer runs it.

While you work, run only the step that you change: `cargo test -p <crate>`,
`scripts/web-test.sh` or `scripts/web-e2e.sh <scene>`.

Never skip, disable or loosen a test to make the gate pass. A flaky test is a bug, and the bug
has a cause. Find the cause and fix it.

## A change is a series of commits

Put one logical change in each commit, for example the primitive, the operation, the route,
the wire, the page or the docs. Each commit must build and pass its tests on its own. Then
`git bisect` and `git blame` can explain each change. Also:

- In the subject line, state what the commit does, in 72 characters or fewer.
- Name a tracked file in the commit message only if the commit changes it. Otherwise, list it
  in a `Mentions:` trailer. To check every commit on your branch, run
  `scripts/check-commits.sh origin/master..HEAD`.
- If you change `app/` or `web/`, add your entry as a new file, `changelog.d/<branch>.md`: Keep a
  Changelog `### <kind>` headings over `- ` lines. Never edit `CHANGELOG.md`.
- If your change completes a todo on the project chain, write `Completes: <todo>` on its own
  line in the pull request. The train records the todo done and sets the version; never set
  the version yourself.
- Do not keep backward compatibility. Rename and delete freely, and update every caller in the
  same series. If you change the format of stored data, write an explicit migration.
- In the pull request, say what changed and why in two or three plain sentences. Then add one
  line about how you tested the change.

## Fixtures

Tests and screenshots use the *cast*: `suzatary_core::cast` in Rust and `web/test/cast.ts` in
TypeScript. The cast is Ada and her invented correspondents, at reserved domains such as
`example.test`. Never use a real person, address or host.

The gate builds the public tree and runs `scripts/public-tree.sh --check` over it. The check
refuses each of these:

- a real address;
- a home path;
- a tailnet name;
- a link to a file that the tree does not contain;
- a secret that gitleaks finds;
- the deployment's own names, when a deployment is present.

The check also refuses a file that it cannot read as words, such as a binary file or a long
run of base64 or hex. To admit such a file, add the line that `sha256sum <path>` prints to
`scripts/public-admitted.sha256`. The maintainer admits the file when it is published.

## Licence

Suzatary is licensed under MIT OR Apache-2.0. Unless you say otherwise, you offer each
contribution under the same two licences, with no additional terms.
