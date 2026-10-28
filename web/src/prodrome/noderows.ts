// Where a store's row lives on the device: an object store of its own, one row per node and one per key of its words,
// beside a head row holding the generation, the box's words, its refusals, the frontier and the nodes the box has not
// taken, and the pieces of a pull not yet landed. A node is named by its hash and never changes, so a landing writes the
// nodes and keys it brought and the head, never the store, and two tabs writing the same node write the same row. A
// key's bytes are in its key row alone, a piece's included, so a forget is one delete. A reset or a later epoch empties
// the store and writes what replaces it.

import idb from "../app/idb"
import lo from "../algebra/lo"
import d from "../wire/decoder"
import payload from "../wire/generated/payload"
import type { RefusedObject } from "../wire/generated/wire"
import locked from "./locked"
import { Spoken, Standing, storeRows } from "./objects"
import type pull from "./pull"
import type { Frontier, Held, Obj, Replica, Word } from "./replica"
import { empty, epochOf, replicaOf, restsOn } from "./replica"

export namespace noderows {
  // What the head row holds: the format the rows were written in, the row's generation, the box's words, its refusals,
  // the frontier its next pull asks from, and the nodes it holds that the box is not known to hold (a tab's own writes);
  // the nodes and keys are rows.
  type Head = {
    readonly format: number
    readonly generation: number
    readonly words: readonly Word[]
    readonly refused: readonly RefusedObject[]
    readonly frontier: Frontier
    readonly unsent: readonly string[]
  }
  const HEAD = "head"
  const Head = d.obj<Head>({ format: d.num, generation: d.num, words: d.arr(Spoken), refused: d.arr(payload.RefusedObject), frontier: Standing, unsent: d.arr(d.str) })

  const NODE = "node/"
  const KEY = "key/"
  const PIECE = "piece/"
  const range = (prefix: string): IDBKeyRange => IDBKeyRange.bound(prefix, `${prefix}￿`)
  const pieceAt = (at: number): string => `${PIECE}${String(at).padStart(12, "0")}`

  // The head a raw row holds, or none for an absent one or one that does not read.
  const headIn = (raw: unknown): Head | null => {
    if (raw === undefined) return null
    const h = Head(raw)
    return h.type === "Val" ? h.v : null
  }

  const headWith = <N extends Obj>(format: number, r: Replica<N>): Head => {
    const synced = new Set(r.held.synced)
    return { format, generation: r.generation, words: r.held.words, refused: r.held.refused, frontier: r.held.frontier, unsent: r.held.objects.flatMap(n => (synced.has(n.hash) ? [] : [n.hash])) }
  }

  // Parents first, as a replica holds its objects, the least name first among those ready: the nodes are read by name.
  const ordered = <N extends Obj>(nodes: readonly N[]): N[] => {
    const byName = new Map(nodes.map(n => [n.hash, n]))
    const waiting = new Map<string, number>()
    const children = new Map<string, string[]>()
    for (const n of nodes) {
      const parents = restsOn(n).filter(p => byName.has(p))
      waiting.set(n.hash, parents.length)
      for (const p of parents) {
        const under = children.get(p)
        if (under === undefined) children.set(p, [n.hash])
        else under.push(n.hash)
      }
    }
    const ready = nodes.filter(n => waiting.get(n.hash) === 0).map(n => n.hash).sort()
    const out: N[] = []
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

  const rowOf = <N extends Obj>(head: Head, nodes: readonly N[], keys: locked.Keys): locked.Row<N> => {
    const objects = ordered(nodes)
    const unsent = new Set(head.unsent)
    const held: Held<N> = { generation: head.generation, objects, synced: objects.flatMap(n => (unsent.has(n.hash) ? [] : [n.hash])), refused: head.refused, words: head.words, frontier: head.frontier }
    return locked.of(replicaOf(held), keys)
  }

  // A head printed with its fields in one order, so the head a tab wrote and the head it reads back compare equal.
  const printed = (v: unknown): string =>
    JSON.stringify(v, (_, x: unknown) =>
      x !== null && typeof x === "object" && !Array.isArray(x) ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) : x,
    )

  const without = <A>(f: () => Promise<A>, otherwise: A): Promise<A> => (idb.available() ? f() : Promise.resolve(otherwise))
  const settled = (p: Promise<lo.Settled<null>>): Promise<lo.Settled<null>> => p.catch((e: unknown) => lo.error(lo.reason(e)))
  // Each key of the row's words a row of its own: the one place the store holds a key's bytes.
  const keysOf = (s: IDBObjectStore, keys: locked.Keys): Promise<unknown>[] => Object.entries(keys).map(([name, hex]) => idb.done(s.put(hex, `${KEY}${name}`)))

