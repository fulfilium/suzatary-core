// The order the core's worker serves its one thread in: the page on screen first, the shell around it after. A question
// starts, or goes on after waiting on a store or the network, only while no earlier lane has one waiting or being
// answered, and within its lane in the order it came.

export namespace lanes {
  // The order they are served in.
  export type Lane = "Page" | "Shell"
  export const FIRST: Lane = "Page"
  export const LAST: Lane = "Shell"
  export const ORDER: readonly Lane[] = [FIRST, LAST]

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

  // A reading is asked in the earliest lane of the programs watching it now (the page on screen's, then a program's
  // behind it, as the Inbox's badge), the last while none does.
  export const watchers = () => {
    const now = new Map<string, Set<{ readonly lane: Lane }>>()
    return {
      watch: (key: string, lane: Lane): (() => void) => {
        const one = { lane }
        now.set(key, (now.get(key) ?? new Set()).add(one))
        return () => void now.get(key)?.delete(one)
      },
      laneOf: (key: string): Lane => ORDER.find(l => [...(now.get(key) ?? [])].some(w => w.lane === l)) ?? LAST,
    }
  }

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
