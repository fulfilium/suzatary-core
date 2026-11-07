// Inputs taken one at a time: one sent during a turn waits until the turn ends, in the order sent.

export namespace mailbox {
  export type T<I> = {
    readonly send: (input: I) => void
    readonly turn: <R>(f: () => R) => R
  }

  export const of = <I>(take: (input: I) => void): T<I> => {
    const waiting: I[] = []
    let busy = false
    const turn = <R>(f: () => R): R => {
      if (busy) return f()
      busy = true
      try {
        const r = f()
        while (waiting.length > 0) for (const next of waiting.splice(0, 1)) take(next)
        return r
      } catch (e) {
        waiting.length = 0
        throw e
      } finally {
        busy = false
      }
    }
    return { send: input => (busy ? void waiting.push(input) : turn(() => take(input))), turn }
  }
}
export default mailbox
