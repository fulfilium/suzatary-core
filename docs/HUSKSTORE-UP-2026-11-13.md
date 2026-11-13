# The husked store moves up (2026-11-13)

The plan for the todo `huskstoreup`. Delete this file once the move lands.

## Where it stands

Since 2.1.123 (#454, #455, #456) the box reads its lines through `suzatary_io::husked::Husked`.
It is the Prodrome's `EventStore` with one more case: a file that does not hash to its name, and
reads as a `HuskObject`, is the object it reads as. Two pieces carry it:

- `app/core/src/store/husk.rs`: `Husking`, `Husk`, `HuskObject` and `NotAHusk`, the general
  piece, which already names no Suzatary type.
- `app/io/src/husked.rs`, the store, and `app/io/src/object_files.rs`, the trait through which
  `Objects<E, F>` keeps its objects in either store.

Until the Prodrome learns husks, `husked.rs` is a second store beside the Prodrome's. The
context ratchet was raised by hand by 15,065 tokens to carry it (`context.toml`, the line above
`reading`). This move pays that back.

## What `husked.rs` copies, what is new, and what each part becomes

| part of `husked.rs` | from the Prodrome's store | new or different | in the Prodrome it becomes |
|---|---|---|---|
| `listing` | `EventStore::listing`: `<name>.py` objects, every other entry as garbage | sees a file by the inode its directory lists, not by `stat` | `EventStore::listing` and `look` |
| `kept` (read one file) | `dag::decode`: rehash, then parse at the schema | **new:** a file that fails its hash and reads as a `HuskObject` is that object | `decode` answering a husk for a `Husking` schema, beside `Unread::Tampered` |
| `history` | `Dag::from_prints`, then `interior` | husks join the history as their objects | `Dag::read_prints` with husks |
| `memory` (the look) | `EventStore::look` with `Memory::level` | an unread file is read on every look, as `level` does. A change another writer made rebuilds the reading from the files held, where `Memory::admit` extends in place | `look` and `Memory` |
| `spread` | `spread` | a copy | `spread` |
| `lock` | the `flock` on `root/.lock` (`EventStore::lock`) | the same file, taken through `durable::hold` | `EventStore::lock` |
| `writing` (decide and place) | `EventStore::decision` (lock, look, `Dag::whole`), `Decision::append`, `place` | the decision is an `Overlay` over a reading kept across writes. A run's objects are placed at its end, where `Decision` writes each as it goes. `place` writes no `.gitattributes` | `Decision` over `Memory`; the overlay goes |
| `Read` (the overlay's base) | none | **new glue:** a replica that answers what it was made with | nothing; `Memory` is the base |
| `Replica` (`held`, `tips`, `print`, `append`, `receive`) | `impl Replica for EventStore` | `print` answers nothing for a husk, which holds no bytes its name is the hash of. `receive` walks no further than a name the store holds, whole or husk | `impl Replica for EventStore` |
| `append_each` | `Objects::append_each` over `EventStore::decide` | one look and one placement per run | `Decision` |
| `init` | `EventStore::init` | calls it: an empty store holds no husk | `EventStore::init` |
| `findings` (verify) | `EventStore::verify`: a cold read, `Dag::verify`, garbage | reads each husk. **Missing:** `Leftover` (`HEAD`, `refs/`), quarantine receipts and temps | `EventStore::verify` |
| `husk` (the prune) | none | **new:** each whole replaced by its husk, one file at a time (temp, sync, rename, directory sync) under the lock; a husk is passed over | `EventStore::husk(names)` |

## The differences the Prodrome should take too

Each is something `husked.rs` does differently from `EventStore`, apart from the husk case.
The Prodrome takes it, or the Prodrome PR says why not.

1. **The inode listing.** `husked.rs` sees a file by the inode its directory lists, and calls
   no `stat`. A whole is written once and a husk replaces it by a rename, so either moves the
   inode. A file written over in place is read again by the next process, and `verify` reads
   every file afresh. `EventStore` stats each file and keeps a racy window. Take the inode, or
   keep `stat` and say what the inode misses.
2. **A run decided over one look.** `append_each` decides every event of a run over one look and
   places them all at its end; those decided before a refusal are placed too. A run of 10,000
   reseals (`migrate-scrub --apply`) costs one look, not 10,000.
3. **`verify` without leftovers.** `husked.rs` does not report `HEAD`, `refs/`, quarantine
   receipts or temps, because the lines' store never had them. The Prodrome keeps its own
   findings; this is a gap in `husked.rs`, not a behaviour to take.
4. **`.gitattributes` through `init` alone.** `husked.rs` writes the store's `.gitattributes`
   only when `init` begins it; `EventStore::place` writes it on any placement that finds none.
   The Prodrome keeps its own rule.
5. **`spread`.** `husked.rs` copies the Prodrome's `spread`, which is private. It goes with the
   move.

The kept overlay is not a difference: in the Prodrome it is `Memory` itself.

## What the Prodrome must decide

- **How a schema says it has husks.** `EventStore<E>` is generic over `Schema`. Either
  `Husking` becomes a supertrait the store asks for only to read husks, or `Schema` gains a
  default method that answers no husk. Only a schema that implements it reads a husk; every
  other store refuses one as `Tampered`, as today.
- **SPEC.** A husk is a stored form the database's contract does not name. SPEC §3 states what a
  store that holds one has verified of its name and what it has not. The table in
  `app/core/src/store/husk.rs`'s module doc is the text to start from. The frozen
  `conformance/` vectors stay as they are; a husk adds vectors of its own.

## The order of the work

1. **A Prodrome pull request** (fulfilium/prodrome):
   - `core/src/store/husk.rs` from this repository's `app/core/src/store/husk.rs`, unchanged but
     for its paths;
   - `EventStore` learning the case per the table, and taking the differences above;
   - `EventStore::husk(names)`;
   - the laws: the husk law (`held_lines::a_line_whose_keys_are_gone_reads_the_same_as_its_husk`,
     its `HuskObject` half), the prune law and the never-take-back law (`app/cli/tests/husked.rs`),
     restated over the Prodrome's test schema;
   - SPEC §3.
2. **This repository's pin**, one pull request once the Prodrome's lands. It moves the sha in
   `app/Cargo.toml` and `flake.nix` together and:
   - deletes `app/io/src/husked.rs`, `app/io/src/object_files.rs` and
     `app/core/src/store/husk.rs`;
   - returns `Objects` to `Objects<E>` over `EventStore`. `ConversationStores` opens the lines
     with `Objects::open`, and `Objects::husk` calls `EventStore::husk`;
   - keeps `impl Husking for LineEvent` here, now naming the Prodrome's trait;
   - keeps `app/cli/tests/husked.rs`, the census, `prune-lines`, `verify --husked`,
     `secretary readings` and `safe-push.sh` as they are, over the Prodrome's store;
   - runs `scripts/context.sh record`, which lowers `ratchet.reading` by the deleted files'
     tokens (about 6,200 for `husked.rs`, 900 for `object_files.rs` and 2,900 for `husk.rs` as
     measured in November), and deletes the hand-raise comment above `reading`;
   - deletes this file.

Run the proof on a copy of the real store with the pin's release binary before its gate, as for
2.1.123: `secretary readings` before and after `prune-lines --apply` on a store not yet pruned,
the same line; then `verify` and `verify --husked`.
