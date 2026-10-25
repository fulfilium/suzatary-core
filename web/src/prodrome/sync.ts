/**
 * The replica's effectful shell: IndexedDB, the exchange with the box, and the fold in the tab.
 * One exchange for online and offline alike (push what the box lacks, pull what we lack), so a bad network
 * has no second code path. Local-first: the cached fold, then the replica's fold, then the box's answer.
 * `stepSync` is the whole table; a second caller waits on the source for an in-flight read to clear.
 * `commit`'s chain check stays outside `stepSync`, since `step` reads nothing but its two arguments.
 */

import { fold, objectsSince, postObjects } from "../wire/api";
import handover from "./handover";
import type { FoldPayload } from "../wire/generated/wire";
import primed from "../app/primed";
import { cacheKey, fits, parseFold, printFold } from "../app/cache";
import app from "../app/app";
import type { Chains, Core, HeldChain, Kept, Standing } from "./core";
import { keptChains, loadCore, signedObject } from "./core";
import remote from "./remote";
import type questions from "./questions";
import signer from "./signer";
import type device from "../app/device";
import richkeep from "../app/richkeep";
import kv from "../app/kv";
import type { Write } from "./objects";
import { CHAIN, heldRow, holdRow } from "./objects";
import idb from "../app/idb";
import lo from "../algebra/lo";
import heads from "./heads";
import excluded from "./excluded";
import readings from "./readings";
import type { Replica } from "./replica";
import { NOTHING, ancestry, appended, boxHeadsOf, join, landed, pulled, rootOf, said, sinceOf, stranded, unsignedWrites, unsynced, untrustedOf, wentBack } from "./replica";
import time from "../app/time";
import stores from "./stores";
import onscreen, { store } from "../app/store";
import serial from "../algebra/serial";
import folds from "./folds";
import once from "../algebra/once";
import sources from "../algebra/source";

/** The box's last fold, kept on the device and dated, for the first paint of the next visit. */
function keep(payload: FoldPayload, fetchedAt: number): void {
  const printed = printFold({ at: payload.at, fetchedAt, entries: payload.entries });
  // A fold too big is not kept, rather than evicting the reader's remembered filter for it.
  if (fits(printed)) kv.local.set(cacheKey(), printed);
}

// --- the state, and the fold -------------------------------------------------

/** A refusal is a decision, shown until the next exchange and never retried; `on` names the chain an exchange is about. */
export type Phase =
  | { readonly kind: "idle" }
  | { readonly kind: "pushing"; readonly on: string }
  | { readonly kind: "pulling"; readonly on: string }
  | { readonly kind: "refused"; readonly reason: string };

export interface SyncState {
  /** Never null: a replica holding nothing is a value. */
  readonly replica: Replica;
  /** Set only by a chain switch, so an exchange in flight stays about the chain it started on. */
  readonly holding: string;
  readonly phase: Phase;
  /** The chain whose fold fetch (`/api/fold`) is in flight, or null. */
  readonly fetching: string | null;
}

export type SyncEvent =
  | { readonly kind: "chainSwitched"; readonly on: string }
  /** A row kept on the device, or read from it: joined into what is held, so memory only climbs. */
  | { readonly kind: "committed"; readonly replica: Replica }
  | { readonly kind: "pushing"; readonly on: string }
  | { readonly kind: "pulling"; readonly on: string }
  | { readonly kind: "refused"; readonly reason: string }
  | { readonly kind: "exchangeSettled" }
  | { readonly kind: "fetchStarted"; readonly on: string }
  | { readonly kind: "fetchSettled" };

export const INITIAL_SYNC: SyncState = {
  replica: NOTHING,
  holding: app.now().chain,
  phase: { kind: "idle" },
  fetching: null,
};

