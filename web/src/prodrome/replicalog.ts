// A store's replica on the device: the core's snapshot and the frames appended since, in the core's own bytes. Each
// write of a frame is an append and each frame a join, so two tabs lose nothing. A compaction and a reset are the only
// ways bytes leave, each in one transaction, so neither deletes a frame its snapshot does not hold. An append wakes the
// other tabs on a `BroadcastChannel`, never carrying the data, and a compaction or a reset runs under a Web Lock, both
// named for the store.
//
// THE LOG HAS AN IDENTITY, kept with its floor (the last place given): a place is never given twice while the log is
// kept, and a reset writes a new identity. This tab's place in it is held here, not by its caller: how far it read, the
// identity it read, and the frames it appended itself, which the core holds already. A log of another identity than the
// one this tab read, or whose last place is below what this tab read, began again (a reset in another tab, or the
// device's data cleared under the page): it is read whole, from its start, and never as a continuation.
//
// What does not read is never deleted unread. A snapshot or a frame of an earlier build's form is that build's copy: of
// a store the box alone writes it holds nothing the box cannot give again and goes without a word; of a store this
// device writes it may hold a write not yet sent and is kept aside, one copy from before the last change of form. A part
// that does not read in this build's form, or a row that is no part, is kept aside and named. Each part set aside has a
// key of its own under `aside/<store>/`, and the floor is raised first, so no place is ever given again; the owner's
// reset of the store removes what is aside.

import idb from "../app/idb"
import lo from "../algebra/lo"
import d from "../wire/decoder"
import { BLOCKS, REPLICA } from "./objects"
import { framed } from "./core"

export namespace replicalog {
  // One frame, by the place it was appended at.
  export type Frame = { readonly at: number; readonly bytes: Uint8Array }

  type Snapshot = { readonly through: number; readonly bytes: Uint8Array }
  const Snapshot = d.obj<Snapshot>({ through: d.num, bytes: d.bytes })

  // What this tab reads of the log: the whole of it, its snapshot if any and every frame after it, which the tab loads;
  // or the frames another tab appended after this tab's place, in order. `strays`: the places of rows that are no frame;
  // `strayed`: a snapshot row that is no snapshot.
  export type Kept =
    | { readonly from: "Start"; readonly snapshot: Snapshot | null; readonly strayed: boolean; readonly frames: readonly Frame[]; readonly strays: readonly number[] }
    | { readonly from: "After"; readonly frames: readonly Frame[]; readonly strays: readonly number[] }

  // Whether some bytes, a snapshot or a frame, are of the form this build reads, of an earlier build's, or of none.
  export type Form = "This" | "Other" | "None"
  // The core's answer for a form; any other answer reads None.
  export const formOf = (said: string): Form => (said === "This" || said === "Other" ? said : "None")
  // What a store's copy can hold that the box cannot give again: nothing (`BoxAlone`, a store the box alone writes), or
  // this device's writes not yet sent (`OwnWrites`).
  export type Holds = "BoxAlone" | "OwnWrites"
  // The core a log is read into: the whole log loaded, one frame taken, and the form of some bytes. `load` and `apply`
  // throw, the core unmoved, where the bytes do not read.
  export type Reader = {
    readonly load: (snapshot: Uint8Array | undefined, frames: Uint8Array) => void
    readonly apply: (bytes: Uint8Array) => void
    readonly form: (bytes: Uint8Array) => Form
  }
  // What a read took: whether anything was read into the core, and, where a part did not read in this build's form,
  // what was set aside, for the store's row to name.
  export type Taken = { readonly taken: boolean; readonly unread: string | null }
  export type Aside = { readonly earlier: boolean; readonly unread: boolean }

  // How an append ended: the log goes on from what this tab read, or it began again and is to be read whole.
  export type Appended = "Kept" | "BeganAgain"

  // How a compaction or a reset ended: done, or this tab is behind another's writes and reads them first.
  export type Folded = "Done" | "Behind"

