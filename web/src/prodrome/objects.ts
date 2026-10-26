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
import { NOTHING, NO_FRONTIER, canonical, join, losses, replicaOf, reset } from "./replica";

export const DB_NAME = "suzatary";
const STORE_NAME = "chain";
const CONTENT_NAME = "content";
const LINES_NAME = "lines";

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
 * A row this build does not read is never read as one it does: a device holding rows from before version 3 drops them
 * all, and pulls each chain and store again from the box. What a dropped row held that the box had not taken is lost.
 * Version 3's rows are carried: a word of the box that kept `null` for "did not say" is dropped, so the replica reads
 * as unheard until its next pull, and every object it held stays. Version 5 keeps the lines as a skeleton beside a
 * store of content by name, so the lines' row and its pieces from before it are dropped: the box alone writes the
 * lines, so nothing in them is this device's own; the blocks rendered from them are cleared too. Version 6 names where
 * each replica stands with the box by the frontier its last answer named (`replica.Frontier`), so every row is carried
 * standing nowhere, and its next pull asks from nothing. Version 7 keeps the lines' skeleton one node per row in a
 * store of its own (`linerows`), so a landing writes what arrived; the core's worker moves a version 6 lines' row into
 * it and then deletes that row.
 */
const DB: idb.Db = {
  name: DB_NAME,
  version: 7,
  store: STORE_NAME,
  upgrade: (db, tx, from) => {
    if (from >= 3 && from <= 6) {
      const rows = tx.objectStore(STORE_NAME);
      if (from < 5) {
        rows.delete(under(LINES));
        rows.put(true, keyFor(RENDERED_OWED));
      }
      if (from < 6) carry(rows, (row) => framed(from === 3 ? carried(row) : row));
    } else {
      if (db.objectStoreNames.contains(STORE_NAME)) db.deleteObjectStore(STORE_NAME);
      db.createObjectStore(STORE_NAME);
      // A device at version 1 or 2 rendered blocks from lines its rows no longer hold: cleared once, as from 3 or 4.
      if (from > 0) tx.objectStore(STORE_NAME).put(true, keyFor(RENDERED_OWED));
    }
    if (!db.objectStoreNames.contains(CONTENT_NAME)) db.createObjectStore(CONTENT_NAME);
    if (!db.objectStoreNames.contains(LINES_NAME)) db.createObjectStore(LINES_NAME);
  },
};

/** The lines' row and its pieces' rows, by the prefix of their names. */
const LINES = "store/lines";

/**
 * A mark the upgrade leaves for the tab: the blocks rendered from the dropped lines are still in the rendered store,
 * another database, so the tab clears them once (`storesync`).
 */
export const RENDERED_OWED = "store/rendered-owed";

/** The device's content by name: one literal per object name, written once and never rewritten. */
export const CONTENT: idb.Db = { ...DB, store: CONTENT_NAME };

/** The lines' skeleton, one row per node and per key beside a head row (`linerows`). */
export const LINE_ROWS: idb.Db = { ...DB, store: LINES_NAME };

/** Every row rewritten by `step`, in the upgrade's own transaction. */
function carry(store: IDBObjectStore, step: (row: unknown) => unknown): void {
  const cursor = store.openCursor();
  cursor.onsuccess = () => {
    const at = cursor.result;
    if (at === null) return;
    at.update(step(at.value));
    at.continue();
  };
}

/**
 * A row from before version 6 standing nowhere with the box: its replica's next pull asks from nothing, once, and every
 * object it holds stays. A chain's row is a replica, a store's holds one.
 */
export function framed(row: unknown): unknown {
  if (typeof row !== "object" || row === null) return row;
  if ("replica" in row) return { ...row, replica: framed(row.replica) };
  return "objects" in row && !("frontier" in row) ? { ...row, frontier: NO_FRONTIER } : row;
}

/** A version 3 row with every word that named no geneses left out: a chain's row is a replica, a store's holds one. */
export function carried(row: unknown): unknown {
  if (typeof row !== "object" || row === null) return row;
  if ("replica" in row) return { ...row, replica: carried(row.replica) };
  if (!("words" in row) || !Array.isArray(row.words)) return row;
  const words: unknown[] = row.words;
  return { ...row, words: words.filter((w) => typeof w === "object" && w !== null && "geneses" in w && w.geneses !== null) };
}

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

/**
 * The row deleted, under a reset's own rule: never one holding an unsent object the reader was not shown. What a build
 * no longer reads goes this way, so a copy of words the box no longer serves does not stay on the device.
 */
export const dropRow = <A>(rows: Rows<A>, name: string, shown: ReadonlySet<string>): Promise<lo.Settled<null>> =>
  idb
    .within(DB, "readwrite", async (store) => {
      const raw: unknown = await idb.done(store.get(keyFor(name)));
      if (raw === undefined) return lo.val(null);
      const may = written(rows, rows.parse(raw), raw, { type: "Reset", shown, floor: 0, over: "Unread" });
      if (may.type === "Val") await idb.done(store.delete(keyFor(name)));
      return lo.map(may, () => null);
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

/**
 * Every row whose name starts with `prefix`, in name order, each read the way `rows` reads one, in one read-only
 * transaction: what a run of rows written one at a time holds. A row that does not read is named, never passed over.
 */
export const heldRows = <A>(rows: Pick<Rows<A>, "parse">, prefix: string): Promise<{ readonly rows: readonly A[]; readonly unread: readonly string[] }> =>
  !idb.available()
    ? Promise.resolve({ rows: [], unread: [] })
    : idb
        .within(DB, "readonly", (store): Promise<readonly unknown[]> => idb.done(store.getAll(under(prefix))))
        .then(
          (raw) => {
            const read = raw.map(rows.parse);
            return { rows: read.flatMap((r) => (r.type === "Val" ? [r.v] : [])), unread: read.flatMap((r) => (r.type === "Error" ? [r.e] : [])) };
          },
          (error: unknown) => ({ rows: [], unread: [lo.reason(error)] }),
        );

/** Every row whose name starts with `prefix` deleted, in one read-write transaction. */
export const dropRows = (prefix: string): Promise<lo.Settled<null>> =>
  idb
    .within(DB, "readwrite", async (store) => {
      await idb.done(store.delete(under(prefix)));
      return lo.val(null);
    })
    .catch((error: unknown) => lo.error(lo.reason(error)));
