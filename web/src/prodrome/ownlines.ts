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

  // How far an answer climbed; the core's order (`writes::Answer`).
  const Trusting = d.obj<{ Refused: { kind: "Trust" } }>({ Refused: d.obj<{ kind: "Trust" }>({ kind: d.lit("Trust") }) })
  const rank = (answer: unknown): number =>
    answer === "Taken" ? 3 : answer === "Unsent" ? 0 : Trusting(answer).type === "Val" ? 1 : 2

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

  // The records joined in.
  export const keep = (records: readonly Kept[]): Promise<lo.Settled<Row>> => holdRow(ROWS, NAME, { type: "Join", replica: { records } })

  // Every record kept, or why the row does not read.
  export const kept = async (): Promise<lo.Settled<readonly Kept[]>> => {
    const row = await heldRow(ROWS, NAME)
    return row.type === "Held" ? lo.val(row.v.records) : row.type === "Absent" ? lo.val([]) : lo.error(row.why)
  }

  // Every record the box answered for good, taken or refused, let go once a load has read it back: a taken line is
  // the box's to serve, and a refused one's words stay in the field they were typed in, which keeps them itself. Only
  // what may still be handed over stays.
  export const settled = (): Promise<lo.Settled<null>> =>
    alterRow(ROWS, NAME, row => ({ records: row.records.filter(r => rank(r.answer) < 2) }))

  // The records a move of the own lines answered (its `delta`, UTF-8 JSON).
  const Moved = d.json(d.arr(RecordOf))
  export const ofDelta = (delta: Uint8Array): lo.Settled<readonly Kept[]> => Moved(new TextDecoder().decode(delta))
}
export default ownlines
