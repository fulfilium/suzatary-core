// The memoised fold over a content-addressed tree (the Prodrome's `memo`, design §6.2): `memo f = f`, with the
// algebra tabulated at every node by (its name, the node's name), so a change computes exactly its spine.

import table from "./table"

export namespace memo {
  // A node's name covers its own bytes and its children's names, so a change renames exactly its path to the root;
  // the host names nodes (the core's `memo_name`).
  export type Name = string
  export type Namer = (own: string, kids: Name[]) => Name

  export type Tree<A> = { readonly name: Name; readonly v: A; readonly kids: Tree<A>[] }

  // A namer read through a cache, keyed by the children's names and then the own bytes: names are fixed width,
  // so no two arguments print alike.
  export const tabulatedNamer = (named: Namer, cache: table.Table<Name>): Namer => (own, kids) => {
    const k = `${kids.join(" ")}\n${own}`
    const held = table.hit(cache.get(k))
    if (held !== undefined) return held
    const n = named(own, kids)
    cache.insert(k, n)
    return n
  }

  export const node = <A>(named: Namer, own: string, v: A, kids: Tree<A>[]): Tree<A> => ({ name: named(own, kids.map(k => k.name)), v, kids })

  // What a fold answers: anything but `undefined`, which a cache answers for a key it does not hold.
  export type Value = NonNullable<unknown>

  export type Layer<A, R extends Value> = { readonly name: Name; readonly v: A; readonly rs: readonly R[] }

  // An algebra applied to a wave of layers at once, `apply(ls) = ls.flatMap(l => apply([l]))`, so a step with a
  // fixed cost pays it once per wave. It is pure, and its name covers its code and everything that code reads.
  export type Algebra<A, R extends Value> = { readonly name: Name; readonly apply: (ls: readonly Layer<A, R>[]) => R[] }

  export const pointwise = <A, R extends Value>(name: Name, f: (v: A, rs: readonly R[]) => R): Algebra<A, R> => ({ name, apply: ls => ls.map(l => f(l.v, l.rs)) })

  // The meaning a cache may not change.
  export const plain = <A, R extends Value>(alg: Algebra<A, R>, t: Tree<A>): R =>
    one(alg.apply([{ name: t.name, v: t.v, rs: t.kids.map(k => plain(alg, k)) }]))

  const one = <R extends Value>(rs: readonly R[]): R => {
    const [r] = rs
    if (r === undefined || rs.length !== 1) throw new Error(`an algebra answered ${rs.length} results for one layer`)
    return r
  }

  // A finding is a key holding two values: computed, neither used, and reported.
  export type Outcome = "Hit" | "Computed" | "Finding"

  // A computed node's time is its share of its wave's.
  export type Visit = { readonly name: Name; readonly depth: number; readonly outcome: Outcome; readonly ms: number }
  export type Report = { readonly visits: readonly Visit[]; readonly waves: readonly { readonly nodes: number; readonly ms: number }[] }

  export type Counts = { readonly hits: number; readonly computed: number; readonly findings: number; readonly ms: number }
  const none: Counts = { hits: 0, computed: 0, findings: 0, ms: 0 }

  export const byDepth = (r: Report): Counts[] =>
    r.visits.reduce<Counts[]>((ds, x) => {
      const at = ds[x.depth] ?? none
      ds[x.depth] =
        x.outcome === "Hit" ? { ...at, hits: at.hits + 1 }
        : x.outcome === "Computed" ? { ...at, computed: at.computed + 1, ms: at.ms + x.ms }
        : { ...at, findings: at.findings + 1, ms: at.ms + x.ms }
      return ds
    }, []).map(c => c ?? none)

  export const names = (r: Report, outcome: Outcome): Name[] => r.visits.filter(x => x.outcome === outcome).map(x => x.name)

  type Slot<A, R extends Value> = { readonly t: Tree<A>; readonly depth: number; readonly kids: Slot<A, R>[]; readonly height: number; readonly finding: boolean; r?: R }

  const got = <A, R extends Value>(s: Slot<A, R>): R => {
    if (s.r === undefined) throw new Error("a node read before its wave")
    return s.r
  }

  // Bottom-up, never entering a subtree whose top the table holds, one `apply` per wave of nodes whose children are
  // known; a name seen twice in one fold is computed once, and a key holding two values is computed and never chosen.
  export const fold = <A, R extends Value>(alg: Algebra<A, R>, t: Tree<A>, cache: table.Table<R>, clock: () => number = () => performance.now()): [R, Report] => {
    const visits: Visit[] = []
    const waves: Slot<A, R>[][] = []
    const pending = new Map<Name, Slot<A, R>>()
    const key = (n: Tree<A>): table.Key => table.key(alg.name, n.name)
    const walk = (n: Tree<A>, depth: number): Slot<A, R> => {
      const seen = pending.get(n.name)
      const held: table.Lookup<R> = seen === undefined ? cache.get(key(n)) : { type: "Miss" }
      if (seen !== undefined || held.type === "Hit") visits.push({ name: n.name, depth, outcome: "Hit", ms: 0 })
      if (seen !== undefined) return seen
      if (held.type === "Hit") return { t: n, depth, kids: [], height: -1, finding: false, r: held.v }
      const kids = n.kids.map(k => walk(k, depth + 1))
      const s: Slot<A, R> = { t: n, depth, kids, height: kids.reduce((h, k) => Math.max(h, k.height + 1), 0), finding: held.type === "Finding" }
      pending.set(n.name, s)
      ;(waves[s.height] ??= []).push(s)
      return s
    }
    const root = walk(t, 0)
    const times = waves.map(wave => {
      const t0 = clock()
      const rs = alg.apply(wave.map(s => ({ name: s.t.name, v: s.t.v, rs: s.kids.map(got) })))
      const ms = clock() - t0
      wave.forEach((s, i) => {
        const r = rs[i]
        if (r === undefined || rs.length !== wave.length) throw new Error(`an algebra answered ${rs.length} results for ${wave.length} layers`)
        s.r = r
        // The key missed when the node was entered, so a value there now is a second one, reported where it lands.
        const finding = s.finding || cache.insert(key(s.t), r)
        visits.push({ name: s.t.name, depth: s.depth, outcome: finding ? "Finding" : "Computed", ms: ms / wave.length })
      })
      return { nodes: wave.length, ms }
    })
    return [got(root), { visits, waves: times }]
  }

  // The most nodes one chunk holds, so a run with no boundary is still a tree.
  export const WIDEST = 16

  // A run closes after a node whose name's last hex digit is a multiple of four: four nodes on average.
  const ends = (n: Name): boolean => parseInt(n.slice(-1), 16) % 4 === 0

  // A long sequence balanced into a tree of the host's chunks, its shape a function of the elements alone, so an
  // edit renames a logarithmic spine and two hosts holding one sequence build one tree.
  // A run closes by `cut`: the node's name, or, where elements have an identity apart from their content, a name
  // of that identity (and a chunk's, of its children's), so an edit of content never moves a cut.
  export const balance = <T>(xs: readonly T[], cut: (t: T) => Name, chunk: (run: T[]) => T): T | undefined => {
    let level = [...xs]
    while (level.length > 1) {
      const next: T[] = []
      let run: T[] = []
      for (const x of level) {
        run.push(x)
        if (ends(cut(x)) || run.length === WIDEST) {
          next.push(chunk(run))
          run = []
        }
      }
      if (run.length > 0) next.push(chunk(run))
      // Every node ended its own run, so the level did not shrink: it closes into one chunk.
      level = next.length === level.length ? [chunk(next)] : next
    }
    return level[0]
  }
}
export default memo
