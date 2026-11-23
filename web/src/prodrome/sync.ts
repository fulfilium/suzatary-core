/**
 * The page's side of the chains, which the core's worker holds (`chainsync`): the fold on screen read from the worker's
 * answer, the box's fold beside it, and every act and exchange asked of the worker. The page holds no copy of a chain:
 * what it shows of one is an answer, and what it knows of the box's heads is what the worker last said.
 */

import { fold } from "../wire/api"
import type { FoldPayload } from "../wire/generated/wire"
import type { UnsentRead } from "../wire/generated/pages"
import primed from "../app/primed"
import { cacheKey, fits, parseFold, printFold } from "../app/cache"
import app from "../app/app"
import type { Order, Standing } from "./core"
import remote from "./remote"
import type questions from "./questions"
import lanes from "./lanes"
import signer from "./signer"
import type device from "../app/device"
import richkeep from "../app/richkeep"
import kv from "../app/kv"
import heads from "./heads"
import excluded from "./excluded"
import readings from "./readings"
import { told } from "./said"
import time from "../app/time"
import onscreen, { store } from "../app/store"
import serial from "../algebra/serial"
import folds from "./folds"
import sources from "../algebra/source"

/** The box's last fold, kept on the device and dated, for the first paint of the next visit. */
function keep(payload: FoldPayload, fetchedAt: number): void {
  const printed = printFold({ at: payload.at, fetchedAt, entries: payload.entries })
  // A fold too big is not kept, rather than evicting the reader's remembered filter for it.
  if (fits(printed)) kv.local.set(cacheKey(), printed)
}

// --- Asking the worker ---------------------------------------------------------

/** What a question of the shell's lane about the chains waits for before it is sent. */
let shellAfter: Promise<void> = Promise.resolve()

/** Said once by the shell: its questions about the chains wait until the first page has drawn. */
export function shellAsksAfter(after: Promise<void>): void {
  shellAfter = after
}

const turnOf = (lane: lanes.Lane): Promise<void> => (lane === "Shell" ? shellAfter : Promise.resolve())

/** The chains the box serves, the open one first where it serves it not: what the worker holds. */
const served = (): readonly string[] => {
  const { chain, served } = app.now()
  return served.includes(chain) ? served : [chain, ...served]
}

let toldServed = ""

/** `q` asked of the worker in `lane`, behind the chains served as the tab knows them now. */
export async function ask<K extends questions.Kind>(q: questions.Of<K>, lane: lanes.Lane): Promise<questions.Answer<K>> {
  await turnOf(lane)
  const names = served()
  const key = JSON.stringify(names)
  // In the first lane, so the worker takes it before any question asked after it.
  if (key !== toldServed) {
    toldServed = key
    void remote.ask({ type: "Served", chains: names }, lanes.FIRST)
  }
  return remote.ask(q, lane)
}

// --- What the worker says of the chains ----------------------------------------------------

type Heard = { readonly chain: string; readonly box: ReadonlyMap<string, heads.Heads | null> }
const heard = sources.create<Heard, { readonly chain: string; readonly box: readonly string[] | null }>(
  { chain: "", box: new Map() },
  (h, e) => ({ chain: e.chain, box: new Map([...h.box, [e.chain, e.box === null ? null : heads.of(e.box)]]) }),
)

/** Said by the worker: a reading of `chain` moved, and the heads the box last said of it. */
export function heardChain(chain: string, box: readonly string[] | null): void {
  heard.dispatch({ chain, box })
}

/** Wakes when a reading of the open chain moves; returns its own removal. */
export function watchReplica(listener: () => void): () => void {
  return heard.subscribe(h => {
    if (h.chain === app.now().chain) listener()
  })
}

/** Wakes when a reading of another chain moves, so a reading that links into it reprices; returns its own removal. */
export function watchOthers(listener: () => void): () => void {
  return heard.subscribe(h => {
    if (h.chain !== app.now().chain) listener()
  })
}

// --- What the tab wrote that the box has not taken -------------------------------------------

const NONE: UnsentRead = { todos: {}, acts: [], refused: [], blocked: 0, stranded: 0, losses: [], waiting: 0 }
const unsentNow = sources.create<UnsentRead, UnsentRead>(NONE, (_, u) => u)

/** The open chain's unsent writes as the worker last read them. */
export const unsent = (): UnsentRead => unsentNow.state()
export const watchUnsent = (listener: () => void): (() => void) => unsentNow.subscribe(() => listener())

