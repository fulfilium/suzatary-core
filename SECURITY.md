# Security

Suzatary reads its owner's mail, and some of that mail is written by people
who would like it to do something else. Its safety rests on mechanisms, so a
way around one of them is the most valuable report you can send.

## Reporting a vulnerability

Please report it **privately**, through GitHub's private vulnerability
reporting: the *Security* tab of this repository, then *Report a
vulnerability*. Do not open a public issue or pull request for it.

Say what you found, the commit you found it at, and the smallest steps that
show it. A failing test is the best form, written against the fixture
deployment and its made-up cast (`suzatary_core::cast`). The maintainer aims
to answer within a week. When a fix lands, the advisory is published and you
are credited, unless you ask not to be.

Only the latest commit on `master` is supported. There are no maintained
release branches.

## The threat model

The adversary writes email. The triage agent reads every new message with a
model, so an injected instruction is assumed to reach that model. The design
promises that **at worst such a message mis-triages or writes bad todo
text**, and that it never reaches a shell, the network, a credential, or
anything that leaves the box.

[`CLAUDE.md`](CLAUDE.md) states the whole model, and every claim in it names
the mechanism that holds it. *Irreversibles* covers what is human-only.
*Containment* covers what the agent can and cannot reach. In short:

- **Capability records.** Each role holds a record of the effects it may
  use. The agent's has no sending effect, so a send verb would not compile.
- **The fence.** The agent's tools are denied by default except Suzatary's
  own closed, snapshotted verb list.
- **Provisional writes.** The agent's writes are cards in the owner's Inbox,
  performed only by the owner's tap; its record holds the chain read-only, with no write reachable from it.
- **The tap.** Sending mail, starting a sign-in or setting a secret takes a
  request from a named tailnet login, with the anti-forgery headers, against
  a stored proposal. Loopback, and so the box's own shell and the agent,
  can never tap.
- **The door.** An address is not an identity: every request, loopback
  included, is admitted by tailscaled's whois alone, and one that came
  through a proxy or is addressed to another name is refused. The app
  serves https, or plain http on loopback only, with a Content-Security-
  Policy on every answer and `no-store` on every API answer.
- **The closed Typst world.** Every authored text compiles with no HTML, no
  plugin, no file, no package and no clock, under a time and memory budget.
  A tripwire refuses scripts, inline handlers and executable URLs (read as
  a browser reads them) before the browser parses anything, and the page
  keeps a link only of a scheme that navigates.
- **Secrets are write-only from the app.** A page may set a declared secret
  by name and learn only whether it resolves. A value never comes back.
- **Plugins hold only their imports.** A WebAssembly plugin reaches the world
  through the host's `fetch`, limited to the origins its manifest declares,
  and never holds a credential.

## What is in scope

Anything that breaks one of those promises. For example:

- a message, attachment or calendar entry that makes the agent act outside
  its verbs, or write as an actor other than itself;
- a way for anything other than the owner's tap to send mail or perform a
  proposal, or for the tap to perform something other than what was shown;
- markup that runs script, or escapes the closed world, in the browser or on
  the box;
- a secret's value, or a redacted identity number, reaching a page, a log, a
  transcript or a model;
- a plugin reaching a host, a file or a credential its manifest does not
  grant;
- a stored object the box adopts that verification should have refused.

Out of scope: attacks that start from the owner's own tailnet login or the
box's user account. Those are the trusted base this design assumes.

## What publication holds

The repository every deployment runs is published from a private one, and
the Prodrome is public too. Both publication paths pass one guard
(`scripts/public-tree.sh --check`, and `scripts/public-scan.sh` over a
Prodrome range), and it holds one invariant:

**Every byte that reaches a public tree is either plain text that both
checks read, or in a file listed by its path and SHA-256 in
`scripts/public-admitted.sha256`, a line a person admitted at a terminal.**

