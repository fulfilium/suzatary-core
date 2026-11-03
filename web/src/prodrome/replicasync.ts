// One store whose replica the core holds, as the core's worker drives it: restored from the device's log of the core's
// frames (`replicalog`), pulled from the box by pieces asked from the frontier the core names, and landed in one step,
// each move kept as the frame the core printed. It holds no object, word or frontier of its own; every question is the
// core's. Another tab's frames are read back before each pull, each a join; a compaction, after a quiet or when the page
// is hidden, is the one way bytes leave the device. The inbox and the topic stores are its instances.

import type { LineStep } from "./core"
import { framed } from "./core"
import { CONTENT } from "./objects"
import excluded from "./excluded"
import type heads from "./heads"
import phases from "./phases"
import pieces from "./pieces"
import type progress from "./progress"
import replicalog from "./replicalog"
import type stores from "./stores"
import once from "../algebra/once"
import serial from "../algebra/serial"
import idb from "../app/idb"
import time from "../app/time"

export namespace replicasync {
  // The core's handle on a store's replica: the questions a pull asks and the moves it makes. Each move answers its
  // frame, and what it moved as the core says it.
  export interface Held {
    since: () => string
    heads: () => string
    heard: () => boolean
    behind: (heads: string) => boolean
    overtaken: (epoch: number) => boolean
    went_back: (epoch: number) => boolean
    land: () => LineStep
    start_over: () => LineStep
    apply: (frame: Uint8Array) => string
    load: (bytes: Uint8Array | undefined, frames: Uint8Array) => string
    snapshot: () => Uint8Array
  }

  // One piece the box answered, as far as a pull reads it.
  export type Piece = { readonly rest: number; readonly epoch: number; readonly size: number; readonly weight: number }

  // What differs between stores: the handle, how a piece is asked for and handed to the core, and what follows a landing.
  export type Spec<A extends Piece> = {
    readonly name: stores.Name
    readonly held: Held
    readonly ask: (since: string) => Promise<A>
    readonly piece: (answer: A, at: number) => Uint8Array
    readonly landed: () => Promise<void>
  }

