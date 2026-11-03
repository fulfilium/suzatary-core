/**
 * The replica kept in IndexedDB: objects, not a fold, so an offline list is today's answer from yesterday's
 * chain; and it holds this browser's own acts, so losing it loses writes the box has not taken.
 * IndexedDB rather than `localStorage`, which is synchronous and too small for megabytes of chain.
 */

import d from "../wire/decoder";
import lo from "../algebra/lo";
import idb from "../app/idb";
import locked from "./locked";
import payload from "../wire/generated/payload";
import type { Frontier, Held, Obj, Replica, Word } from "./replica";
import type { StoredObject } from "../wire/generated/wire";
import { NOTHING, canonical, join, losses, replicaOf, reset } from "./replica";

export const DB_NAME = "suzatary";
const STORE_NAME = "chain";
const CONTENT_NAME = "content";
const INBOX_NAME = "inbox";
const TOPICS_NAME = "topics";
const ALIASES_NAME = "topic-aliases";
const REPLICA_NAME = "replica";

/**
 * One row per chain, which only climbs: every tab of this origin joins into it, so none erases what another sealed.
 * The chain is a parameter, never `App.chain` at write time: an exchange in flight when the reader
 * switches would file one chain's objects under the other's key.
 */
function keyFor(chain: string): string {
  return `objects:${chain}`;
}

/** A word of the box as a row keeps it. */
export const Spoken = d.obj<Word>({
  epoch: d.num,
  heads: d.arr(d.str),
  untrusted: d.arr(d.str),
  root: d.nullable(d.str),
  fetchedAt: d.num,
  geneses: d.arr(d.str),
});

/** Where a replica stands with the box, as a row keeps it. */
export const Standing: d.Decoder<Frontier> = d.sum<Frontier, "type">("type", { Nothing: {}, Named: { digest: d.str, count: d.num, era: d.num } });

/** A replica's row, its objects read by `objects`. */
export const replicaRow = <O extends Obj>(objects: d.Decoder<O>): d.Decoder<Held<O>> =>
  d.obj<Held<O>>({
    generation: d.num,
    objects: d.arr(objects),
    synced: d.arr(d.str),
    refused: d.arr(payload.RefusedObject),
    words: d.arr(Spoken),
    frontier: Standing,
  });

export const Stored = replicaRow(payload.StoredObject);

/** A row holding no objects is a value too: a reset's, which is what tells another tab of it. */
export const parseReplica = (raw: unknown): lo.Settled<Replica> => lo.map(Stored(raw), (held) => replicaOf(canonical(held)));

// --- the shell: IndexedDB ----------------------------------------------------

/**
 * The device keeps no format but this build's, and nothing is carried from another: a store this build does not read
 * is taken again from the box. The one thing a device holds that the box does not is what this tab wrote and the box
 * has not taken, in a chain's row, so the rows' store of version 8 or later, which is this build's, is kept; its rows
 * that are no chain's, which a build before this one kept, are deleted, and every other store goes and is made anew. A
 * device before version 8 keeps nothing. Version 11 keeps the lines as the core's own form (`replicalog`).
 */
const DB: idb.Db = {
  name: DB_NAME,
  version: 11,
  store: STORE_NAME,
  upgrade: (db, tx, from) => {
    const kept = from >= 8 && db.objectStoreNames.contains(STORE_NAME);
    for (const name of Array.from(db.objectStoreNames)) if (!(kept && name === STORE_NAME)) db.deleteObjectStore(name);
    if (kept) for (const prefix of OTHERS) tx.objectStore(STORE_NAME).delete(under(prefix));
    else db.createObjectStore(STORE_NAME);
    for (const name of [CONTENT_NAME, INBOX_NAME, TOPICS_NAME, ALIASES_NAME, REPLICA_NAME]) db.createObjectStore(name);
  },
};

