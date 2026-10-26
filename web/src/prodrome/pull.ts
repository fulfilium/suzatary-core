// One store's pull from the box, by pieces asked from what is held, and its landing: the row a store's pages read,
// restored from where the device keeps it, joined with what each landing brought, and started over when the box answers
// in a later epoch. The same machine runs on the page for the whole stores and in the core's worker for the lines; what
// differs is what a row is (`Rows`: a replica alone, or the lines' with the keys of their words, `locks`), where it is
// kept (`Place`), how a piece is asked for, and who hears what it says (`Effects`).

import type { Erased } from "./core"
import excluded from "./excluded"
import locked from "./locked"
import pieces from "./pieces"
import type progress from "./progress"
import { empty, join, pulled, replicaOf, sinceOf, wentBack } from "./replica"
import type { Obj, Replica, Said } from "./replica"
import stores from "./stores"
import purge from "./purge"
import take from "./take"
import lo from "../algebra/lo"
import once from "../algebra/once"
import serial from "../algebra/serial"
import time from "../app/time"

export namespace pull {
  // One piece the box answered, as each store's pull answers it: what it said, what it erased, and the row it makes of
  // the replica it is taken into.
  export type Answer<O extends Obj, R> = {
    readonly said: Said<O>
    readonly erased: readonly string[]
    readonly piece: (r: Replica<O>) => R
    readonly rest: number
    readonly size: number
    readonly weight: number
  }

  // What the device kept of a row: nothing, the row, a row that does not read, or a row of a form this build drops,
  // dropped (or not, and why).
  export type Restored<R> =
    | { readonly type: "Absent" }
    | { readonly type: "Held"; readonly row: R }
    | { readonly type: "Unreadable"; readonly why: string }
    | { readonly type: "Dropped"; readonly failed: string | null }

  // Where a store's row lives on the device, and its pieces beside it. Without a device store every write answers done
  // and holds nothing, and what is held in memory is the only copy.
  export type Place<R> = {
    readonly restore: () => Promise<{ readonly row: Restored<R>; readonly pieces: readonly R[]; readonly unread: readonly string[] }>
    // A piece kept by itself, named by how many objects were held when it was asked for.
    readonly keepPiece: (at: number, piece: R) => Promise<lo.Settled<null>>
    // The row a landing moved to, `now`, written, and what the device holds now, another tab's included.
    readonly land: (now: R) => Promise<{ readonly row: lo.Settled<R>; readonly unread: string | null }>
    readonly dropPieces: () => Promise<lo.Settled<null>>
    // The one way down: an empty row of the generation after, its pieces gone.
    readonly reset: (held: R) => Promise<lo.Settled<R>>
  }

  // What a store's rows are: each one's replica, and their join, `none` its unit.
  export type Rows<O extends Obj, R> = {
    readonly none: R
    readonly replica: (r: R) => Replica<O>
    readonly join: (a: R, b: R) => R
  }

  // How a store's rows take in what comes: a row joined into what is held once whatever the erasures it names owe is
  // done, and the pieces held less what a piece of the pull erased, with `rest` objects still to come.
  export type Taking<R> = {
    readonly take: (erased: readonly string[], incoming: R) => Promise<void>
    readonly erasing: (erased: readonly string[], rest: number, pending: R) => Promise<R>
  }

  // What differs between the stores: what their rows are and hold, how they take in what comes, and how a piece is
  // asked for.
  export type Store<O extends Obj, R> = Taking<R> & {
    readonly name: stores.Name
    readonly rows: Rows<O, R>
    readonly held: () => R
    readonly place: Place<R>
    readonly ask: (since: string) => Promise<Answer<O, R>>
    // What follows a landing that moved the row.
    readonly landed: (row: R) => Promise<void>
    // What runs before the row is first read.
    readonly restoring: () => Promise<void>
  }

