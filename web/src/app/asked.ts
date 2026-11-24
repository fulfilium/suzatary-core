// What the tab asks the box and holds no replica of: one register per question, its answers ordered by the ask that
// brought each, so a late answer to an earlier ask is never shown and a failed ask leaves the answer that stands.
// Asked when first read, when read again after nobody read it, and when the box moves; one nobody reads is never asked.

import data from "../algebra/data"
import lo from "../algebra/lo"
import type reading from "../algebra/reading"
import time from "./time"

export namespace asked {
  type Stamped<A> = { readonly n: number; readonly v: A }
  // An answer, with when it landed: data, never part of the order.
  type Landed<A> = Stamped<A> & { readonly at: number }

  export type Register<V> = { readonly answer: Landed<V> | null; readonly asked: number; readonly failed: Stamped<string> | null }

  export const empty: Register<never> = { answer: null, asked: 0, failed: null }

  const newer = <A, S extends Stamped<A>>(a: S | null, b: S | null): S | null => (a === null ? b : b === null || a.n >= b.n ? a : b)

  // A semilattice: each part is the greater of the two, so registers join in any order to one.
  export const join = <V>(a: Register<V>, b: Register<V>): Register<V> => ({
    answer: newer(a.answer, b.answer),
    asked: Math.max(a.asked, b.asked),
    failed: newer(a.failed, b.failed),
  })

  export const asking = <V>(r: Register<V>, n: number): Register<V> => join(r, { ...empty, asked: n })
  export const answer = <V>(r: Register<V>, n: number, v: V, at: number): Register<V> => join(r, { ...empty, answer: { n, v, at } })
  export const fail = <V>(r: Register<V>, n: number, e: string): Register<V> => join(r, { ...empty, failed: { n, v: e } })

  // An answer stands; with none, the newest ask's failure is said, else it is on its way.
  export const read = <V>(r: Register<V>): lo.T<V> =>
    r.answer !== null ? lo.val(r.answer.v) : r.failed !== null && r.failed.n === r.asked ? lo.error(r.failed.v) : lo.loading

  // Whether what is held may be behind: the newest ask failed after an answer stood, or, `slow`, one is long on its way.
  export const behind = <V>(r: Register<V>, slow: boolean): reading.Behind | null =>
    r.answer === null ? null
    : r.failed !== null && r.failed.n === r.asked && r.failed.n > r.answer.n ? { why: r.failed.v, since: r.answer.at }
    : slow && r.asked > r.answer.n ? { why: null, since: r.answer.at }
    : null

  // An ask on its way this long is said to be; bounding it is the request's (`wire/wait.ts`), never the register's,
  // since an ask may be the tab's own work too.
  export const SLOW_MS = 1500

  let count = 0
  const families = new Set<() => void>()
  const ears = new Set<() => void>()

  // Unread registers kept per shape: one evicted is asked again when read, so the bound never changes what is read.
  export const KEPT = 64

  // A register per question, named by `print`; an answer `same` as the one standing moves nothing and wakes nobody.
  export const family = <A, V>(
    print: (a: A) => string,
    ask: (a: A) => Promise<V>,
    same: (x: V, y: V) => boolean = data.same,
  ): ((a: A) => reading.Reading<V>) => {
    // `asking` an ask in flight, `due` that the box moved since it was made: one more is made when it lands.
    type Cell = { r: Register<V>; seen: lo.T<V>; said: reading.Behind | null; readonly ears: Set<() => void>; readonly a: A; asking: boolean; due: boolean; slow: boolean }
    const cells = new Map<string, Cell>()
    const alike = (x: lo.T<V>, y: lo.T<V>): boolean =>
      x.type === "Val" && y.type === "Val" ? same(x.v, y.v) : x.type === "Error" && y.type === "Error" ? x.e === y.e : x.type === y.type
    // What a reader is handed stays the same object while it reads the same.
    const told = (c: Cell): void => {
      const now = read(c.r)
      const said = behind(c.r, c.slow)
      if (alike(now, c.seen) && data.same(said, c.said)) return
      if (!alike(now, c.seen)) c.seen = now
      c.said = said
      for (const ear of [...c.ears]) ear()
    }
    // Asks made while one is in flight are one, made when it lands: the order makes many safe, this makes them few.
    const again = (c: Cell): void => {
      if (c.asking) return void (c.due = true)
      const n = (count += 1)
      c.asking = true
      c.r = asking(c.r, n)
      told(c)
      const slow = setTimeout(() => ((c.slow = true), told(c)), SLOW_MS)
      const landed = (r: Register<V>): void => {
        clearTimeout(slow)
        c.r = r
        c.asking = false
        c.slow = false
        told(c)
        if (c.due) {
          c.due = false
          again(c)
        }
      }
      new Promise<V>(ok => ok(ask(c.a))).then(
        v => landed(answer(c.r, n, v, time.now())),
        (e: unknown) => landed(fail(c.r, n, lo.reason(e))),
      )
    }
    // The least recently read first, never one being read.
    const shrink = (): void => {
      for (const [name, c] of cells) {
        if (cells.size <= KEPT) return
        if (c.ears.size === 0) cells.delete(name)
      }
    }
    families.add(() => cells.forEach(c => c.ears.size > 0 && again(c)))
    return a => {
      const name = print(a)
      const cell = (): Cell => {
        const held = cells.get(name)
        cells.delete(name)
        const c: Cell = held ?? { r: empty, seen: lo.loading, said: null, ears: new Set(), a, asking: false, due: false, slow: false }
        cells.set(name, c)
        if (held === undefined) shrink()
        return c
      }
      return {
        now: () => cell().seen,
        hear: moved => {
          const c = cell()
          c.ears.add(moved)
          if (c.ears.size === 1) again(c)
          return () => void c.ears.delete(moved)
        },
        again: () => again(cell()),
        behind: () => cell().said,
      }
    }
  }

  export const question = <V>(ask: () => Promise<V>, same?: (x: V, y: V) => boolean): reading.Reading<V> =>
    family<null, V>(() => "", ask, same)(null)

  // The box moved: every question a page reads is asked again, and what reads the box another way hears it.
  export const moved = (): void => (families.forEach(f => f()), ears.forEach(e => e()))

  export const hearMoves = (ear: () => void): (() => void) => (ears.add(ear), () => void ears.delete(ear))
}
export default asked