/** A flag already set returns the same reference, so the source's `Object.is` dedup wakes no waiter. */
export function stepSync(state: SyncState, event: SyncEvent): SyncState {
  switch (event.kind) {
    case "chainSwitched":
      // The memo only: a running exchange stops at its next step once `holding` is no longer its `on`.
      return { ...state, replica: NOTHING, holding: event.on };
    case "committed": {
      const replica = join(state.replica, event.replica);
      return replica === state.replica ? state : { ...state, replica };
    }
    case "pushing":
    case "pulling":
      return state.phase.kind === event.kind && state.phase.on === event.on
        ? state
        : { ...state, phase: { kind: event.kind, on: event.on } };
    case "refused":
      return { ...state, phase: { kind: "refused", reason: event.reason } };
    case "exchangeSettled":
      return state.phase.kind === "idle" ? state : { ...state, phase: { kind: "idle" } };
    case "fetchStarted":
      return state.fetching === event.on ? state : { ...state, fetching: event.on };
    case "fetchSettled":
      return state.fetching === null ? state : { ...state, fetching: null };
  }
}

const source = sources.create<SyncState, SyncEvent>(INITIAL_SYNC, stepSync);

/** Wakes only on the events that move `replica`, never on a phase change; returns its own removal. */
export function watchReplica(listener: () => void): () => void {
  return source.subscribe((_state, event) => {
    if (event.kind === "committed" || event.kind === "chainSwitched") listener();
  });
}

export function held(): Replica {
  return source.state().replica;
}

function whenClear(clear: (state: SyncState) => boolean): Promise<void> {
  return new Promise((resolve) => {
    const stop = source.subscribe((state) => {
      if (clear(state)) {
        stop();
        resolve();
      }
    });
  });
}

/**
 * Kept on the device first, then joined into memory from the row the write left, so nothing reaches memory, nor an
 * exchange reading it, that the device did not keep. A join that comes back on a later generation landed nowhere.
 */
async function commit(write: Write<Replica>): Promise<lo.Settled<Replica>> {
  const holding = source.state().holding;
  // An answer about another chain is dropped, never stored under this one; it reports kept, since its own row is on disk.
  if (holding !== app.now().chain) return write.type === "Join" ? lo.val(write.replica) : lo.error("the chain changed before the reset was kept");
  // Without IndexedDB memory is the only copy: what the box sent is still held for this session, and no act is sealed.
  if (!idb.available() && write.type === "Join") {
    joinIn(holding, write.replica);
    return lo.val(source.state().replica);
  }
  const row = await holdRow(CHAIN, holding, write);
  // What another tab of this browser kept comes back with the row, joined in, its reset included.
  if (row.type === "Val") joinIn(holding, row.v);
  return write.type === "Join" ? lo.bind(row, (r) => landed(write.replica, r)) : row;
}

/** `row` joined into what is held, unless the tab moved to another chain meanwhile; and what it strands, named. */
function joinIn(on: string, row: Replica): void {
  if (source.state().holding !== on) return;
  source.dispatch({ kind: "committed", replica: row });
  const lost = stranded(source.state().replica);
  readings.told(
    `stranded/${on}`,
    excluded.left(excluded.NONE, [], lost.length === 0 ? [] : [{
      chain: on,
      why: `${lost.length} unsent ${lost.length === 1 ? "act was" : "acts were"} written in a store the box no longer serves; kept on this device, and sent if it serves it again, unless this replica is reset`,
      cure: "None",
    }]),
  );
}

/**
 * Drops the memory only; each chain's row stays on disk. A replica left standing would have the next chain's
 * acts sealed onto the previous chain's objects. The fold resets too, to an unloaded page, not an empty list.
 */
export function forgetChainReplica(): void {
  // The chain left keeps acts on disk this session's memory of it never saw, so it is read from disk again.
  others.delete(source.state().holding);
  source.dispatch({ kind: "chainSwitched", on: app.now().chain });
  others.delete(app.now().chain);
  store.dispatch({ type: "Reset" });
}

