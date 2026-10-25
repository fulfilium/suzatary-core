/**
 * The phone is a replica: it holds objects and the heads they end at, an act is the core's `Change` over its
 * todo's heads in this replica, folded locally, and sync exchanges objects by name. Union of sets is idempotent, so a re-post needs no nonce, and
 * the local fold is the truth shown, never a guess; only whether the box has heard is uncertain.
 * Pure decisions over sets of names; the shells are `sync.ts` and `objects.ts`.
 */

import type { Frontier as Named, ReceiptPayload, RefusedObject, SinceRead, SkeletonNode, StoredObject } from "../wire/generated/wire";
import heads from "./heads";
import lo from "../algebra/lo";

/**
 * What a replica can hold: an object with its literal, or a skeleton's node, which names what it rests on and carries
 * no literal.
 */
export type Obj = StoredObject | SkeletonNode;

/**
 * What the box said at one contact: the epoch of what it served, its heads, the policy and root they are read under, and
 * the prodromes it holds. The epoch is the box's own clock over the stores it serves, so a drop or a rollback, which hold
 * less, are later words all the same.
 */
export interface Word {
  readonly epoch: number;
  readonly heads: readonly string[];
  readonly untrusted: readonly string[];
  readonly root: string | null;
  /** ms since the epoch, from this browser's clock. */
  readonly fetchedAt: number;
  /** The prodromes the box held then: none for a store that holds no object. */
  readonly geneses: readonly string[];
}

/**
 * Where a replica stands with the box: nothing the box named yet, or the frontier an answer named, which the next pull
 * names back as `since`. The era counts the answers that said the box no longer held the frontier asked from, so a
 * frontier named after such an answer stands over every one named before it.
 */
export type Frontier = { readonly type: "Nothing" } | { readonly type: "Named"; readonly digest: string; readonly count: number; readonly era: number };

export const NO_FRONTIER: Frontier = { type: "Nothing" };

/**
 * The later of two frontiers, a semilattice: the later era, then the larger count, then the greater digest. Each names
 * only objects the replica holding it holds, so either is safe in their union; the later is the one asked from.
 */
export const laterFrontier = (a: Frontier, b: Frontier): Frontier => {
  if (a.type === "Nothing") return b;
  if (b.type === "Nothing") return a;
  if (a.era !== b.era) return a.era > b.era ? a : b;
  if (a.count !== b.count) return a.count > b.count ? a : b;
  return a.digest >= b.digest ? a : b;
};

/** The frontier an answer names, of a replica that asked from `asked`: an era later when the box no longer held it. */
export const answered = (asked: Frontier, since: SinceRead, named: Named): Frontier => {
  const era = asked.type === "Named" ? asked.era : 0;
  return { type: "Named", digest: named.digest, count: named.count, era: since === "Unknown" ? era + 1 : era };
};

/** What a pull or a post names as `since`: the frontier's digest, or nothing. */
export const sinceOf = <O extends Obj>(replica: Replica<O>): string => (replica.held.frontier.type === "Named" ? replica.held.frontier.digest : "");

/**
 * What a replica holds, all of it: the state its join unites by plain union, and the one thing kept on the device.
 * `synced` is knowledge about the box's store, so it is remembered, not derived; being wrong costs one re-post the box
 * answers with `already`. A refused object is kept, because it is real and this replica holds it; so is a store the
 * box dropped, which a later word of the box may serve again.
 */
export interface Held<O extends Obj = StoredObject> {
  /** How many resets this replica has seen: a reset starts a later one, which no copy from before it reaches. */
  readonly generation: number;
  /** Every object, parents first. */
  readonly objects: readonly O[];
  /** The names the box is known to hold. */
  readonly synced: readonly string[];
  /** What the box refused, and why, one per name. */
  readonly refused: readonly RefusedObject[];
  /** What the box said, in key order, less a word whose heads another's rest strictly above: such a word never wins. */
  readonly words: readonly Word[];
  /** Where it stands with the box, as the last answer it took named it. */
  readonly frontier: Frontier;
}

/** Where the box stood at the contact a replica reads: not heard yet, or one word, its heads and its epoch together. */
export type Box = { readonly type: "Unheard" } | { readonly type: "Heard"; readonly word: Word };

export const UNHEARD: Box = { type: "Unheard" };

