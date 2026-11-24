// The clock as a source, observed: a reading names the next instant what it shows moves (push), and is read
// again then (pull), never on a period.

import time from "../app/time"

export namespace clock {
  // Epoch milliseconds on this device's clock; not `exact` is a bound that may come before anything moves.
  export type Next = { readonly type: "Never" } | { readonly type: "At"; readonly ms: number; readonly exact: boolean }

  export const never: Next = { type: "Never" }
  export const at = (ms: number, exact = true): Next => ({ type: "At", ms, exact })

  export const fold = <R>(n: Next, k: { readonly Never: () => R; readonly At: (ms: number, exact: boolean) => R }): R =>
    n.type === "At" ? k.At(n.ms, n.exact) : k.Never()

  // The earlier, and at one instant exact where either is: a meet-semilattice whose top, `never`, is its identity.
  const meet = (a: Next, b: Next): Next =>
    fold(a, {
      Never: () => b,
      At: (x, e) => fold(b, { Never: () => a, At: (y, f) => (x < y ? a : y < x ? b : at(x, e || f)) }),
    })

  export const sooner = (...ns: readonly Next[]): Next => ns.reduce(meet, never)

  // An instant is a point in time, so it is armed on this device's clock as it is.
  export const of = (next: { readonly at: string; readonly exact: boolean } | null): Next => {
    const ms = next === null ? null : time.ms(next.at)
    return ms === null || next === null ? never : at(ms, next.exact)
  }

  // The owner's next midnight after `ms`, when what is printed to the day (an age, today's date) moves.
  export const nextDay = (zone: time.Zone, ms: number): Next => at(time.nextDay(zone, ms))

  // A conservative bound asked again at once could come back at once again (a window over a moving term answers
  // "now"), so it is paced from the first wait, doubling to the longest; an exact answer is waited for exactly.
  export const FIRST_PACE_MS = 1000
  export const LONGEST_PACE_MS = 60_000

  export type Pacer = { readonly last: number; readonly pace: number }

  export const START: Pacer = { last: -Infinity, pace: FIRST_PACE_MS }

  const heldBack = (n: Next, p: Pacer): boolean => fold(n, { Never: () => false, At: (ms, exact) => !exact && ms < p.last + p.pace })

  // A bound sooner than the pace after the last wake waits for it.
  // A function of the answer and the wakes that fired, never of when it is asked, so arming again on every change of
  // the store moves nothing.
  export const paced = (n: Next, p: Pacer): Next => (heldBack(n, p) ? at(p.last + p.pace, false) : n)

  // A wake the pace held back doubles it, any other resets it.
  export const fired = (n: Next, p: Pacer, now: number): Pacer => ({
    last: now,
    pace: heldBack(n, p) ? Math.min(2 * p.pace, LONGEST_PACE_MS) : FIRST_PACE_MS,
  })

  export type Timers = {
    readonly now: () => number
    readonly set: (k: () => void, ms: number) => number
    readonly clear: (h: number) => void
  }

  export const browser: Timers = { now: () => Date.now(), set: (k, ms) => window.setTimeout(k, ms), clear: h => window.clearTimeout(h) }

  // One timer reaches no further than 2³¹ − 1 ms; a wake beyond it re-arms from there.
  const FURTHEST = 2 ** 31 - 1

  // Returns the disarm; a timer counts only the time this device is awake, so a caller arms again after sleep.
  export const wake = (n: Next, k: () => void, fx: Timers = browser): (() => void) => {
    let h: number | null = null
    const arm = (ms: number): void => {
      const wait = Math.max(0, ms - fx.now())
      h = wait > FURTHEST ? fx.set(() => arm(ms), FURTHEST) : fx.set(() => ((h = null), k()), wait)
    }
    fold(n, { Never: () => undefined, At: arm })
    return () => {
      if (h !== null) fx.clear(h)
      h = null
    }
  }

  // Arming disarms the last wake; only the wakes that fired move the pace.
  export const pacer = (fx: Timers = browser): ((n: Next, k: () => void) => void) => {
    let p = START
    let disarm = (): void => {}
    return (n, k) => {
      disarm()
      disarm = wake(paced(n, p), () => ((p = fired(n, p, fx.now())), k()), fx)
    }
  }
}
export default clock
