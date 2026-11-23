# The io split, measured (2026-11-22)

The plan for the roadmap task `iosplit`. It follows the core split's method
(`CORESPLIT-PLAN-2026-11-06.md`): measure, cut along the graph the code already has, move files whole,
one crate per pull request, bottom first. Delete this file once `suzatary-io` is gone.

## What was measured, and how

- **The tool** is `train context order <unit> [<cut>]` (`app/tooling/src/context/order.rs`,
  through `scripts/context.sh order io docs/IOSPLIT-CUT.toml`). A top module is `src/<m>.rs` with
  the files under `src/<m>/`. Its **source** is its production code (each file up to
  `#[cfg(test)]`) and its **face** is `interface::rust` of each file, the measure's own rule; both
  in tokens by `context.toml`'s ratios. Its **names** are the first segments of its `crate::` paths
  (through `{...}` groups, comments left out), its **units** the units whose `suzatary_<unit>::`
  or `prodrome_*::` paths it holds, and its **users** the dependents' production files that hold
  `suzatary_io::<m>`. It reads paths, not impls: §2 names the edges only the compiler sees.
- **A cut** lists layers bottom first; the modules it does not list stay in `io`, above them. For
  each layer the tool gives its USE (the sum of its faces, no README) and its DEVELOP the way
  `measure` counts a unit: its source and the manuals, the documents they cite, and the USE of
  each unit or lower layer it names. It fails when a module names a module of a higher layer. On
  the merge base it gives 810k for `io` uncut against `scripts/context.sh`'s 813k (the README and
  the scripts are the rest).
- **The law** `components_are_the_mutually_reachable_classes_in_order` pins the cycle finder.
- All figures below are at `1ab9462` (2.1.217) unless a row says otherwise.

## 1. The face of io by module

`io` is 71 top modules, 229k tokens of source and 49.6k of face (50.5k with the README: twice
the 25k budget). DEVELOP is 813k: 229k of source, 563k of the 23 units it depends on, of which
prodrome-core alone is 140k. The ten largest faces:

| module | face | | module | face |
|---|---|---|---|---|
| `model` | 5.3k | | `http` | 1.7k |
| `mailbox` | 4.3k | | `durable` | 1.3k |
| `secrets` | 2.5k | | `setup_token` | 1.2k |
| `google` | 2.4k | | `notify` | 1.2k |
| `run` | 1.8k | | `objects` | 1.2k |

## 2. The order the modules have

The `crate::` graph has **one cycle**, `pullers` ↔ `sync`. Every other module sits in a DAG. What
each module charges its reader is mostly the units it names, not the modules: `clock` is 0.1k of
face and names `prodrome_core` (140k) for one call, `secrets` names `lines` (41k) for one
function, `briefings` names `secretary` (28k), and `notify` is the one module that names `cards`.
Four modules name no unit and no other module but each other: `present`, `durable`, `scratch` and
`tier`. Next above them, `run` names only `held`, and `tool` only `run`.

**What the graph does not show.** A trait implemented for a type, where the trait comes from one
crate and the type from another, can live only in one of those two crates. Two such impls tie
modules that no path ties: `objects.rs` held `impl WriteLock for durable::Held` (the trait is
`store`'s), and `repos.rs` holds `impl From<RunError> for RepoFailure` (the type is `services`').
The compiler found both on the first try of stage 1. Each moves as an item beside its type, so
`os` depends on `store`, and stage 2 moves the second into `run.rs`. A scan of io's other impls of
a foreign trait for a type of another module finds none whose two modules sit in different layers.

## 3. Who uses what

`k:n` is "n production files of unit k name it". The layer column is the cut of §4.