/** `heard` of the box's word, or `unheard` before the box has spoken. */
export const said = <R>(b: Box, unheard: R, heard: (w: Word) => R): R => (b.type === "Heard" ? heard(b.word) : unheard);

/**
 * A replica read: what it holds, and what that means, made only by `replicaOf`, so no reading is carried beside the
 * set it reads. The box's word is the one that has seen more, kept whole, so its heads are never read beside another
 * pull's epoch or objects; the objects are those of the prodromes it holds.
 */
export interface Replica<O extends Obj = StoredObject> {
  readonly held: Held<O>;
  /** Every object of a prodrome the box holds, parents first. */
  readonly objects: O[];
  /** This replica's own heads. */
  readonly heads: string[];
  /** What the box last said this replica holds: its heads, epoch, policy and root. */
  readonly box: Box;
  readonly synced: string[];
  readonly refused: RefusedObject[];
  /** The prodromes the box no longer holds: a store it migrated away from, read here as forgotten with all on it. */
  readonly gone: string[];
  readonly generation: number;
}

/** This deployment's trust policy, as the box reported it; none before the box has spoken, when nothing it sent is held. */
export const untrustedOf = <O extends Obj>(r: Replica<O>): readonly string[] => said(r.box, [], (w) => w.untrusted);

/** The root a device key counts under: none before the box has spoken, or when it said it signs nothing. */
export const rootOf = (r: Replica): string | null => said(r.box, null, (w) => w.root);

/** The box's heads as this replica last heard them, or null for never. */
export const boxHeadsOf = <O extends Obj>(r: Replica<O>): heads.Heads | null => said(r.box, null, (w) => heads.of(w.heads));

const EMPTY: Held<never> = { generation: 0, objects: [], synced: [], refused: [], words: [], frontier: NO_FRONTIER };

/** The reading of `held`: its box word, what that word forgets, and the rest derived from the objects left. */
export function replicaOf<O extends Obj>(held: Held<O>): Replica<O> {
  const word = spoken(held);
  const gone = word === null
    ? []
    : [...new Set(held.objects.flatMap((object) => prodromeOf(object) ?? []))].filter((genesis) => !word.geneses.includes(genesis)).sort();
  const lost = forgotten(held.objects, gone);
  const objects = lost.size === 0 ? [...held.objects] : held.objects.filter((object) => !lost.has(object.hash));
  return {
    held,
    objects,
    heads: headsOf(objects),
    box: word === null ? UNHEARD : { type: "Heard", word },
    synced: held.synced.filter((name) => !lost.has(name)),
    refused: held.refused.filter((one) => !lost.has(one.hash)),
    gone,
    generation: held.generation,
  };
}

export const NOTHING: Replica = replicaOf<StoredObject>(EMPTY);

/** The empty replica of `O`s, the unit of their join. */
export const empty = <O extends Obj>(): Replica<O> => replicaOf<O>(EMPTY);

/** The one way down: an empty replica of a later generation, which every join keeps over the copies from before it. */
export const reset = <O extends Obj>(replica: Replica<O>): Replica<O> => replicaOf<O>({ ...EMPTY, generation: replica.generation + 1 });

/**
 * Whether `next` landed in the row it was joined into, `row` being what the row holds now. A row of a later generation
 * is another tab's reset, which kept nothing of `next`: that is said, never passed over, so an act sealed onto the
 * replica the reset dropped is reported lost rather than shown and then gone.
 */
export const landed = (next: Replica, row: Replica): lo.Settled<Replica> =>
  row.generation > next.generation
    ? lo.error("another tab of this browser reset its copy of the chain, so this was not kept: act again once it has synced")
    : lo.val(row);

/**
 * The join of the free semilattice the replicas are: within a generation, the plain union of what each holds (objects,
 * what the box is known to hold, its refusals by name, its words), so its laws are the union's; the box's word and what
 * it forgets are read after, by `replicaOf`. A later generation is kept whole over an earlier one, so a reset stands
 * whatever copy from before it is joined in. `NOTHING` is its unit.
 */
export function join<O extends Obj>(a: Replica<O>, b: Replica<O>): Replica<O> {
  if (a === b) return a;
  if (a.generation !== b.generation) return a.generation > b.generation ? a : b;
  const held = union(a.held, b.held);
  // Nothing new is `a` itself, so a step that joins in what it holds wakes nobody.
  return held === a.held ? a : held === b.held ? b : replicaOf(held);
}

