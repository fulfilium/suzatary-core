# AGENTS.md: instructions for a coding agent

You are a coding agent working on Suzatary's Rust and TypeScript. Follow this file for every
change under `app/` or `web/`, and for every comment, document, commit message and pull request
that you write.

Read these files too:

- [`CLAUDE.md`](CLAUDE.md) is the project's constitution. It contains the rules for security,
  data and operations.
- [`docs/TS-STYLE.md`](docs/TS-STYLE.md) adds layout rules for the TypeScript in `web/`. Name
  it in every brief that touches `web/`.
- [`docs/WEB-SPEC.md`](docs/WEB-SPEC.md) contains the web app's types, laws and tests.
- [`AGENT.md`](AGENT.md) (no "S") is a different file. Use it only to set up a new deployment.

## The project's memory

The project's decisions and lessons are in `memory/`, a store of recorded decisions. Each one
says what was decided, who decided it, when, and why. `CLAUDE.md` names a rule's decision as
`(→ id)`. It is not the secretary's memory, which is the `memory/` of the data tree.

1. Before you change a mechanism, run `suzatary decision list --store memory --grep <words>`.
   It lists the decisions whose title, body or rationale holds every word.
2. To read one decision whole, run `suzatary decision show <id> --store memory`.
3. Without an installed binary, run the same verbs as
   `cargo run --manifest-path app/Cargo.toml -q --bin suzatary -- decision …`.
4. Do not write to `memory/` yourself. To propose a lesson, put it on its own line of your pull
   request's description, for example `Lesson: a clean rebase can still fail to compile`. The
   main session records the lesson, or not, when it reviews the pull request.
5. The main session records with `suzatary decision record|revise|withdraw --store memory`,
   with `--at` set to the date the decision was made. The actor is `owner` or `assistant`,
   never a name. `memory/` is published with the code, so a decision holds nothing personal.
   Each new decision is a new published path, which the main session admits at publish.

## Commands

1. Before your first commit in a clone, run `git config core.hooksPath scripts/hooks`.
2. Before you push, run `nix develop .#triage -c scripts/merge-gate.sh --no-live`.
3. While you work, run only the step that you change: `cargo test -p <crate>`,
   `scripts/web-test.sh` or `scripts/web-e2e.sh <scene>`.
4. Before you push, run `scripts/check-commits.sh origin/master..HEAD`.
5. To check the public tree, run `scripts/public-tree.sh <out>`, then
   `scripts/public-tree.sh --check <out>`.

The merge gate runs `cargo fmt --check`, Clippy at `pedantic` with `-D warnings`, the tests and
the browser suite. Every crate is `#![forbid(unsafe_code)]`. A public Rust item with no doc
comment causes a warning. Give each crate typed errors with `thiserror`, and use `anyhow` only
in the binary. In `web/`, use no framework and no npm.

## Never do these things

- Never skip, disable, loosen or delete a test to get a green gate. Find the cause.
- Never let a fixture touch live data. Use absolute paths under your scratch directory, and set
  `NOTMUCH_CONFIG` to a file of your own.
- Never write a private term in a brief, a public file, a test or a fixture. Use the cast in
  `suzatary_core::cast` and `web/test/cast.ts`, at reserved domains like `example.test`.
- Never put a secret's value in a file, a command line, a URL or a transcript.
- Never add a tool or a code path that sends mail.
- Never push to `master`. Code reaches `master` only through `scripts/train.sh`.

## The style rules

**Known debt** under a rule is code that breaks the rule today. Do not copy the debt. Fix it
when your change touches it.

### 1. Make invalid states unrepresentable

Model a state as a sum type with one case for each state. Do not model it as a record with a
`bool` flag or an `Option` field that is meaningful only in some states. When a type allows only
valid values, no code has to check for invalid ones. A sum type is an `enum` in Rust and a union
tagged on `type` in TypeScript.

- Rust: `secretary::phase::Phase` has one case for each stage of a proposal, such as
  `Trying`, `Sent`, `MayHaveGone` and `Dismissed`. `capability::Turn` has an `Erased` case for
  a turn whose words were deleted. It does not use an `Option` that can be `None`.
