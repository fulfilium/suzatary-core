// What this tab wrote to the lines, kept by the device until the box serves it back: each own line's print, its
// slots' keys and the box's answer, in a row of the rows' store beside the chains', which every upgrade keeps
// (`objects`). The row is plain JSON of what no build changes, so a snapshot of another build, a reset of the lines and
// an upgrade drop the lines' log and never an own line: the worker reads the row back into its core on every load.
//
// The row is a join: by print, the answer that climbed furthest (unsent, refused until trusted, refused for good,
// taken), so two tabs keeping their moves in either order keep the same row.

import type { Rows } from "./objects"
import { alterRow, holdRow, heldRow } from "./objects"
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

  // A reset never reaches this row: nothing in it is the box's to take again.
  const ROWS: Rows<Row> = {
    parse: (raw: unknown) => parse(raw),
    print: r => r,
    generation: () => 0,
    unsent: () => [],
    join,
    after: () => NOTHING,
    nothing: NOTHING,
  }

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
    const kept = await holdRow(ROWS, NAME, { type: "Join", replica: { records } })
    const was = before.type === "Held" ? before.v : NOTHING
    if (kept.type === "Val" && JSON.stringify(kept.v) !== JSON.stringify(join(was, NOTHING))) channel()?.postMessage(null)
    return kept
  }

  // `wake` run each time another tab keeps a line in the row.
  export const heard = (wake: () => void): void => {
    const heard = channel()
    if (heard !== null) heard.onmessage = () => wake()
  }

  // Every record kept, or why the row does not read.
  export const kept = async (): Promise<lo.Settled<readonly Kept[]>> => {
    const row = await heldRow(ROWS, NAME)
    return row.type === "Held" ? lo.val(row.v.records) : row.type === "Absent" ? lo.val([]) : lo.error(row.why)
  }

  // The prints of the refused lines the person was told of, with their words: a row of its own beside the records, a
  // join of sets, so two tabs telling the same refusal keep it once.
  type Seen = { readonly literals: readonly string[] }
  const SEEN = "own/seen"
  const NO_ONE: Seen = { literals: [] }
  const SEEN_ROWS: Rows<Seen> = {
    parse: (raw: unknown) => d.obj<Seen>({ literals: d.arr(d.str) })(raw),
    print: r => r,
    generation: () => 0,
    unsent: () => [],
    join: (a, b) => ({ literals: [...new Set([...a.literals, ...b.literals])].sort() }),
    after: () => NO_ONE,
    nothing: NO_ONE,
  }

  // The refused lines whose prints `literals` names were said to the person, their words with them.
  export const seen = async (literals: readonly string[]): Promise<void> => {
    if (literals.length > 0) await holdRow(SEEN_ROWS, SEEN, { type: "Join", replica: { literals } })
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

  // Every record the box answered for good let go once a load has read it back, as far as nothing is lost: a refused
  // one only once the person was told of it with its words (`seen`), which until then the record alone keeps; a taken
  // one only where `free` names its print, since its record holds its keys until the device's landed rows do. What may
  // still be handed over stays.
  export const settled = async (free: ReadonlySet<string>): Promise<lo.Settled<null>> => {
    const said = await told()
    const done = await alterRow(ROWS, NAME, row => ({
      records: row.records.filter(r => {
        const climbed = rank(r.answer)
        return climbed < 2 || (climbed === 2 && !said.has(r.literal)) || (climbed === 3 && !free.has(r.literal))
      }),
    }))
    // The seen prints of records let go are read by nothing.
    const held = await kept()
    if (held.type === "Val") {
      const left = new Set(held.v.map(r => r.literal))
      await alterRow(SEEN_ROWS, SEEN, row => ({ literals: row.literals.filter(l => left.has(printOf(l))) }))
    }
    return done
  }

  // The records a move of the own lines answered (its `delta`, UTF-8 JSON).
  const Moved = d.json(d.arr(RecordOf))
  export const ofDelta = (delta: Uint8Array): lo.Settled<readonly Kept[]> => Moved(new TextDecoder().decode(delta))
}
export default ownlines
