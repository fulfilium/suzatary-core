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

export namespace dressed {
  // A block the box will not render stays its source, which the view draws.
  const blockOf = async (id: string, b: BlockRow): Promise<BlockRow> => {
    return rendered.through(rendered.store, await rendered.answerKey(b.source), () => blockBySource(id, b.source)).then(
      html => ({ ...b, html }),
      () => b,
    )
  }

  const turnOf = (id: string, t: TurnRow): Promise<TurnRow> =>
    t.who !== "model" || t.blocks.length === 0 ? Promise.resolve(t) : Promise.all(t.blocks.map(b => blockOf(id, b))).then(blocks => ({ ...t, blocks }))

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
  export const inbox = (at: string, memory: MemoryPayload | null): Promise<Inbox> => storesync.proposals(at, memory)

  // A conversation's window, refused in the core's words where the store holds no line of it.
  const held = (id: string, w: Window, memory: MemoryPayload | null): Promise<ConversationPayload> =>
    storesync.conversation(id, w, memory).then(one => (one.type === "Held" ? one.payload : storesync.absent(id)))

  // What a proposal's card needs and nothing more: every proposal, drawn, beside the newest turn, a remember card with
  // the words `memory` keeps for it.
  export const cards = async (id: string, memory: MemoryPayload | null): Promise<ConversationPayload> => {
    const read = await held(id, { limit: 1 }, memory)
    return { ...read, proposals: await proposalsOf(id, read.proposals) }
  }

  // The stream a filter shows, each answer dressed with its blocks' renderings as a conversation's window is.
  export const stream = async (chosen: readonly string[], pages: number): Promise<storesync.Streamed> => {
    const read = await storesync.streamed(chosen, pages)
    const rounds = await Promise.all(
      read.stream.rounds.map(async r => (r.answer === null ? r : { ...r, answer: await turnOf(r.conversation, r.answer) })),
    )
    return { ...read, stream: { ...read.stream, rounds } }
  }

  export const window = async (id: string, w: Window): Promise<ConversationPayload> => {
    const at = (m: MemoryPayload | null): Promise<ConversationPayload> => held(id, w, m)
    const bare = await at(null)
    const m = await memory(bare)
    const read = m === null ? bare : await at(m)
    const [turns, proposals] = await Promise.all([
      Promise.all(read.turns.map(t => turnOf(id, t))),
      proposalsOf(id, read.proposals),
    ])
    return { ...read, turns, proposals }
  }
}
export default dressed