  export type T = {
    // The snapshot and every frame after it, in one transaction: this tab's place is where they end.
    readonly read: () => Promise<lo.Settled<Extract<Kept, { from: "Start" }>>>
    // What this tab lacks: the frames after its place that it did not append, or the whole log where a snapshot folded
    // some of them or the log began again.
    readonly since: () => Promise<lo.Settled<Kept>>
    // `bytes` appended after the last place given.
    readonly append: (bytes: Uint8Array) => Promise<lo.Settled<Appended>>
    // In one transaction: the frames after this tab's place handed to `take`, `print`'s snapshot kept, every frame up to
    // it, each print `husked` names and each block `swept` names of those kept deleted.
    readonly compact: (take: (frames: readonly Frame[]) => void, print: () => Uint8Array, husked: () => readonly string[], swept?: (blocks: readonly string[]) => readonly string[]) => Promise<lo.Settled<Folded>>
    // In one transaction, under a new identity: every frame this tab read or appended and the snapshot deleted, and
    // `print`'s snapshot kept in their place, with what the store holds aside. A frame another tab appended after this
    // tab's place stays, for this tab to read. Nothing a reset keeps lives only in memory between its steps.
    readonly reset: (print: () => Uint8Array) => Promise<lo.Settled<Folded>>
    // `kept` read into `reader`, whole, or part by part where the whole does not read, each part that does not read
    // dropped or set aside by the store's `Holds`, in one transaction.
    readonly take: (kept: Kept, reader: Reader) => Promise<lo.Settled<Taken>>
    readonly aside: () => Promise<lo.Settled<Aside>>
    // Whether, in `s`, the replica's store in a transaction of the caller's, another tab wrote what this tab has not
    // taken.
    readonly behind: (s: IDBObjectStore) => Promise<boolean>
    // `wake` run each time another tab appends to the log.
    readonly heard: (wake: () => void) => void
  }

  const PLACE = 12
  const placed = (at: number): string => String(at).padStart(PLACE, "0")

  // The floor and the identity: the last place a drop or a reset kept, and the log's name since its last reset.
  type Floor = { readonly at: number; readonly id: string | null }
  const FloorRow = d.obj<{ at: number; id: string }>({ at: d.num, id: d.str })
  const fresh = (): string => Array.from(crypto.getRandomValues(new Uint8Array(16)), b => b.toString(16).padStart(2, "0")).join("")