- Rust: `secretary::outcome::Outcome` says how a verb's call ended: `Answered`, `Refused`, or
  `Remembered` with the fact it wrote. The turn row on the wire carries the same sum
  (`wire::VerbOutcome`), not a `refused` flag beside an optional receipt.
- TypeScript: `lo.T<V>` (`web/src/algebra/lo.ts`) is `Loading`, `Error` or `Val`. It is not a
  record with a `loading: boolean` field. `app.Reach` (`web/src/app/app.ts`) is `Writes`,
  `Waits` or `Reads`.
- A plain `bool` is correct for a fact that does not change which other fields have meaning,
  such as `truncated` on a mail body.

### 2. Parse, do not validate

Check untrusted input once, where it enters the program. Turn it into a type whose values passed
the check. After that point, the code uses the type and does not check again. A
constructor that can refuse its input is the only way to make a value of such a type. The
project calls this constructor a *smart constructor*.

- Rust: `SessionId::new` returns `Result<SessionId, NotAUuid>`. Code that holds a `SessionId`
  knows that it is a UUID.
- Rust: `record::MarkupSource` has exactly three constructors. `checked` parses and compiles a
  writer's Typst markup. `escaped` turns plain text into markup. `from_chain` reads bytes
  that were already stored.
- TypeScript: `parsed.T<A>` (`web/src/algebra/parsed.ts`) is `Ok` with the value, or `Refused`
  with the sentence to show.
- TypeScript: the decoder `d` (`web/src/wire/decoder.ts`) turns JSON from the server into the
  generated wire types when the JSON arrives.

Choose the representation that makes the logic simple, and convert at the edges. For example,
a stored conversation line is a `LineShape` and a list of `Words<LineWords>`.
`line_shape::join` combines the two when a reader needs the full line.

### 3. Write total functions

A total function returns a value for every input that its type allows. Do not use `unwrap`,
`expect`, `unreachable!` or `panic!` for a state that the types allow. If you need one, change
the types so that the state cannot occur.

- Rust: `Present::present` (`app/io/src/present.rs`) returns `Ok(None)` for a missing file. It
  does not return an error that the caller must catch.
- Rust: `Turn::Erased` is a turn whose words are gone. A reader matches on it and does not
  unwrap a missing value.
- TypeScript: `lo.Settled<V>` removes `Loading` from the type. A function that reads bytes
  already in memory returns `lo.Settled`, so its caller has no `Loading` case to handle.

The code allows one exception. An `expect` on a constant that the program itself writes is
acceptable, such as a literal regular expression or a fixed identifier. The message must say
why the constant is valid, for example `.expect("a literal pattern")`.

Known debt: `secretary::card::decision` uses `let … else { unreachable!() }`. `view.rs` calls
`.expect("ancestry is acyclic")`. `secretary/store.rs` has an `unreachable!` for a day with
more than `u32::MAX` conversations.

### 4. Match exhaustively

In every `match` or `switch` over one of our own sums, name every case. Do not use `_ =>` in
Rust or `default:` in TypeScript. When someone adds a case, the compiler then lists every
place that must handle it. A `default` is correct only over a type that we do not own, such
as a string from a user or a key name.

- Rust: `web::failure` (`app/io/src/web.rs`) names every case of `HttpError`.
  `StreamEvent::text` names every event.
- TypeScript: the `switch` over `Event` in `app.step` has no `default`. `d.Fields<A>` makes
  the author give a decoder for every field of `A`.

Known debt:

- `line_shape::join` uses `_ => None` over `LineWords`.
- `activeFor` (`web/src/shell/nav.ts`) and `referenceInView` (`web/src/chat/inview.ts`) use
  `default` over our own `Route` and `Screen` types.
- `badgeOf` (`web/src/pages/mailrow.ts`) and `digest.phrase` (`web/src/inbox/digest.ts`) use
  `default` over strings. The wire sends a string where a sum type belongs.

