// A conversation's window as the replicas read it, dressed with what only the box says: block renderings, the memory's
// words, and proposals drawn against their chains.

import type { MemoryPayload, ProposalRow } from "../wire/generated/wire"
import type { Dressed, Streamed, Windowed } from "../wire/generated/pages"
import { blockBySource, drawnProposals, getMemory } from "../wire/api"
import box from "../app/asked"
import rendered from "../app/rendered"
import storesync from "../prodrome/storesync"
import { watchOthers, watchReplica } from "../prodrome/sync"
import time from "../app/time"
import type grouped from "./grouped"
import pageBlocks from "../app/blocks"
import type questions from "../prodrome/questions"
import type { AnsweredBlock } from "../prodrome/core"
import type lanes from "../prodrome/lanes"
import { measure } from "../app/perf"
import text from "../wire/text"

export namespace dressed {
  // Each block the device lacks, from the box, kept on the device, so the next reading holds it; a block the box will
  // not render stays its source, which the view draws.
  const fetched = (missing: readonly AnsweredBlock[]): Promise<unknown> =>
    missing.length === 0
      ? Promise.resolve()
      : rendered.throughAll(pageBlocks.store, missing.map(b => ({ k: b.key, ask: () => blockBySource(b.conversation, b.source) })))

  // Each proposal as the box drew it, kept until anything a row is drawn from moves: the box's chains, the tab's chains,
  // or the stores the tab pulls.
  const kept = new Map<string, { readonly id: string; readonly drawn: questions.Drawn }>()
  const moved = (): void => kept.clear()
  box.hearMoves(moved)
  storesync.watch(moved)
  watchReplica(moved)
  watchOthers(moved)
  // A proposal by its index and the row the replicas read: drawn again once that row moves.
  const rowKey = (p: ProposalRow): string => `${p.index}|${JSON.stringify(p)}`
  const keyOf = (id: string, p: ProposalRow): string => `${id}/${rowKey(p)}`

  const drawnOf = (id: string): questions.Drawn[] => [...kept.values()].flatMap(k => (k.id === id ? [k.drawn] : []))

  // Each of `ps` as the box drew it, kept; a failed read draws none, and the reading draws the replica's row.
  const draw = async (id: string, ps: readonly ProposalRow[]): Promise<questions.Drawn[]> => {
    if (ps.length === 0) return []
    const rows = await drawnProposals(id, ps.map(p => p.index)).then(
      d => new Map(d.proposals.map(r => [r.index, r])),
      () => new Map<number, ProposalRow>(),
    )
    return ps.flatMap(p => {
      const row = rows.get(p.index)
      if (row === undefined) return []
      const drawn = { from: JSON.stringify(p), row }
      kept.set(keyOf(id, p), { id, drawn })
      return [drawn]
    })
  }

  // What a reading lacks that only the box says.
  type Needs = { readonly missing: readonly AnsweredBlock[]; readonly drawing: readonly ProposalRow[] }
  const NOTHING: Needs = { missing: [], drawing: [] }

  // `read`, asked again until it lacks nothing the box was not already asked for: the box is asked once for each block
  // and each proposal, and one it does not answer stays as the replicas read it. `drawBy` draws proposals, and each ask
  // is handed every one drawn so far.
  const dressedBy = async <R>(
    read: R,
    ask: (drawn: readonly questions.Drawn[]) => Promise<R>,
    needs: (r: R) => Needs,
    drawBy: (ps: readonly ProposalRow[]) => Promise<questions.Drawn[]>,
    tried: ReadonlySet<string> = new Set(),
    drawn: readonly questions.Drawn[] = [],
  ): Promise<R> => {
    const n = needs(read)
    const missing = n.missing.filter(b => !tried.has(b.key))
    const drawing = n.drawing.filter(p => !tried.has(rowKey(p)))
    if (missing.length === 0 && drawing.length === 0) return read
    const [, more] = await Promise.all([fetched(missing), drawBy(drawing)])
    const all = [...drawn, ...more]
    return dressedBy(await ask(all), ask, needs, drawBy, new Set([...tried, ...missing.map(b => b.key), ...drawing.map(rowKey)]), all)
  }

  // null where the box did not answer: a receipt then reads unread.
  export const asked = (): Promise<MemoryPayload | null> => getMemory().catch(() => null)

  // Nothing waits on the network before the pricing, so `held` is what was priced.
  export const inbox = (at: string, memory: MemoryPayload | null, lane: lanes.Lane): Promise<text.Text<storesync.Proposals>> => storesync.proposals(at, memory, lane)

  // The worker's text as it printed it, read again while it lacks a block the box was not yet asked for. Measured as
  // `read:Stream`, the ask and the turns' words together.
  export const stream = async (chosen: readonly string[], pages: number, lane: lanes.Lane): Promise<text.Text<Dressed<Streamed>>> => {
    const start = performance.now()
    const ask = (): Promise<questions.Worded<Dressed<Streamed>>> => storesync.streamed(chosen, pages, lane)
    const read = await dressedBy(await ask(), ask, w => ({ missing: w.missing, drawing: [] }), () => Promise.resolve([]))
    measure("read:Stream", start)
    return read.text
  }

  export const window = async (id: string, w: grouped.Asked, zone: time.Zone, lane: lanes.Lane): Promise<text.Text<Windowed>> => {
    type At = Exclude<questions.Grouped, { readonly type: "Absent" }>
    const at = (m: MemoryPayload | null, drawn: readonly questions.Drawn[] = drawnOf(id)): Promise<At> =>
      storesync.window(id, w, m, drawn, zone, lane).then((one): At => (one.type === "Absent" ? storesync.absent(id) : one))
    const bare = await at(null)
    if (bare.type !== "Held") return text.printed(bare)
    const m = bare.receipts === 0 ? null : await asked()
    // Each ask is handed the rows this reading drew: a move between clears what the tab keeps, not those.
    const ask = (drawn: readonly questions.Drawn[]): Promise<At> => at(m, [...drawnOf(id), ...drawn])
    const read = await dressedBy(m === null ? bare : await ask([]), ask, r => (r.type === "Held" ? r : NOTHING), ps => draw(id, ps))
    return read.type === "Held" ? read.text : text.printed(read)
  }
}
export default dressed
