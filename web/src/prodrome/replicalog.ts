// A store's replica on the device: the core's snapshot and the frames appended since, in the core's own bytes. Each
// write of a frame is an append and each frame a join, so two tabs lose nothing. A compaction is the one way bytes
// leave, in one transaction, so it never deletes a frame its snapshot does not hold. An append wakes the other tabs on
// a `BroadcastChannel`, never carrying the data, and a compaction runs under a Web Lock, both named for the store.

import idb from "../app/idb"
import lo from "../algebra/lo"
import d from "../wire/decoder"
import { REPLICA } from "./objects"

export namespace replicalog {
  // One frame, by the place it was appended at.
  export type Frame = { readonly at: number; readonly bytes: Uint8Array }

  // The snapshot, if any, and the frames after it, in order.
  export type Kept = { readonly snapshot: { readonly through: number; readonly bytes: Uint8Array } | null; readonly frames: readonly Frame[] }

  // How a compaction ended: the snapshot holds every frame through `at`, or this tab is behind another's snapshot.
  export type Compacted = { readonly type: "Through"; readonly at: number } | { readonly type: "Behind" }

  const Snapshot = d.obj<{ through: number; bytes: Uint8Array }>({ through: d.num, bytes: d.bytes })

  export type T = {
    // The snapshot and every frame after it, in one transaction.
    readonly read: () => Promise<lo.Settled<Kept>>
    // What a tab that took every frame up to `after` lacks: the frames after it, or a snapshot that folded some.
    readonly since: (after: number) => Promise<lo.Settled<Kept>>
    // `bytes` appended after the last frame; the place it was appended at.
    readonly append: (bytes: Uint8Array) => Promise<lo.Settled<number>>
    // In one transaction: the frames after `after` handed to `take`, `print`'s snapshot kept, every frame up to it and
    // each print `husked` names deleted; or `Behind`, where another tab's snapshot folded frames this tab did not take.
    readonly compact: (after: number, take: (frames: readonly Frame[]) => void, print: () => Uint8Array, husked: () => readonly string[]) => Promise<lo.Settled<Compacted>>
    // The snapshot and every frame deleted: the store is taken again from the box.
    readonly drop: () => Promise<lo.Settled<null>>
    // `wake` run each time another tab appends to the log.
    readonly heard: (wake: () => void) => void
  }

  const PLACE = 12
  const placed = (at: number): string => String(at).padStart(PLACE, "0")

  // The replica of `store`, whose content by name is kept in `prints`, a store of the same database.
  export const of = (store: string, prints: idb.Db): T => {
    const NAME = `replica/${store}`
    // A device that keeps nothing has no log to wake a tab for.
    const channel = typeof BroadcastChannel === "undefined" || !idb.available() ? null : new BroadcastChannel(NAME)
    // `work` alone among every tab's compactions of this store, where the browser keeps locks.
    const alone = <A>(work: () => Promise<A>): Promise<A> => {
      const locks = typeof navigator === "undefined" ? undefined : navigator.locks
      return locks === undefined ? work() : locks.request(NAME, work)
    }
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
        ).then(at => {
          if (at.type === "Val") channel?.postMessage(at.v)
          return at
        }),
      heard: wake => {
        if (channel !== null) channel.onmessage = () => wake()
      },
      compact: (after, take, print, husked) =>
        alone(() =>
          settled(
            idb.withBeside(REPLICA, prints, "readwrite", async (s, content): Promise<Compacted> => {
              const snapshot = await snapshotIn(s)
              if (snapshot !== null && snapshot.through > after) return { type: "Behind" }
              const missed = await frames(s, after)
              take(missed)
              const through = missed.at(-1)?.at ?? after
              await Promise.all([
                idb.done(s.put({ through, bytes: print() }, SNAPSHOT)),
                idb.done(s.delete(IDBKeyRange.bound(`${LOG}`, `${LOG}${placed(through)}`))),
                ...husked().map(name => idb.done(content.delete(name))),
              ])
              return { type: "Through", at: through }
            }),
          ),
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