/**
 * Before the first route, so an act taken offline is still here after the phone was locked: the row on this device,
 * joined into what the tab holds, which also takes in what another tab of this browser kept since.
 */
export async function restoreReplica(): Promise<void> {
  const on = source.state().holding;
  const found = await heldRow(CHAIN, on);
  if (found.type === "Held") joinIn(on, found.v);
  // A row that does not read is said, with the one write that may stand in for it; nothing else is kept until then.
  readings.told(`row/${on}`, excluded.left(excluded.NONE, [], found.type === "Unreadable" ? [excluded.unread(on, found.why)] : []));
}


/** Every chain the tab holds at one instant, the core's one reading of them, and what names that reading. */
export interface HeldSet {
  readonly held: readonly HeldChain[];
  /** The tab's; never freed by a reader. */
  readonly chains: Chains;
  /** Each held chain's heads and policy: the same key is the same set. */
  readonly key: string;
}

let kept: { readonly core: Core; readonly of: (held: readonly HeldChain[]) => Kept } | null = null;
// The chains last told to `readings`, so one the set no longer holds is cleared there too.
let told: ReadonlySet<string> = new Set();

/**
 * Every chain the box serves as this tab holds it: the open one read now from memory, unsent acts included, each other
 * from memory once its row on this device was read. A chain held nowhere yet is left out, and a reading says what refers
 * into it is not yet priced.
 */
export async function heldChains(): Promise<readonly HeldChain[]> {
  return (await heldNow()).held;
}

/** The chains held now, and the open chain's name and replica as the holding read it. */
async function heldNow(): Promise<Holding> {
  await restoreReplica();
  const { holding } = source.state();
  const served = app.now().served;
  const names = served.includes(holding) ? served : [holding, ...served];
  const rows = await Promise.all(names.map((name) => (name === holding ? Promise.resolve(lo.val(NOTHING)) : otherHeld(name))));
  // The open chain as it is once the others are read, so a reading made right after holds every act sealed meanwhile.
  const open = source.state().holding === holding ? source.state().replica : NOTHING;
  const held = names.flatMap((name, i): HeldChain[] => {
    if (name === holding) return open.objects.length === 0 ? [] : [{ name, replica: open }];
    // A row that does not read is left out like one held nowhere, and `pullServed` says which and why.
    const r = rows[i];
    return r?.type !== "Val" || r.v.objects.length === 0 ? [] : [{ name, replica: r.v }];
  });
  return { held, holding, replica: open };
}

/**
 * The one set `held` makes in the core, at once: the tab's `Chains`, holding a chain again only when its heads or policy
 * moved. It is shared, so a reading that awaits nothing between this and its last use prices exactly `held`.
 */
export function setOf(core: Core, held: readonly HeldChain[]): HeldSet {
  if (kept?.core !== core) kept = { core, of: keptChains(core) };
  const key = held.map((c) => `${c.name}:${heads.of(c.replica.heads).join(",")}|${[...untrustedOf(c.replica)].sort().join(",")}`).join(";");
  const { chains, left } = kept.of(held);
  tellLeft(left);
  return { held, chains, key };
}

/** Each chain's reading's leavings, told under its own name by the one set that holds it, whichever page read it. */
function tellLeft(left: ReadonlyMap<string, excluded.Left>): void {
  for (const name of told) if (!left.has(name)) readings.told(`chain/${name}`, excluded.NOTHING);
  for (const [name, l] of left) readings.told(`chain/${name}`, l);
  told = new Set(left.keys());
}

/** What the core's worker holds once every chain held now is sent to it: the chains, and the open one as read. */
export interface Holding {
  readonly held: readonly HeldChain[];
  readonly holding: string;
  readonly replica: Replica;
}

/**
 * The core's answer about the chains held now: every chain the tab holds, sent to its worker by what changed since the
 * last time, and the question right behind it, with no wait between, so the answer reads exactly that holding. What
 * each chain's reading leaves out is told.
 */