- **The secret scanner** is [gitleaks](https://github.com/gitleaks/gitleaks),
  pinned through the flake, with its maintained rules and the few this
  project adds (`scripts/secret-rules.toml`). It runs offline and never sends a
  finding anywhere. It decodes base64, hex and percent-encoding itself. It
  reads a copy in which every file is named by a number, so its own default
  path allowlist (pictures' text, lock files, `node_modules/`, its config)
  passes over nothing. A test's throwaway key is admitted by its finding's
  fingerprint in `.gitleaksignore`, and its bytes by the manifest, since a
  fingerprint names a path and a line, never the bytes.
- **The term matcher** (`suzatary_core::leak`) finds the owner's private
  terms and this deployment's names as whole words, folded (accents,
  invisible characters and line breaks hide nothing), and addresses, home
  paths, tailnet names and telephone numbers. It decodes nothing.
- **The admitted files.** A file the guard recognises as encoded fails the
  guard until the manifest lists it. That is judged over the whole file, so
  wrapping, quoting or escaping the line breaks of an encoding does not
  hide it: a binary; base64 (160 characters, in all, of tokens that mix
  case and digits as base64 does, an integrity hash's random bytes not
  counted); hex whose bytes read as text (a token of 32 digits or more, from
  either digit) or sixteen separated hex pairs; or text spelled with escapes
  (sixteen `%XX`, `&#…;`, `\uXXXX` or `\xXX` escapes of letters, digits or
  non-ASCII bytes, one escape of a letter inside a word, or a
  quoted-printable body).
- **Admissions are held.** The train publishes a new manifest line, a new
  `.gitleaksignore` fingerprint or any change to the secret rules only once
  `scripts/train.sh publish --admit <path>` names it, and the Prodrome's scan
  reads every commit of a range and takes `--admit` the same way.

**What "at a terminal" stops, said plainly.** `--admit` refuses a standard
input that is not a terminal (`[[ -t 0 ]]`). That stops cron, CI and an
unattended `train.sh land`. It does **not** stop anything that holds a shell
on the box: an agent with a shell can open a pseudo-terminal (`script -qec
… </dev/null` passes the check). Admission is a deliberate act of the
operator, not a mechanism an agent cannot reach; the agents that run here
hold no shell (CLAUDE.md, Containment). Admissions signed by the owner's key
are recorded for 2.1, not built.

What each layer does **not** catch, by design:

- A term inside an admitted file is the admitting person's to see. The
  matcher reads that file's plain lines, never what it encodes.
- gitleaks finds only what its rules know. A secret in a shape no rule
  describes passes it, and the matcher does not look for secrets.
- The matcher finds only the terms on the lists and the shapes it states.
  A private fact nobody listed, written in plain words, passes it.
- The thresholds are thresholds. These pass the matcher as text, by design:
  an escape at a word's edge (`%74eapot`), percent-encoded UTF-8 of an
  accented name (`Jo%C3%A3o`), ten to fifteen escapes in a file, a few short
  base64 tokens, hex under 32 digits to a line, byte arrays (`[0x74, …]`),
  UTF-16 text, encodings the guard does not recognise (base32, among
  others), the bytes inside an integrity hash of exactly its length, a
  sealed slot's ciphertext (a `Locked(key='…', box='…')` box of whole
  256-byte buckets and its tag: random bytes under a key that is destroyed
  or never leaves the box, so it hides no word, and its shape is all the
  guard can check; the secret scanner reads its line with the slot replaced
  by a placeholder, so everything else on the line is still scanned; neither
  check reads inside a recognised box, so words someone places in one in a
  form that does not read as text (compressed, or UTF-16) pass, while plain
  words in a box are no slot, and a real sealing mistake cannot take that
  shape, since
  words left unsealed are whole buckets of 256·k bytes, never 256·k + 16), and a
  payload cut across several files each under the per-file thresholds.
  Chasing every encoding is the arms race this guard stopped running.

The guarantees are the plain-text checks (gitleaks and the matcher), the
admission of opaque bytes by a person, and the review of every pull request
before it lands. The thresholds catch the accidental (a pasted mail body, a
fixture of encoded text); they do not stand in for that review.

## If something private was published

The published repository's `master` only climbs: every commit on it is the
public tree of one landed commit, and `scripts/train.sh` never rewrites it.
A leak that reached it is the **one stated exception**, and the owner decides
at the time whether to take it: rewriting the published history cannot recall
a clone, a fork or a cache, so for a key the remedy is always to revoke it,
and the rewrite only limits who finds it later.

1. **Stop publishing.** Land nothing until the leak is out of `master`'s
   tree. A key or a password is revoked at its issuer now, before anything
   else; nothing below makes a published key safe.
2. **Teach the guard.** Add the leaked word to the data repository's lists
   (a secret's shape to `scripts/secret-rules.toml`, or a rule to
   `suzatary_core::leak`, each with its throwaway-tree test), so the next
   check fails where this one passed. Confirm that
   `scripts/public-tree.sh --check --strict` now finds it in the published
   tree.
3. **Remove it at the source.** Land a commit of the private repository that
   takes it out. Its own publication passes the guard again.
4. **Rewrite the published history, once.** In a fresh clone of the
   published repository, remove the path or the text from every commit that
   holds it (`git filter-repo`), keeping each commit's `Source:`, `Skipped:`
   and `Admitted:` trailers, and push the result over `master` with
   `--force-with-lease`. The rewritten table no longer matches its sources
   byte for byte at the rows it touched; say so in a commit of the private
   repository, naming the rows, so the exception is on record.
5. **Repin every deployment.** A user repository's `flake.lock` names a
   commit the rewrite removed: run `scripts/deploy.sh` with the landed commit
   it should run, which pins that commit's rewritten publication.
6. **Ask the host to forget.** Ask GitHub to purge cached views of the
   removed commits, and treat what leaked as known to anyone who looked.
