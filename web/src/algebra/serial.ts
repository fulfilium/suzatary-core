// One run of an effect at a time and one queued behind it: a call during a run gets a run that starts after the
// call, so whatever the caller did before calling is in what answers it. Calls during one run share the run queued
// behind it, and a failed run still lets the queued one start.

export namespace serial {
  export const of = <A>(run: () => Promise<A>): (() => Promise<A>) => {
    let running: Promise<A> | null = null
    let behind: Promise<A> | null = null
    const start = (): Promise<A> => {
      // Kept while one is queued, so a call between this run settling and the queued one starting joins the queue.
      const now: Promise<A> = run().finally(() => void (behind === null && running === now && (running = null)))
      running = now
      return now
    }
    const next = (): Promise<A> => ((behind = null), start())
    return () => (running === null ? start() : (behind ??= running.then(next, next)))
  }
}
export default serial