| module | layer | source | face | other units | used by | names in io |
|---|---|---|---|---|---|---|
| `durable` | os | 4.0 | 1.3 | — | cli:10 | present |
| `present` | os | 0.3 | 0.1 | — | cli:3 server:1 | — |
| `scratch` | os | 0.4 | 0.2 | — | cli:2 | — |
| `tier` | os | 0.2 | 0.1 | — | cli:1 | — |
| `run` | io-access | 6.5 | 1.8 | held | cli:2 | — |
| `tool` | io-access | 0.5 | 0.1 | — | — | run |
| `repository` | io-access | 1.4 | 0.3 | connect os services | cli:3 server:1 | run (at 2.1.220) |
| `api_error` | io-access | 0.3 | 0.2 | — | — | — |
| `github_tokens` | io-access | 2.2 | 0.3 | connect | cli:1 server:2 | http secrets |
| `http` | io-access | 5.3 | 1.7 | google services | cli:4 plugin:2 | — |
| `routine` | io-access | 2.0 | 0.9 | base connect inbox | cli:2 | http secrets |
| `secrets` | io-access | 13.5 | 2.5 | base connect lines | cli:8 plugin:1 server:2 | run |
| `tokens` | io-access | 0.8 | 0.4 | — | — | http |
| `attachments` | io-local | 3.3 | 1.1 | base held store | cli:2 server:3 | blobs docs pdftotext |
| `blobs` | io-local | 0.6 | 0.2 | held store | cli:3 | docs |
| `clock` | io-local | 0.2 | 0.1 | base prodrome-core | cli:17 | — |
| `docs` | io-local | 3.0 | 0.8 | store | cli:1 | durable present |
| `epochs` | io-local | 0.8 | 0.2 | signatures | cli:1 | docs |
| `frontiers` | io-local | 1.5 | 0.1 | prodrome-core store | cli:1 | docs |
| `health_file` | io-local | 0.3 | 0.1 | health store | — | docs |
| `manuals` | io-local | 0.5 | 0.2 | base | cli:2 | — |
| `model_health` | io-local | 0.8 | 0.4 | health store | cli:4 | docs health_file |
| `pairing` | io-local | 0.6 | 0.2 | signatures store | cli:2 | docs |
| `pdftotext` | io-local | 1.2 | 0.2 | held | cli:2 | run tool |
| `pulse` | io-local | 1.3 | 0.5 | store triage | cli:1 | docs |
| `requests` | io-local | 2.5 | 0.6 | prodrome-core signatures store | cli:3 server:1 | docs present |
| `source_health` | io-local | 0.3 | 0.2 | health | cli:4 | docs health_file |
| `state` | io-local | 2.2 | 0.7 | mail store triage | cli:4 | docs secrets tier |
| `tapped` | io-local | 0.6 | 0.2 | held | cli:2 | blobs docs |
| `text_keys` | io-local | 0.9 | 0.2 | memory store | — | docs durable |
| `texts` | io-local | 1.2 | 0.2 | base memory store | cli:4 server:1 | docs present |
| `upload_index` | io-local | 1.9 | 0.2 | held store | cli:2 | attachments docs present |
| `whois` | io-local | 2.1 | 0.2 | base | cli:2 | — |
| `render` | io-typeset | 2.5 | 0.5 | base prodrome-core todo topics | cli:4 | rendered typeset |
| `rendered` | io-typeset | 2.5 | 0.6 | base prodrome-core | — | durable present |
| `typeset` | io-typeset | 3.0 | 0.8 | base prodrome-typst-wasm todo | cli:10 | run |
| `unrendered` | io-typeset | 0.2 | 0.1 | base todo | cli:1 | — |
| `calendar` | io-chains | 0.6 | 0.2 | base google prodrome-core store | cli:1 | plugin_store |
| `conversations` | io-chains | 1.2 | 0.5 | conversations lines todo | cli:5 | objects runs text_keys texts |
| `devices` | io-chains | 0.9 | 0.2 | prodrome-core signatures | cli:3 | lines |
| `ignores` | io-chains | 2.3 | 1.0 | — | cli:3 | durable present |
| `lines` | io-chains | 3.3 | 0.8 | base store | — | durable present |
| `objects` | io-chains | 7.0 | 1.2 | digest lines prodrome-core record signatures store stores | cli:11 server:1 | durable |
| `outside` | io-chains | 2.3 | 0.1 | os prodrome-core | — | repository (new at 2.1.220) |
| `plugin_store` | io-chains | 2.0 | 1.0 | prodrome-core store | cli:2 server:1 | durable objects present |
| `reading` | io-chains | 0.3 | 0.2 | prodrome-core record store | cli:3 | objects |
| `runs` | io-chains | 0.8 | 0.0 | conversations lines store | — | texts |
| `topics` | io-chains | 0.5 | 0.3 | store topics | cli:2 | objects |
| `watches` | io-chains | 0.3 | 0.1 | triage | cli:2 | lines |
| `corpus` | io-mail | 3.2 | 0.5 | base health mail prodrome-core triage | cli:3 | clock durable mail_sync mailbox pulse source_health state sync watches |
| `gmail` | io-mail | 3.3 | 1.0 | base triage | — | http tokens |
| `google` | io-mail | 6.7 | 2.4 | base google mail prodrome-core store | cli:7 server:2 | durable http oauth plugin_store secrets |
| `imap` | io-mail | 4.8 | 0.9 | base mail | cli:1 | durable present |
| `junk` | io-mail | 4.0 | 0.4 | base mail triage | cli:1 | imap mailbox oauth |
| `mail_sync` | io-mail | 1.6 | 0.4 | base mail prodrome-core | cli:3 | present run |
| `mailbox` | io-mail | 27.0 | 4.3 | base held mail prodrome-core store triage | cli:5 | docs pdftotext present run state |
| `oauth` | io-mail | 5.3 | 0.9 | base mail | cli:5 | durable http present tokens |
| `older` | io-mail | 5.5 | 0.9 | base mail triage | cli:1 | durable gmail google http imap junk mailbox sync tokens |
| `pullers` | io-mail | 2.6 | 0.9 | mail | — | google imap oauth run secrets sync |
| `smtp` | io-mail | 9.9 | 1.1 | base mail | cli:1 | gmail http secrets tokens |
| `sync` | io-mail | 3.8 | 0.7 | held mail triage | cli:2 | durable google mailbox oauth present pullers run |
| `blocks` | io-model | 2.2 | 0.4 | conversations held | — | attachments blobs |
| `model` | io-model | 16.6 | 5.3 | base conversations health held inbox prodrome-core | cli:7 server:2 | attachments blobs blocks clock prompt run secrets |
| `prompt` | io-model | 4.1 | 0.2 | base conversations held | — | — |
| `setup_token` | io-model | 5.2 | 1.2 | base inbox | server:1 | model run |
| `usage` | io-model | 4.6 | 0.6 | services | cli:1 | durable http model secrets |
| `briefings` | io-sources | 1.0 | 0.2 | secretary | cli:1 | present |
| `drive` | io-sources | 4.7 | 0.7 | base held services | cli:2 | mailbox pdftotext run scratch secrets |
| `notify` | io-sources | 5.0 | 1.2 | base cards signatures store | cli:3 | http secrets tokens |
| `repos` | io-sources | 14.8 | 1.0 | base inbox memory services | cli:4 server:2 | http run scratch secrets |
| `weather` | io-sources | 0.7 | 0.2 | base services | cli:3 | http |
| `web` | io-sources | 2.8 | 0.2 | google services | cli:2 | http mailbox |

