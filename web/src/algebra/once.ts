// `of` keeps a success and forgets a failure, so the next call asks again; `shared` keeps a run only while it is in
// flight, so a call during it joins it and a call after it starts another.

export namespace once {
  export type T<A> = { readonly get: () => Promise<A>; readonly forget: () => void }

  // A run that throws before it has a promise is a failed run, never a thrown call.
  const start = <A>(run: () => Promise<A>): Promise<A> => new Promise<A>(ok => ok(run()))

  export const of = <A>(run: () => Promise<A>): T<A> => {
    let held: Promise<A> | null = null
    const get = (): Promise<A> => {
      if (held !== null) return held
      const p = start(run)
      held = p
      p.catch(() => void (held === p && (held = null)))
      return p
    }
    return { get, forget: () => void (held = null) }
  }

  export const shared = <A>(run: () => Promise<A>): (() => Promise<A>) => {
    let running: Promise<A> | null = null
    return () => {
      if (running !== null) return running
      const p = start(run)
      running = p
      const done = (): void => void (running === p && (running = null))
      p.then(done, done)
      return p
    }
  }
}
export default once