/** `held` as the union keeps it: its words one per key, in key order, less those under another. */
export const canonical = <O extends Obj>(held: Held<O>): Held<O> => ({ ...held, words: spokenBoth(held.objects, held.words, []) });

function union<O extends Obj>(a: Held<O>, b: Held<O>): Held<O> {
  const names = new Set(a.objects.map((object) => object.hash));
  const fresh = b.objects.filter((object) => !names.has(object.hash));
  const objects = fresh.length === 0 ? a.objects : [...a.objects, ...fresh];
  const synced = new Set([...a.synced, ...b.synced]);
  const refused = refusedBoth(a.refused, b.refused);
  const words = spokenBoth(objects, a.words, b.words);
  const frontier = laterFrontier(a.frontier, b.frontier);
  const mine = new Set(a.refused);
  const same = (x: Held<O>): boolean => x.objects.length === objects.length && new Set(x.synced).size === synced.size &&
    x.refused.length === refused.length && sameWords(x.words, words) && x.frontier === frontier;
  if (same(a) && refused.every((one) => mine.has(one))) return a;
  if (same(b) && refused.every((one) => b.refused.includes(one))) return b;
  return { generation: a.generation, objects, synced: [...synced].sort(), refused, words, frontier };
}

const wordKey = (w: Word): string =>
  // Separated by a byte no name holds.
  [String(w.epoch), heads.of(w.heads).join(","), heads.of(w.untrusted).join(","), w.root ?? "", heads.of(w.geneses).join(",")].join("\u0000");

const sameWords = (a: readonly Word[], b: readonly Word[]): boolean =>
  a.length === b.length && a.every((w, i) => {
    const v = b[i];
    return v !== undefined && wordKey(w) === wordKey(v) && w.fetchedAt === v.fetchedAt;
  });

/**
 * The union of two sets of words, read as the box's register: the latest epoch's words only, the epoch a max; within it
 * one per key, its latest contact, less each word whose heads rest strictly under another's. Under is read off the
 * objects, which only grow and never change what a held object rests on, so a word once under another stays under it in
 * every larger union, as a word of an earlier epoch stays earlier, and the union stays associative.
 */
function spokenBoth(objects: readonly Obj[], a: readonly Word[], b: readonly Word[]): Word[] {
  const both = [...a, ...b];
  const latest = Math.max(...both.map((w) => w.epoch));
  const byKey = new Map<string, Word>();
  for (const w of both.filter((w) => w.epoch === latest)) {
    const k = wordKey(w);
    const was = byKey.get(k);
    if (was === undefined || w.fetchedAt > was.fetchedAt) byKey.set(k, w);
  }
  const all = [...byKey.entries()].sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0)).map(([, w]) => w);
  if (all.length < 2) return all;
  const order = ancestry(objects);
  return all.filter((w) => !all.some((v) => order(w.heads, v.heads) === "Below"));
}

/**
 * The box's word of those held, all of one epoch since the union keeps the latest only: the one whose heads reach the
 * most objects, then the later contact, then by print.
 */
function spoken<O extends Obj>(held: Held<O>): Word | null {
  let best: { readonly word: Word; readonly reach: number } | null = null;
  for (const word of held.words) {
    const r = held.words.length === 1 ? 0 : reach(held.objects, word.heads);
    if (best === null || r > best.reach || (r === best.reach && (word.fetchedAt > best.word.fetchedAt ||
      (word.fetchedAt === best.word.fetchedAt && wordKey(word) > wordKey(best.word))))) best = { word, reach: r };
  }
  return best?.word ?? null;
}

/** How many of `objects` the names rest on, themselves included: one pass back over a parents-first order. */
function reach(objects: readonly Obj[], names: readonly string[]): number {
  const wanted = new Set(names);
  let count = 0;
  for (let i = objects.length - 1; i >= 0; i -= 1) {
    const object = objects[i];
    if (object === undefined || !wanted.has(object.hash)) continue;
    count += 1;
    for (const under of restsOn(object)) wanted.add(under);
  }
  return count;
}

