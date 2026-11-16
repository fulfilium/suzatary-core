# A kept form of the first read (`foldkept`), 2026-11-15

A design note, and no code yet: the owner reviews this before any of it is
built. It comes after the series that removed the waste (#504 to #508 and the
pin, #513, released as 2.1.143), because the first read of a new process is
still over 2 s.

## What is left

On the deployment at 2.1.143 (46,146 line objects, 10,412 slots, 143
conversations), a new process's first read takes 6.1 to 6.9 s, and its parts
sum to 98.6% of the whole. One of its three runs (figures on #504):

```
first read: record 0, objects 1202, files 63, dag 487, excluded 58, graph 130, fresh 63, keys 137 x47624, gone 0, ring 27, hold 580, dead 88, nodes 574 x143, open 147 x46163, words 754 x37402, close 510 x143, gather 82, fold 1117 ms=6019 whole=6108
```

| part | ms on the deployment | what it is |
| --- | --- | --- |
| `objects` | 1,200–1,630 | the listing, and every file read, hashed and parsed, over the cores |
| `fold` | 1,060–1,220 | the walk of the memoised fold, less its leaves' parts: each leaf's reseals ordered and asked which seal stands |
| `words` | 690–860 | 37,402 asks of a slot's words, 10,412 of which open (see below) |
| `hold` | 570–670 | each object decoded into the held lines: the edges, the history, each conversation's lines, each key's owners |
| `nodes` | 570–720 | each conversation's leaves built and named |
| `dag` | 490–620 | the Prodrome's DAG over every object, cloned from the files read |
| `close`, `gather` | 420–590 | each conversation's transcript, and the digest's gathering |
| `graph`, `excluded`, `fresh`, `keys`, `open`, `dead`, `files`, `ring` | 650–700 together | ordering, holes, copies, key-store reads, entries, dead keys |

None of these is waste a read does not need; each is paid by every object,
husks included. The cost is the store's size, read from nothing in each
process.

## What a kept form keeps

Two things, kept apart, because only one of them holds words.

1. **The skeleton (no words).** For each object of the lines store: its name,
   its parents, its conversation, its shape header and the names of its
   slots' keys. For each conversation: the order its build reads and, for each
   slot, which seal stands, given which keys are held. This is everything
   `objects`, `dag`, `graph`, `hold`, `nodes` and the walk part of `fold`
   derive from the files, 4.0 to 4.8 s of a 6.1 to 6.9 s read on the
   deployment. None of it is a word: the store already publishes all of it in
   the clear (`skeleton=` in `secretary readings`).
   - The file is named by the build's identity. Inside, it records the
     store's `Heads` and the digest of the set of key names held, since which
     seal stands depends on which keys are held.
   - A read takes it where both match: the record gives the heads with no
     listing, and the key store's names give the key set. Otherwise the read
     proceeds as today and writes a new one.
   - Expected first read with a matching skeleton: `record` + loading the
     skeleton (one file, about 46k entries) + `keys` + `words` + `open` +
     `close` + `gather`. That is about 1.5 to 2.0 s on the deployment, and
     less once the asks below are answered without work.
2. **Opened words (not proposed).** If a measurement ever asks for them, the
   rules settled in the comments of #496 hold: an entry is one slot's, keyed by
   the slot (its key's name) alone; the sweep holds the file's lock from its
   forget to the key's destruction; a slot's text is named by the sealed
   format's and the rules' versions; the file lives in the box's state
   directory, 0600, out of reach of the vault's list and the push's paths.

The skeleton is a derived value with no words. Of those rules it takes the
naming by the build's identity and the place (the state directory, 0600, out
of the vault's list and the push's paths), so that no restore or push carries
it. The privacy engineer's question for it: can it outlive a key? It names
keys and holds none, and a key name opens nothing. A key's destruction
changes the key set it records, so the next read does not take it. The sweep
need not touch it.

## How often it matches

The skeleton matches only a process that starts while the lines and the keys
are as the last process left them, since every line written moves both. The
deployment's counts (on #509): lines are written on 3 to 11 occasions a day
(55 to 144 new line objects on an ordinary day), while the timer's pass alone
starts processes 288 times a day, each pass several commands. So nearly every
first read finds the lines as the last process left them, and the whole-match
form is the design.

Extending a skeleton by what changed is a later step, and only if a
measurement asks for it. That means reading only the new objects (walking
parents back from the record's tips until a name the skeleton holds) and
recomputing the seal plan only for the conversations whose keys moved (which
`owners` maps). The first read after a write pays today's cost once.

## The 27,000 `words` asks that open nothing

`words` counts each ask of a slot's words that the leaf's reader (`Once`) has
not answered before. On the deployment there are 37,402 asks for 10,412
slots that open. The other 27,000 come from finding which seal stands:

- A slot sealed again forms a chain: the first seal, then each reseal sealed
  from the one before. Deciding whether a seal is superseded
  (`transcript::supersedes`) walks the chain and asks each reseal whether its
  slot opens (`LineEvent::slot_opens`) until one does.
- After a sweep, every seal but the last has its key destroyed and its line
  husked (a `Seal::Dropped`). So each chain of n reseals asks n − 1 seals
  that answer "missing" before the last one opens. The deployment's 27,000
  are those husked seals: on the scratch store, with chains of three, it is
  two per slot (30,551 asks for about 10,400 slots).

**They can be answered without work.** Each such ask today prints the line's
binding (`LineEvent::slot`: the whole bare line through the literal printer)
before the reader looks at the key. The reader then answers "missing" from
the key's name alone: the leaf's slot says `Key::Missing`, or the seal is
`Dropped` and has no bytes. No `Slots` reader reads the `binding` or `sealed`
arguments: the three impls (the key-store closure, `Once`, `Through`) only
pass them on or use the `open` closure. So the binding can be made inside
`open`, only when a key is held and the seal has bytes, and an ask that ends
"missing" costs a map lookup.

That is a pull request of its own, before step 1: it shrinks `words` with or
without a skeleton. With a skeleton, the seal plan records which seal stands,
so those asks are not made at all.

## The server's start: `notice` and `erase.texts`

On the deployment the start is 13.7 s, and neither of its two largest steps
is the lines' read:

```
serve start: warm 0, reconcile 119, erase.forgotten 11, erase.lines 2248, erase.texts 5025, erase.uploads 137, default 116, notice 5836, waiting 213 whole=13710
```

- **`notice` (5.8 s)** is `State::notice`, run once at the start and then
  hourly and whenever a model moves. It reads the sources' health and the
  instance file, then forgets every remembered secret answer
  (`secrets_answered.forget()`) and resolves every declared secret afresh
  through the secret store. With `pass` behind `secretspec`, that is one
  subprocess and one GPG decryption per secret. It then folds the inbox,
  files each owed card (a write of lines, then a read), and withdraws the
  trusted devices' cards. The lines' read is a small part of it.
- **`erase.texts` (5.0 s at this start, 1.6 s at the one before)** is
  `Memories::sweep`. Under the memory's hold, it checks that the memory, the
  inbox, the digests and the lines each read whole (a store whose read left
  anything out deletes nothing). It then reads every naming of a fact (the
  memory's history and the inbox) and of a summary (the digest's history),
  and deletes each text something named once and nothing live names now. Its
  rise at this start may be the renders after the pin moved, as #504 says; the
  next start without a pin move will tell.

**Neither belongs to this note.** The skeleton keeps the lines' reading, and
neither step spends its time there. Each is a pull request of its own, in the
order the series used:

1. Name its parts until they sum to the whole (secrets, the inbox fold, the
   filing for `notice`; each store's read and each naming for
   `erase.texts`), and measure on the deployment.
2. Remove what a start does not need. Likely candidates are resolving every
   secret at each start, where the remembered answers or the names alone
   would do, and sweeping every text at each start, where only a forget or a
   write since the last sweep can make a text dead. The measurement decides.

Both can go before or beside step 1 of this note; neither depends on it.

## Types

- `Skeleton`: a value of `prodrome_core`-free fields, built by a pure function
  from the held lines (`HeldLines::skeleton`) and read back into them
  (`HeldLines::from_skeleton`), with one law: reading a store, or reading its
  skeleton, gives the same `secretary readings` fingerprint. That law is the
  standing test (AGENTS.md §8: a stored format).
- `SkeletonName`: the build identity, hashed: the file's name. A file of
  another build is never parsed. The skeleton inside records its `Heads` and
  its key-set digest, and a read takes it only where both match the stores.
- The file is a stored format of its own, versioned by the build identity, so
  a new build reads none and writes its own: no migration, no backward
  compatibility (AGENTS.md §10).

## The one general piece

The skeleton is an instance of a store's kept reading, named by its heads: the
Prodrome's `memo::File` already keeps a memoised fold's entries on disk by a
name. The skeleton is that file over a fold whose entries hold no words, so
it belongs in Prodrome as a kept graph (heads, order, parents, headers), and
only the slot plan, which names our keys, stays here. Proposed as the module
`prodrome_core::store::kept` (or beside `memo::file`), free of any Suzatary
type.

## Order of the work, once reviewed

0. Before it, on their own: the `words` asks answered without work (above),
   and, apart from this note, `notice` and `erase.texts` split and trimmed.
1. `Skeleton` and its round-trip law, in memory only.
2. The file: its name, its place, its 0600, refused by the vault's list and
   the push's paths, with a guard test for each.
3. The read takes it where its heads and key set match the stores; the
   fingerprint law over the deployment-shaped fixture, with a line written
   and a key destroyed between two reads (each makes the next read take
   none).
4. Measure on the scratch store and on the deployment; decide on extending it,
   and on opened words, only then.
