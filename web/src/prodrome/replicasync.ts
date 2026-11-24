// One store whose replica the core holds, as the core's worker drives it: restored from the device's log of the core's
// frames (`replicalog`), pulled from the box by pieces asked from the frontier the core names, and landed in one step,
// each move kept as the frame the core printed. It holds no object, word or frontier of its own; every question is the
// core's. Another tab's frames are read back before each pull, each a join; a compaction, after a quiet or when the page
// is hidden, is the one way bytes leave the device. The inbox, the topic stores and the chains are its instances.
//
// ONE OWNER: every step that reads the core or the log and then writes either runs alone, in the order asked
// (`exclusive`), and no step holds that turn across a request to the box. A move this tab makes of its own writes is
// kept on the device before the core holds it (`kept`), so a write the device could not keep is no write at all.

import type { LineStep } from "./core"
import { CONTENT } from "./objects"
import copies from "./copies"
import excluded from "./excluded"
import type heads from "./heads"
import phases from "./phases"
import pieces from "./pieces"
import replicalog from "./replicalog"
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
    closed: () => boolean
  }

  // One piece the box answered, as far as a pull reads it.
  export type Piece = { readonly rest: number; readonly epoch: number; readonly size: number; readonly weight: number }

  // A pull's progress, by the bytes its pieces say.
  export type Pulled = { readonly type: "Piece"; readonly size: number; readonly weight: number } | { readonly type: "Pulled" }

  // What differs between stores: the log's name (a store's, or a chain's as `chain/<name>`), the handle, how a piece is
  // asked for and handed to the core, and what follows a landing.
  export type Spec<A extends Piece> = {
    readonly name: string
    readonly held: Held
    readonly ask: (since: string) => Promise<A>
    readonly piece: (answer: A, at: number) => Uint8Array
    readonly landed: () => Promise<void>
    // The form of a snapshot's or a frame's bytes, and what an earlier build's copy of the store can hold.
    readonly form: (bytes: Uint8Array) => replicalog.Form
    readonly holds: replicalog.Holds
  }

  // Who hears the store: a reading's leavings by reader, a pull's progress, what a move moved as the core said it, and a
  // later epoch, after which what waited on the store waits again.
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: Pulled) => void
    readonly moved: (said: string) => void
    readonly replaced: () => void
  }

  export type Store = {
    readonly want: () => Promise<boolean>
    readonly wanted: () => boolean
    readonly heard: (box: heads.Heads) => void
    readonly reset: () => Promise<readonly string[]>
    readonly sync: { readonly restored: once.T<void>; readonly landed: () => boolean; readonly whole: () => Promise<void> }
  }

  export type T = Store & {
    // `f` alone among this store's steps, in the order asked; `read`, `kept` and `stepped` are called only inside it.
    readonly exclusive: <R>(f: () => Promise<R>) => Promise<R>
    // A reset whose `first` step, run in the reset's turn once the log is read, may write (`kept`) or stop it with why.
    readonly resetAfter: (first: () => Promise<readonly string[]>) => Promise<readonly string[]>
    // What another tab of the device kept since this one last read the log, taken, in its turn.
    readonly caughtUp: () => Promise<void>
    // The same, inside a turn the caller holds: what a plan made in that turn is made over.
    readonly read: () => Promise<void>
    // A move of this tab's own writes the core planned: kept on the device, then taken by the core. Throws, the core
    // unmoved, where the device did not keep it.
    readonly kept: (frame: Uint8Array) => Promise<void>
    // A move the core made, the box's answer to a write: heard, and its frame kept; throws where it was not kept.
    readonly stepped: (step: LineStep) => Promise<void>
    // Whether the device keeps an earlier build's copy of the store aside.
    readonly earlier: () => Promise<boolean>
    // The page is shown or hidden; a page hidden may be closed, so the log is compacted now, settling once it is.
    readonly shown: (on: boolean) => Promise<void>
  }

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
    const KEPT = copies.kept(name)
    // Its objects carry their literals, so it holds no print and husks none.
    const log = replicalog.of(name, CONTENT, spec.holds)
    const told = (why: string | null): void => fx.told(`row/${KEPT}`, copies.unread(name, why))
    // A pull's keep that failed, said until one is kept: nothing to reset, the next pull takes it again.
    const keepTold = (why: string | null): void =>
      fx.told(`keep/${KEPT}`, excluded.left(excluded.NONE, [], why === null ? [] : [{ chain: KEPT, why, cure: "None" }]))

    // Each step after the last asked, alone.
    let turn: Promise<unknown> = Promise.resolve()
    const exclusive = <R>(f: () => Promise<R>): Promise<R> => {
      const run = turn.then(f, f)
      turn = run.catch(() => {})
      return run
    }

    // The core a log is read into, what each load or frame moved heard.
    const reader: replicalog.Reader = {
      load: (snapshot, frames) => fx.moved(held.load(snapshot, frames)),
      apply: bytes => fx.moved(held.apply(bytes)),
      form: spec.form,
    }
    // A part that did not read in this build's form, set aside and named once with the reset that removes it.
    const asideTold = (what: string | null): void => fx.told(`aside/${KEPT}`, copies.aside(name, what))
    // Whether the box's own copy, pulled whole from nothing, is not closed under its parents: then it is held unchanged, said
    // once, and never started over for that again.
    let open = false
    // What the log holds read into the core, part by part where the whole does not read (`replicalog.take`). Where frames
    // were taken, rows that do not hold all a pull can leave start over: a frame a tab appended before the device was
    // cleared under it can rest on what the clearing took.
    // Whether it named a part it set aside.
    const read = async (kept: replicalog.Kept): Promise<boolean> => {
      const took = await log.take(kept, reader)
      if (took.type === "Error") {
        told(took.e)
        return false
      }
      if (took.v.unread !== null) asideTold(took.v.unread)
      if (took.v.taken && !open && !held.closed()) await startOver()
      return took.v.unread !== null
    }
    const caughtUp = async (): Promise<void> => {
      if (!idb.available()) return
      const kept = await log.since()
      if (kept.type === "Error") return told(kept.e)
      await read(kept.v)
    }

    // The log compacted: what another tab appended read first, then the core's snapshot kept and every frame up to it
    // deleted. A frame appended between the two that the core does not read stops the compaction, so nothing unread is
    // folded away.
    let quiet: ReturnType<typeof setTimeout> | undefined
    const compact = async (): Promise<void> => {
      clearTimeout(quiet)
      quiet = undefined
      if (!idb.available()) return
      for (let tries = 0; tries < 2; tries += 1) {
        await caughtUp()
        const done = await log.compact(frames => frames.forEach(f => fx.moved(held.apply(f.bytes))), () => held.snapshot(), () => [])
        if (done.type === "Error") return
        if (done.v === "Done") return void phases.mark(`${name} snapshot written`)
      }
    }
    // A frame kept on the device, or why it was not: a log that began again is read whole.
    const append = async (bytes: Uint8Array): Promise<void> => {
      if (!idb.available()) return
      const appended = await log.append(bytes)
      if (appended.type === "Error") throw new Error(`this device did not keep a move of ${name} (${appended.e})`)
      if (appended.v === "BeganAgain") await caughtUp()
      clearTimeout(quiet)
      quiet = setTimeout(() => void exclusive(compact), QUIET_MS)
    }
    const stepped = async (step: LineStep): Promise<void> => {
      const [delta, said] = [step.delta, step.moved]
      step.free()
      fx.moved(said)
      await append(delta)
    }
    const kept = async (frame: Uint8Array): Promise<void> => {
      await append(frame)
      fx.moved(held.apply(frame))
    }

    // One read of the device's log per page: the core loaded from it, before anything reads the store.
    const restored = once.of(() =>
      exclusive(async (): Promise<void> => {
        phases.mark(`${name} restore`)
        if (idb.available()) {
          const kept = await log.read()
          let named = false
          if (kept.type === "Error") told(kept.e)
          else named = await read(kept.v)
          // What an earlier load set aside, where this one set nothing aside of its own to name.
          const aside = named ? null : await log.aside()
          if (aside !== null && aside.type === "Val" && aside.v.unread) asideTold("parts of it, from an earlier load")
        }
        phases.mark(`${name} restored`)
      }),
    )

    let landing = awaited()
    let wait = AGAIN_MS.first
    let timer: ReturnType<typeof setTimeout> | undefined
    const settled = (): void => {
      clearTimeout(timer)
      wait = AGAIN_MS.first
    }
    // A later epoch overtook what is held, or the log lost its early frames: every row reset, in the generation after,
    // and what waited on a landing waits on the next.
    const startOver = async (): Promise<void> => {
      await stepped(held.start_over())
      replaced()
    }
    const replaced = (): void => {
      const was = landing
      landing = awaited()
      was.done("Replaced")
      fx.replaced()
      settled()
    }

    // By pieces, each asked from what the core holds, landed and not, and kept on the device as it comes, each step in its
    // turn and no turn held across a request: a piece answered from a frontier the core no longer stands at (a reset or
    // another tab's landing came between) is passed over and the next is asked from where it stands.
    const pullNow = serial.of(async (): Promise<boolean> => {
      await restored.get()
      const once = async (): Promise<{ readonly ended: pieces.Ended; readonly passed: number; readonly fromNothing: boolean }> => {
        let passed = 0
        let fromNothing = false
        try {
          await exclusive(caughtUp)
          fromNothing = held.since() === ""
          let asked = ""
          const ended = await pieces.pull<A>({
            ask: async () => {
              asked = held.since()
              const a = await spec.ask(asked)
              if (!held.overtaken(a.epoch)) return a
              await exclusive(startOver)
              asked = held.since()
              return spec.ask(asked)
            },
            keep: a =>
              exclusive(async () => {
                if (held.since() !== asked) return void (passed += 1)
                fx.told(`epoch/${KEPT}`, excluded.behind(KEPT, held.went_back(a.epoch)))
                fx.bars({ type: "Piece", size: a.size, weight: a.weight })
                await append(spec.piece(a, time.now())).then(
                  () => keepTold(null),
                  (e: unknown) => {
                    keepTold(`a piece of it pulled from the box was not kept on this device (${e instanceof Error ? e.message : String(e)}); the next pull takes it again`)
                    throw e
                  },
                )
              }),
            land: () =>
              exclusive(async () => {
                await stepped(held.land())
                landing.done("Landed")
                await spec.landed()
              }),
            held: () => [held.since(), held.heads()],
          })
          return { ended, passed, fromNothing }
        } catch (e: unknown) {
          return { ended: { type: "Cut", taken: 0, e }, passed, fromNothing }
        }
      }
      // A pull a piece of which was passed over asks again from where the core now stands.
      let run = await once()
      while (run.ended.type === "Whole" && run.passed > 0) run = await once()
      const ended = run.ended
      fx.bars({ type: "Pulled" })
      if (ended.type === "Whole") {
        // The box's own copy, taken whole from nothing, that is not closed under its parents: no start-over can close it,
        // so it is held unchanged and said once.
        if (run.fromNothing && !open && !held.closed()) {
          open = true
          fx.told(`open/${KEPT}`, copies.open(name, "the box's copy of it is not closed under its parents, so this device holds it unchanged and does not take it again for that"))
        }
        settled()
        return true
      }
      clearTimeout(timer)
      timer = setTimeout(() => void (wanted === "Yes" ? pullNow() : undefined), wait)
      wait = Math.min(wait * 2, AGAIN_MS.most)
      return false
    })

    // The log read, then `first` in the same turn (what it answers stops the reset), then the store started over in the
    // core and kept in one transaction (`replicalog.reset`): the frames read deleted and the snapshot of the store as it
    // starts over written in their place, what this tab wrote and the box has not taken in it, and what was aside gone.
    // Nothing the reset keeps lives only in memory between its steps. A frame another tab appended meanwhile is read and
    // the reset kept again over it.
    const reset = async (first: () => Promise<readonly string[]>): Promise<readonly string[]> => {
      await restored.get()
      // Whether the core started over: a reset not kept then pulls all the same, since the store stands empty.
      let startedOver = false
      const stopped = await exclusive(async (): Promise<readonly string[]> => {
        await caughtUp()
        const no = await first()
        if (no.length > 0) return no
        startedOver = true
        const step = held.start_over()
        fx.moved(step.moved)
        step.free()
        replaced()
        if (!idb.available()) return []
        for (let tries = 0; tries < 2; tries += 1) {
          const done = await log.reset(() => held.snapshot())
          if (done.type === "Error") return [`the reset was not kept: ${done.e}`]
          if (done.v === "Done") {
            told(null)
            asideTold(null)
            return []
          }
          await caughtUp()
        }
        return ["the reset was not kept: another tab kept writing to this store meanwhile"]
      })
      if (startedOver) await want()
      return stopped
    }

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
      earlier: () => restored.get().then(() => log.aside().then(a => a.type === "Val" && a.v.earlier)),
      heard: box => {
        if (wanted === "Yes" && held.behind(JSON.stringify(box))) void pullNow()
      },
      reset: () => reset(async () => []),
      resetAfter: reset,
      sync: { restored, landed: () => held.heard(), whole },
      exclusive: f => restored.get().then(() => exclusive(f)),
      caughtUp: () => restored.get().then(() => exclusive(caughtUp)),
      read: caughtUp,
      kept,
      stepped,
      shown: on => (on ? Promise.resolve() : restored.get().then(() => exclusive(compact))),
    }
  }
}
export default replicasync
