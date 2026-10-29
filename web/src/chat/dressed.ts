// A conversation's window as the replicas read it, dressed with what only the box says: each answer block's
// rendering by its sha, the memory's words for a receipt or a remember card, and a proposal drawn against its chain.

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
import turns from "../chat/turns"
import time from "../app/time"
import grouped from "./grouped"
import pageBlocks from "../app/blocks"
import type questions from "../prodrome/questions"

export namespace dressed {
  // Every model turn's blocks with their renderings, each turn of the conversation beside it, kept by the keys the
  // worker named them by, read together and asked of the box for the rest; a block the box will not render stays its
  // source, which the view draws.
  const turnsOf = async (ts: readonly { readonly id: string; readonly turn: TurnRow }[], keys: questions.Keyed): Promise<TurnRow[]> => {
    const blocks = ts.flatMap(({ id, turn }) => (turn.who === "model" ? turn.blocks.map(b => ({ id, b })) : []))
    if (blocks.length === 0) return ts.map(t => t.turn)
    const html = await rendered.throughAll(pageBlocks.store, blocks.map(({ id, b }) => ({ k: keys[b.source] ?? null, ask: () => blockBySource(id, b.source) })))
    const dressed = new Map(blocks.map(({ b }, i): [BlockRow, BlockRow] => [b, html[i] === null || html[i] === undefined ? b : { ...b, html: html[i] }]))
    return ts.map(({ turn }) => (turn.who !== "model" || turn.blocks.length === 0 ? turn : { ...turn, blocks: turn.blocks.map(b => dressed.get(b) ?? b) }))
  }

  // The card counts what it draws, so a drawn proposal is the box's whole row; a secret's card links to where the
  // deployment's own declarations say it is issued, and a connection's names what its plugin may reach now.
  const drawn = (p: ProposalRow): boolean =>
    p.act.kind === "changes" || p.act.kind === "secret" || p.act.kind === "connect" || p.coding?.state === "reported"

  // Each drawn row, by its conversation, its index and the row the replicas read, until anything it is drawn from moves:
  // the box's chains, the chains the tab holds, or the inbox and conversations the tab pulls (an answer to a card the box
  // performs, a session handed over). A page read again with nothing moved asks nothing, and a conversation's rows not
  // yet held come in one read.
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
        // A failed read is not kept, so the next reading asks again; this one draws the replica's row.
        const row = read.then(rows => rows.get(p.index) ?? p)
        kept.set(key, row.catch(() => (kept.delete(key), p)))
      }
    }
    return Promise.all(ps.map(p => (drawn(p) ? (kept.get(keyOf(id, p)) ?? Promise.resolve(p)) : Promise.resolve(p))))
  }

  // The memory's words as the box keeps them, or null where it did not answer: a receipt then reads unread.
  export const asked = (): Promise<MemoryPayload | null> => getMemory().catch(() => null)

  const memory = (w: ConversationPayload): Promise<MemoryPayload | null> =>
    w.turns.some(t => turns.remembered(t) !== null) || w.proposals.some(p => p.act.kind === "remember")
      ? asked()
      : Promise.resolve(null)

  // The Inbox's rows and the chains they were priced through, a remember card's headline with `memory`'s words. Nothing
  // waits on the network: the chains are the ones the tab holds, and the set is made inside the reading, with no wait
  // between it and the pricing, so `held` is what was priced.
  export type Inbox = storesync.Proposals
  export const inbox = (at: string, memory: MemoryPayload | null): Promise<Inbox> => storesync.proposals(at, memory, "Page")

  // A conversation's window, or how many of its lines are not on this device and why, refused in the core's words where
  // the store holds no line of it.
  type Held = { readonly type: "Read"; readonly payload: ConversationPayload } | questions.NotHere
  const held = (id: string, w: Window, memory: MemoryPayload | null): Promise<Held> =>
    storesync.conversation(id, w, memory).then((one): Held => (one.type === "Absent" ? storesync.absent(id) : one.type === "Held" ? { type: "Read", payload: one.payload } : one))

  // What a proposal's card needs and nothing more: every proposal, drawn, beside the newest turn, a remember card with
  // the words `memory` keeps for it.
  export const cards = async (id: string, memory: MemoryPayload | null): Promise<ConversationPayload> => {
    const one = await held(id, { limit: 1 }, memory)
    if (one.type === "NotHere") throw new Error(storesync.notHere(one.lines, "this conversation", one.why))
    return { ...one.payload, proposals: await proposalsOf(id, one.payload.proposals) }
  }

  // The stream a filter shows, each answer dressed with its blocks' renderings as a conversation's window is.
  export const stream = async (chosen: readonly string[], pages: number): Promise<storesync.Streamed> => {
    const read = await storesync.streamed(chosen, pages)
    const answers = read.stream.rounds.flatMap(r => (r.answer === null ? [] : [{ id: r.conversation, turn: r.answer }]))
    const dressed = await turnsOf(answers, read.keys)
    let next = 0
    const rounds = read.stream.rounds.map(r => (r.answer === null ? r : { ...r, answer: dressed[next++] ?? r.answer }))
    return { ...read, stream: { ...read.stream, rounds } }
  }

  // A window grouped into rounds, or how many of the conversation's lines are not on this device and why, as the
  // worker says.
  export type Windowed = { readonly type: "Read"; readonly payload: grouped.Window } | questions.NotHere

  // A window, grouped into rounds on the owner's days in `zone`.
  export const window = async (id: string, w: grouped.Asked, zone: time.Zone): Promise<Windowed> => {
    type At = { readonly type: "Held"; readonly payload: grouped.Window; readonly keys: questions.Keyed } | questions.NotHere
    const at = (m: MemoryPayload | null): Promise<At> =>
      storesync.window(id, w, m, zone).then((one): At => (one.type === "Absent" ? storesync.absent(id) : one))
    const bare = await at(null)
    if (bare.type === "NotHere") return bare
    const m = await memory(bare.payload)
    const read = m === null ? bare : await at(m)
    if (read.type === "NotHere") return read
    const [turns, proposals] = await Promise.all([
      turnsOf(read.payload.turns.map(turn => ({ id, turn })), read.keys),
      proposalsOf(id, read.payload.proposals),
    ])
    return { type: "Read", payload: { ...read.payload, turns, proposals } }
  }
}
export default dressed