### 5. Defunctionalize a closed set of behaviours

When the set of behaviours is closed and known, write it as a sum type. Then write one function
that interprets the sum. Do not pass closures, callbacks or trait objects for a closed set. A
sum can be stored, printed, compared and tested. A closure cannot.

- Rust: `secretary::changes::Act` is the closed set of changes that the secretary can propose
  to a todo: `Add`, `Update`, `Complete`, `Cancel`, `Tend`, `Reprice` and others. A proposal
  stores the `Act` as data. `Act::event` is the one function that turns an `Act` into a chain
  event.
- Rust: `secretary::acts::Act` is the closed set of actions that the server performs on a tap,
  such as `Auth`, `Secret` and `Web`. The tap's handler interprets it.
- Rust: `suzatary_io::run::Cmd` is the description of a subprocess, as data. One function,
  `run(&Cmd) -> Result<Ran, RunError>`, runs every subprocess.
- TypeScript: `app.Event` has one case for each change to the shell's state. `app.step` is the
  one function that interprets it.

Use a callable argument only when the set is open:

- An effect trait such as `Clock` or `Render` has one implementation in production and others
  in tests.
- The `run` field of an `act.T` button (`web/src/view/act.ts`) is a function, because every
  page adds its own acts.
- `tree.cata` (`web/src/algebra/tree.ts`) takes the fold's algebra as a function.

### 6. Keep a pure core, with effects at the edge

Code that decides is pure. Code that reads files, calls the network or reads the clock is
separate. The crate `suzatary-core` is pure and never sees a file path. `suzatary-io`
contains the implementations of the effects. The binary in `app/cli` connects the two.

- **Effects are values.** A function receives the effects it may use as an argument. The
  argument is a record with one trait object for each effect. `capability::Agent` has seven
  fields, from `chain` to `clock`, and no field that can send mail. A test replaces one field
  with struct update syntax, for example `Agent { clock, ..agent }`.
- **State between steps is an explicit value.** `coding::Session` is the state of a coding
  session, and only `Session::step` changes it. In the web app, `source.create(initial, step)`
  (`web/src/algebra/source.ts`) holds a page's state and changes it only through a pure `step`.
- **A run is a fold.** A state is the result of a fold of all events over the start state.
  `prodrome_core::fold` is a monoid action, so `extend` folds new events into an old result.
  `sourcelaws.test.ts` tests "law: a source is the fold of its events".
- **A view is a function of a value.** In TypeScript, a view takes a value and its effects and
  returns DOM nodes. A view does not fetch data. It does not keep flags in a closure.

### 7. Write generic functions and one combinator with laws

Write a function once over a type parameter, not once for each type. Express an aggregate as a
monoid with an identity, or as a semilattice with a join. Then every caller uses the same
`combine` or `join`.

When you see that two things are instances of one higher-level structure, move the code to that
structure. Do it in your change if the move is small. If it is not small, name the stage that
does it in your pull request, with the lines it would remove. The special case then becomes data
or configuration of the general piece. Make the code smaller by making it more general. Do not
make it smaller by removing a capability that someone uses.

- Rust: `store::Database` uses three generic store shapes: `Prodrome<E>` for objects of a
  schema `E`, `Log<E>` for lines, and `Doc<K, V>` for values. A new store is one row of the
  `stores!` list (`app/core/src/store/kind.rs`), its schema and a field of `Database`, not a
  new trait. The row gives its name, tier, files and hold to every layer.
- Rust: the Prodrome's `schema::headed::Event<B>` is the event of every schema whose events
  print as `Constructor(<key>, at, actor, <body>)`. A schema gives only its body, which
  implements `Body` and checks a key and a body together in `Body::admits`. `Event::new` is
  the only constructor, so a schema's smart constructors are functions of its module that call
  it. The Prodrome's `fold::sole::One` is the register product of every schema with one
  register. Its parameter is the register's type, which implements `Sole` (and says how an
  append climbs, where it must), or the Prodrome's `Whole` where the value is the whole event.
