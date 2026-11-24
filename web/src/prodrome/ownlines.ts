// What this tab wrote to the lines, kept by the device until the box serves it back: each own line's print, its
// slots' keys and the box's answer, in a row of the rows' store, which every upgrade keeps
// (`objects`). The row is plain JSON of what no build changes, so a snapshot of another build, a reset of the lines and
// an upgrade drop the lines' log and never an own line: the worker reads the row back into its core on every load.
//
// The row is a join: by print, the answer that climbed furthest (unsent, refused until trusted, refused for good,
// taken), so two tabs keeping their moves in either order keep the same row.

import type { Rows } from "./objects"
import { alterRow, holdRow, heldRow, heldRows } from "./objects"
import lo from "../algebra/lo"
import idb from "../app/idb"
import d from "../wire/decoder"

export namespace ownlines {
  // One own line or signature: its print, its slots' keys in hex by name, and the box's answer in the core's spelling
  // (`"Unsent"`, `{"Refused": {kind, reason}}` or `"Taken"`).
  export type Kept = { readonly literal: string; readonly keys: Readonly<Record<string, string>>; readonly answer: unknown }
  export type Row = { readonly records: readonly Kept[] }

  const NAME = "own/lines"

  const Answer = d.unknown
  const RecordOf = d.obj<Kept>({ literal: d.str, keys: d.rec(d.str), answer: Answer })
  const parse = d.obj<Row>({ records: d.arr(RecordOf) })

  // How far an answer climbed; the core's order (`writes::Answer`): a refusal for now, awaiting trust or in the rule's
  // words, is posted again, and below one for good.
  type Retried = "Trust" | "Later"
  const ForNow = d.obj<{ Refused: { kind: Retried } }>({ Refused: d.obj<{ kind: Retried }>({ kind: d.oneOf<Retried>({ Trust: null, Later: null }) }) })
  const rank = (answer: unknown): number =>
    answer === "Taken" ? 3 : answer === "Unsent" ? 0 : ForNow(answer).type === "Val" ? 1 : 2

  const join = (a: Row, b: Row): Row => {
    const by = new Map<string, Kept>()
    for (const r of [...a.records, ...b.records]) {
      const was = by.get(r.literal)
      if (was === undefined || rank(r.answer) > rank(was.answer)) by.set(r.literal, r)
    }
    return { records: [...by.values()].sort((x, y) => (x.literal < y.literal ? -1 : x.literal > y.literal ? 1 : 0)) }
  }

  const NOTHING: Row = { records: [] }

  const ROWS: Rows<Row> = { parse: (raw: unknown) => parse(raw), print: r => r, join, nothing: NOTHING }

  // Every other tab of the device is told the row moved, never what moved: it reads the row itself, so a line sealed in a
  // tab closed before its hand-over is still handed over by another. A device that keeps nothing has no row to wake for.
  let made: BroadcastChannel | null | undefined
  const channel = (): BroadcastChannel | null => {
    if (made !== undefined) return made
    made = typeof BroadcastChannel === "undefined" || !idb.available() ? null : new BroadcastChannel(`rows/${NAME}`)
    // Node keeps a process alive while a channel is open; a page has no such thing to release.
    if (made !== null && "unref" in made && typeof made.unref === "function") made.unref()
    return made
  }

  // The records joined in, and the other tabs woken where the join moved the row: a record kept
  // again as it stood wakes nobody, so two tabs never wake each other over one line for ever.
  export const keep = async (records: readonly Kept[]): Promise<lo.Settled<Row>> => {
    const before = await heldRow(ROWS, NAME)
    const kept = await holdRow(ROWS, NAME, { records })
    const was = before.type === "Held" ? before.v : NOTHING
    if (kept.type === "Val" && JSON.stringify(kept.v) !== JSON.stringify(join(was, NOTHING))) channel()?.postMessage(null)
    return kept
  }

  // `wake` run each time another tab keeps a line in the row.
  export const heard = (wake: () => void): void => {
    const heard = channel()
    if (heard !== null) heard.onmessage = () => wake()
  }