Source and face in thousands of tokens. `cli` names 63 of the 71 modules, `server` 12 and `plugin`
two (`http`, `secrets`). No leaf group is used by all three: the plugin uses only `io-access`.

## 4. The proposed units

The layers are the graph's: each names only layers below it, and the tool says so. Two placements
follow the charges of §2 rather than a concept: `clock`, `whois` and `manuals` join `io-local`, which
names prodrome-core anyway, so that `os` does not pay 140k for `clock`; `briefings` joins
`io-sources`, the one layer that already names a high unit, so that `io` empties. `run` and
`tool` join `io-access`, not `os`, because of the impl of §2 that must sit beside `RunError` in a
crate that names `services`; `os` then names no unit but `store`.

| crate | modules | source | USE | DEVELOP | depends on (beside lower layers) |
|---|---|---|---|---|---|
| `os` | 4 | 4.8 | 1.7 | 81 | store |
| `io-access` | 9 | 32.7 | 8.3 | 246 | base connect google held inbox lines services |
| `io-local` | 20 | 26.2 | 6.7 | 339 | base health held mail memory prodrome-core signatures store triage |
| `io-typeset` | 4 | 8.2 | 2.0 | 258 | base prodrome-core prodrome-typst-wasm todo topics |
| `io-chains` | 12 | 21.8 | 5.4 | 430 | base conversations digest google lines prodrome-core record signatures store stores todo topics triage |
| `io-mail` | 12 | 77.9 | 14.4 | 385 | base google health held mail prodrome-core store triage |
| `io-model` | 5 | 32.7 | 7.8 | 336 | base conversations health held inbox prodrome-core services |
| `io-sources` | 6 | 28.9 | 3.6 | 298 | base cards google held inbox memory secretary services signatures store |

The successors' rows are remeasured on 2.1.220, which added `outside` and made `objects` name it and `repository`; `repository` moved down to `io-access`, `outside` joined `io-chains`, and prodrome-core's USE fell, which lowered every DEVELOP that names it. In thousands of tokens; USE has no README, so a crate that adds one adds its length.

**The target is met**: no successor is over 25k to use (the largest, `io-mail`, is 14.4k) or 500k
to develop (the largest, `io-chains`, is 430k, of which 89k is prodrome-core at its 2.1.220 pin). Who reads what
after the last stage:

| dependent | reads | in place of io's 50.5k | its DEVELOP today |
|---|---|---|---|
| `plugin` | `io-access` | 8.3k (−40.7k) | 304k → 263k (at 2.1.220) |
| `server` | all but `io-typeset` | 47.6k (−2.9k) | 787k → 785k |
| `cli` | all eight | 49.6k (−0.9k) | 981k → 980k |