- Rust: `laws::partial_order<V: Order>` (`app/core/tests/laws.rs`) states the partial-order
  laws once. Each schema's tests call it.
- TypeScript: `lo.combine` is a monoid whose identity is `val([])`. `lo.map`, `lo.bind` and
  `lo.fold` serve every loading state in the app. A caller uses these functions and does not
  `switch` on the tag of `lo.T`.
- TypeScript: `tree.cata` folds any `Tree<A>`. `tree.size`, `tree.nodes` and `tree.sort` use
  it.

### 8. State each law as a test, and name the test

A *law* is a property that must hold for every input, such as "parsing a printed route gives
the same route". When a type cannot express a law, write the law as a test. Run it over many
generated inputs. Write the test's name next to the law in the docs.

- Rust tests use `proptest`. For example, `inbox_schema.rs` has
  `the_state_order_is_a_partial_order`.
- TypeScript tests run in `node:test` over the seeded generator in `web/test/gen.ts`. A failure
  prints the seed. `lo.test.ts` has "law: map preserves identity and composition".
- In `docs/WEB-SPEC.md` §3, each law is followed by the name of its test. For example:
  "Round trip. `parseRoute(hrefOf(r)) = r` for every route. — `laws.test.ts` 'a route
  survives print and parse'."
- The Rust code writes test vectors to `app/core/tests/web/`. The TypeScript modules must give
  the same answers for them.

Do not make a test pass because a read is fast. If a test needs one event before another, make
that order happen in the test. Do not add a sleep or a retry.

What to test, and where:

- Keep a standing test only where the code is complicated or costly to get wrong: merge and
  fold laws, orders and concurrency, migrations and stored formats, and the security and
  privacy boundaries. A simple pure transform between two types needs no standing test once
  it is written. Do not test what a type already proves, or what another test already checks.
- Test a pure function directly. A unit test reaches the core's folds and the page's steps
  without a server or a browser. The e2e scenes (`web/e2e/`) run a real browser against a real
  server, so use a scene only for what only that pair can show: the page drawn from the box's
  answers, a click reaching a route, a device's storage across a reload. A rule a unit test can
  reach does not belong in a scene.
- Write one test for each law, and loop over its cases inside that test. Do not copy a test
  for each case.
- Comments follow the same rule: write one only where the code cannot say it, such as an
  invariant, a hazard, an outside constraint or a reason that is not obvious. Do not restate
  what the code does, tell its history or justify a choice. A public item needs no doc comment
  when its name and type say what it is.
- When a change alters behaviour, change its test in the same commit. Regenerate a pin, a
  vector or a golden file (`app/cli/tests/pins/`, `app/core/tests/web/`) only when the change
  is meant to move it, and say in the commit what moved.

### 9. Put one concept in each file

Name each file for its main type, and declare that type first, after the imports. If a file has
two main types, split it into two files. Do not make `utils` files. A reader learns the program
from its folder names and types.

- Rust: `app/core/src/secretary/phase.rs` contains `Phase`. `app/core/src/store/` has one file
  for each store shape.
- TypeScript: a module is a namespace named for one idea, such as `lo`, `tree` or `parsed`.
  The details are in `docs/TS-STYLE.md`.

Known debt: `app/core/src/secretary/store.rs` contains 28 public types, from `ConversationId` to
`Kept`.

### 10. No defensive programming, and no backward compatibility

Do not check again what a type already guarantees. Do not catch an error that you cannot
handle. Pass it up as a typed error value. The security fences in `CLAUDE.md` ("Containment")
are separate layers on purpose. This rule does not apply to them.

Do not keep old names, aliases or a second API for old callers. Rename and delete freely, and
update every caller in the same series of commits. When the format of stored data changes,
follow these steps:

1. Write an explicit migration command.
2. Run it on the deployment.
3. Delete the migration command, the reader of the old format and the old store.

The old data stays in the history of the data repository, beside the code that read it. Old
event objects on a todo chain are different: they are the history, and the code reads them
forever.