  // Who hears the store: a reading's leavings by reader, a pull's progress, what a move moved as the core said it, and a
  // later epoch, after which what waited on the store waits again.
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: progress.Event) => void
    readonly moved: (said: string) => void
    readonly replaced: () => void
  }

  // A store as the set of stores asks each of its stores.
  export type Store = {
    readonly want: () => Promise<boolean>
    readonly wanted: () => boolean
    readonly heard: (box: heads.Heads) => void
    readonly reset: () => Promise<readonly string[]>
    readonly sync: { readonly restored: once.T<void>; readonly landed: () => boolean; readonly whole: () => Promise<void> }
  }

  export type T = Store & {
    // A move this tab made itself, a write or the box's answer to one: its moves heard and its frame kept.
    readonly stepped: (step: LineStep) => Promise<void>
    // The page is shown or hidden; a page hidden may be closed, so the log is compacted now, settling once it is.
    readonly shown: (on: boolean) => Promise<void>
  }

  // A store's copy on the device sits beside the chains', under a key no chain can be named.
  export const kept = (name: string): string => `store/${name}`

  // A copy that does not read is said, with its own reset, never passed over.
  export const unread = (name: string, why: string | null): excluded.Left =>
    excluded.left(excluded.NONE, [], why === null ? [] : [excluded.unread(kept(name), why)])

  // How long a store stays unmoved before its log is compacted: a burst of landings is compacted once.
  export const QUIET_MS = 30_000
  // A pull that did not end whole is asked again after a wait that doubles from two seconds to a minute.
  const AGAIN_MS = { first: 2_000, most: 60_000 }

  // How a landing settled: it landed, or a later epoch replaced it before it did.
  type Settled = "Landed" | "Replaced"
  type Landing = { readonly once: Promise<Settled>; readonly done: (how: Settled) => void }
  const awaited = (): Landing => {
    let done = (_how: Settled): void => {}
    const once = new Promise<Settled>(resolve => (done = resolve))
    return { once, done }
  }

  export const of = <A extends Piece>(spec: Spec<A>, fx: Effects): T => {
    const { name, held } = spec
    const KEPT = kept(name)
    // Its objects carry their literals, so it holds no print and husks none.
    const log = replicalog.of(name, CONTENT)
    const told = (why: string | null): void => fx.told(`row/${KEPT}`, unread(name, why))

    // Where this tab stands in the device's log: every frame up to `after` is held, and the frames it appended itself,
    // which the core already holds, are passed over when it reads the log again.
    let after = 0
    const mine = new Set<number>()
    const take = (frames: readonly replicalog.Frame[]): void => {
      for (const f of frames) {
        if (!mine.has(f.at)) fx.moved(held.apply(f.bytes))
        after = Math.max(after, f.at)
      }
    }
    // The core loaded from a snapshot and the frames after it; a snapshot of another form is a miss: the log is dropped
    // and the store is taken again from the box.
    const loaded = async (kept: replicalog.Kept): Promise<string | null> => {
      try {
        const said = held.load(kept.snapshot?.bytes, framed(kept.frames.map(f => f.bytes)))
        after = kept.frames.at(-1)?.at ?? kept.snapshot?.through ?? 0
        mine.clear()
        return said
      } catch {
        const gone = await log.drop()
        told(gone.type === "Error" ? `the copy kept on this device is of another build and was not dropped (${gone.e})` : null)
        return null
      }
    }
    // What another tab wrote since this one last read the log.
    const caughtUp = async (): Promise<void> => {
      if (!idb.available()) return
      const kept = await log.since(after)
      if (kept.type === "Error") return told(kept.e)
      if (kept.v.snapshot === null) return take(kept.v.frames)
      const said = await loaded(kept.v)
      if (said !== null) fx.moved(said)
    }

    // The log compacted: the frames another tab wrote taken, then the core's snapshot kept as of the last frame and every
    // frame up to it deleted. Where another tab's compaction came first, its snapshot is read, and this compacts again.
    let quiet: ReturnType<typeof setTimeout> | undefined
    const compact = serial.of(async (): Promise<void> => {
      clearTimeout(quiet)
      quiet = undefined
      if (!idb.available()) return
      for (let tries = 0; tries < 2; tries += 1) {
        const done = await log.compact(after, take, () => held.snapshot(), () => [])
        if (done.type === "Error") return
        if (done.v.type === "Through") {
          after = done.v.at
          mine.clear()
          phases.mark(`${name} snapshot written`)
          return
        }
        await caughtUp()
      }
    })
    // A move the core answered, kept on the device as its frame.
    const append = async (bytes: Uint8Array): Promise<void> => {
      if (!idb.available()) return
      const at = await log.append(bytes)
      if (at.type === "Error") return told(`a move of ${name} was not kept on this device (${at.e}); the next load takes it again from the box`)
      mine.add(at.v)
      clearTimeout(quiet)
      quiet = setTimeout(() => void compact(), QUIET_MS)
    }
    const stepped = async (step: LineStep): Promise<void> => {
      const [delta, said] = [step.delta, step.moved]
      step.free()
      fx.moved(said)
      await append(delta)
    }

    // One read of the device's log per page: the core loaded from it, before anything reads the store.
    const restored = once.of(async (): Promise<void> => {
      phases.mark(`${name} restore`)
      if (idb.available()) {
        const kept = await log.read()
        // Nothing reads the store before it is restored, so what the load moved is told to no one.
        if (kept.type === "Error") told(kept.e)
        else await loaded(kept.v)
      }
      phases.mark(`${name} restored`)
    })

    let landing = awaited()
    let wait = AGAIN_MS.first
    let timer: ReturnType<typeof setTimeout> | undefined
    const settled = (): void => {
      clearTimeout(timer)
      wait = AGAIN_MS.first
    }
    // A later epoch overtook what is held: every row reset, in the generation after, and what waited on a landing waits
    // on the next.
    const startOver = async (): Promise<void> => {
      await stepped(held.start_over())
      const was = landing
      landing = awaited()
      was.done("Replaced")
      fx.replaced()
      settled()
    }

    // By pieces, each asked from what the core holds, landed and not, and kept on the device as it comes. A piece of a
    // later epoch than what is held starts the store over first. Another tab's frames are read before the pull, never
    // between a question and its piece.
    const pullNow = serial.of(async (): Promise<boolean> => {
      await restored.get()
      const ended = await (async (): Promise<pieces.Ended> => {
        try {
          await caughtUp()
          return await pieces.pull<A>({
            ask: async () => {
              const a = await spec.ask(held.since())
              if (!held.overtaken(a.epoch)) return a
              await startOver()
              return spec.ask(held.since())
            },
            keep: async a => {
              fx.told(`epoch/${KEPT}`, excluded.behind(KEPT, held.went_back(a.epoch)))
              fx.bars({ type: "Piece", name, size: a.size, weight: a.weight })
              await append(spec.piece(a, time.now()))
            },
            land: async () => {
              await stepped(held.land())
              landing.done("Landed")
              await spec.landed()
            },
            held: () => [held.since(), held.heads()],
          })
        } catch (e: unknown) {
          return { type: "Cut", taken: 0, e }
        }
      })()
      fx.bars({ type: "Pulled", name })
      if (ended.type === "Whole") {
        settled()
        return true
      }
      clearTimeout(timer)
      timer = setTimeout(() => void (wanted === "Yes" ? pullNow() : undefined), wait)
      wait = Math.min(wait * 2, AGAIN_MS.most)
      return false
    })

    // Whether a reading wants the store yet: before one does, nothing pulls it.
    let wanted: "No" | "Yes" = "No"
    const want = (): Promise<boolean> => {
      wanted = "Yes"
      return pullNow()
    }
    // A replaced landing is waited on again through the one that replaced it; a landed one ends the wait, even empty.
    const whole = (): Promise<void> => (held.heard() ? Promise.resolve() : landing.once.then(how => (how === "Landed" ? undefined : whole())))

    return {
      want,
      wanted: () => wanted === "Yes",
      heard: box => {
        if (wanted === "Yes" && held.behind(JSON.stringify(box))) void pullNow()
      },
      // A copy that does not read, or one the owner asked to take again: the log dropped and the store started over.
      reset: async () => {
        const gone = await log.drop()
        after = 0
        mine.clear()
        await startOver()
        // What this tab wrote and the box has not taken is in the core alone until a snapshot holds it.
        await compact()
        await want()
        return gone.type === "Error" ? [`the reset was not kept: ${gone.e}`] : []
      },
      sync: { restored, landed: () => held.heard(), whole },
      stepped,
      shown: on => (on ? Promise.resolve() : compact()),
    }
  }
}
export default replicasync
