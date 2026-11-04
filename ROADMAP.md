# Roadmap

**The roadmap is a Prodrome** (since 2026-09-09): the project's work — plan,
audit leftovers, issues the system notices about itself — is the `suzatary`
chain (`events-suzatary/`), priced with the specs of the owner's todos and shown
by the same app under its chain switcher:

```console
$ nix develop .#triage -c scripts/app-bin.sh todos --chain suzatary open
```

That list is the queue, ascending in urgency, each item with its rationale.
Triage files system findings into it (`OPERATOR.md`, "Which list a todo goes
in"); the assistant session closes them with evidence in the note. A
rejected idea there is a cancelled item with the reason in its note. The
crate's roadmap lives the same way, in the public repository's `roadmap/`.

Shipped work: `CHANGELOG.md`. Constitution: `CLAUDE.md`. The app's lens:
`docs/APP.md` (2026-09-12; destinations graded by need, flows, what should
and must not exist), against which its chain items are priced. This file
keeps the 2.0 lens and the rejections that predate the chain.

## 2.0: a second host

The owner, 2026-09-04: "we are very slowly also heading towards a goal of anyone
else hosting the suzatary. i think that might be the goal for 2.0." A lens,
not a task: every change is judged partly by whether it moves deployment
knowledge into `instance.py` or out of it. Already true: `instance.py` is the
one module that knows this deployment; the API is the product and clients
are thin; the toolchain is a flake; secrets are declared in
`secretspec.toml`; identity is the tailnet's. What a second host hits today,
in order:

1. **Home paths in scripts and units** — the box's home in
   `scripts/{serve-web,safe-push,hourly-triage,voice-server,git-credential-secretspec}.sh`
   and `nix/services.nix`; `REPO`/`HOME` should come from one place the
   NixOS module sets.
2. **The person in the prompts** — the briefing prompt (the owner's city,
   his name, the places whose news he follows) is in the nixos config; the
   triage system prompt names accounts and categories. Instance data: a `persona`/`briefing` conf
   beside `CATEGORIES`.
3. **Mail accounts** — `mail/accounts.py` and `mbsyncrc` are this
   deployment's roster; the sync machinery is generic, the roster is not
   separated from it.
4. **Identity** — `WEB_VIEWERS`/`WEB_ACTORS` assume a Tailscale login. Keep
   it first-class (it is why there is no auth code to get wrong) and state it
   as a requirement; build no second scheme.
5. **The agent's subscription** — hourly triage needs a Claude login; a
   hoster brings their own. Document, don't abstract.
6. **A LICENSE** — settled 2026-09-10: MIT OR Apache-2.0, two files at the
   root. The repository stays private; a licence is not a publication.
7. **A `suzatary init`** that creates `instance.py` from answers (actors,
   categories, accounts, viewers) is the last step: honest only once
   everything it writes is actually read from there.

Not for 2.0: multi-tenant (one host, one person, one chain), a hosted
service (they run it), a DAG (one writer per host is the stated limit).

## One tabulation of rendered values

Recorded 2026-10-07 by app-as-reading stage 4 (#113's review), to be filed
on the `suzatary` chain by the main session. The box keeps three tables of
rendered values (the fragments and answer blocks on disk, the printed pages
in memory, the closed world's answers with their refusals) and the tab a
table in front of its IndexedDB replica beside the list's fold table: one
structure (the Prodrome's `memo::Cache`, keyed by function and argument),
not yet one tabulation, because a cache is monomorphic in its value and the
three hold three value types with three persistences. What makes it one: a
function declared as a value (its name, its tier, its codec to bytes), one
`Table<Bytes>` keyed by `Key` whose tiers each function chooses, and the box
handing a tab pairs by key rather than HTML beside a field; the tab's fold
table joins it once a node's drawing is a box function too. A series of its
own: it changes every caller of `Render` and the wire.

## Rejected

Kept on purpose: a deleted idea gets re-proposed; a rejected one with its
reason does not.

- **Lean spec/proof layer** — dropped by the owner 2026-08-31. The calibration
  tests pin the semantics.
- **Haskell rewrite** — declined 2026-08-31 for staying in Python. The FPL
  evaluator is not where the difficulty lives.
- **Persistent memory for the triage agent** — rejected 2026-08-30. Runs are
  stateless by design: memory would make a mail injection durable instead
  of one-run. Continuity lives in reviewed data.
- **A send MCP tool** — never. Sending is structurally human-only and
  test-pinned as absent.
- **Committing the rendered document, and releasing PDFs** — ended
  2026-09-02. A rendered tree is a function of the chain and the clock;
  checking it in was a second copy and a daily commit about it. The PDF
  itself went on 2026-10-09 (the owner: "We don't need the pdfs anymore"),
  and the static site `suzatary web` went on 2026-11-04 (nobody read it).
- **A tree object / snapshot commit on the chain** — rejected for 1.0
  (2026-09-02): with one writer and ~500 events the fold is microseconds,
  and a snapshot is a derived index the writer must keep consistent with
  the events — the dual-write class the inversion removed. Revisit if a
  DAG needs a merge base (the DAG of 2026-09-04 needed none: a merge names
  its parents only).
- **Promoting subtodos to todos** — 2026-09-02. Checklists are content;
  dependent todos already exist as `After` terms on the chain, the
  mechanism a parent relation would want. Revisit if a checklist is ever
  authored open.
- **`None` for absent fields on shipped kinds** — rejected 2026-09-02; `""`
  is the sentinel with one meaning, and 239 objects print it.
- **JSON projection files beside the chain** (`todo-state.json`,
  `todo-authors.json`, `categories.json`) — removed 2026-09-02. They let
  typst read a fold from the side; a block is now rendered from the fold,
  carrying `state`/`resolvedAt`/`authoredBy`, and the enum is
  Python conf with a generated typst copy. The database is typed literals.
- **JSON as the storage format** — rejected 2026-08-31 ("I don't like that
  json is so untyped"), as was a second config implementation beside the
  Python types; the constructor fragment of Python came out of that.