export async function askHeld<K extends questions.Kind>(
  question: (h: Holding) => questions.Of<K>,
): Promise<{ readonly held: Holding; readonly answer: questions.Answer<K> }> {
  const held = await heldNow();
  return { held, answer: await askOf(held, question) };
}

/** `askHeld` over a holding already read, so a caller can look at it before anything is asked. */
async function askOf<K extends questions.Kind>(held: Holding, question: (h: Holding) => questions.Of<K>): Promise<questions.Answer<K>> {
  const left = remote.hold(held.held.map((c) => ({ name: c.name, objects: c.replica.objects, untrusted: untrustedOf(c.replica) })));
  const answer = remote.ask(question(held));
  tellLeft(new Map(await left));
  return answer;
}

// --- The other chains -------------------------------------------------------------

/**
 * Each served chain but the open one: its row on this device read once, joined with what this session pulled. Memory only,
 * so a pull never races the open chain's row on disk.
 */
const others = new Map<string, Replica>();
const othersHeard = new Set<() => void>();

/** Wakes when a pull brings another chain something, so a reading that links into it reprices; returns its own removal. */
export function watchOthers(listener: () => void): () => void {
  othersHeard.add(listener);
  return () => othersHeard.delete(listener);
}

/** A chain the pages are not reading: its row on this device, read once, joined with what this session pulled of it; or why the row does not read. */
async function otherHeld(name: string): Promise<lo.Settled<Replica>> {
  const known = others.get(name);
  if (known !== undefined) return lo.val(known);
  const disk = await heldRow(CHAIN, name);
  if (disk.type === "Unreadable") return lo.error(disk.why);
  // A pull that landed while the disk was read is kept, joined over the row.
  const joined = join(disk.type === "Held" ? disk.v : NOTHING, others.get(name) ?? NOTHING);
  others.set(name, joined);
  return lo.val(joined);
}

/**
 * Every served chain but the open one pulled from what is held of it: one request each, no stream, one pull at a time.
 * A chain the box does not answer for stays as held, and what refers into it reads as that. True when one moved; a chain
 * whose row on this device did not read is told to `readings`, which the one line draws.
 */
export const pullServed: () => Promise<boolean> = once.shared(() =>
  pullEach().then(({ moved, unread }) => {
    if (moved) for (const listener of othersHeard) listener();
    readings.told("served", excluded.left(excluded.NONE, [], unread.map(({ name, why }) => excluded.unread(name, why))));
    return moved;
  }),
);

type Pulled = { readonly type: "Moved" } | { readonly type: "Stood" } | { readonly type: "Unread"; readonly name: string; readonly why: string };

async function pullEach(): Promise<{ moved: boolean; unread: { readonly name: string; readonly why: string }[] }> {
  const holding = source.state().holding;
  const each = await Promise.all(
    app.now().served.filter((name) => name !== holding).map(async (name): Promise<Pulled> => {
      const read = await otherHeld(name);
      if (read.type === "Error") return { type: "Unread", name, why: read.e };
      const was = read.v;
      try {
        const answer = await objectsSince(sinceOf(was), name);
        readings.told(`epoch/${name}`, excluded.behind(name, wentBack(was, answer)));
        const now = pulled(was, answer, time.now());
        others.set(name, now);
        // Kept on this device too, so the next visit asks only for what came after these heads; a row that does not
        // read is never written over, and this session holds the pull in memory either way.
        const row = await holdRow(CHAIN, name, { type: "Join", replica: now });
        if (row.type === "Val" && source.state().holding !== name) others.set(name, join(others.get(name) ?? NOTHING, row.v));
        // A policy that moved reprices as objects do: who is believed changes what the chain reads.
        const moved = !heads.same(heads.of(now.heads), heads.of(was.heads)) || !stores.sameSet(untrustedOf(now), untrustedOf(was));
        return moved ? { type: "Moved" } : { type: "Stood" };
      } catch {
        // The network: what is held stays held, and the next exchange asks again.
        return { type: "Stood" };
      }
    }),
  );
  return { moved: each.some((p) => p.type === "Moved"), unread: each.flatMap((p) => (p.type === "Unread" ? [{ name: p.name, why: p.why }] : [])) };
}