The split pays `io`'s own developers: an edit in any of its modules reads 81k to 430k instead of
813k. It does not bring `cli` or `server` under 500k; for them the lever is a narrower face (the
`pub` items of these crates that no dependent names) or a split of `cli` itself, which this plan
does not cover.

## 5. The stages

Each stage is one pull request that lands alone: `git mv` of whole files, a manifest and a root,
every path into them rewritten in the same pull request (io's own `crate::` paths, rustdoc links
among them, and every dependent's `suzatary_io::` paths), and the lists that name crates
(`app/Cargo.toml`, `flake.nix`'s `members`, `scripts/merge-gate.sh`'s `web_app_skips` and
`live_reads`, `context.toml`'s unit and its dependents' `depends`). A move changes an item's path,
never its name or its body; an item that is `pub(crate)` and used across the cut becomes `pub`,
and the pull request names each one. **No stage leaves a re-export**, so none has one to delete.
Each stage runs `scripts/context.sh order io docs/IOSPLIT-CUT.toml` first, and deletes its layer
from the cut.

| # | crate | files moved | waits for |
|---|---|---|---|
| 1 | `os` | 4 | — (this pull request) |
| 2 | `io-access` | 9; `impl From<RunError> for RepoFailure` moves into `run.rs`, and `run::end_group` (`setup_token`) and `tool::on_path` (tests of `attachments`, `upload_index`) become `pub` | stage 1 |
| 3 | `io-local` | 20 | stage 2 |
| 4 | `io-typeset` | 5 | stage 2 |
| 5 | `io-chains` | 12 | stage 3, and the changes in flight to `objects.rs`, `plugin_store.rs`, `ignores.rs` and the calendar |
| 6 | `io-mail` | 21 | stage 5, and the change in flight to `google.rs` |
| 7 | `io-model` | 11 | stage 3 |
| 8 | `io-sources` | 8 | stages 6 and 7; it empties `io`, which this pull request deletes |

Stages 2, 3 and 4 go together in one pull request. Stages 5 to 8 each follow the changes they wait for.

**Stage 1, measured** (this branch at its head, against the merge base):

| | before | after |
|---|---|---|
| `io` USE / DEVELOP / tests | 50,473 / 813.2k / 220.7k | 48,738 / 810.1k / 217.8k |
| `os` USE / DEVELOP / tests | — | 1,724 / 80.5k / 3.0k |
| `cli` USE / DEVELOP | 54.4k / 981.0k | 54.4k / 980.9k |
| `server` USE / DEVELOP | 13.8k / 787.4k | 13.8k / 787.4k |
| `plugin` USE / DEVELOP | 3.0k / 329.8k | 3.0k / 328.1k |

`cli` and `server` read `io` and `os` both, so they pay what they paid; `plugin` reads only
`io`, which is 1.7k smaller. The four files moved equal themselves at the merge base with `use`
lines and paths set aside, but for the one item moved into `durable.rs`. The build times are in
the pull request.

## 6. What argues against it

- **Its users gain almost nothing.** `cli` and `server` name seven or eight of the eight layers;
  their DEVELOP falls by 1k and 3k at the end, and by nothing at stage 1. Only `io`'s
  developers and the plugin gain. If the aim were `cli` and `server` under 500k, this is the wrong
  lever.
- **Each crate is its own reading cost.** A manifest is about 250 tokens and a root about 100;
  eight crates add about 3k to the reading set, and every dependent's `depends` grows by a word per
  crate it names.
- **Each crate is a row in every list and a binary in every test run.** Seven lists name crates
  (§5). Each crate with unit tests is one more test binary that `cargo test` links, in the gate's
  `test` and `test-fixture` steps.
- **Compile time** does not improve for the common edit: an edit in `io` rechecks `io` and its
  three dependents with or without the split, and an edit in a lower layer rechecks everything
  above it, as an edit in `io` does today.
- **A split by wish.** Some placements serve a charge, not a concept (§4), and `io-local` is the
  box's documents beside its clock. A finer split would make crates of 2k of source.

**Stop after stage 1 if**: the clean build or the gate's `test` step grows by more than 5%; or the
new crate adds more than 1k to the reading set beyond its manifest and root (each later stage would
cost the same); or no edit lands in `os` before stage 2 is due, since the reading it saves is paid
only when someone edits there. Of the 20 commits that touched `io` in this clone's history (since
2026-11-20), 6 touched `os`'s four files, and `durable.rs` was the file touched most.