  // Who hears a pull: what a reading left out, by reader; each move of its progress; what to do once a later epoch
  // replaced a landing; and the pull asked again after a wait.
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: progress.Event) => void
    readonly replaced: () => void
    readonly again: () => void
  }

  // One store's pull, its pieces kept beside its row, and its landing.
  export type Sync<R> = {
    readonly restored: once.T<void>
    readonly pull: () => Promise<boolean>
    readonly holding: () => Replica<Obj>
    readonly landed: () => boolean
    readonly whole: () => Promise<void>
    readonly reset: (said: (why: string) => void) => Promise<void>
    // A row this tab moved itself: held, then written on the device joined with what another tab kept there.
    readonly keep: (row: R) => Promise<void>
    // What the row owes let go, as a landing does: asked again once the store reads more of what its row names.
    readonly purge: () => Promise<void>
  }

  // A row that is a replica alone, a store whose objects lock no words: the page's stores.
  export const plain = <O extends Obj>(hold: (r: Replica<O>) => void): Taking<Replica<O>> & { readonly rows: Rows<O, Replica<O>> } => ({
    rows: { none: empty<O>(), replica: r => r, join },
    take: (_, incoming) => Promise.resolve(hold(incoming)),
    erasing: (_, __, pending) => Promise.resolve(pending),
  })

  // What a row with the keys of its words needs beyond a plain one: how a key is let go, what the objects name as
  // erased, the blocks each key's words fed, and how those are forgotten, from the device's row as well.
  export type Locking<O extends Obj> = {
    readonly name: stores.Name
    readonly held: () => locked.Row<O>
    readonly hold: (row: locked.Row<O>) => void
    readonly forgot: (keys: readonly string[]) => void
    readonly dead: locked.Dead<O>
    // Every key `r`'s objects name as erased and every key in `told`, with the blocks `r.keys` opens of each.
    readonly erased: (r: locked.Row<O>, told: readonly string[]) => Promise<Erased>
    readonly forget: (blocks: readonly string[]) => Promise<lo.Settled<null>>
    // The keys `gone` taken out of the device's row, whatever else another tab wrote there.
    readonly alter: (gone: readonly string[]) => Promise<lo.Settled<null>>
    readonly told: (reader: string, left: excluded.Left) => void
  }

  // A row with the keys of its words, the lines': a key an erasure names is let go only once the blocks its words fed
  // are forgotten, wherever it is held, the row or a pull's pieces; a forget that fails keeps it, owed, to be asked again.
  export const locks = <O extends Obj>(l: Locking<O>): Taking<locked.Row<O>> & { readonly rows: Rows<O, locked.Row<O>> } => {
    // A purge that could not forget is said, and asked again at the next sync; the keys it owes stay until it can.
    const owing = (failed: string | null): void =>
      l.told(
        `purge/${kept(l.name)}`,
        excluded.left(excluded.NONE, [], failed === null ? [] : [{ chain: kept(l.name), why: `the blocks some erased words were rendered to are still on this device (${failed}); it tries again at the next sync`, cure: "None" }]),
      )
    const dead = (erased: readonly string[]): locked.Dead<O> => (erased.length === 0 ? l.dead : r => l.dead(r).then(d => new Set([...d, ...erased])))
    const taking = (erased: readonly string[]): take.Effects<O> => ({
      held: l.held,
      dispatch: e => (e.type === "Held" ? l.hold(e.row) : l.forgot(e.keys)),
      purge: { erased: r => l.erased(r, erased), forget: l.forget },
      dead: dead(erased),
      alter: l.alter,
      owing,
    })
    const none = locked.none<O>()
    return {
      rows: { none, replica: r => r.replica, join: locked.join },
      take: (erased, incoming) => take.into(taking(erased), incoming),
      // While more pieces are to come: the keys the erasure owes, the pieces' among them, purged over the row and the
      // pieces together, and let go from both and from the device. The last piece lands, and its landing purges the rest.
      erasing: async (erased, rest, pending) => {
        if (erased.length === 0 || rest === 0) return pending
        const gone = await purge.forgotten({ erased: r => l.erased(r, erased), forget: l.forget }, dead(erased), locked.join(l.held(), pending))
        if (gone.type === "Error") return (owing(gone.e), pending)
        if (gone.v.length === 0) return (owing(null), pending)
        l.forgot(gone.v)
        const kept = await l.alter(gone.v)
        owing(kept.type === "Error" ? `the device's copy was not written (${kept.e})` : null)
        return locked.without(pending, gone.v)
      },
    }
  }

  // A store's row on the device sits beside the chains', under a key no chain can be named.
  export const kept = (name: stores.Name): string => `store/${name}`

  // A row that does not read is said, with its own reset, never passed over.
  export const unread = (name: stores.Name, why: string | null): excluded.Left =>
    excluded.left(excluded.NONE, [], why === null ? [] : [excluded.unread(kept(name), why)])

  // A pull that did not end whole is asked again after a wait that doubles from two seconds to a minute, and whatever
  // a stream says meanwhile asks sooner; a whole pull starts the waits over.
  const AGAIN_MS = { first: 2_000, most: 60_000 }

  // How a landing settled: the store landed, or a later epoch replaced the landing before it did (`startOver`).
  type Settled = "Landed" | "Replaced"
  type Landing = { readonly once: Promise<Settled>; readonly done: (how: Settled) => void }
  const awaited = (): Landing => {
    let done = (_how: Settled): void => {}
    const once = new Promise<Settled>(resolve => (done = resolve))
    return { once, done }
  }

  export const of = <O extends Obj, R>(st: Store<O, R>, fx: Effects): Sync<R> => {
    const name = st.name
    const rows = st.rows
    const told = (why: string | null): void => fx.told(`row/${kept(name)}`, unread(name, why))

    // The pieces of the pull that have not landed: held beside the row and asked from, never read by a page.
    let pending = rows.none
    // What the tab holds of the store, landed and not: what a pull asks from, and what a stream's heads are compared with.
    const holding = (): Replica<O> => rows.replica(rows.join(st.held(), pending))
    // The two parts of it, each asked on its own whether a later epoch overtook it.
    const parts = (): readonly Replica<O>[] => [rows.replica(st.held()), rows.replica(pending)]

    // One read of the row per page.
    const restoredOne = async (): Promise<void> => {
      await st.restoring()
      const read = await st.place.restore()
      if (read.row.type === "Dropped") return told(read.row.failed)
      told(read.row.type === "Unreadable" ? read.row.why : null)
      if (read.row.type === "Held") await st.take([], read.row.row)
      // The pieces a pull kept and did not land, another tab's included: held beside the row, never read by a page.
      pending = read.pieces.reduce(rows.join, pending)
      if (read.unread.length > 0) told(read.unread.join("; "))
    }
    const restored = once.of(restoredOne)

    type Piece = { readonly asked: Replica<O>; readonly answer: Answer<O, R>; readonly rest: number }

    // One piece kept: the erasures it names forgotten from what pages read, then the piece held beside it, less any key
    // those name, and written on the device by itself. It holds only what was asked of a replica, so another tab's reset
    // kept meanwhile, a later generation, keeps it out when it lands, and the next piece is asked of the reset. It stands
    // where that replica stood, so the frontier it names is read against the one asked from: an answer that did not
    // know it moves the era on from there, and a later piece of the same era is the later frontier.
    const keep = async ({ asked, answer }: Piece): Promise<void> => {
      fx.told(`epoch/${kept(name)}`, excluded.behind(kept(name), wentBack(asked, answer.said)))
      fx.bars({ type: "Piece", name, size: answer.size, weight: answer.weight })
      const piece = answer.piece(pulled(replicaOf<O>({ ...empty<O>().held, generation: asked.generation, frontier: asked.held.frontier }), answer.said, time.now()))
      pending = rows.join(await st.erasing(answer.erased, answer.rest, pending), piece)
      if (answer.said.objects.length > 0) {
        const written = await st.place.keepPiece(asked.objects.length, piece)
        if (written.type === "Error") told(written.e)
      }
    }

    let landing = awaited()

    // What the pieces brought, landed in one step, so a page reading the store reads it once, whole; then the row written
    // with what another tab kept joined in, and the pieces' own rows deleted once it is, so a write that failed leaves
    // what they brought on the device for the next load.
    const land = async (last: Piece): Promise<void> => {
      const was = st.held()
      const brought = pending
      await st.take(last.answer.erased, brought)
      pending = rows.none
      const now = st.held()
      let wrote = true
      if (now !== was) {
        const { row, unread } = await st.place.land(now)
        if (row.type === "Val") await st.take(last.answer.erased, row.v)
        wrote = row.type === "Val"
        told(unread)
      }
      if (brought !== rows.none && wrote) {
        const gone = await st.place.dropPieces()
        if (gone.type === "Error") told(gone.e)
      }
      landing.done("Landed")
      await st.landed(st.held())
    }

    // One wait at a time: a later failure moves it, never adds another.
    let wait = AGAIN_MS.first
    let timer: ReturnType<typeof setTimeout> | undefined
    const retry = (): void => {
      clearTimeout(timer)
      timer = setTimeout(fx.again, wait)
      wait = Math.min(wait * 2, AGAIN_MS.most)
    }
    const settled = (): void => {
      clearTimeout(timer)
      wait = AGAIN_MS.first
    }

    // A piece asked of what is held, landed and not.
    const askOf = (): Promise<Piece> => {
      const asked = holding()
      return st.ask(sinceOf(asked)).then((answer): Piece => ({ asked, answer, rest: answer.rest }))
    }

    // What the box's later epoch overtook dropped, on the device first, so no copy from before it, another tab's included,
    // reaches it: the row reset to an empty one of the generation after, keys and all, the kept pieces deleted, and the
    // pull's progress with them, so it starts again from nothing.
    const startOver = async (): Promise<void> => {
      const row = await st.place.reset(st.held())
      if (row.type === "Error") throw new Error(row.e)
      pending = rows.none
      // The store is unlanded again, so a reading waits for its next landing, never reads the emptied row. The landing
      // a reading already waits on is settled as it is replaced, so that reading asks again and waits on this one.
      const was = landing
      landing = awaited()
      was.done("Replaced")
      fx.replaced()
      settled()
      await st.take([], row.v)
    }

    // By pieces, each asked from what is held, landed and not, and kept before the next. A pull that did not end whole
    // answers false, keeps what came, and is asked again for the rest. A piece of a later epoch than all that is held
    // starts the pull over, and the store is asked for again in pieces.
    const pullNow = (): Promise<boolean> =>
      restored
        .get()
        .then(() =>
          pieces.pull({
            ask: async () => {
              const piece = await askOf()
              if (!stores.overtaken(parts(), piece.answer.said)) return piece
              await startOver()
              return askOf()
            },
            keep,
            land,
            held: () => [st.held(), pending],
          }),
        )
        .catch((e: unknown): pieces.Ended => ({ type: "Cut", taken: 0, e }))
        .then(ended => {
          fx.bars({ type: "Pulled", name })
          if (ended.type === "Whole") {
            settled()
            return true
          }
          retry()
          return false
        })

    const landed = (): boolean => rows.replica(st.held()).box.type === "Heard"
    // A replaced landing is waited on again through the one that replaced it; a landed one ends the wait, even empty.
    const whole = (): Promise<void> => (landed() ? Promise.resolve() : landing.once.then(how => (how === "Landed" ? undefined : whole())))

    // A row that does not read, written over by a reset and taken again from the box; a reset that fails is said.
    const reset = async (said: (why: string) => void): Promise<void> => {
      const done = await st.place.reset(st.held())
      if (done.type === "Error") said(`the reset was not kept: ${done.e}`)
      pending = rows.none
      restored.forget()
    }

    const keepOwn = async (row: R): Promise<void> => {
      await st.take([], row)
      const written = await st.place.land(st.held())
      if (written.row.type === "Val") await st.take([], written.row.v)
      told(written.unread)
    }

    // One pull at a time, so a pull asked for during another is a pull that starts after it.
    const purgeNow = (): Promise<void> => restored.get().then(() => st.take([], rows.none))

    return { restored, pull: serial.of(pullNow), holding, landed, whole, reset, keep: keepOwn, purge: serial.of(purgeNow) }
  }
}
export default pull