// --- The box's folds --------------------------------------------------------------

/**
 * Each chain the box serves, with its heads as this tab last heard them from the box, `null` for never: the open chain's
 * from its stream, which hears a move before any pull does, and each other's from its last pull.
 */
export function boxHeads(): folds.Known {
  const { holding } = source.state();
  const served = app.now().served;
  const names = served.includes(holding) ? served : [holding, ...served];
  const streamed = app.now().stream.heads;
  const pulled = (r: Replica | undefined): heads.Heads | null => (r === undefined ? null : boxHeadsOf(r));
  return new Map(names.map((name) => [name, name !== holding ? pulled(others.get(name)) : streamed.length === 0 ? null : heads.of(streamed)]));
}

/** The box's fold of a chain, one per chain while nothing it read has moved. */
export const boxFolds: folds.Cache = folds.cache(boxHeads, (chain) => fold(chain), time.now);

// --- Appending, locally -------------------------------------------------------

/**
 * `event` written into this replica by the core: a `Change` over its todo's heads in what is held, however many
 * heads the replica has. Folded locally at once from the same reading, so nothing is guessed or rolled back;
 * with no network it simply stays unsynced.
 */
export async function appendAct(event: string): Promise<void> {
  // Another tab's reset kept nothing of the act sealed before it: it is sealed once more onto what the reset holds.
  for (const last of [false, true]) if ((await sealAct(event, last)) === "Kept") return;
}

async function sealAct(event: string, last: boolean): Promise<"Kept" | "Overtaken"> {
  if (!idb.available()) throw new Error("this browser keeps nothing (no IndexedDB), so an act here would be forgotten — nothing was recorded");
  await restoreReplica();
  // A row a reset emptied, in this tab or another, is filled from the box before anything is sealed onto it.
  if (source.state().replica.objects.length === 0) await sync();
  // Under a root, nothing is sealed that this device cannot sign with a key the chain trusts: the box would refuse it.
  const key = rootOf(source.state().replica) === null ? null : await trustedKey();
  // Held after the last wait, so the act rests on everything held when it is sealed.
  const held = await heldNow();
  const { replica, holding } = held;
  // Another tab's reset kept during those waits emptied what was held: the act is sealed again once it is filled.
  if (replica.objects.length === 0 && !last) return "Overtaken";
  if (replica.objects.length === 0) throw new Error("this browser holds no chain yet — syncing first");
  const { appended: written, answer } = await askOf(held, (h) => ({ type: "Act", chain: h.holding, event, at: time.moment() }));
  const signature = key === null ? [] : [signedObject(await loadCore(), written.hash, key.public, await signer.sign(key, written.hash))];
  // Signing waits; joined into what is held now, which is a union, unless the tab has moved to another chain.
  if (source.state().holding !== holding) throw new Error("the chain changed while this act was signed — nothing was recorded");
  const next = appended(key === null ? replica : source.state().replica, [...written.objects, ...signature]);
  const kept = await commit({ type: "Join", replica: next });
  if (kept.type === "Error" && !last && source.state().replica.generation > next.generation) return "Overtaken";
  if (kept.type === "Error") throw new Error(`this browser could not keep the act (${kept.e}) — nothing was recorded`);
  await showFold(answer, next, holding);
  await sync();
  // Again after the exchange: the numbers are the same, but a chain the box now has takes the banner down.
  await refold();
  return "Kept";
}

