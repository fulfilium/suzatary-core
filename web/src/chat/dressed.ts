// A conversation's window as the replicas read it, dressed with what only the box says: block renderings, the memory's
// words, and proposals drawn against their chains.

import type {
  BlockRow,
  ConversationPayload,
  MemoryPayload,
  ProposalRow,
  TurnRow,
} from "../wire/generated/wire"
import type { Window } from "../prodrome/core"
import { blockBySource, drawnProposals, getMemory } from "../wire/api"
import box from "../app/asked"
import rendered from "../app/rendered"
import storesync from "../prodrome/storesync"
import { watchOthers, watchReplica } from "../prodrome/sync"
import time from "../app/time"
import grouped from "./grouped"
import pageBlocks from "../app/blocks"
import type questions from "../prodrome/questions"
import type lanes from "../prodrome/lanes"

export namespace dressed {
  // A block the box will not render stays its source, which the view draws.
  const turnsOf = async (ts: readonly { readonly id: string; readonly turn: TurnRow }[], keys: questions.Keyed): Promise<TurnRow[]> => {
    const blocks = ts.flatMap(({ id, turn }) => (turn.who === "model" ? turn.blocks.map(b => ({ id, b })) : []))
    if (blocks.length === 0) return ts.map(t => t.turn)
    const html = await rendered.throughAll(pageBlocks.store, blocks.map(({ id, b }) => ({ k: keys[b.source] ?? null, ask: () => blockBySource(id, b.source) })))
    const dressed = new Map(blocks.map(({ b }, i): [BlockRow, BlockRow] => [b, html[i] === null || html[i] === undefined ? b : { ...b, html: html[i] }]))
    return ts.map(({ turn }) => (turn.who !== "model" || turn.blocks.length === 0 ? turn : { ...turn, blocks: turn.blocks.map(b => dressed.get(b) ?? b) }))
  }

  // The card counts what it draws, so a drawn proposal is the box's whole row.
  const drawn = (p: ProposalRow): boolean =>
    p.act.kind === "changes" || p.act.kind === "secret" || p.act.kind === "connect" || p.coding?.state === "reported"

  // Kept until anything a row is drawn from moves: the box's chains, the tab's chains, or the stores the tab pulls.
  const kept = new Map<string, Promise<ProposalRow>>()
  const moved = (): void => kept.clear()
  box.hearMoves(moved)
  storesync.watch(moved)
  watchReplica(moved)
  watchOthers(moved)
  const keyOf = (id: string, p: ProposalRow): string => `${id}/${p.index}|${JSON.stringify(p)}`

  const proposalsOf = (id: string, ps: readonly ProposalRow[]): Promise<ProposalRow[]> => {
    const lacking = ps.filter(p => drawn(p) && !kept.has(keyOf(id, p)))
    if (lacking.length > 0) {
      const read = drawnProposals(id, lacking.map(p => p.index)).then(d => new Map(d.proposals.map(r => [r.index, r])))
      for (const p of lacking) {
        const key = keyOf(id, p)
        // A failed read is not kept; this reading draws the replica's row.
        const row = read.then(rows => rows.get(p.index) ?? p)
        kept.set(key, row.catch(() => (kept.delete(key), p)))
      }
    }
    return Promise.all(ps.map(p => (drawn(p) ? (kept.get(keyOf(id, p)) ?? Promise.resolve(p)) : Promise.resolve(p))))
  }

  // null where the box did not answer: a receipt then reads unread.
  export const asked = (): Promise<MemoryPayload | null> => getMemory().catch(() => null)

  const memory = (w: ConversationPayload): Promise<MemoryPayload | null> =>
    w.turns.some(t => t.who === "verb" && t.outcome.is === "remembered") || w.proposals.some(p => p.act.kind === "remember" || p.act.kind === "consolidate")
      ? asked()
      : Promise.resolve(null)

  // Nothing waits on the network before the pricing, so `held` is what was priced.
  export type Inbox = storesync.Proposals
  export const inbox = (at: string, memory: MemoryPayload | null, lane: lanes.Lane): Promise<Inbox> => storesync.proposals(at, memory, lane)

  type Held = { readonly type: "Read"; readonly payload: ConversationPayload } | questions.NotHere | questions.Unheard
  const held = (id: string, w: Window, memory: MemoryPayload | null, lane: lanes.Lane): Promise<Held> =>
    storesync.conversation(id, w, memory, lane).then((one): Held => (one.type === "Absent" ? storesync.absent(id) : one.type === "Held" ? { type: "Read", payload: one.payload } : one))

  // What a proposal's card needs and nothing more: every proposal, drawn, beside the newest turn.
  export const cards = async (id: string, memory: MemoryPayload | null, lane: lanes.Lane): Promise<ConversationPayload> => {
    const one = await held(id, { limit: 1 }, memory, lane)
    if (one.type === "NotHere") throw new Error(storesync.notHere(one.lines, "this conversation", one.why))
    if (one.type === "Unheard") throw new Error(storesync.unheard)
    return { ...one.payload, proposals: await proposalsOf(id, one.payload.proposals) }
  }

  export const stream = async (chosen: readonly string[], pages: number, lane: lanes.Lane): Promise<storesync.Streamed> => {
    const read = await storesync.streamed(chosen, pages, lane)
    const answers = read.stream.rounds.flatMap(r => (r.answer === null ? [] : [{ id: r.conversation, turn: r.answer }]))
    const dressed = await turnsOf(answers, read.keys)
    let next = 0
    const rounds = read.stream.rounds.map(r => (r.answer === null ? r : { ...r, answer: dressed[next++] ?? r.answer }))
    return { ...read, stream: { ...read.stream, rounds } }
  }

  export type Windowed = { readonly type: "Read"; readonly payload: grouped.Window } | questions.NotHere | questions.Unheard

  export const window = async (id: string, w: grouped.Asked, zone: time.Zone, lane: lanes.Lane): Promise<Windowed> => {
    type At = { readonly type: "Held"; readonly payload: grouped.Window; readonly keys: questions.Keyed } | questions.NotHere | questions.Unheard
    const at = (m: MemoryPayload | null): Promise<At> =>
      storesync.window(id, w, m, zone, lane).then((one): At => (one.type === "Absent" ? storesync.absent(id) : one))
    const bare = await at(null)
    if (bare.type !== "Held") return bare
    const m = await memory(bare.payload)
    const read = m === null ? bare : await at(m)
    if (read.type !== "Held") return read
    const [turns, proposals] = await Promise.all([
      turnsOf(read.payload.turns.map(turn => ({ id, turn })), read.keys),
      proposalsOf(id, read.payload.proposals),
    ])
    return { type: "Read", payload: { ...read.payload, turns, proposals } }
  }
}
export default dressed