Known debt: some fields of `capability::Turn` have `#[serde(default)]` only so that the code can
still read older transcripts.

### 11. Simple, not simpler

Do not add a wrapper type or other machinery that no code inspects. A plain `String` or `Vec`
is correct when no code looks inside it. When code does look inside a value, give the value a
type.

- Rust: `Turn::Act` has `text: String`, because no code parses that text.
- TypeScript: the `Error` case of `lo.T` has `e: string`, because the app only shows it.

### 12. Explain what and why in comments, never history

Write a comment only to record a decision that the types and names cannot express. Use one
sentence. Write a comment for one of these:

- a reason;
- a safety argument, for example about the fence, escaping, secrets, sending or the tap;
- a law;
- an invariant that is not obvious.

Do not put dates, plan references, chain references, quotes or history in a comment. Put them
in `CHANGELOG.md` or `docs/`, or leave them out. Do not repeat the code in words. Give every
public Rust item a doc comment, because `missing_docs` warns. Use one line when one line is
enough.

- Good, from `secretary::acts::Act::Secret`: "No `value` field on purpose: the value travels
  only in the tap's body, never stored."
- Bad (an invented example): `// Added on 2026-09-14 for the secret card (see the plan).`

Known debt: some comments still carry history. An example is the model comment in
`app/cli/src/instance/file.rs`.

### Review questions

Ask these questions about each type in your change:

1. Does the type allow only valid values? (Rules 1 and 2.)
2. Did you choose the representation that makes the logic simple? (Rule 2.)
3. Does every function return a value for every input that its type allows? (Rule 3.)
4. If someone adds a case, does the compiler find every place that must handle it? (Rule 4.)
5. Does every wrapper type have code that looks inside it? (Rule 11.)

## Written English

Write comments, documents, commit messages and pull request text by these rules. The rules
are close to ASD-STE100, a standard for plain technical English. A reader whose first language
is not English must understand each sentence on the first read.

1. Give each sentence a concrete subject, a plain verb and an explicit object. Say who does
   what to what.
2. Let only people and programs act. Data, bytes, documents and notes do not "promise", "say",
   "plan", "keep" or "name".
3. Do not write riddles, wordplay or slogans. State the fact plainly.
4. Use common words. Define a term of art in one plain sentence where it first appears, or
   link to its definition. Do not put two terms of art in one phrase.
5. Put one idea in each sentence. Write an instruction in the imperative, with the condition
   first, in 20 words or fewer. Write a description in 25 words or fewer.
6. Keep "the" and "a". Do not drop words to save space.
7. Use the active voice and the simple present tense.
8. Use one word for one thing: the term that the code uses, everywhere.
9. Write for a stranger, not as notes to yourself.

Examples (compressed, then plain):

- "a step's pass is kept by those bytes of HEAD" → "The gate records each passing step with a
  hash of the files that step reads. If those files have not changed, the next run skips the
  step."
- "what the stored bytes promise" → "which changes affect the stored format"
- "the history is free, and a schema is its reading" → "Events are stored without a schema,
  and a schema is applied when the events are read."
- "a deletion is safe only if its reader agrees" → "Before you delete something, read the code
  that uses it and check that nothing still needs it."

Write a pull request description in two or three plain sentences, then a short list of the
changes and one line on how you tested them.

## Releases and `Completes:`

The version is read from the project chain, and `scripts/train.sh` writes it. Never change the
version in `app/release.toml` or `app/Cargo.toml` (fixed at 0.0.0), and never close the
`## [Unreleased]` section of `CHANGELOG.md`. A train writes both files in one release commit.

When your pull request finishes a todo on the project chain, say so on its own line of the
description:

```
Completes: calendarlinks
```

Name each todo by its id; separate several with spaces or commas. The train checks each id
against the chain and skips a pull request that names a todo the chain does not hold. When the
train lands, it marks each todo done and lists it first in the release's changelog section.
Name only a todo that your change finishes. A pull request that only moves a todo forward has no
`Completes:` line.
