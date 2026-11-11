// The lines as one store of the core's worker. The core holds their replica whole (`Lines`); this fetches what it lacks
// and keeps each move it answers on the device as its frame (`replicalog`), holding no node, key or head of its own.

import type { Lines } from "./core"
import { admitsOf, framed, heldOf, holdAdmitted, husksOf, lackingIn, lackingOf, lacksIn, movedOf, owedOf, planOf } from "./core"
import content from "./content"
import replicasync from "./replicasync"
import excluded from "./excluded"
import type heads from "./heads"
import keep from "./keep"
import type lines from "./lines"
import { CONTENT } from "./objects"
import phases from "./phases"
import pieces from "./pieces"
import type progress from "./progress"
import replicalog from "./replicalog"
import lo from "../algebra/lo"
import once from "../algebra/once"
import serial from "../algebra/serial"
import idb from "../app/idb"
import type rendered from "../app/rendered"
import time from "../app/time"
import http from "../wire/http"
import storereads from "../wire/storereads"
import type { SkeletonPayload } from "../wire/generated/wire"

export namespace linesync {
  // Which conversations a move touched, or all of them after a reset or a later epoch.
  export type Moved = "All" | readonly string[]

  // Who hears the lines: a reading's leavings by reader, a pull's progress, and a move.
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: progress.Event) => void
    readonly moved: (conversations: Moved) => void
    readonly replaced: () => void
  }

  export type T = {
    // The store as the set of stores asks each of its stores.
    readonly engine: replicasync.Store
    // `read` once every line `scope` reads is held; the scope is asked again after each fetch.
    readonly over: <A>(scope: () => lines.Scope, read: () => A) => Promise<A>
    readonly holds: (id: string) => boolean
    // Whether a page is shown: a prefetch waits while none is, and a page hidden may be closed, so the log is compacted.
    readonly shown: (on: boolean) => Promise<void>
    // How many lines `scope` reads that the skeleton names and the device does not hold.
    readonly lacking: (scope: lines.Scope) => number
    // Which blocks the device may keep, decided against every frame on it (`rendered.Admit`).
    readonly admit: rendered.Admit
  }

  const NAME = "lines"
  const KEPT = replicasync.kept(NAME)
  // How long the lines stay unmoved before their log is compacted.
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

  export const of = (held: Lines, fx: Effects, blocks: rendered.Device): T => {
    const log = replicalog.of(NAME, CONTENT)
    const told = (why: string | null): void => fx.told(`row/${KEPT}`, replicasync.unread(NAME, why))
    const moved = (said: string): void => fx.moved(movedOf(said))

    let shown = false
    const fetcher = content.fetcher({
      ask: hashes => storereads.content("lines", hashes),
      asked: bytes => fx.bars({ type: "Asked", name: "lines", bytes }),
      came: bytes => fx.bars({ type: "Came", name: "lines", bytes }),
      lost: bytes => fx.bars({ type: "Lost", name: "lines", bytes }),
      atOnce: () => content.atOnce(http.protocol()),
      lacking: names => lackingOf(held, names),
      held: rows => Promise.resolve(holdAdmitted(held, rows)),
      husked: husks => stepped(held.take_husks(JSON.stringify(husks))),
      shown: () => shown,
    })
    const fetching = fetcher.fetch

    // The device's budget for the lines' content, read once from the origin's quota.
    const budget = once.of(async (): Promise<number> => {
      const s = typeof navigator === "undefined" ? undefined : navigator.storage
      const quota = s === undefined || typeof s.estimate !== "function" ? undefined : (await s.estimate().catch(() => undefined))?.quota
      return keep.budget(quota === undefined ? { type: "Unknown" } : { type: "Known", bytes: quota })
    }).get

    // What the device keeps of the lines' content within the budget, and what of it a prefetch fetches.
    const planned = async (): Promise<ReturnType<typeof planOf>> => planOf(held, await budget())

    // The heads last prefetched, so each landing is prefetched once, and again after a prefetch that failed.
    let ahead: string | null = null
    // The first reading's end: every prefetch waits for it, so the first draw reads first.
    let ended = (): void => {}
    const firstRead = new Promise<void>(done => (ended = done))
    // The plan's content the device lacks, fetched behind every reading's fetch, one answer at a time.
    const prefetch = serial.of(async (): Promise<void> => {
      await firstRead
      if (!idb.available()) return
      await fetching("Ahead", (await planned()).ahead).catch(() => {
        ahead = null
      })
    })
    const prefetched = (heads: string): void => {
      if (ahead === heads) return
      ahead = heads
      void prefetch()
    }

    // Every frame up to `after` is held; the frames this tab appended are passed over when it reads the log again.
    let after = 0
    const mine = new Set<number>()
    const take = (frames: readonly replicalog.Frame[]): void => {
      for (const f of frames) {
        if (!mine.has(f.at)) moved(held.apply(f.bytes))
        after = Math.max(after, f.at)
      }
    }
    // The core loaded from a snapshot and the frames after it: what the load moved, or none for a snapshot of another
    // form, which drops the log, so the lines are taken again from the box.
    const loaded = async (kept: replicalog.Kept): Promise<string | null> => {
      try {
        const said = held.load(kept.snapshot?.bytes, framed(kept.frames.map(f => f.bytes)))
        after = kept.frames.at(-1)?.at ?? kept.snapshot?.through ?? 0
        mine.clear()
        return said
      } catch {
        const gone = await log.drop()
        told(gone.type === "Error" ? `the lines kept on this device are of another build and were not dropped (${gone.e})` : null)
        return null
      }
    }
    // A pull, a purge, a compaction and a read of the log take turns. ⚠️ No frame of another tab is taken while a pull
    // runs: another tab's landing replaces the pieces this pull kept and has not landed. A pull's own purge runs in the
    // pull's turn, and its compaction once the pull ends.
    let lane: Promise<unknown> = Promise.resolve()
    const inTurn = <A>(run: () => Promise<A>): Promise<A> => {
      const now = lane.then(run, run)
      lane = now.catch(() => undefined)
      return now
    }
    let pulling = false
    let compactOwed = false

    // What another tab wrote since this one last read the log, one read at a time.
    const caughtUp = serial.of(async (): Promise<void> => {
      if (!idb.available()) return
      const kept = await log.since(after)
      if (kept.type === "Error") return told(kept.e)
      if (kept.v.snapshot === null) return take(kept.v.frames)
      const said = await loaded(kept.v)
      if (said !== null) moved(said)
    })

    // Where another tab's compaction came first, its snapshot is read, and this compacts again.
    let quiet: ReturnType<typeof setTimeout> | undefined
    const compactNow = serial.of(async (): Promise<void> => {
      clearTimeout(quiet)
      quiet = undefined
      if (!idb.available()) return
      for (let tries = 0; tries < 2; tries += 1) {
        const done = await log.compact(after, take, () => held.snapshot(), () => husksOf(held), kept => blocks.swept(kept, admitsOf(held, kept)))
        if (done.type === "Error") return
        if (done.v.type === "Through") {
          after = done.v.at
          mine.clear()
          phases.mark("lines snapshot written")
          return
        }
        await caughtUp()
      }
    })
    // A compaction in its own turn: when the lines are quiet, or the page is hidden.
    const compact = (): Promise<void> => inTurn(compactNow)
    const owe = (): void => {
      clearTimeout(quiet)
      quiet = setTimeout(() => void compact(), QUIET_MS)
    }
    const append = async (bytes: Uint8Array): Promise<void> => {
      if (!idb.available()) return
      const at = await log.append(bytes)
      if (at.type === "Error") return told(`a move of the lines was not kept on this device (${at.e}); the next load takes it again from the box`)
      mine.add(at.v)
      owe()
    }
    // A step the core took, its moves heard and its frame kept.
    const stepped = async (step: { readonly delta: Uint8Array; readonly moved: string; readonly free: () => void }): Promise<void> => {
      const [delta, said] = [step.delta, step.moved]
      step.free()
      moved(said)
      await append(delta)
    }

    // A purge that could not forget is said, once per change, and its keys stay owed until it can.
    let owed: string | null = null
    const owing = (failed: string | null): void => {
      if (failed === owed) return
      owed = failed
      fx.told(
        `purge/${KEPT}`,
        excluded.left(excluded.NONE, [], failed === null ? [] : [{ chain: KEPT, why: `the blocks some erased words were rendered to are still on this device (${failed}); it tries again at the next sync`, cure: "None" }]),
      )
    }
    // Every dead key let go: its lines' content fetched, so each line it leaves erased is husked; the compaction then
    // leaves no frame holding the key and deletes the husked lines' prints and every block the core no longer admits.
    const purge = serial.of(async (): Promise<void> => {
      const owed = owedOf(held, [])
      const names = Object.keys(owed)
      if (names.length === 0 && !held.husking()) return owing(null)
      try {
        await fetching("First", lackingIn(held, { type: "Naming", keys: names }))
      } catch (e: unknown) {
        return owing(lo.reason(e))
      }
      const found = owedOf(held, [])
      // ⚠️ Kept as a frame first: a tab that compacts without it would write the keys back.
      await stepped(held.forget(JSON.stringify(Object.keys(found))))
      if (pulling) compactOwed = true
      else await compactNow()
      owing(null)
    })
    // A purge reads the erasures in the prints, so it runs again as more come.
    let purgedAt = -1
    const purgeSoon = (): void => {
      const v = held.version()
      if (v === purgedAt) return
      purgedAt = v
      // On a later task, so the reading's answer goes out first.
      setTimeout(() => void inTurn(purge), 0)
    }

    // One read of the device's log per page: the core loaded from it, before anything reads the lines.
    const restored = once.of(async (): Promise<void> => {
      phases.mark("lines restore")
      if (idb.available()) {
        const kept = await log.read()
        if (kept.type === "Error") told(kept.e)
        else await loaded(kept.v)
      }
      phases.mark("lines restored")
      // A wake lost is caught up before the next pull.
      log.heard(() => void inTurn(caughtUp))
    })

    let landing = awaited()
    // A later epoch overtook what is held: every row reset, and a wait on a landing waits on the next.
    const startOver = async (): Promise<void> => {
      await stepped(held.start_over())
      const was = landing
      landing = awaited()
      was.done("Replaced")
      fx.replaced()
      settled()
    }

    // After a landing: the content outside the plan let go, and the plan prefetched once a page reads the lines.
    const landedNow = async (): Promise<void> => {
      const gone = await content.keepOnly(new Set((await planned()).keep), heldOf(held), drop => Promise.resolve(held.hold("[]", JSON.stringify(drop))))
      if (gone.type === "Error") fx.told(`row/${replicasync.kept("lines")}`, replicasync.unread("lines", gone.e))
      if (wanted === "Yes") prefetched(held.heads())
    }

    let wait = AGAIN_MS.first
    let timer: ReturnType<typeof setTimeout> | undefined
    const retry = (): void => {
      clearTimeout(timer)
      timer = setTimeout(() => void (wanted === "Yes" ? pullNow() : undefined), wait)
      wait = Math.min(wait * 2, AGAIN_MS.most)
    }
    const settled = (): void => {
      clearTimeout(timer)
      wait = AGAIN_MS.first
    }

    // By pieces, each kept on the device as it comes; the purge runs before they land, so no reading reads a row whose
    // keys are owed, and a piece of a later epoch starts the store over first.
    const pullNow = serial.of(async (): Promise<boolean> => {
      await restored.get()
      const ended = await inTurn(async (): Promise<pieces.Ended> => {
        try {
          await caughtUp()
          pulling = true
          return await pieces.pull<SkeletonPayload>({
            ask: async () => {
              const a = await storereads.skeleton(held.since())
              if (!held.overtaken(a.epoch)) return a
              await startOver()
              return storereads.skeleton(held.since())
            },
            keep: async a => {
              fx.told(`epoch/${KEPT}`, excluded.behind(KEPT, held.went_back(a.epoch)))
              fx.bars({ type: "Piece", name: NAME, size: a.size, weight: a.weight })
              await append(held.piece(JSON.stringify(a), JSON.stringify(a.keys), time.now()))
            },
            land: async () => {
              await purge()
              await stepped(held.land())
              landing.done("Landed")
              await landedNow()
              await purge()
            },
            held: () => [held.since(), held.heads()],
          })
        } catch (e: unknown) {
          return { type: "Cut", taken: 0, e }
        } finally {
          pulling = false
        }
      })
      if (compactOwed) {
        compactOwed = false
        await compact()
      }
      fx.bars({ type: "Pulled", name: NAME })
      if (ended.type === "Whole") {
        settled()
        return true
      }
      retry()
      return false
    })

    // Whether a reading wants the lines yet: before one does, nothing pulls them.
    let wanted: "No" | "Yes" = "No"
    const want = (): Promise<boolean> => {
      wanted = "Yes"
      return pullNow()
    }
    // A replaced landing is waited on through its replacement; a landed one ends the wait.
    const whole = (): Promise<void> => (held.heard() ? Promise.resolve() : landing.once.then(how => (how === "Landed" ? undefined : whole())))

    const over = async <A>(scope: () => lines.Scope, read: () => A): Promise<A> => {
      const lane = scope().type === "Whole" ? "Behind" : "First"
      try {
        for (;;) {
          const need = lackingIn(held, scope())
          phases.mark(`lines ${lane} scoped`)
          if (need.length === 0) {
            const answer = read()
            purgeSoon()
            return answer
          }
          await fetching(lane, need)
        }
      } finally {
        ended()
      }
    }

    return {
      engine: {
        want,
        wanted: () => wanted === "Yes",
        heard: (box: heads.Heads) => {
          if (wanted === "Yes" && held.behind(JSON.stringify(box))) void pullNow()
        },
        // A copy that does not read, or one the owner asked to take again: the log dropped and the lines started over.
        reset: async () => {
          const gone = await log.drop()
          after = 0
          mine.clear()
          await startOver()
          await want()
          return gone.type === "Error" ? [`the reset was not kept: ${gone.e}`] : []
        },
        sync: { restored, landed: () => held.heard(), whole },
      },
      over,
      holds: id => held.holds(id),
      shown: on => {
        shown = on
        if (!on) return compact()
        fetcher.wake()
        return Promise.resolve()
      },
      lacking: scope => lacksIn(held, scope).lines,
      admit: {
        admits: async (ks, s) => ((await log.behind(s, after, mine)) ? "Behind" : admitsOf(held, ks)),
        behind: () => inTurn(caughtUp),
      },
    }
  }
}
export default linesync