/** One refusal per name: a final word over a wait for a key, then the least sentence, so either order keeps the same one. */
function refusedBoth(a: readonly RefusedObject[], b: readonly RefusedObject[]): RefusedObject[] {
  const rank = (one: RefusedObject): string => `${one.kind === "final" ? 0 : 1}${one.reason}`;
  const byName = new Map<string, RefusedObject>();
  for (const one of [...a, ...b]) {
    const was = byName.get(one.hash);
    if (was === undefined || rank(one) < rank(was)) byName.set(one.hash, one);
  }
  return [...byName.values()].sort((x, y) => (x.hash < y.hash ? -1 : x.hash > y.hash ? 1 : 0));
}

/** What an append answered, deliberately not marked synced, which is what makes it show as unsent. */
export function appended(replica: Replica, objects: readonly StoredObject[]): Replica {
  const held = new Set(replica.held.objects.map((object) => object.hash));
  const fresh = objects.filter((object) => !held.has(object.hash));
  if (fresh.length === 0) return replica;
  return replicaOf({ ...replica.held, objects: [...replica.held.objects, ...fresh] });
}

/** What one answer of the box says, about the `O`s it sends. */
export interface Said<O extends Obj> {
  readonly heads: readonly string[];
  readonly untrusted: readonly string[];
  readonly root: string | null;
  readonly geneses: readonly string[];
  readonly epoch: number;
  readonly objects: readonly O[];
  readonly since: SinceRead;
  readonly frontier: Named;
}

/**
 * What the box answered, joined in: its objects, the names it holds, and its word, with the prodromes it holds. A
 * prodrome it holds none of any more reads as forgotten with all that rests on it, until a later word serves it again:
 * a store the box migrated away from is history its data repository keeps, and its objects need not parse under today's
 * schema. A word naming no prodrome is a store holding none, and forgets every prodrome this replica holds.
 */
export function pulled<O extends Obj>(replica: Replica<O>, answer: Said<O>, fetchedAt: number): Replica<O> {
  return join(replica, replicaOf<O>({
    generation: replica.generation,
    objects: answer.objects,
    synced: [...answer.heads, ...answer.objects.map((object) => object.hash)],
    refused: [],
    words: [{ epoch: answer.epoch, heads: [...answer.heads], untrusted: [...answer.untrusted], root: answer.root, fetchedAt, geneses: [...answer.geneses] }],
    frontier: answered(replica.held.frontier, answer.since, answer.frontier),
  }));
}

/**
 * Whether the box answered with an earlier epoch than the word this replica holds: its own count went back, which only a
 * lost state directory does. The later word keeps being read, so this is said rather than passed over.
 */
export const wentBack = <O extends Obj>(replica: Replica<O>, answer: { readonly epoch: number }): boolean => said(replica.box, false, (w) => w.epoch > answer.epoch);

