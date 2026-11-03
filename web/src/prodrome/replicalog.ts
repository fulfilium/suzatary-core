// A store's replica as the device keeps it: the core's snapshot of it and the frames the core appended since, each a
// change of the replica in the core's own form, in the database's `replica` store under the store's name. The bytes are
// the core's; this keeps them in order and nothing else. Every write of a frame is an append, so two tabs appending
// lose nothing, and a frame is a join, so the order of two tabs' joins does not matter. A compaction is the one way
// bytes leave: in one transaction it reads the frames after what its core took, hands them to the core, writes the
// core's snapshot as of the last frame and deletes every frame up to it. IndexedDB runs one read-write transaction over
// a store at a time, so a compaction never deletes a frame its snapshot does not hold.

import idb from "../app/idb"
import lo from "../algebra/lo"
import d from "../wire/decoder"
import { REPLICA } from "./objects"

export namespace replicalog {
  // One frame, by the place it was appended at.
  export type Frame = { readonly at: number; readonly bytes: Uint8Array }

  // What the device keeps of a store: nothing, or the snapshot and the place of the last frame it holds; and the frames
  // after it, in order.
  export type Kept = { readonly snapshot: { readonly through: number; readonly bytes: Uint8Array } | null; readonly frames: readonly Frame[] }

  // How a compaction ended: the snapshot holds every frame through `at`, or this tab is behind another's snapshot.
  export type Compacted = { readonly type: "Through"; readonly at: number } | { readonly type: "Behind" }

  const Snapshot = d.obj<{ through: number; bytes: Uint8Array }>({ through: d.num, bytes: d.bytes })

  export type T = {
    // The snapshot and every frame after it, in one transaction.
    readonly read: () => Promise<lo.Settled<Kept>>
    // What a tab that took every frame up to `after` lacks: the frames after it, or, where a compaction folded frames it
    // did not take into a snapshot, that snapshot and the frames after it.
    readonly since: (after: number) => Promise<lo.Settled<Kept>>
    // `bytes` appended after the last frame; the place it was appended at.
    readonly append: (bytes: Uint8Array) => Promise<lo.Settled<number>>
    // The frames after `after` handed to `take`, then `print`'s snapshot kept as of the last frame and every frame up to
    // it deleted, all in one transaction; or nothing, where another tab's compaction folded frames after `after` that
    // this tab did not take, so it reads them first (`since`).
    readonly compact: (after: number, take: (frames: readonly Frame[]) => void, print: () => Uint8Array) => Promise<lo.Settled<Compacted>>
    // The snapshot and every frame deleted: the store is taken again from the box.
    readonly drop: () => Promise<lo.Settled<null>>
  }

  const PLACE = 12
  const placed = (at: number): string => String(at).padStart(PLACE, "0")

  export const of = (store: string): T => {
    const SNAPSHOT = `${store}/snapshot`
    const LOG = `${store}/log/`
    const log = (after: number): IDBKeyRange => IDBKeyRange.bound(`${LOG}${placed(after)}`, `${LOG}￿`, true)
    const atOf = (key: IDBValidKey): number => Number(String(key).slice(LOG.length))

    const frames = async (s: IDBObjectStore, after: number): Promise<Frame[]> => {
      const [keys, values] = await Promise.all([idb.done(s.getAllKeys(log(after))), idb.done(s.getAll(log(after)))])
      return keys.flatMap((k, i) => {
        const v = values[i]
        return v instanceof Uint8Array ? [{ at: atOf(k), bytes: v }] : []
      })
    }
    const snapshotIn = async (s: IDBObjectStore): Promise<Kept["snapshot"]> => {
      const raw: unknown = await idb.done(s.get(SNAPSHOT))
      if (raw === undefined) return null
      const read = Snapshot(raw)
      return read.type === "Val" ? read.v : null
    }
    const last = async (s: IDBObjectStore, floor: number): Promise<number> => {
      const keys = await idb.done(s.getAllKeys(log(floor)))
      const at = keys.at(-1)
      return at === undefined ? floor : atOf(at)
    }
    const settled = <A>(p: Promise<A>): Promise<lo.Settled<A>> => p.then(lo.val, (e: unknown) => lo.error(lo.reason(e)))

    const readAfter = (after: number): Promise<lo.Settled<Kept>> =>
      settled(
        idb.within(REPLICA, "readonly", async s => {
          const snapshot = await snapshotIn(s)
          const folded = snapshot !== null && snapshot.through > after
          return { snapshot: folded || after === 0 ? snapshot : null, frames: await frames(s, folded ? snapshot.through : Math.max(after, snapshot?.through ?? 0)) }
        }),
      )

    return {
      read: () => readAfter(0),
      since: readAfter,
      append: bytes =>
        settled(
          idb.within(REPLICA, "readwrite", async s => {
            const snapshot = await snapshotIn(s)
            const at = (await last(s, snapshot?.through ?? 0)) + 1
            await idb.done(s.put(bytes, `${LOG}${placed(at)}`))
            return at
          }),
        ),
      compact: (after, take, print) =>
        settled(
          idb.within(REPLICA, "readwrite", async (s): Promise<Compacted> => {
            const snapshot = await snapshotIn(s)
            if (snapshot !== null && snapshot.through > after) return { type: "Behind" }
            const missed = await frames(s, after)
            take(missed)
            const through = missed.at(-1)?.at ?? after
            await Promise.all([idb.done(s.put({ through, bytes: print() }, SNAPSHOT)), idb.done(s.delete(IDBKeyRange.bound(`${LOG}`, `${LOG}${placed(through)}`)))])
            return { type: "Through", at: through }
          }),
        ),
      drop: () =>
        settled(
          idb.within(REPLICA, "readwrite", async s => {
            await Promise.all([idb.done(s.delete(SNAPSHOT)), idb.done(s.delete(IDBKeyRange.bound(LOG, `${LOG}￿`)))])
            return null
          }),
        ),
    }
  }
}
export default replicalog
