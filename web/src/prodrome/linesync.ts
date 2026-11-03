// The lines as one store of the core's worker. The core holds their replica whole (`Lines`): the landed row and a pull's
// pieces, the box's word, the frontier, the keys, the prints, and every reading of them. This asks the box and the device
// for what the core lacks and hands it over, and keeps each move the core answers on the device, as the frame the core
// printed (`replicalog`); it holds no node, key or head of its own. A warm load hands the core its snapshot and the frames
// after it and reads no row. Another tab's frames are taken as they are read, each a join; a compaction, after a quiet,
// when the page is hidden, or once a purge let keys go, is the one way bytes leave the device, the prints of the lines
// the core holds as husks among them.

import type { Lines } from "./core"
import { framed, heldOf, holdAdmitted, husksOf, lackingIn, lackingOf, lacksIn, movedOf, owedOf, planOf } from "./core"
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
  // Which conversations a move touched: those with a line come or gone or a key of their words given or taken, or all of
  // them, when the row was reset or heard a later epoch.
  export type Moved = "All" | readonly string[]

  // Who hears the lines: a reading's leavings by reader, a pull's progress, a move, and the rendered blocks forgotten.
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: progress.Event) => void
    readonly moved: (conversations: Moved) => void
    readonly forgot: (blocks: readonly string[]) => void
    readonly replaced: () => void
  }

  export type T = {
    // The store as the set of stores asks each of its stores.
    readonly engine: replicasync.Store
    // `read` once every line `scope` reads is held, a conversation's fetched before the whole store's. The scope is
    // asked again after each fetch, of the skeleton then held.
    readonly over: <A>(scope: () => lines.Scope, read: () => A) => Promise<A>
    // Whether the lines hold any line of `id`.
    readonly holds: (id: string) => boolean
    // Whether a page is shown, as the page says: a prefetch sends nothing while none is, and sends what it held back
    // once one is again; a page hidden may be closed, so the log is compacted now, settling once it is.
    readonly shown: (on: boolean) => Promise<void>
    // How many lines `scope` reads that the skeleton names and the device does not hold.
    readonly lacking: (scope: lines.Scope) => number
  }

  const NAME = "lines"
  const KEPT = replicasync.kept(NAME)
  // How long the lines stay unmoved before their log is compacted: a burst of landings is compacted once.
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

  export const of = (held: Lines, fx: Effects, blocks: rendered.Store): T => {
    const log = replicalog.of(NAME, CONTENT)
    const told = (why: string | null): void => fx.told(`row/${KEPT}`, replicasync.unread(NAME, why))
    const moved = (said: string): void => fx.moved(movedOf(said))

    // What a purge takes from the rendered blocks, taken from the page's table as well as the device's.
    // A block forgotten here is gone from the device and marked there before the page hears of it, so no tab's worker,
    // whatever it heard, writes it back.
    const forget = (gone: readonly string[]): Promise<lo.Settled<null>> => blocks.forget(gone).then(done => (fx.forgot(gone), done))

    // Whether a page is shown; none is until one says so.
    let shown = false
    // The content, fetched by name and held by the core as it comes.
    const fetcher = content.fetcher({
      ask: hashes => storereads.content("lines", hashes),
      asked: bytes => fx.bars({ type: "Asked", name: "lines", bytes }),
      came: bytes => fx.bars({ type: "Came", name: "lines", bytes }),
      lost: bytes => fx.bars({ type: "Lost", name: "lines", bytes }),
      atOnce: () => content.atOnce(http.protocol()),
      lacking: names => lackingOf(held, names),
      held: rows => Promise.resolve(holdAdmitted(held, rows)),
      shown: () => shown,
    })
    const fetching = fetcher.fetch

    // The device's budget for the lines' content, read once from the origin's quota.
    const budget = once.of(async (): Promise<number> => {
      const s = typeof navigator === "undefined" ? undefined : navigator.storage
      const quota = s === undefined || typeof s.estimate !== "function" ? undefined : (await s.estimate().catch(() => undefined))?.quota
      return keep.budget(quota === undefined ? { type: "Unknown" } : { type: "Known", bytes: quota })
    }).get

    // The core's plan of the skeleton it holds, within the budget: what the device keeps of the lines' content (the plan,
    // and every print the core holds, which a reading loaded, since a prefetch hands the core nothing), and what of the
    // plan a prefetch fetches.
    const planned = async (): Promise<ReturnType<typeof planOf>> => planOf(held, await budget())

    // The heads last prefetched, so each landing is prefetched once, and again after a prefetch that failed.
    let ahead: string | null = null
    // The first reading's end, which every prefetch waits for: a prefetch reads the device's every name, and the first
    // draw reads before it.
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

    // Where this tab stands in the device's log: every frame up to `after` is held, and the frames it appended itself,
    // which the core already holds, are passed over when it reads the log again.
    let after = 0
    const mine = new Set<number>()
    const take = (frames: readonly replicalog.Frame[]): void => {
      for (const f of frames) {
        if (!mine.has(f.at)) moved(held.apply(f.bytes))
        after = Math.max(after, f.at)
      }
    }
    // The core loaded from a snapshot and the frames after it: what a warm load reads, and what a tab reads once another
    // tab's compaction folded frames it had not read; what the load moved, or none for a snapshot of another form, which
    // is a miss: the log is dropped and the lines are taken again from the box.
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
    // What another tab wrote since this one last read the log.
    const caughtUp = async (): Promise<void> => {
      if (!idb.available()) return
      const kept = await log.since(after)
      if (kept.type === "Error") return told(kept.e)
      if (kept.v.snapshot === null) return take(kept.v.frames)
      const said = await loaded(kept.v)
      if (said !== null) moved(said)
    }

    // The log compacted: the frames another tab wrote taken, then the core's snapshot kept as of the last frame and every
    // frame up to it deleted. Where another tab's compaction came first, its snapshot is read, and this compacts again.
    let quiet: ReturnType<typeof setTimeout> | undefined
    const compact = serial.of(async (): Promise<void> => {
      clearTimeout(quiet)
      quiet = undefined
      if (!idb.available()) return
      for (let tries = 0; tries < 2; tries += 1) {
        const done = await log.compact(after, take, () => held.snapshot(), () => husksOf(held))
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
    const owe = (): void => {
      clearTimeout(quiet)
      quiet = setTimeout(() => void compact(), QUIET_MS)
    }
    // A move the core answered, kept on the device as its frame.
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

    // A purge that could not forget is said, and asked again at the next sync; the keys it owes stay until it can. Said
    // only when it changes, since a purge runs after every reading whose prints moved.
    let owed: string | null = null
    const owing = (failed: string | null): void => {
      if (failed === owed) return
      owed = failed
      fx.told(
        `purge/${KEPT}`,
        excluded.left(excluded.NONE, [], failed === null ? [] : [{ chain: KEPT, why: `the blocks some erased words were rendered to are still on this device (${failed}); it tries again at the next sync`, cure: "None" }]),
      )
    }
    // Every dead key the core holds, landed or in a pull's pieces, let go: the content of the lines naming it fetched, so
    // the blocks its words fed are found; those blocks forgotten; then the key, from the core, and from the device by a
    // compaction, so no frame holds it; and every line each key of whose words is then gone kept as its husk, its print
    // deleted by the same compaction. A step that fails keeps the keys, owed.
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
      const forgot = await forget(Object.values(found).flat())
      if (forgot.type === "Error") return owing(forgot.e)
      // Kept as a frame first, so a tab that compacts before taking it is behind this one and takes it: its frame names
      // the keys and holds none, and the compaction that follows deletes it.
      await stepped(held.forget(JSON.stringify(Object.keys(found))))
      await compact()
      owing(null)
    })
    // The prints the core held at the last purge: a purge reads the erasures in the prints, so it is asked again as more
    // come.
    let purgedAt = -1
    const purgeSoon = (): void => {
      const v = held.version()
      if (v === purgedAt) return
      purgedAt = v
      // On a later task, so the reading's answer goes out first.
      setTimeout(() => void purge(), 0)
    }

    // One read of the device's log per page: the core loaded from it, before anything reads the lines.
    const restored = once.of(async (): Promise<void> => {
      phases.mark("lines restore")
      if (idb.available()) {
        const kept = await log.read()
        // Nothing reads the lines before they are restored, so what the load moved is told to no one.
        if (kept.type === "Error") told(kept.e)
        else await loaded(kept.v)
      }
      phases.mark("lines restored")
    })

    let landing = awaited()
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

    // By pieces, each asked from what the core holds, landed and not, and kept on the device as it comes; the purge runs
    // before the pieces land, so no reading reads a row whose keys are owed. A piece of a later epoch than what is held
    // starts the store over first. Another tab's frames are read before the pull, never between a question and its piece.
    const pullNow = serial.of(async (): Promise<boolean> => {
      await restored.get()
      const ended = await (async (): Promise<pieces.Ended> => {
        try {
          await caughtUp()
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
        }
      })()
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
    // A replaced landing is waited on again through the one that replaced it; a landed one ends the wait, even empty.
    const whole = (): Promise<void> => (held.heard() ? Promise.resolve() : landing.once.then(how => (how === "Landed" ? undefined : whole())))

    const over = async <A>(scope: () => lines.Scope, read: () => A): Promise<A> => {
      const lane = scope().type === "Whole" ? "Behind" : "First"
      try {
        for (;;) {
          const need = lackingIn(held, scope())
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
    }
  }
}
export default linesync