/** This device's key, when the replica's objects trust it for the actor this tab writes as; otherwise why not, thrown. */
async function trustedKey(): Promise<signer.Key> {
  const actor = app.actor(app.now());
  const held = await signer.held();
  if (held.type !== "Held") throw new Error(`this browser cannot sign what it writes (${held.e}), so the box would refuse it`);
  const standing = (): Promise<Standing> => (actor === null ? Promise.resolve("untrusted") : standingHere(actor, held.key.public));
  // A key trusted since the last pull reads trusted after one exchange.
  const now = await standing();
  const after = now === "trusted" ? now : (await sync(), await standing());
  if (after !== "trusted") {
    throw new Error(
      after === "revoked"
        ? "this device's key was revoked, so nothing it writes is taken"
        : "this device is not trusted to write yet: trust it from Settings → This device with the code from the box, or ask there, then tap its card on a trusted device",
    );
  }
  return held.key;
}

/**
 * Whether the replica trusts this device's key, and, when it does, every write of this replica the box has not taken
 * and nothing signs, signed now: an act sealed before the deployment signed, or before this device was trusted, is
 * posted again rather than lost to a reset.
 */
async function signedOwn(): Promise<boolean> {
  const replica = source.state().replica;
  const actor = app.actor(app.now());
  if (rootOf(replica) === null || actor === null) return false;
  const held = await signer.held();
  if (held.type !== "Held") return false;
  if ((await standingHere(actor, held.key.public)) !== "trusted") return false;
  const unsigned = unsignedWrites(source.state().replica);
  if (unsigned.length === 0) return true;
  const core = await loadCore();
  const signatures = await Promise.all(
    unsigned.map(async (object) => signedObject(core, object.hash, held.key.public, await signer.sign(held.key, object.hash))),
  );
  return (await commit({ type: "Join", replica: appended(source.state().replica, signatures) })).type === "Val";
}

/** What `key` is to `actor` in the open chain as held now, read by the core in its worker. */
async function standingHere(actor: string, key: string): Promise<Standing> {
  const held = await heldNow();
  // A replica a reset emptied trusts no key, as the core reads no objects.
  if (held.replica.objects.length === 0) return "untrusted";
  return askOf(held, (h) => ({ type: "Standing", chain: h.holding, root: rootOf(h.replica), actor, key }));
}

/** What this device's key is to the chain this tab holds, read by the core from the replica. */
export async function signingHere(): Promise<device.Signing> {
  await restoreReplica();
  const replica = source.state().replica;
  if (rootOf(replica) === null) return { type: "Unheard" };
  const actor = app.actor(app.now());
  if (actor === null) return { type: "Reads" };
  const held = await signer.held();
  if (held.type !== "Held") return held;
  return { type: "Keyed", key: held.key.public, standing: await standingHere(actor, held.key.public) };
}

// --- The exchange -------------------------------------------------------------

/**
 * Push first, so the pull already carries the box's merge of our divergence. Serialised: a caller during an exchange
 * gets the one queued behind it, which starts after the call and so carries whatever the caller sealed, on the chain
 * held when it starts. Never throws, since the pill and banner are the report; a refusal is shown and never retried.
 */
export const sync: () => Promise<void> = serial.of(() =>
  exchange()
    .catch(() => {
      // The network failed: what is held stays held, and the next tip change or reconnect tries again.
    })
    .finally(() => {
      // An offline rejection unwinds `exchange` before it settles, and the phase it left says an exchange still runs.
      const stuck = source.state().phase.kind;
      if (stuck === "pushing" || stuck === "pulling") source.dispatch({ kind: "exchangeSettled" });
    }),
);

