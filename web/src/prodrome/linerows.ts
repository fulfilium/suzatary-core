// Where the lines' row lives on the device: one row per skeleton node and one per key of their words, beside a head row
// holding the generation, the box's words, its refusals and the frontier, and the pieces of a pull not yet landed. A
// node is named by its hash and never changes, so a landing writes the nodes and keys it brought and the head, never the
// store, and two tabs writing the same node write the same row. A key's bytes are in its key row alone, a piece's
// included, so a forget is one delete. A reset or a later epoch empties the store and writes what replaces it.

import idb from "../app/idb"
import lo from "../algebra/lo"
import d from "../wire/decoder"
import payload from "../wire/generated/payload"
import type { RefusedObject, SkeletonNode } from "../wire/generated/wire"
import locked from "./locked"
import { CHAIN, LINE_ROWS, Spoken, Standing, dropRow, dropRows, heldRow, storeRows } from "./objects"
import type pull from "./pull"
import type { Frontier, Held, Replica, Word } from "./replica"
import { empty, epochOf, replicaOf } from "./replica"

export namespace linerows {
  // What the head row holds: the row's generation, the box's words, its refusals and the frontier its next pull asks
  // from; the nodes and keys are rows.
  type Head = { readonly generation: number; readonly words: readonly Word[]; readonly refused: readonly RefusedObject[]; readonly frontier: Frontier }
  const HEAD = "head"
  const Head = d.obj<Head>({ generation: d.num, words: d.arr(Spoken), refused: d.arr(payload.RefusedObject), frontier: Standing })

  const NODE = "node/"
  const KEY = "key/"
  const PIECE = "piece/"
  const range = (prefix: string): IDBKeyRange => IDBKeyRange.bound(prefix, `${prefix}￿`)
  const pieceAt = (at: number): string => `${PIECE}${String(at).padStart(12, "0")}`

  const NONE = locked.none<SkeletonNode>()
  // The lines' row and its pieces' rows as version 5 kept them, whole, in the rows' store: read once to move them here.
  const WHOLE = "store/lines"
  const wholeRows = storeRows(payload.SkeletonNode, NONE, () => [])
  // A piece's row holds its nodes alone; its keys are key rows.
  const pieceRows = wholeRows

  // The head a raw row holds, or none for an absent one or one that does not read.
  const headIn = (raw: unknown): Head | null => {
    if (raw === undefined) return null
    const h = Head(raw)
    return h.type === "Val" ? h.v : null
  }

  const headOf = (r: Replica<SkeletonNode>): Head => ({ generation: r.generation, words: r.held.words, refused: r.held.refused, frontier: r.held.frontier })

  // Parents first, as a replica holds its objects, the least name first among those ready: the nodes are read by name.
  const ordered = (nodes: readonly SkeletonNode[]): SkeletonNode[] => {
    const byName = new Map(nodes.map(n => [n.hash, n]))
    const waiting = new Map<string, number>()
    const children = new Map<string, string[]>()
    for (const n of nodes) {
      const parents = n.parents.filter(p => byName.has(p))
      waiting.set(n.hash, parents.length)
      for (const p of parents) children.set(p, [...(children.get(p) ?? []), n.hash])
    }
    const ready = nodes.filter(n => waiting.get(n.hash) === 0).map(n => n.hash).sort()
    const out: SkeletonNode[] = []
    for (let i = 0; i < ready.length; i += 1) {
      const name = ready[i]!
      out.push(byName.get(name)!)
      for (const c of children.get(name) ?? []) {
        const left = (waiting.get(c) ?? 0) - 1
        waiting.set(c, left)
        if (left === 0) ready.push(c)
      }
    }
    return out
  }

  const rowOf = (head: Head, nodes: readonly SkeletonNode[], keys: locked.Keys): locked.Row<SkeletonNode> => {
    const objects = ordered(nodes)
    const held: Held<SkeletonNode> = { generation: head.generation, objects, synced: objects.map(n => n.hash), refused: head.refused, words: head.words, frontier: head.frontier }
    return locked.of(replicaOf(held), keys)
  }