/** The prefixes of the rows of the rows' store that are no chain's: every row a build before this one kept beside them. */
const OTHERS = ["store/", "pages/"];

/** The device's content by name: one literal per object name, written once and never rewritten. */
export const CONTENT: idb.Db = { ...DB, store: CONTENT_NAME };

/** Each store the core's worker holds as rows, one row per node beside a head row (`noderows`). */
export const INBOX_ROWS: idb.Db = { ...DB, store: INBOX_NAME };
export const TOPIC_ROWS: idb.Db = { ...DB, store: TOPICS_NAME };
export const ALIAS_ROWS: idb.Db = { ...DB, store: ALIASES_NAME };

/** The replicas the core keeps in its own form, a snapshot and the frames after it, by the store's name (`replicalog`). */
export const REPLICA: idb.Db = { ...DB, store: REPLICA_NAME };

/**
 * What a kind of row holds and how it reads, joins and starts over: a chain's row is its replica, a store's its replica
 * and its keys. Every row holds a replica, which is what a reset is checked against.
 */
export interface Rows<A> {
  readonly parse: (raw: unknown) => lo.Settled<A>;
  readonly print: (a: A) => unknown;
  readonly generation: (a: A) => number;
  /** What the row holds that the box has not taken: what a reset would lose. */
  readonly unsent: (a: A) => readonly StoredObject[];
  readonly join: (a: A, b: A) => A;
  /** The empty row of the generation after `generation`. */
  readonly after: (generation: number) => A;
  readonly nothing: A;
}

/** A store's row: its replica of `O`s and the keys of their words, a reset's the empty row of the generation after. */
export const storeRows = <O extends Obj>(objects: d.Decoder<O>, none: locked.Row<O>, unsent: (r: Replica<O>) => readonly StoredObject[]): Rows<locked.Row<O>> => {
  const Kept = d.obj<{ replica: Held<O>; keys: locked.Keys }>({ replica: replicaRow(objects), keys: d.rec(d.str) });
  return {
    parse: (raw) => lo.map(Kept(raw), (k) => locked.of(replicaOf(canonical(k.replica)), k.keys)),
    print: (r) => ({ replica: r.replica.held, keys: r.keys }),
    generation: (r) => r.replica.generation,
    unsent: (r) => unsent(r.replica),
    join: locked.join,
    after: (g) => locked.of(replicaOf<O>({ ...none.replica.held, generation: g + 1 })),
    nothing: none,
  };
};

export const CHAIN: Rows<Replica> = {
  parse: parseReplica,
  print: (r) => r.held,
  generation: (r) => r.generation,
  unsent: losses,
  join,
  after: (generation) => reset(replicaOf({ ...NOTHING.held, generation })),
  nothing: NOTHING,
};

/** What this device holds of a row: nothing, its value, or a row that does not read, and why. */
export type Read<A> = Exclude<idb.Row<A>, { readonly type: "Unavailable" }>;

/**
 * Never throws: a row that cannot be read says so, and is never taken for one that is not there. A store that does not
 * open is said the same way, with its reset, since the replica's acts are what this device would lose.
 */
export const heldRow = <A>(rows: Pick<Rows<A>, "parse">, name: string): Promise<Read<A>> =>
  idb.read(DB, keyFor(name), rows.parse).then((r): Read<A> => (r.type === "Unavailable" ? { type: "Unreadable", why: r.why } : r));

/**
 * A write to a row: what a replica holds, joined in; or the one way down, a reset, which names every unsent object it
 * was shown to lose and the generation the reader holds, and says whether it may stand in for a row that does not read.
 */
export type Write<A> =
  | { readonly type: "Join"; readonly replica: A }
  | { readonly type: "Reset"; readonly shown: ReadonlySet<string>; readonly floor: number; readonly over: "Read" | "Unread" };