  // The replica of `store`, whose content by name is kept in `prints`, a store of the same database.
  export const of = (store: string, prints: idb.Db, holds: Holds): T => {
    const NAME = `replica/${store}`
    // A device that keeps nothing has no log to wake a tab for.
    const channel = typeof BroadcastChannel === "undefined" || !idb.available() ? null : new BroadcastChannel(NAME)
    // `work` alone among every tab's compactions and resets of this store, where the browser keeps locks.
    const alone = <A>(work: () => Promise<A>): Promise<A> => {
      const locks = typeof navigator === "undefined" ? undefined : navigator.locks
      return locks === undefined ? work() : locks.request(NAME, work)
    }
    const SNAPSHOT = `${store}/snapshot`
    // Outside the store's own range, so nothing that reads or deletes the store's snapshot and frames by range meets it.
    const FLOOR = `floor/${store}`
    const LOG = `${store}/log/`
    const ASIDE = `aside/${store}/`
    const EARLIER = `${ASIDE}earlier/`
    const UNREAD = `${ASIDE}unread/`
    const under = (prefix: string): IDBKeyRange => IDBKeyRange.bound(prefix, `${prefix}￿`)
    const log = (after: number): IDBKeyRange => IDBKeyRange.bound(`${LOG}${placed(after)}`, `${LOG}￿`, true)
    const upTo = (through: number): IDBKeyRange => IDBKeyRange.bound(LOG, `${LOG}${placed(through)}`)
    const atOf = (key: IDBValidKey): number => Number(String(key).slice(LOG.length))

    // This tab's place: every frame through `after` taken, of the log named `id`; and the frames it appended itself.
    let after = 0
    let id: string | null = null
    const mine = new Set<number>()

    const framesIn = async (s: IDBObjectStore, from: number): Promise<{ readonly frames: Frame[]; readonly strays: number[] }> => {
      const [keys, values] = await Promise.all([idb.done(s.getAllKeys(log(from))), idb.done(s.getAll(log(from)))])
      const frames: Frame[] = []
      const strays: number[] = []
      keys.forEach((k, i) => {
        const v = values[i]
        if (v instanceof Uint8Array) frames.push({ at: atOf(k), bytes: v })
        else strays.push(atOf(k))
      })
      return { frames, strays }
    }
    const frames = async (s: IDBObjectStore, from: number): Promise<Frame[]> => (await framesIn(s, from)).frames
    // A snapshot whose row is not one reads like none, and is said to be a stray: the frames after it are read whole.
    const snapshotRow = async (s: IDBObjectStore): Promise<{ readonly snapshot: Snapshot | null; readonly strayed: boolean }> => {
      const raw: unknown = await idb.done(s.get(SNAPSHOT))
      if (raw === undefined) return { snapshot: null, strayed: false }
      const read = Snapshot(raw)
      return read.type === "Val" ? { snapshot: read.v, strayed: false } : { snapshot: null, strayed: true }
    }
    const snapshotIn = async (s: IDBObjectStore): Promise<Snapshot | null> => (await snapshotRow(s)).snapshot
    const floorIn = async (s: IDBObjectStore): Promise<Floor> => {
      const raw: unknown = await idb.done(s.get(FLOOR))
      const read = FloorRow(raw)
      return read.type === "Val" ? read.v : { at: typeof raw === "number" ? raw : 0, id: null }
    }
    type Parts = { snapshot: boolean; frames: number[] }
    const any = (p: Parts): boolean => p.snapshot || p.frames.length > 0
    const keysOf = (p: Parts): string[] => [...(p.snapshot ? [SNAPSHOT] : []), ...p.frames.map(at => `${LOG}${placed(at)}`)]
    const described = (p: Parts): string | null =>
      !any(p) ? null : [p.snapshot ? "its snapshot" : "", p.frames.length === 0 ? "" : `${p.frames.length} of its frames`].filter(w => w !== "").join(" and ")
    // Each of `keys` moved under `prefix`, each to a key of its own.
    const moveAside = async (s: IDBObjectStore, keys: readonly string[], prefix: string): Promise<void> => {
      for (const key of keys) {
        const value: unknown = await idb.done(s.get(key))
        if (value === undefined) continue
        await idb.done(s.put(value, `${prefix}${fresh()}`))
        await idb.done(s.delete(key))
      }
    }
    const settled = <A>(p: Promise<A>): Promise<lo.Settled<A>> => p.then(lo.val, (e: unknown) => lo.error(lo.reason(e)))
    // The last place given: of a frame, the snapshot, or the floor.
    const top = async (s: IDBObjectStore, snapshot: Snapshot | null, floor: Floor): Promise<number> => {
      const base = Math.max(snapshot?.through ?? 0, floor.at)
      const keys = await idb.done(s.getAllKeys(log(base)))
      const at = keys.at(-1)
      return at === undefined ? base : atOf(at)
    }
    // Whether the log this tab read goes on in `s`: the same identity, nothing it took folded away, and no place below its.
    const goesOn = async (s: IDBObjectStore, snapshot: Snapshot | null, floor: Floor): Promise<boolean> =>
      floor.id === id && (snapshot === null || snapshot.through <= after) && (await top(s, snapshot, floor)) >= after

    const whole = async (s: IDBObjectStore, floor: Floor): Promise<Extract<Kept, { from: "Start" }>> => {
      const { snapshot, strayed } = await snapshotRow(s)
      const kept: Extract<Kept, { from: "Start" }> = { from: "Start", snapshot, strayed, ...(await framesIn(s, snapshot?.through ?? 0)) }
      after = await top(s, snapshot, floor)
      id = floor.id
      mine.clear()
      return kept
    }

    return {
      read: () =>
        settled(
          idb.within(REPLICA, "readonly", async s => whole(s, await floorIn(s))),
        ),
      since: () =>
        settled(
          idb.within(REPLICA, "readonly", async (s): Promise<Kept> => {
            const snapshot = await snapshotIn(s)
            const floor = await floorIn(s)
            if (!(await goesOn(s, snapshot, floor))) return whole(s, floor)
            const later = await framesIn(s, after)
            after = Math.max(after, later.frames.at(-1)?.at ?? after, later.strays.at(-1) ?? after)
            return { from: "After", frames: later.frames.filter(f => !mine.has(f.at)), strays: later.strays }
          }),
        ),
      append: bytes =>
        settled(
          idb.within(REPLICA, "readwrite", async (s): Promise<Appended> => {
            const floor = await floorIn(s)
            const named = floor.id ?? fresh()
            if (floor.id === null) await idb.done(s.put({ at: floor.at, id: named }, FLOOR))
            const at = (await top(s, await snapshotIn(s), floor)) + 1
            await idb.done(s.put(bytes, `${LOG}${placed(at)}`))
            // A log named otherwise than the one this tab read, or a place at or below its: the log began again. A log
            // named for the first time by this append is the one this tab read.
            const again = (floor.id !== null && floor.id !== id) || at <= after
            if (!again) {
              id = named
              mine.add(at)
            }
            return again ? "BeganAgain" : "Kept"
          }),
        ).then(appended => {
          if (appended.type === "Val") channel?.postMessage(null)
          return appended
        }),
      behind: async s => {
        const snapshot = await snapshotIn(s)
        if (!(await goesOn(s, snapshot, await floorIn(s)))) return true
        return (await idb.done(s.getAllKeys(log(after)))).some(k => !mine.has(atOf(k)))
      },
      heard: wake => {
        if (channel !== null) channel.onmessage = () => wake()
      },
      compact: (take, print, husked, swept = () => []) =>
        alone(() =>
          settled(
            idb.withBeside(REPLICA, [prints, BLOCKS], "readwrite", async (s, [content, blocks]): Promise<Folded> => {
              if (content === undefined || blocks === undefined) throw new Error("a compaction spans the replica, its prints and the blocks")
              const snapshot = await snapshotIn(s)
              if (!(await goesOn(s, snapshot, await floorIn(s)))) return "Behind"
              const later = await frames(s, after)
              take(later.filter(f => !mine.has(f.at)))
              const through = later.at(-1)?.at ?? after
              const kept = (await idb.done(blocks.getAllKeys())).filter((k): k is string => typeof k === "string")
              await Promise.all([
                idb.done(s.put({ through, bytes: print() }, SNAPSHOT)),
                idb.done(s.delete(upTo(through))),
                ...husked().map(name => idb.done(content.delete(name))),
                ...swept(kept).map(key => idb.done(blocks.delete(key))),
              ])
              after = through
              mine.clear()
              return "Done"
            }),
          ),
        ),
      reset: print =>
        alone(() =>
          settled(
            idb.within(REPLICA, "readwrite", async (s): Promise<Folded> => {
              const snapshot = await snapshotIn(s)
              const floor = await floorIn(s)
              const last = await top(s, snapshot, floor)
              // A log another tab reset, or one cleared under the page, holds frames this tab never read: they are read
              // first, and the reset made again over them.
              if (!(await goesOn(s, snapshot, floor))) return "Behind"
              // Printed before anything is deleted: a print that throws deletes nothing.
              const bytes = print()
              const named = fresh()
              const own = [...mine].filter(at => at > after)
              await Promise.all([
                idb.done(s.put({ at: last, id: named }, FLOOR)),
                idb.done(s.delete(upTo(after))),
                ...own.map(at => idb.done(s.delete(`${LOG}${placed(at)}`))),
                idb.done(s.delete(under(ASIDE))),
              ])
              await idb.done(s.put({ through: after, bytes }, SNAPSHOT))
              id = named
              mine.clear()
              return "Done"
            }),
          ),
        ),
      take: async (kept, reader) => {
        // Each part that does not read, by what it is: an earlier build's, or no part of any build.
        const earlier: Parts = { snapshot: false, frames: [] }
        const unread: Parts = { snapshot: kept.from === "Start" && kept.strayed, frames: [...kept.strays] }
        const sort = (bytes: Uint8Array): Parts => (reader.form(bytes) === "Other" ? earlier : unread)
        const each = (frames: readonly Frame[]): void => {
          for (const f of frames) {
            try {
              reader.apply(f.bytes)
            } catch {
              sort(f.bytes).frames.push(f.at)
            }
          }
        }
        if (kept.from === "After") each(kept.frames)
        else {
          try {
            reader.load(kept.snapshot?.bytes, framed(kept.frames.map(f => f.bytes)))
          } catch {
            if (kept.snapshot !== null) {
              try {
                reader.load(kept.snapshot.bytes, framed([]))
              } catch {
                sort(kept.snapshot.bytes).snapshot = true
              }
            }
            each(kept.frames)
          }
        }
        const taken = kept.from === "Start" || kept.frames.length > 0
        const what = described(unread)
        if (!any(earlier) && what === null) return lo.val({ taken, unread: null })
        const moved = await settled(
          idb.within(REPLICA, "readwrite", async s => {
            // The floor raised first: a place a part set aside held is never given again.
            const floor = await floorIn(s)
            const last = await top(s, await snapshotIn(s), floor)
            await idb.done(s.put(floor.id === null ? last : { at: Math.max(floor.at, last), id: floor.id }, FLOOR))
            if (any(earlier)) {
              if (holds === "BoxAlone") await Promise.all(keysOf(earlier).map(key => idb.done(s.delete(key))))
              else {
                // One copy from before the last change of form: an older one goes when this one is kept.
                await idb.done(s.delete(under(EARLIER)))
                await moveAside(s, keysOf(earlier), EARLIER)
              }
            }
            await moveAside(s, keysOf(unread), UNREAD)
            return null
          }),
        )
        if (moved.type === "Error") return lo.error(`${what ?? "an earlier build's copy"} could not be set aside (${moved.e})`)
        return lo.val({ taken, unread: what })
      },
      aside: () =>
        settled(
          idb.within(REPLICA, "readonly", async s => ({
            earlier: (await idb.done(s.count(under(EARLIER)))) > 0,
            unread: (await idb.done(s.count(under(UNREAD)))) > 0,
          })),
        ),
    }
  }
}
export default replicalog