async function exchange(): Promise<void> {
  const on = source.state().holding;
  source.dispatch({ kind: "pushing", on });
  // Checked after every wait: a chain switch ends this exchange there.
  const switched = (): boolean => source.state().holding !== on;
  // Never before the read: an empty replica would pull everything and fail to push the acts still on disk.
  await restoreReplica();
  if (switched()) return;
  const trusted = await signedOwn();
  if (switched()) return;
  const ended = await handover.of(() => source.state().replica, trusted, postObjects);
  switch (ended.type) {
    case "Nothing":
      break;
    // A box past this build's contract: the page offers the new one, and what it holds stays held for it to send.
    case "Outdated":
      return;
    // The box answered no to the whole request (the gate, CSRF, size): said once, never retried.
    case "Refused":
      source.dispatch({ kind: "refused", reason: ended.reason });
      handover.told(ended.reason);
      return;
    // No answer: what is held stays held, and the next tip change or reconnect tries again.
    case "Unanswered":
      throw ended.e;
    case "Taken":
      if (switched()) return;
      await commit({ type: "Join", replica: ended.replica });
      break;
  }
  source.dispatch({ kind: "pulling", on });
  // The answer is joined into the replica it was asked of: it names only what that one lacked, so another tab's reset
  // kept meanwhile, a later generation, keeps it out, rather than taking new objects without their ancestors. The reset
  // is then asked of in turn, so an exchange that ends has pulled into the replica held when it ends.
  for (;;) {
    const asked = source.state().replica;
    const answer = await objectsSince(sinceOf(asked));
    if (switched()) return;
    readings.told(`epoch/${on}`, excluded.behind(on, wentBack(source.state().replica, answer)));
    if (answer.objects.length > 0 || !said(source.state().replica.box, false, (w) => heads.same(heads.of(answer.heads), heads.of(w.heads)))) {
      await commit({ type: "Join", replica: pulled(asked, answer, time.now()) });
    }
    if (source.state().replica.generation === asked.generation) break;
  }
  source.dispatch({ kind: "exchangeSettled" });
  // After the open chain's own, never on a stream of their own, so the rows' references into the others read them.
  void pullServed();
}

/**
 * The one escape from a refusal, which loses the refused objects and all sealed on them. Never "drop just the
 * bad one": its children would be left resting on a hole.
 */
export async function resetReplica(shown: ReadonlySet<string>): Promise<void> {
  await resetRow(source.state().holding, shown, "Read");
}

/**
 * A chain's row reset from the box: the generation after the row's, so no tab's copy from before it comes back, and
 * refused, with the reason toasted, when the row holds an unsent object the reader was not shown, or does not read and
 * `over` does not say the reader was told it writes over that copy.
 */
export async function resetRow(chain: string, shown: ReadonlySet<string>, over: "Read" | "Unread"): Promise<void> {
  if (chain !== source.state().holding) {
    const done = await holdRow(CHAIN, chain, { type: "Reset", shown, floor: 0, over });
    if (done.type === "Error") handover.told(`the reset was not kept: ${done.e}`);
    others.delete(chain);
    void pullServed();
    return;
  }
  const done = await commit({ type: "Reset", shown, floor: source.state().replica.generation, over });
  // What the row holds now is read again, so a notice that missed another tab's acts draws them before it is pressed again.
  await restoreReplica();
  if (done.type === "Error") {
    handover.told(`the reset was not kept: ${done.e}`);
    return;
  }
  store.dispatch({ type: "Reset" });
  await sync();
  await foldHeld();
}


/**
 * Under the trust policy the box reported with these objects, never another. This browser's clock is used
 * only for "now"; the banner names the tip, because what is uncertain is the chain, not the instant.
 */
export async function foldHeld(): Promise<boolean> {
  // Never before the read-back: a refused connection fails faster than IndexedDB answers, and would fold nothing.
  await restoreReplica();
  if (source.state().replica.objects.length === 0) return false;
  try {
    const { held, answer } = await askHeld((h) => ({ type: "List", chain: h.holding, at: time.moment() }));
    const { replica, holding } = held;
    if (source.state().holding !== holding) return false;
    const shown = await showFold(answer, replica, holding);
    if (shown) primed.cancel(primed.fold(holding));
    return shown;
  } catch {
    return false;
  }
}