const readUnsent = serial.of(async (): Promise<void> => {
  const chain = app.now().chain
  const u = await ask({ type: "Unsent", chain }, "Shell")
  if (app.now().chain !== chain) return
  unsentNow.dispatch(u)
  readings.told(
    `stranded/${chain}`,
    excluded.left(excluded.NONE, [], u.stranded === 0 ? [] : [{
      chain,
      why: `${u.stranded} unsent ${u.stranded === 1 ? "act was" : "acts were"} written in a store the box no longer serves; kept on this device, and sent if it serves it again, unless this replica is reset`,
      cure: "None",
    }]),
  )
})

/** A switch of chain: what was read of the previous one is dropped; the fold resets to an unloaded page. */
export function forgetChainReplica(): void {
  unsentNow.dispatch(NONE)
  store.dispatch({ type: "Reset" })
}

// --- The box's folds --------------------------------------------------------------

/**
 * Each chain the box serves, with its heads as this tab last heard them from the box, `null` for never: the open chain's
 * from its stream, which hears a move before any pull does, and each other's as the worker last said.
 */
export function boxHeads(): folds.Known {
  const open = app.now().chain
  const streamed = app.now().stream.heads
  const said = heard.state().box
  return new Map(served().map(name => [name, name !== open ? (said.get(name) ?? null) : streamed.length === 0 ? null : heads.of(streamed)]))
}

/** The box's fold of a chain, one per chain while nothing it read has moved. */
export const boxFolds: folds.Cache = folds.cache(boxHeads, chain => fold(chain), time.now)

// --- Acting, and the exchange ----------------------------------------------------

/**
 * `event` sealed by the worker as this tab's act onto the open chain, kept, and handed over behind the answer; the fold
 * it answers is shown at once, and with no network the act simply stays unsent.
 */
export async function appendAct(event: string): Promise<void> {
  const actor = app.actor(app.now())
  if (actor === null) throw new Error("this device only reads, so it acts on nothing")
  const chain = app.now().chain
  const listed = await ask({ type: "Act", chain, event, at: time.moment(), actor }, "Page")
  if (listed.type === "Read") await showFold(listed, chain)
}

/** One exchange of the open chain: what the tab wrote handed over, then pulled. Never throws: the banner is the report. */
export const sync: () => Promise<void> = serial.of(() =>
  ask({ type: "Exchange", chain: app.now().chain }, "Shell").then(
    () => undefined,
    () => undefined,
  ),
)

/** One exchange per move of the box's heads, never per fold, so an offline session holds what it folds and acts on. */
export function keepObjects(): Promise<void> {
  return sync()
}

/**
 * The one escape from a refusal, which lets go of the refused acts and all sealed on them, after the reader was shown
 * each: a reset that finds an unsent act the reader was not shown refuses, and says so.
 */
export async function resetReplica(shown: ReadonlySet<string>): Promise<void> {
  await resetChain(app.now().chain, shown)
}

/** `chain` reset from the box, letting go of the unsent acts `shown`; what it could not do is said. */
export async function resetChain(chain: string, shown: ReadonlySet<string>): Promise<void> {
  const failed = await ask({ type: "ResetChain", chain, shown: [...shown] }, "Page")
  failed.forEach(told)
  if (failed.length > 0 || chain !== app.now().chain) return
  store.dispatch({ type: "Reset" })
  await foldHeld()
}

/** What this device's key is to the chain this tab holds, read by the core. */
export async function signingHere(lane: lanes.Lane): Promise<device.Signing> {
  const chain = app.now().chain
  if ((await ask({ type: "Root", chain }, lane)) === null) return { type: "Unheard" }
  const actor = app.actor(app.now())
  if (actor === null) return { type: "Reads" }
  const held = await signer.held()
  if (held.type !== "Held") return held
  const standing: Standing = (await ask({ type: "Standing", chain, actor, key: held.key.public }, lane)) ?? "untrusted"
  return { type: "Keyed", key: held.key.public, standing }
}

// --- The fold on screen -----------------------------------------------------------

/** The lane the fold on screen is read in: the page's on a route that shows it, the shell's on any other. */
let folding = (): lanes.Lane => "Page"

/** Said once by the shell, which knows the route on screen. */
export function foldWhere(where: () => lanes.Lane): void {
  folding = where
}

/**
 * The open chain's fold as the worker reads what stands of it, under the policy the box last said; false where it holds
 * nothing a pull landed, or the worker could not read it.
 */