  // The row the store holds, read in one transaction; a node or a head that does not read is named, never passed over.
  type Read = {
    readonly head: Head | null
    // The head as the store holds it, printed, or null for none.
    readonly printed: string | null
    readonly row: locked.Row<SkeletonNode>
    readonly keys: locked.Keys
    readonly pieces: readonly locked.Row<SkeletonNode>[]
    readonly unread: readonly string[]
  }
  const readAll = (s: IDBObjectStore): Promise<Read> =>
    Promise.all([
      idb.done(s.get(HEAD)),
      idb.done(s.getAll(range(NODE))),
      idb.done(s.getAllKeys(range(KEY))),
      idb.done(s.getAll(range(KEY))),
      idb.done(s.getAll(range(PIECE))),
    ]).then(([rawHead, rawNodes, keyNames, keyValues, rawPieces]) => {
      const unread: string[] = []
      const head = headIn(rawHead)
      if (rawHead !== undefined && head === null) unread.push("the lines' head row does not read")
      const nodes = rawNodes.flatMap(raw => {
        const n = payload.SkeletonNode(raw)
        if (n.type === "Error") unread.push(`a node of the lines does not read (${n.e})`)
        return n.type === "Val" ? [n.v] : []
      })
      const keys: Record<string, string> = {}
      keyNames.forEach((name, i) => {
        const v = keyValues[i]
        if (typeof name === "string" && typeof v === "string") keys[name.slice(KEY.length)] = v
      })
      const pieces = rawPieces.flatMap(raw => {
        const p = pieceRows.parse(raw)
        if (p.type === "Error") unread.push(p.e)
        return p.type === "Val" ? [p.v] : []
      })
      return { head, printed: rawHead === undefined ? null : printed(rawHead), row: head === null ? NONE : rowOf(head, nodes, keys), keys, pieces, unread }
    })

  // `row` written whole over what the store holds: a reset's, a later epoch's, or a row moved from version 5.
  const writeAll = (s: IDBObjectStore, row: locked.Row<SkeletonNode>): Promise<unknown> => {
    s.clear()
    return Promise.all([idb.done(s.put(headOf(row.replica), HEAD)), ...nodesOf(s, row.replica.held.objects), ...keysOf(s, row.keys)])
  }
  // Each node and each key of the row's words a row of its own: the one place the store holds a key's bytes.
  const nodesOf = (s: IDBObjectStore, nodes: readonly SkeletonNode[]): Promise<unknown>[] => nodes.map(n => idb.done(s.put(n, `${NODE}${n.hash}`)))
  const keysOf = (s: IDBObjectStore, keys: locked.Keys): Promise<unknown>[] => Object.entries(keys).map(([name, hex]) => idb.done(s.put(hex, `${KEY}${name}`)))

