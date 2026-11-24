// A register's value is the discrete order's completion of its FRONTIER, the writes no other write descends from
// (`of`, for every register in the tab). A ui register in this browser carries its order as version vectors (one
// writer per tab makes each replica's writes a chain), and the join of two is the frontier of their union, so a
// write two replicas made unseen by each other is a conflict the reading shows, never a write lost.

import lo from "./lo"
import d from "../wire/decoder"

export namespace register {
  export type Clock = { readonly [replica: string]: number }
  // `by` and `n` name the write (its dot); `seen` is its past, itself included.
  export type Write<A> = { readonly by: string; readonly n: number; readonly seen: Clock; readonly v: A }
  // An antichain under descent, in dot order, so equal sets are equal arrays.
  export type T<A> = readonly Write<A>[]
  export type Reading<A> =
    | { readonly type: "Unwritten" }
    | { readonly type: "Value"; readonly v: A }
    | { readonly type: "Conflict"; readonly vs: readonly A[] }

  export const EMPTY: T<never> = []

  const same = <A>(w: Write<A>, u: Write<A>): boolean => w.by === u.by && w.n === u.n
  const descends = <A>(w: Write<A>, u: Write<A>): boolean => !same(w, u) && (w.seen[u.by] ?? 0) >= u.n
  const order = <A>(w: Write<A>, u: Write<A>): number => (w.by < u.by ? -1 : w.by > u.by ? 1 : w.n - u.n)

  const frontier = <A>(ws: readonly Write<A>[]): T<A> =>
    ws
      .filter((w, i) => ws.findIndex(u => same(u, w)) === i)
      .filter(u => !ws.some(w => descends(w, u)))
      .sort(order)

  // A semilattice: idempotent, commutative, associative, `EMPTY` its unit.
  export const join = <A>(a: T<A>, b: T<A>): T<A> => frontier([...a, ...b])

  const past = <A>(r: T<A>): Clock =>
    r.reduce<Clock>((c, w) => Object.fromEntries([...new Set([...Object.keys(c), ...Object.keys(w.seen)])].map(k => [k, Math.max(c[k] ?? 0, w.seen[k] ?? 0)])), {})

  // A write by `by` that saw `r` (what its writer read), and none of `by`'s own writes past `after`: it descends from
  // every write in `r` and from nothing else.
  export const write = <A>(r: T<A>, by: string, v: A, after = 0): T<A> => {
    const seen = past(r)
    const n = Math.max(seen[by] ?? 0, after) + 1
    return [{ by, n, seen: { ...seen, [by]: n }, v }]
  }

  // Each write's past cut to the replicas the frontier holds: every reading and verdict stays, but a superseded write
  // arriving again misreads, so only a store no superseded write re-enters may keep it cut.
  export const cut = <A>(r: T<A>): T<A> =>
    r.map(w => ({ ...w, seen: Object.fromEntries(r.flatMap((u): [string, number][] => ((k = w.seen[u.by]) => (k === undefined ? [] : [[u.by, k]]))())) }))

  // The highest write `by` made that `r` holds or descends from: where its next write's count starts.
  export const latest = <A>(r: T<A>, by: string): number => past(r)[by] ?? 0

  // THE reading: a frontier's values in the discrete order's completion, twins once, in the frontier's order.
  export const of = <A>(frontier: readonly A[], print: (a: A) => string = a => JSON.stringify(a)): Reading<A> => {
    const vs = frontier.filter((v, i, all) => all.findIndex(u => print(u) === print(v)) === i)
    const [v, ...more] = vs
    return v === undefined ? { type: "Unwritten" } : more.length === 0 ? { type: "Value", v } : { type: "Conflict", vs }
  }

  export const read = <A>(r: T<A>, print?: (a: A) => string): Reading<A> => of(r.map(w => w.v), print)

  export const candidates = <A>(g: Reading<A>): readonly A[] => (g.type === "Unwritten" ? [] : g.type === "Value" ? [g.v] : g.vs)

  // What a place that can show one value shows: the value, `initial` where unwritten, and in a conflict the
  // candidate of the least dot; a reader that can show the conflict reads `read` instead.
  export const one = <A>(g: Reading<A>, initial: A): A => candidates(g)[0] ?? initial

  // Stored bytes are input from outside: a write set that does not parse reads as nothing written.
  const count: d.Decoder<number> = d.bind(d.num, n => (Number.isInteger(n) && n > 0 ? lo.val(n) : lo.error(`${n} is not a count`)))
  const written = <A>(value: d.Decoder<A>): d.Decoder<Write<A>> =>
    d.bind(d.obj<Write<A>>({ by: d.str, n: count, seen: d.rec(count), v: value }), w => (w.seen[w.by] === w.n ? lo.val(w) : lo.error(`${w.by}'s write has not seen itself`)))

  export const stored = <A>(value: d.Decoder<A>): d.Decoder<T<A>> => d.map(d.arr(written(value)), frontier)
}
export default register