  // How a store's rows are written: `version` counts the forms of its nodes, and a store whose head names another is
  // dropped and taken again from the box at restore, its content by name kept, since a name holds one literal;
  // `dropped` runs first, for what the old rows found (the lines' rendered blocks, by their keys), and a failure of it
  // keeps them, said, to be dropped at the next load.
  export type Format = { readonly version: number; readonly dropped: () => Promise<lo.Settled<null>> }

  // Where a store's row lives, and how keys are taken out of it, whatever else another tab wrote there.
  export type Place<N extends Obj> = pull.Place<N> & { readonly forget: (gone: readonly string[]) => Promise<lo.Settled<null>> }

  // The place of the store `db` names, whose nodes read by `node`. It remembers the row the device last confirmed holding
  // and the head it held then: while the store's head is still that one, no other tab has written, and a landing writes
  // what it holds beyond that row; otherwise the store's row is read back. Either way the row written is the JOIN of the
  // device's and the landing's (`locked.join`, the one rule that composes a head, its words and its frontier), so a tab
  // behind another never writes its older word or frontier over the later one, and within a generation and epoch the
  // store is never cleared, so another tab's nodes stay. A later generation or epoch on either side is written whole. A
  // head that does not read, or that names another format, is never written over by a landing: only a reset, or the
  // restore's reformat, writes over it.
  export const placeOf = <N extends Obj>(db: idb.Db, node: d.Decoder<N>, format: Format): Place<N> => {
    const headOf = (r: Replica<N>): Head => headWith(format.version, r)
    const NONE = locked.none<N>()
    // A piece's row holds its nodes alone; its keys are key rows.
    const pieceRows = storeRows(node, NONE, () => [])

    // The row the store holds, read in one transaction; a node or a head that does not read is named, never passed over.
    type Read = {
      readonly head: Head | null
      // The head as the store holds it, printed, or null for none.
      readonly printed: string | null
      readonly row: locked.Row<N>
      readonly keys: locked.Keys
      readonly pieces: readonly locked.Row<N>[]
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
        if (rawHead !== undefined && head === null) unread.push(`the head row of ${db.store} does not read`)
        const nodes = rawNodes.flatMap(raw => {
          const n = node(raw)
          if (n.type === "Error") unread.push(`a node of ${db.store} does not read (${n.e})`)
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

    // `row` written whole over what the store holds: a reset's, or a later epoch's.
    const writeAll = (s: IDBObjectStore, row: locked.Row<N>): Promise<unknown> => {
      s.clear()
      return Promise.all([idb.done(s.put(headOf(row.replica), HEAD)), ...nodesOf(s, row.replica.held.objects), ...keysOf(s, row.keys)])
    }
    const nodesOf = (s: IDBObjectStore, nodes: readonly N[]): Promise<unknown>[] => nodes.map(n => idb.done(s.put(n, `${NODE}${n.hash}`)))

    let confirmed: { readonly row: locked.Row<N>; readonly head: string } | null = null
    const confirming = (row: locked.Row<N>): void => void (confirmed = { row, head: printed(headOf(row.replica)) })
    const confirm = <A>(p: Promise<lo.Settled<A>>, row: (a: A) => locked.Row<N> | null): Promise<lo.Settled<A>> =>
      p.then(
        a => {
          const r = a.type === "Val" ? row(a.v) : null
          if (r === null) confirmed = null
          else confirming(r)
          return a
        },
        (e: unknown) => ((confirmed = null), lo.error(lo.reason(e))),
      )
    // What the store holds: the confirmed row while its head is the one confirmed, else the store's row read back; or
    // why its head does not read.
    const deviceRow = async (s: IDBObjectStore): Promise<lo.Settled<locked.Row<N>>> => {
      const raw: unknown = await idb.done(s.get(HEAD))
      const head = raw === undefined ? null : headIn(raw)
      if (raw !== undefined && head === null) return lo.error(`the copy of ${db.store} kept on this device does not read, so nothing was written over it`)
      if (head !== null && head.format !== format.version)
        return lo.error(`the copy of ${db.store} kept on this device is in format ${head.format}, this build's is ${format.version}, so nothing was written over it`)
      const known = confirmed
      return lo.val(known !== null && raw !== undefined && printed(raw) === known.head ? known.row : (await readAll(s)).row)
    }
    const sameEpoch = (a: locked.Row<N>, b: locked.Row<N>): boolean =>
      a.replica.generation === b.replica.generation && epochOf(a.replica.held) === epochOf(b.replica.held)
    // The one way down: an empty row of the generation after both the store's and `held`'s, written whole.
    const reset = (held: locked.Row<N>): Promise<lo.Settled<locked.Row<N>>> =>
      without(
        () =>
          confirm(
            idb.within(db, "readwrite", async (s): Promise<lo.Settled<locked.Row<N>>> => {
              const head = headIn(await idb.done(s.get(HEAD)))
              const generation = Math.max(head?.generation ?? 0, held.replica.generation) + 1
              const row = locked.of(replicaOf<N>({ ...empty<N>().held, generation }))
              await writeAll(s, row)
              return lo.val(row)
            }),
            row => row,
          ),
        lo.val(locked.reset(held)),
      )

    // Rows of another format: what they found dropped, then, in one transaction, the store emptied to a row of the
    // generation after, holding the nodes this tab wrote and the box has not taken. Each of those is read in this build's
    // form, or nothing is dropped and the restore says which does not read. A store another tab already moved to this
    // format is read as it is.
    type Restored = Awaited<ReturnType<Place<N>["restore"]>>
    const reformat = async (read: Read & { readonly head: Head }): Promise<Restored> => {
      const why = (e: string): string => `the copy of ${db.store} is in format ${read.head.format}, this build's is ${format.version}, and it was not dropped (${e})`
      const failed = (e: string): Restored => ({ row: { type: "Unreadable", why: why(e) }, pieces: [], unread: [] })
      const unsent = new Set(read.head.unsent)
      const own = read.row.replica.held.objects.filter(n => unsent.has(n.hash))
      if (own.length < unsent.size) return failed(`${unsent.size - own.length} of the objects this tab wrote and the box has not taken do not read in this build's form`)
      const dropped = await format.dropped()
      if (dropped.type === "Error") return failed(dropped.e)
      const row = await confirm(
        idb.within(db, "readwrite", async (s): Promise<lo.Settled<locked.Row<N>>> => {
          const now = headIn(await idb.done(s.get(HEAD)))
          if (now !== null && now.format === format.version) return lo.val((await readAll(s)).row)
          const generation = Math.max(now?.generation ?? 0, read.head.generation) + 1
          const row = locked.of(replicaOf<N>({ ...empty<N>().held, generation, objects: own }))
          await writeAll(s, row)
          return lo.val(row)
        }),
        row => row,
      )
      return row.type === "Error" ? failed(row.e) : { row: { type: "Held", row: row.v }, pieces: [], unread: [] }
    }
    return {
      restore: () =>
        without(
          async () => {
            const read = await idb.within(db, "readonly", readAll).catch((e: unknown): Read | string => lo.reason(e))
            if (typeof read === "string") return { row: { type: "Unreadable", why: read }, pieces: [], unread: [] }
            if (read.head !== null && read.head.format !== format.version) return reformat({ ...read, head: read.head })
            confirmed = read.unread.length === 0 && read.printed !== null ? { row: read.row, head: read.printed } : null
            // The pieces' keys are the store's key rows, which hold the row's as well: a union, so either holds them.
            const pieces = read.pieces.map(p => locked.of(p.replica, read.keys))
            return { row: read.head === null ? { type: "Absent" } : { type: "Held", row: read.row }, pieces, unread: read.unread }
          },
          { row: { type: "Absent" }, pieces: [], unread: [] },
        ),
      // The piece's nodes joined into its row, and its keys each a key row: never a key's bytes inside a piece.
      keepPiece: (at, piece) =>
        without(
          () =>
            settled(
              idb.within(db, "readwrite", async s => {
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
              idb.within(db, "readwrite", async (s): Promise<lo.Settled<locked.Row<N>>> => {
                const read = await deviceRow(s)
                if (read.type === "Error") return read
                const device = read.v
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
      dropPieces: () => without(() => settled(idb.within(db, "readwrite", s => idb.done(s.delete(range(PIECE))).then(() => lo.val(null)))), lo.val(null)),
      reset,
      forget: gone =>
        without(
          () =>
            confirm(
              settled(idb.within(db, "readwrite", s => Promise.all(gone.map(name => idb.done(s.delete(`${KEY}${name}`)))).then(() => lo.val(null)))),
              () => (confirmed === null ? null : locked.without(confirmed.row, gone)),
            ),
          lo.val(null),
        ),
    }
  }
}
export default noderows