const NAMED_GENESIS = /^Change\(genesis='([0-9a-f]{64})', /;

/**
 * The prodrome an object is in: a change's genesis or a `Genesis`'s own name; none for a legacy object or a signature.
 * A node names only a `Genesis`'s own: a change's genesis is among its parents, so what rests on it is found by them.
 */
export function prodromeOf(object: Obj): string | null {
  if (!("literal" in object)) return object.header.node === "structure" && object.header.kind === "Genesis" ? object.hash : null;
  if (object.literal.startsWith("Genesis(")) return object.hash;
  return NAMED_GENESIS.exec(object.literal)?.[1] ?? null;
}

/** The objects of a gone prodrome, and every object resting on one, by one pass over a parents-first order. */
export function forgotten(objects: readonly Obj[], gone: readonly string[]): Set<string> {
  const lost = new Set<string>();
  if (gone.length === 0) return lost;
  const dropped = new Set(gone);
  for (const object of objects) {
    const prodrome = prodromeOf(object);
    if ((prodrome !== null && dropped.has(prodrome)) || restsOn(object).some((parent) => lost.has(parent))) lost.add(object.hash);
  }
  return lost;
}

/**
 * Derived from the objects, never carried beside them, so the two cannot disagree. A `Genesis` a change rests in is
 * no head, though it is no parent either: the box's tips read it so, and a head the box never names would leave
 * every local fold reading as behind it.
 */
export function headsOf(objects: readonly Obj[]): string[] {
  const named = new Set(objects.flatMap(restsOn));
  return objects.map((object) => object.hash).filter((hash) => !named.has(hash));
}

/** How two head sets stand in the causal order: the same set, one below the other, concurrent, or not known here. */
export type Order = "Same" | "Below" | "Above" | "Apart" | "Unknown";

/**
 * The causal order on head sets, read off `objects`: `a` is below `b` when everything `a` names rests under `b`. A set
 * naming an object not held is unknown to every other but itself.
 */
export function ancestry(objects: readonly Obj[]): (a: readonly string[], b: readonly string[]) => Order {
  const byName = new Map(objects.map((object) => [object.hash, object]));
  const under = (names: readonly string[]): Set<string> | null => {
    const seen = new Set<string>();
    const left = [...names];
    for (let name = left.pop(); name !== undefined; name = left.pop()) {
      if (seen.has(name)) continue;
      const object = byName.get(name);
      if (object === undefined) return null;
      seen.add(name);
      left.push(...restsOn(object));
    }
    return seen;
  };
  return (a, b) => {
    if (heads.same(heads.of(a), heads.of(b))) return "Same";
    const [x, y] = [under(a), under(b)];
    if (x === null || y === null) return "Unknown";
    return a.every((n) => y.has(n)) ? "Below" : b.every((n) => x.has(n)) ? "Above" : "Apart";
  };
}

/** What an object rests on: its parents, and the genesis of the prodrome a change is in. */
function restsOn(object: Obj): readonly string[] {
  if (!("literal" in object)) return object.parents;
  const genesis = object.literal.startsWith("Genesis(") ? null : prodromeOf(object);
  return genesis === null ? parentsOf(object) : [...parentsOf(object), genesis];
}

/**
 * Parents are the 64-hex names before `event=`, less what names no parent: the genesis an object leads with, which
 * names its prodrome, and a signature's key and bytes. A scan, not a second reading of the grammar: it only finds
 * heads before the Wasm loads, and the box re-derives the edges on receipt, so a mistake costs a redraw, never a
 * wrong write.
 */
const NAME = /[0-9a-f]{64}/g;
const GENESIS = /^[A-Za-z]+\(genesis='[0-9a-f]{64}', /;
const NO_PARENT = /(key|signature)='[0-9a-f]+'/g;

export function parentsOf(object: StoredObject): string[] {
  const cut = object.literal.indexOf("event=");
  const envelope = (cut === -1 ? object.literal : object.literal.slice(0, cut)).replace(GENESIS, "").replace(NO_PARENT, "");
  return [...envelope.matchAll(NAME)].map((found) => found[0]);
}

// --- The two decisions -------------------------------------------------------

/**
 * Parents first, so the post is a closure; a refused object and all resting on it stay out, since the box has answered.
 * One that waits on this device's key goes again once the key is trusted, and never before.
 */
export function toPush(replica: Replica, trusted: boolean): StoredObject[] {
  const known = new Set(replica.synced);
  const blocked = stuck(replica, trusted ? FINAL : EVERY);
  return replica.objects.filter((object) => !known.has(object.hash) && !blocked.has(object.hash));
}

/** This replica's own writes the box has not taken that no signature it holds covers: what an act sealed before its device had a key leaves. */
const SIGNS = /^Signed\(object='([0-9a-f]{64})'/;
export function unsignedWrites(replica: Replica): StoredObject[] {
  const signed = new Set(replica.objects.flatMap((object) => SIGNS.exec(object.literal)?.[1] ?? []));
  return unsentWrites(replica).filter((object) => !signed.has(object.hash));
}

/**
 * `adopted` and `already` both mean the box holds it; a newer refusal of the same object replaces the older reason.
 * The box's heads wait for the pull that follows: they may name objects this replica does not hold yet, and its word
 * is only ever heads it holds, so that how far a word reaches is read off the objects. The receipt's frontier names
 * what the replica of generation `posted` held, so a replica another tab reset meanwhile keeps its own.
 */
export function settled(replica: Replica, receipt: ReceiptPayload, posted: number): Replica {
  const learned = [...receipt.adopted, ...receipt.already, ...receipt.heads];
  const named = new Set(receipt.refused.map((one) => one.hash));
  return replicaOf({
    ...replica.held,
    synced: [...new Set([...replica.held.synced, ...learned])].sort(),
    frontier: replica.generation === posted
      ? laterFrontier(replica.held.frontier, answered(replica.held.frontier, receipt.since, receipt.frontier))
      : replica.held.frontier,
    refused: refusedBoth(
      replica.held.refused.filter((old) => !named.has(old.hash)),
      // A refusal of bytes that were no object has no name to keep: it is toasted, not stored.
      receipt.refused.filter((one) => one.hash !== ""),
    ),
  });
}

const FINAL = (one: RefusedObject): boolean => one.kind === "final";
const EVERY = (): boolean => true;

/** A child of a refused object can never be adopted, so one refusal stops the whole branch above it. */
function stuck(replica: Replica, holds: (one: RefusedObject) => boolean = FINAL): Set<string> {
  const blocked = new Set(replica.refused.filter(holds).map((one) => one.hash));
  if (blocked.size === 0) return blocked;
  // Parents come first in `objects`, so one pass carries the taint forward.
  for (const object of replica.objects) {
    if (parentsOf(object).some((parent) => blocked.has(parent))) blocked.add(object.hash);
  }
  return blocked;
}

/** How many objects final refusals hold back, and the box's reasons; one waiting on this device's key is not stuck. */
export function stuckBehind(replica: Replica): { readonly held: number; readonly reasons: RefusedObject[] } {
  const blocked = stuck(replica);
  const held = replica.objects.filter((object) => blocked.has(object.hash)).length;
  return { held, reasons: replica.refused.filter(FINAL) };
}

/** How many of this replica's writes wait for its device to be trusted: refused for a key, and what rests on them. */
export function awaitingTrust(replica: Replica): number {
  const waiting = stuck(replica, (one) => one.kind === "trust");
  return unsentWrites(replica).filter((object) => waiting.has(object.hash)).length;
}

/** The writes the box has not taken, an act's signature counted with its act rather than beside it. */
const SIGNATURE = /^Signed\(/;
export function unsentWrites(replica: Replica): StoredObject[] {
  return unsynced(replica).filter((object) => !SIGNATURE.test(object.literal));
}

/**
 * Everything a reset of this replica drops that the box has not taken: every write it holds and has not handed over,
 * refused, waiting on a key, waiting on the network, or written in a store the box no longer serves.
 */
export function losses(replica: Replica): StoredObject[] {
  const known = new Set(replica.held.synced);
  return replica.held.objects.filter((object) => !known.has(object.hash) && !SIGNATURE.test(object.literal));
}

/** The writes the box has not taken that were written in a store it no longer serves: kept, and unsendable while it does not. */
export function stranded(replica: Replica): StoredObject[] {
  const read = new Set(replica.objects.map((object) => object.hash));
  return losses(replica).filter((object) => !read.has(object.hash));
}

export function unsynced(replica: Replica): StoredObject[] {
  const known = new Set(replica.synced);
  return replica.objects.filter((object) => !known.has(object.hash));
}

/** Where a todo's act the box has not taken stands: on its way, refused, or waiting for this device to be trusted. */
export type Unsent = "Sending" | "AwaitingTrust" | "Refused";

/** Of two acts on one todo the one further from going through is what the todo says: a refusal outranks the rest. */
const RANK: Readonly<Record<Unsent, number>> = { Sending: 0, AwaitingTrust: 1, Refused: 2 };
export const worse = (a: Unsent, b: Unsent): Unsent => (RANK[b] > RANK[a] ? b : a);

/** Every todo an unsynced print names in its `todo=` field, and where its acts stand: a scan, like `parentsOf`, whose answer the box checks. */
const TODO = /\btodo='([a-z0-9_-]+)'/;

export function unsentTodos(replica: Replica): Map<string, Unsent> {
  const refused = stuck(replica);
  const trust = stuck(replica, (one) => one.kind === "trust");
  const todos = new Map<string, Unsent>();
  for (const object of unsynced(replica)) {
    const todo = TODO.exec(object.literal)?.[1];
    if (todo === undefined) continue;
    const now: Unsent = refused.has(object.hash) ? "Refused" : trust.has(object.hash) ? "AwaitingTrust" : "Sending";
    todos.set(todo, worse(todos.get(todo) ?? now, now));
  }
  return todos;
}