/** The causal order over what is held, read once per set of heads, since the heads name the whole down-set. */
let order: { readonly key: string; readonly of: ReturnType<typeof ancestry> } | null = null;
function orderOf(replica: Replica): ReturnType<typeof ancestry> {
  const key = `${replica.generation}:${heads.of(replica.heads).join(",")}`;
  if (order?.key !== key) order = { key, of: ancestry(replica.objects) };
  return order.of;
}

async function showFold(answer: FoldPayload, replica: Replica, holding: string): Promise<boolean> {
  // The core has no typst: what the box once rendered for the same source is read back from this device.
  const entries = await richkeep.fill(answer.entries);
  if (source.state().holding !== holding) return false;
  // A reading of a replica since moved is not shown: the move that superseded it is folded again (`refold`).
  if (source.state().replica !== replica) return true;
  // Fresh only with everything taken, the box's heads ours, and the live stream up: only the stream says the last contact is now.
  // Judged against the replica as it stands once the reading is ready, not as it stood when the reading began: an
  // exchange that settled meanwhile makes the same heads fresh, and objects taken meanwhile make them old.
  const mine = heads.of(replica.heads);
  const now = source.state().replica;
  const settledNow = unsynced(now).length === 0 && said(now.box, false, (w) => heads.same(mine, heads.of(w.heads))) && app.live(app.now()) === "on";
  store.dispatch({ type: "Local", entries, at: answer.at, next: answer.next, heads: mine, fresh: settledNow, ancestry: orderOf(now) });
  return true;
}

/** The fold on screen follows the replica: every move of it is folded again, one fold at a time and one queued. */
const refold = serial.of(() => foldHeld().then(() => undefined));
source.subscribe((_state, event) => {
  if (event.kind === "committed") void refold();
});

/** One exchange per move of the box's heads, never per fold, so an offline session holds what it folds and acts on. */
export function keepObjects(): Promise<void> {
  return sync();
}

// --- The fold ------------------------------------------------------------------

/** Rejects so the page shows the failure where the list would be; a fold landing after a chain switch is dropped. */
export function refetchFold(): Promise<void> {
  const { fetching, holding } = source.state();
  if (fetching !== null) {
    const cleared = whenClear((state) => state.fetching === null);
    return fetching === holding ? cleared : cleared.then(refetchFold);
  }
  const on = holding;
  source.dispatch({ kind: "fetchStarted", on });
  // What the box was known to hold when the fold was asked, which is what it read.
  const known = boxFolds.key();
  return fold()
    .then((payload) => {
      if (source.state().holding !== on) return;
      const fetchedAt = time.now();
      boxFolds.put(on, known, payload);
      store.dispatch({ type: "Box", payload, fetchedAt, ancestry: orderOf(source.state().replica) });
      // The box names the chains its fold was priced without; the tab's own readings say theirs through the set.
      readings.told(`box/${on}`, excluded.left(excluded.NONE, payload.failed));
      keep(payload, fetchedAt);
      void richkeep.keep(payload.entries);
      // A fetch that worked means the network is back, so whatever is still held goes now.
      void sync();
    })
    .catch(async (error: unknown) => {
      if (source.state().holding !== on) return;
      // What is on screen stays, dated, unless the held objects can be folded now instead.
      store.dispatch({ type: "Stale" });
      await foldHeld();
      throw error;
    })
    .finally(() => {
      source.dispatch({ kind: "fetchSettled" });
    });
}

/** A restored cache still asks, because a cache is not an answer about now. */
export function ensureFold(): Promise<void> {
  return onscreen.fresh(store.state()) ? Promise.resolve() : refetchFold();
}

/** Before the first route, so the first paint is not a spinner. */
export function restoreFold(): void {
  const cached = parseFold(kv.local.get(cacheKey()));
  if (cached !== null) store.dispatch({ type: "Cached", fold: cached });
}

// `/api/chain` stays in `api.ts`: it is the chain page's rendering, while `/api/objects` is the replica's data.