  // A head printed with its fields in one order, so the head a tab wrote and the head it reads back compare equal.
  const printed = (v: unknown): string =>
    JSON.stringify(v, (_, x: unknown) =>
      x !== null && typeof x === "object" && !Array.isArray(x) ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) : x,
    )

  // The version 5 or 6 row moved into this store, once, and then deleted with its pieces: a store already holding a head
  // keeps it, since what the old row holds is the box's, pulled again by heads. A row that does not read is deleted. The
  // old pieces go whether or not a row is there: a pull cut before its first landing kept them, keys and all, with none.
  export const migrate = async (): Promise<lo.Settled<null>> => {
    const old = await heldRow(wholeRows, WHOLE)
    if (old.type === "Held") {
      const moved = await idb
        .within(LINE_ROWS, "readwrite", async s => {
          if ((await idb.done(s.get(HEAD))) === undefined) await writeAll(s, old.v)
          return lo.val(null)
        })
        .catch((e: unknown) => lo.error(lo.reason(e)))
      if (moved.type === "Error") return moved
    }
    const pieces = await dropRows(`${WHOLE}/piece/`)
    if (pieces.type === "Error") return pieces
    return old.type === "Absent" ? lo.val(null) : dropRow(CHAIN, WHOLE, new Set())
  }

  const without = <A>(f: () => Promise<A>, otherwise: A): Promise<A> => (idb.available() ? f() : Promise.resolve(otherwise))
  const settled = (p: Promise<lo.Settled<null>>): Promise<lo.Settled<null>> => p.catch((e: unknown) => lo.error(lo.reason(e)))

  // Where a worker keeps its lines' row. It remembers the row the device last confirmed holding and the head it held
  // then: while the store's head is still that one, no other tab has written, and a landing writes what it holds beyond
  // that row; otherwise the store's row is read back. Either way the row written is the JOIN of the device's and the
  // landing's (`locked.join`, the one rule that composes a head, its words and its frontier), so a tab behind another
  // never writes its older word or frontier over the later one, and within a generation and epoch the store is never
  // cleared, so another tab's nodes stay. A later generation or epoch on either side is written whole.
  //
  // A key's bytes are held in its own `key/` row and nowhere else on the device: a piece keeps its nodes there and its
  // keys beside the row's, so forgetting a key is the one delete of its row.
  // Where the lines' row lives, and how keys are taken out of it, whatever else another tab wrote there.
  export type Place = pull.Place<locked.Row<SkeletonNode>> & { readonly forget: (gone: readonly string[]) => Promise<lo.Settled<null>> }

  export const placeOf = (): Place => {
    let confirmed: { readonly row: locked.Row<SkeletonNode>; readonly head: string } | null = null
    const confirming = (row: locked.Row<SkeletonNode>): void => void (confirmed = { row, head: printed(headOf(row.replica)) })
    const confirm = <A>(p: Promise<lo.Settled<A>>, row: (a: A) => locked.Row<SkeletonNode> | null): Promise<lo.Settled<A>> =>
      p.then(
        a => {
          const r = a.type === "Val" ? row(a.v) : null
          if (r === null) confirmed = null
          else confirming(r)
          return a
        },
        (e: unknown) => ((confirmed = null), lo.error(lo.reason(e))),
      )
    // What the store holds: the confirmed row while its head is the one confirmed, else the store's row read back.
    const deviceRow = async (s: IDBObjectStore): Promise<locked.Row<SkeletonNode>> => {
      const raw: unknown = await idb.done(s.get(HEAD))
      const known = confirmed
      return known !== null && raw !== undefined && printed(raw) === known.head ? known.row : (await readAll(s)).row
    }
    const sameEpoch = (a: locked.Row<SkeletonNode>, b: locked.Row<SkeletonNode>): boolean =>
      a.replica.generation === b.replica.generation && epochOf(headOf(a.replica)) === epochOf(headOf(b.replica))
    return {
      restore: () =>
        without(
          async () => {
            const moved = await migrate()
            const read = await idb.within(LINE_ROWS, "readonly", readAll).catch((e: unknown): Read | string => lo.reason(e))
            if (typeof read === "string") return { row: { type: "Unreadable", why: read }, pieces: [], unread: [] }
            confirmed = read.unread.length === 0 && read.printed !== null ? { row: read.row, head: read.printed } : null
            const unread = moved.type === "Error" ? [...read.unread, `the lines' row of version 5 or 6 was not moved (${moved.e})`] : read.unread
            // The pieces' keys are the store's key rows, which hold the row's as well: a union, so either holds them.
            const pieces = read.pieces.map(p => locked.of(p.replica, read.keys))
            return { row: read.head === null ? { type: "Absent" } : { type: "Held", row: read.row }, pieces, unread }
          },
          { row: { type: "Absent" }, pieces: [], unread: [] },
        ),
      // The piece's nodes joined into its row, and its keys each a key row: never a key's bytes inside a piece.
      keepPiece: (at, piece) =>
        without(
          () =>
            settled(
              idb.within(LINE_ROWS, "readwrite", async s => {
                const raw: unknown = await idb.done(s.get(pieceAt(at)))
                const was = raw === undefined ? null : pieceRows.parse(raw)
                const nodes = locked.of(piece.replica)
                const now = was !== null && was.type === "Val" ? locked.join(locked.of(was.v.replica), nodes) : nodes
                await Promise.all([idb.done(s.put(pieceRows.print(now), pieceAt(at))), ...keysOf(s, piece.keys)])
                return lo.val(null)
              }),
            ),
          lo.val(null),
        ),
      // The join of what the store holds and `now`, written: what it holds beyond the store's row within one generation
      // and epoch, or whole over a row of another. Nothing new is the store's row, answered as it is.
      land: now =>
        without(
          () =>
            confirm(
              idb.within(LINE_ROWS, "readwrite", async (s): Promise<lo.Settled<locked.Row<SkeletonNode>>> => {
                const device = await deviceRow(s)
                const joined = locked.join(device, now)
                if (joined === device) return lo.val(device)
                if (!sameEpoch(joined, device)) {
                  await writeAll(s, joined)
                  return lo.val(joined)
                }
                const had = new Set(device.replica.held.objects.map(n => n.hash))
                await Promise.all([
                  idb.done(s.put(headOf(joined.replica), HEAD)),
                  ...nodesOf(s, joined.replica.held.objects.filter(n => !had.has(n.hash))),
                  ...keysOf(s, Object.fromEntries(Object.entries(joined.keys).filter(([name, hex]) => device.keys[name] !== hex))),
                ])
                return lo.val(joined)
              }),
              row => row,
            ).then(row => ({ row, unread: row.type === "Error" ? row.e : null })),
          { row: lo.val(now), unread: null },
        ),
      dropPieces: () => without(() => settled(idb.within(LINE_ROWS, "readwrite", s => idb.done(s.delete(range(PIECE))).then(() => lo.val(null)))), lo.val(null)),
      reset: held =>
        without(
          () =>
            confirm(
              idb.within(LINE_ROWS, "readwrite", async (s): Promise<lo.Settled<locked.Row<SkeletonNode>>> => {
                const rawHead: unknown = await idb.done(s.get(HEAD))
                const head = headIn(rawHead)
                const generation = Math.max(head?.generation ?? 0, held.replica.generation) + 1
                const row = locked.of(replicaOf<SkeletonNode>({ ...empty<SkeletonNode>().held, generation }))
                await writeAll(s, row)
                return lo.val(row)
              }),
              row => row,
            ),
          lo.val(locked.reset(held)),
        ),
      forget: gone =>
        without(
          () =>
            confirm(
              settled(idb.within(LINE_ROWS, "readwrite", s => Promise.all(gone.map(name => idb.done(s.delete(`${KEY}${name}`)))).then(() => lo.val(null)))),
              () => (confirmed === null ? null : locked.without(confirmed.row, gone)),
            ),
          lo.val(null),
        ),
    }
  }

  export const place: Place = placeOf()
}
export default linerows
