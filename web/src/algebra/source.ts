// A page's state is a value; an event is the only way it changes, folded in by a pure step.


export namespace source {
  export type Step<S, E> = (s: S, e: E) => S
  export type Listener<S, E> = (s: S, e: E) => void
  export type Source<S, E> = {
    readonly state: () => S
    readonly subscribe: (l: Listener<S, E>) => () => void
    readonly dispatch: (e: E) => void
  }

  // The listener gets the event too, because a page may redraw per event kind; a step returning the same value
  // (`Object.is`) wakes nobody.
  export const create = <S, E>(initial: S, step: Step<S, E>): Source<S, E> => {
    let s = initial
    const ls = new Set<Listener<S, E>>()
    return {
      state: () => s,
      subscribe: l => (ls.add(l), () => void ls.delete(l)),
      dispatch: e => {
        const next = step(s, e)
        if (Object.is(next, s)) return
        s = next
        for (const l of [...ls]) l(s, e)
      },
    }
  }
}
export default source