/**
 * The write done in one read-write transaction, so two tabs saving at once both land; the row it now holds is the
 * answer. A join never writes over a row that does not read, since a join with nothing would shrink it. A reset keeps
 * nothing of the row, so it refuses one holding an unsent object the reader was not shown, and starts the generation
 * after the row's, so no copy from before it, a concurrent reset's included, reaches it. Every failure says why, since
 * an act that could not be kept is an act this browser would forget.
 */
export const holdRow = <A>(rows: Rows<A>, name: string, write: Write<A>): Promise<lo.Settled<A>> =>
  idb
    .within(DB, "readwrite", async (store) => {
      const raw: unknown = await idb.done(store.get(keyFor(name)));
      const now = written(rows, raw === undefined ? lo.val(rows.nothing) : rows.parse(raw), raw, write);
      if (now.type === "Val") await idb.done(store.put(rows.print(now.v), keyFor(name)));
      return now;
    })
    .catch((error: unknown) => lo.error(lo.reason(error)));

/**
 * `write` done, and what the row is now: on a failure the row is read again, so a caller says it does not read only when
 * a read says so (a store that does not open included, as `heldRow` says it), never for a refusal or a failed transaction
 * over a row that reads.
 */
export async function keepRow<A>(rows: Rows<A>, name: string, write: Write<A>): Promise<{ readonly row: lo.Settled<A>; readonly unread: string | null }> {
  const row = await holdRow(rows, name, write);
  if (row.type === "Val") return { row, unread: null };
  const read = await heldRow(rows, name);
  return { row, unread: read.type === "Unreadable" ? read.why : null };
}

/**
 * The row as `step` leaves it, in one read-write transaction, so what another tab joined in meanwhile stays: the one way
 * a row gives up part of itself, a purge's. A row that is not there, or does not read, is left as it is.
 */
export const alterRow = <A>(rows: Rows<A>, name: string, step: (a: A) => A): Promise<lo.Settled<null>> =>
  idb
    .within(DB, "readwrite", async (store) => {
      const raw: unknown = await idb.done(store.get(keyFor(name)));
      const row = raw === undefined ? null : rows.parse(raw);
      if (row !== null && row.type === "Val") await idb.done(store.put(rows.print(step(row.v)), keyFor(name)));
      return lo.val(null);
    })
    .catch((error: unknown) => lo.error(lo.reason(error)));

/** What `write` leaves in a row that read as `row`, or why it may not be written. */
function written<A>(rows: Rows<A>, row: lo.Settled<A>, raw: unknown, write: Write<A>): lo.Settled<A> {
  const unread = row.type === "Val" ? null : row.e;
  if (write.type === "Join") {
    return row.type === "Val" ? lo.val(rows.join(row.v, write.replica)) : lo.error(`the replica kept on this device does not read (${unread}), so nothing was written over it`);
  }
  if (row.type !== "Val" && write.over === "Read") return lo.error(`the replica kept on this device does not read (${unread}), so nothing was written over it`);
  const unshown = row.type === "Val" ? rows.unsent(row.v).filter((object) => !write.shown.has(object.hash)).length : 0;
  if (unshown > 0) {
    return lo.error(`this browser kept ${unshown} more unsent ${unshown === 1 ? "act" : "acts"} since the reset was offered, so nothing was reset: look again at what it loses`);
  }
  const was = row.type === "Val" ? rows.generation(row.v) : generationOf(raw);
  return lo.val(rows.after(Math.max(was, write.floor)));
}

/** The generation a row that does not read says it is of, read off its print; 0 for one that does not say. */
function generationOf(raw: unknown): number {
  const held = typeof raw === "object" && raw !== null && "replica" in raw ? raw.replica : raw;
  return typeof held === "object" && held !== null && "generation" in held && typeof held.generation === "number" ? held.generation : 0;
}

/** The names under `prefix`: a key range of the rows' store. */
const under = (prefix: string): IDBKeyRange => IDBKeyRange.bound(keyFor(prefix), `${keyFor(prefix)}￿`);