  export const kept = async (): Promise<lo.Settled<readonly Kept[]>> => {
    const row = await heldRow(ROWS, NAME)
    return row.type === "Held" ? lo.val(row.v.records) : row.type === "Absent" ? lo.val([]) : lo.error(row.why)
  }

  // The prints of the refused lines the person was told of: a row of its own beside the records, a
  // join of sets, so two tabs telling the same refusal keep it once.
  type Seen = { readonly literals: readonly string[] }
  const SEEN = "own/seen"
  const NO_ONE: Seen = { literals: [] }
  const SEEN_ROWS: Rows<Seen> = {
    parse: (raw: unknown) => d.obj<Seen>({ literals: d.arr(d.str) })(raw),
    print: r => r,
    join: (a, b) => ({ literals: [...new Set([...a.literals, ...b.literals])].sort() }),
    nothing: NO_ONE,
  }

  // The refused lines whose prints `literals` names were said to the person, their words with them.
  export const seen = async (literals: readonly string[]): Promise<void> => {
    if (literals.length > 0) await holdRow(SEEN_ROWS, SEEN, { literals })
  }

  // What the told set holds for a line told it waits, beside its print alone for one told it was refused: so being told a
  // line waits never keeps its refusal unsaid.
  export const WAITS = "\u0000waits"
  const printOf = (told: string): string => told.split(WAITS)[0] ?? told

  // The prints of every refused line the person was told of.
  export const told = async (): Promise<ReadonlySet<string>> => {
    const row = await heldRow(SEEN_ROWS, SEEN)
    return new Set(row.type === "Held" ? row.v.literals : [])
  }

  // The names of the refused lines whose words the person acted on: dismissed, or taken back into the composer. A row
  // of its own, a join of sets as `own/seen` is; telling a refusal is one view of it and acting is what lets it go.
  const ACTED = "own/acted"

  // The person acted on the refusals of the own lines `names`: a join of names, so two tabs acting on one refusal act
  // once. A move wakes the other tabs as a keep does, so each reads the set. It keeps each line's name (64 hex digits),
  // one for every refusal the person acted on, which a person makes a handful of, and is never trimmed: a tab still
  // holding a record another tab let go reads its name to hide it. A write the device could not keep is answered.
  export const acted = async (names: readonly string[]): Promise<lo.Settled<null>> => {
    const done = await actedOn()
    if (names.every(n => done.has(n))) return lo.val(null)
    const kept = await holdRow(SEEN_ROWS, ACTED, { literals: [...names] })
    if (kept.type === "Error") return kept
    channel()?.postMessage(null)
    return lo.val(null)
  }

  // How long a tab waits for the device's lock before it goes on alone: a holder frozen (a tab put to sleep mid-seal)
  // never lets it go, and a Send must not wait for it.
  export const LOCK_MS = 2000

  // `f` under the device's lock `name` where the browser has Web Locks (they reach workers). Where it has none, or the
  // lock is not granted within `ms`, `f` runs alone, and what it orders is this tab's alone: two tabs may seal two lines
  // that rest on nothing of each other. `f` waits for nothing but the device's rows, so the lock is never held across
  // the network.
  export const device = async <A>(name: string, f: () => Promise<A>, ms: number = LOCK_MS): Promise<A> => {
    if (typeof navigator === "undefined" || !("locks" in navigator)) return f()
    let ran = false
    const run = (): Promise<A> => ((ran = true), f())
    try {
      return await navigator.locks.request(`suzatary/${name}`, { signal: AbortSignal.timeout(ms) }, run)
    } catch (x: unknown) {
      if (ran) throw x
      return run()
    }
  }

  // The names of every refused line the person acted on.
  export const actedOn = async (): Promise<ReadonlySet<string>> => {
    const row = await heldRow(SEEN_ROWS, ACTED)
    return new Set(row.type === "Held" ? row.v.literals : [])
  }