export async function foldHeld(): Promise<boolean> {
  const chain = app.now().chain
  try {
    const listed = await ask({ type: "List", chain, at: time.moment() }, folding())
    if (listed.type === "Unheld") return false
    // The fold the loader asked the box for is read from what the device holds instead, so its answer goes unread.
    primed.cancel(primed.fold(chain))
    return await showFold(listed, chain)
  } catch {
    return false
  }
}

/**
 * `next` dispatched with how the heads it names stand to those of the fold shown, asked of the worker. Asked again
 * when the fold shown moved while the worker answered, so the order read is the one between these two.
 */
async function ordered(chain: string, mine: heads.Heads, next: (ancestry: onscreen.Ancestry) => onscreen.Event): Promise<void> {
  for (;;) {
    const shown = store.state()
    const was = shown.type === "Shown" && shown.by.type !== "Cached" ? shown.by.heads : null
    const order: Order = was === null ? "Unknown" : await ask({ type: "Order", chain, a: mine, b: was }, "Page")
    if (store.state() !== shown) continue
    store.dispatch(next((a, b) => (heads.same(a, mine) && was !== null && heads.same(b, was) ? order : "Unknown")))
    return
  }
}

async function showFold(listed: Extract<questions.Listed, { type: "Read" }>, chain: string): Promise<boolean> {
  // The core has no typst: what the box once rendered for the same source is read back from this device.
  const entries = await richkeep.fill(listed.fold.entries)
  if (app.now().chain !== chain) return false
  const mine = heads.of(listed.fold.heads)
  // Fresh only with everything taken, the box's heads ours, and the live stream up: only the stream says the last contact is now.
  const fresh = listed.settled && app.live(app.now()) === "on"
  await ordered(chain, mine, ancestry => ({ type: "Local", entries, at: listed.fold.at, next: listed.fold.next, heads: mine, fresh, ancestry }))
  return true
}

/** The fold on screen follows the chain: every move of it is folded again, one fold at a time and one queued. */
const refold = serial.of(() => foldHeld().then(() => undefined))
watchReplica(() => {
  void refold()
  void readUnsent()
})

/** Before the first route and after a switch: the open chain read from what the worker restored. */
export const restoreChain = (): Promise<void> => Promise.all([refold(), readUnsent()]).then(() => undefined)

// --- The box's fold ------------------------------------------------------------------

/** The chain whose fold fetch (`/api/fold`) is in flight. */
let fetching: { readonly chain: string; readonly done: Promise<void> } | null = null

/** Rejects so the page shows the failure where the list would be; a fold landing after a chain switch is dropped. */
export function refetchFold(): Promise<void> {
  const on = app.now().chain
  if (fetching !== null) return fetching.chain === on ? fetching.done : fetching.done.catch(() => {}).then(refetchFold)
  // What the box was known to hold when the fold was asked, which is what it read.
  const known = boxFolds.key()
  const done = fold()
    .then(async payload => {
      if (app.now().chain !== on) return
      const fetchedAt = time.now()
      boxFolds.put(on, known, payload)
      await ordered(on, heads.of(payload.heads), ancestry => ({ type: "Box", payload, fetchedAt, ancestry }))
      // The box names the chains its fold was priced without; the tab's own readings say theirs through the set.
      readings.told(`box/${on}`, excluded.left(excluded.NONE, payload.failed))
      boxRead = true
      keep(payload, fetchedAt)
      void richkeep.keep(payload.entries)
      // A fetch that worked means the network is back, so whatever is still held goes now.
      void sync()
    })
    .catch(async (error: unknown) => {
      if (app.now().chain !== on) return
      // What is on screen stays, dated, unless the held objects can be folded now instead.
      store.dispatch({ type: "Stale" })
      await foldHeld()
      throw error
    })
    .finally(() => {
      fetching = null
    })
  fetching = { chain: on, done }
  return done
}

/** Whether this tab has read the box's fold: only it carries what the box rendered and is kept as the dated copy. */
let boxRead = false

/**
 * A restored cache still asks, because a cache is not an answer about now; and the box is asked once per tab even when
 * the tab's own fold is fresh, since a page mounted once its module loads would otherwise never read it.
 */
export function ensureFold(): Promise<void> {
  return onscreen.fresh(store.state()) && boxRead ? Promise.resolve() : refetchFold()
}

/** Before the first route, so the first paint is not a spinner. */
export function restoreFold(): void {
  const cached = parseFold(kv.local.get(cacheKey()))
  if (cached !== null) store.dispatch({ type: "Cached", fold: cached })
}

export default { ask }
