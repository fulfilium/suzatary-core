// The core's worker serves one thread, and this is the order it serves it in: the page on screen first, the shell
// around it after. A question of a lane starts only while every earlier lane has nothing waiting and nothing being
// answered, and within a lane in the order it came. One that waited on a store or the network goes on under the same
// rule, so an answer of the shell that waited never runs while the page's is owed; it waits on no question of its own
// lane, which it does not overtake.

export namespace lanes {
  // The order they are served in.
  export type Lane = "Page" | "Shell"
  export const FIRST: Lane = "Page"
  export const ORDER: readonly Lane[] = [FIRST, "Shell"]

  type Each<V> = { readonly [L in Lane]: V }

  // What waits in each lane, in the order it came, and how many of each lane's questions have started and not ended.
  export type Queue<A> = { readonly waiting: Each<readonly A[]>; readonly running: Each<number> }

  export const empty = <A>(): Queue<A> => ({ waiting: { Page: [], Shell: [] }, running: { Page: 0, Shell: 0 } })

  export const arrived = <A>(q: Queue<A>, lane: Lane, a: A): Queue<A> => ({ ...q, waiting: { ...q.waiting, [lane]: [...q.waiting[lane], a] } })

  export const started = <A>(q: Queue<A>, lane: Lane): Queue<A> => ({ ...q, running: { ...q.running, [lane]: q.running[lane] + 1 } })
  export const ended = <A>(q: Queue<A>, lane: Lane): Queue<A> => ({ ...q, running: { ...q.running, [lane]: q.running[lane] - 1 } })

  // Whether a question of `lane` may run now: every earlier lane has nothing waiting and nothing being answered.
  export const free = <A>(q: Queue<A>, lane: Lane): boolean => {
    for (const l of ORDER) {
      if (l === lane) return true
      if (q.waiting[l].length > 0 || q.running[l] > 0) return false
    }
    return true
  }

  // What to serve now, its lane, and the queue without it; or nothing, while what waits waits on an earlier lane.
  export type Next<A> = { readonly type: "Serve"; readonly a: A; readonly lane: Lane; readonly q: Queue<A> } | { readonly type: "Wait" }

  export const next = <A>(q: Queue<A>): Next<A> => {
    for (const lane of ORDER) {
      const [a, ...rest] = q.waiting[lane]
      if (a !== undefined) return { type: "Serve", a, lane, q: { ...q, waiting: { ...q.waiting, [lane]: rest } } }
      if (q.running[lane] > 0) return { type: "Wait" }
    }
    return { type: "Wait" }
  }
}
export default lanes