  // What a tab's core is to the row: it takes the records and the acts the row holds, in that order and in one turn,
  // answering what to publish (`R`), and it alone says which records may go (`Lines.releasable`), since it alone knows
  // what rests on what: their prints, and the names of the refused ones among them, joined to the acted ones.
  export type Released = { readonly free: ReadonlySet<string>; readonly refused: readonly string[] }
  export type Core<R> = {
    readonly take: (records: readonly Kept[], acted: ReadonlySet<string>) => R
    readonly releasable: (told: ReadonlySet<string>) => Released
  }

  // The lock every read, decision and write of the row is made under.
  export const LOCK = NAME

  // A reading: what `core.take` answered, and why the acts' row does not read, if it does not (then no act is read).
  export type Reading<R> = { readonly taken: R; readonly acts: string | null }

  // READING: the row read into `core`, its records then the acts, or why the records do not read. The caller holds
  // the lock: a seal reads under its own, and `settle` under its. Both rows in one transaction, so an act kept between
  // two reads is never read without the records it was kept beside.
  export const read = async <R>(core: Core<R>): Promise<lo.Settled<Reading<R>>> => {
    const t0 = performance.now()
    const [records, done] = await heldRows({ rows: ROWS, name: NAME }, { rows: SEEN_ROWS, name: ACTED })
    if (records.type === "Unreadable") return lo.error(records.why)
    const t1 = performance.now()
    const taken = core.take(records.type === "Held" ? records.v.records : [], new Set(done.type === "Held" ? done.v.literals : []))
    performance.measure("row:read", { start: t0, end: t1 })
    performance.measure("row:take", { start: t1, end: performance.now() })
    return lo.val({ taken, acts: done.type === "Unreadable" ? done.why : null })
  }

  // A settle: its reading, published whatever the cleanup did, and why the cleanup failed, if it did; what it did not let
  // go is let go at the next settle.
  export type Settling<R> = { readonly reading: lo.Settled<Reading<R>>; readonly cleanup: string | null }

  // SETTLING, whole under the device's lock: the row read into `core`, then the cleanup: `before` (the taken lines'
  // prints kept for reading), the refused names it lets go joined to the acted ones, and the records the core names let go. A
  // record another tab kept is in the core before it decides, so nothing a kept line rests on goes.
  export const settle = <R>(core: Core<R>, before: () => Promise<void>): Promise<Settling<R>> =>
    device(LOCK, async () => {
      const reading = await read(core)
      if (reading.type === "Error") return { reading, cleanup: null }
      const cleanup = await letGo(core, before).catch((e: unknown) => lo.reason(e))
      return { reading, cleanup }
    })

  const letGo = async <R>(core: Core<R>, before: () => Promise<void>): Promise<string | null> => {
    await before()
    const released = core.releasable(await told())
    if (released.refused.length > 0) {
      const kept = await holdRow(SEEN_ROWS, ACTED, { literals: [...released.refused] })
      // Not let go unless joined to the acted ones: a tab still holding one unsent would post it again.
      if (kept.type === "Error") return `the refusals it lets go were not joined to the acted ones (${kept.e})`
    }
    const gone = await settled(released.free)
    return gone.type === "Error" ? gone.e : null
  }

  // The records `free` names let go, and the seen prints of records let go, which are read by nothing; nothing written
  // when it names none.
  export const settled = async (free: ReadonlySet<string>): Promise<lo.Settled<null>> => {
    if (free.size === 0) return lo.val(null)
    const trimmed = await alterRow(ROWS, NAME, row => ({ records: row.records.filter(r => !free.has(r.literal)) }))
    const held = await kept()
    if (held.type === "Val") {
      const left = new Set(held.v.map(r => r.literal))
      await alterRow(SEEN_ROWS, SEEN, row => ({ literals: row.literals.filter(l => left.has(printOf(l))) }))
    }
    return trimmed
  }

  // The records a move of the own lines answered (its `delta`, UTF-8 JSON).
  const Moved = d.json(d.arr(RecordOf))
  export const ofDelta = (delta: Uint8Array): lo.Settled<readonly Kept[]> => Moved(new TextDecoder().decode(delta))
}
export default ownlines
