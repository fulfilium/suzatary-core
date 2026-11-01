// A reading: what the tab holds of something now, on its way or here, and a way to hear it move; a page is a view
// of one. `again` asks once more what can be asked (the box), and is nothing for what the tab already holds;
// `behind` is whether what is held may be behind and since when it stands, null when nothing is.

import lo from "./lo"
import kept from "./once"

export namespace reading {
  // What is shown may be behind: an ask failed after it (`why`), or one has been long on its way (`why` null).
  // `since` is when the answer shown landed, as the tab's clock read it.
  export type Behind = { readonly why: string | null; readonly since: number }

  export type Reading<V> = {
    readonly now: () => lo.T<V>
    readonly hear: (moved: () => void) => () => void
    readonly again: () => void
    readonly behind: () => Behind | null
  }

  const nothing = (): void => {}
  const none = (): null => null

  // `f` read through its last argument: the same argument, the same object back, so a reading that did not move
  // hands its reader what it handed before and wakes nothing downstream.
  const last = <A, B>(f: (a: A) => B): ((a: A) => B) => {
    let seen: { readonly a: A; readonly b: B } | null = null
    return a => (seen !== null && Object.is(seen.a, a) ? seen.b : (seen = { a, b: f(a) }).b)
  }

  export const constant = <V>(v: V): Reading<V> => {
    const l = lo.val(v)
    return { now: () => l, hear: () => nothing, again: nothing, behind: none }
  }

  // A value the tab holds and hears move: a store, a register.
  export const held = <V>(now: () => V, hear: (moved: () => void) => () => void): Reading<V> => {
    const val = last((v: V) => lo.val(v))
    return { now: () => val(now()), hear, again: nothing, behind: none }
  }

  // What arrives once and stays (code the tab fetches, never a reading of the box): asked at the first hearing, and
  // asked again after a failure.
  export const once = <V>(arrive: () => Promise<V>): Reading<V> => {
    const arrived = kept.of(arrive)
    let now: lo.T<V> = lo.loading
    const ears = new Set<() => void>()
    const settle = (l: lo.T<V>): void => ((now = l), [...ears].forEach(e => e()))
    // One settle per run, however many hear it while it is on its way.
    const run = kept.shared(() => lo.of(arrived.get()).then(settle))
    const ask = (): void => void (now.type !== "Val" && run())
    return {
      now: () => now,
      hear: moved => (ears.add(moved), ask(), () => void ears.delete(moved)),
      again: ask,
      behind: none,
    }
  }

  // The same reading, asked again whenever `moved` says what it reads has moved.
  export const againOn = <V>(r: Reading<V>, moved: (heard: () => void) => () => void): Reading<V> => ({
    ...r,
    hear: ear => {
      const [u, w] = [r.hear(ear), moved(r.again)]
      return () => (u(), w())
    },
  })

  // The same reading, handing back the value it handed before while what it reads now is `same` as that.
  export const settled = <V>(r: Reading<V>, same: (a: V, b: V) => boolean): Reading<V> => {
    let was: lo.T<V> | null = null
    const now = (): lo.T<V> => {
      const l = r.now()
      return (was = was !== null && was.type === "Val" && l.type === "Val" && same(was.v, l.v) ? was : l)
    }
    return { ...r, now }
  }

  // A functor: `map(r, x => x)` reads as `r`, and maps compose.
  export const map = <A, B>(r: Reading<A>, f: (a: A) => B): Reading<B> => {
    const read = last((l: lo.T<A>) => lo.map(l, f))
    return { now: () => read(r.now()), hear: r.hear, again: r.again, behind: r.behind }
  }

  // The same reading never waited on: its value is where it stands (on its way, failed or here), so a page reading it
  // beside another draws once the other arrives, and this part fills in when it does.
  export const unwaited = <V>(r: Reading<V>): Reading<lo.T<V>> => {
    const read = last((l: lo.T<V>): lo.T<lo.T<V>> => lo.val(l))
    return { now: () => read(r.now()), hear: r.hear, again: r.again, behind: r.behind }
  }

  // An applicative on two readings, of `lo.combine`'s monoid: the first failure, else a wait, else both.
  export const both = <A, B>(a: Reading<A>, b: Reading<B>): Reading<readonly [A, B]> => {
    const pair = (x: lo.T<A>, y: lo.T<B>): lo.T<readonly [A, B]> =>
      x.type === "Error" ? x : y.type === "Error" ? y : x.type === "Loading" ? x : lo.map(y, (w): readonly [A, B] => [x.v, w])
    const read = last((x: lo.T<A>) => last((y: lo.T<B>) => pair(x, y)))
    return {
      now: () => read(a.now())(b.now()),
      hear: moved => {
        const [u, w] = [a.hear(moved), b.hear(moved)]
        return () => (u(), w())
      },
      again: () => (a.again(), b.again()),
      behind: () => {
        const [x, y] = [a.behind(), b.behind()]
        return x === null ? y : y === null ? x : { why: x.why ?? y.why, since: Math.min(x.since, y.since) }
      },
    }
  }
}
export default reading
