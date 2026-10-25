// Work drawn in slices: a unit at least each slice and units for at most `BUDGET_MS`, each slice a task of its own, the
// first after the task that asked for it. A hidden tab's timers are throttled, so there it is drawn in one pass, and a
// tab hidden mid-way draws the rest at once.

export namespace slices {
  export const BUDGET_MS = 4

  export type Clock = {
    readonly hidden: () => boolean
    readonly now: () => number
    // `f` in a task of its own; the answer cancels it.
    readonly later: (f: () => void) => () => void
  }

  // One unit drawn, answering whether more are left, and what follows the last.
  export type Work = { readonly unit: () => boolean; readonly done: () => void }

  export type T = {
    // `w` drawn, replacing the work running.
    readonly run: (w: Work) => void
    // The work running drawn to its end now.
    readonly flush: () => void
    readonly cancel: () => void
  }

  export const browser: Clock = {
    hidden: () => typeof document !== "undefined" && document.hidden,
    now: () => performance.now(),
    later: f => {
      const t = setTimeout(f, 0)
      return () => clearTimeout(t)
    },
  }

  export const of = (clock: Clock = browser): T => {
    let pending: { readonly w: Work; readonly cancel: () => void } | null = null
    const slice = (w: Work, whole: boolean): void => {
      pending = null
      const started = clock.now()
      let more = w.unit()
      while (more && (whole || clock.hidden() || clock.now() - started <= BUDGET_MS)) more = w.unit()
      if (!more) return w.done()
      pending = { w, cancel: clock.later(() => slice(w, false)) }
    }
    const cancel = (): void => {
      pending?.cancel()
      pending = null
    }
    return {
      run: w => {
        cancel()
        if (clock.hidden()) return slice(w, true)
        pending = { w, cancel: clock.later(() => slice(w, false)) }
      },
      flush: () => {
        const p = pending
        if (p === null) return
        p.cancel()
        slice(p.w, true)
      },
      cancel,
    }
  }
}
export default slices
