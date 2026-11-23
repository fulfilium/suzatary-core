// What a reading of what this device holds left out (Prodrome SPEC law 44): each object that is no object, why, and the
// objects resting on it, left out with it; and each chain that did not read at all, so what refers into it is unpriced.

import d from "../wire/decoder"

export namespace excluded {
  export type Excluded = { readonly name: string; readonly why: string; readonly resting: readonly string[] }
  // A set by name, kept in name order with each `resting` sorted, so two spellings of one set compare equal.
  export type T = readonly Excluded[]
  export type Chain = { readonly chain: string; readonly why: string }
  // A copy of a chain this device keeps that cannot be used as it is, and whether resetting it from the box is the way out.
  export type Row = { readonly chain: string; readonly why: string; readonly cure: "Reset" | "None" }
  // Everything one reading could not read: its objects left out, the chains it could not read at all, and the copies
  // this device keeps that it could not use.
  export type Left = { readonly objects: T; readonly chains: readonly Chain[]; readonly rows: readonly Row[] }
  // What each reader last left out, by what it read.
  export type Seen = { readonly [reader: string]: Left }
  export type Event = { readonly type: "Read"; readonly reader: string; readonly left: Left }

  export const NONE: T = []

  // A copy whose box answered with an earlier epoch than one this device holds, if it did.
  export const behind = (chain: string, back: boolean): Left =>
    left(NONE, [], back ? [{ chain, why: "the box answered with an earlier epoch than this device has heard from it, so its state was lost; this device keeps reading the later word until the box's epoch passes it, or this copy is reset", cure: "None" }] : [])

  // A copy this device keeps that does not read, with the one write that may stand in for it.
  export const unread = (chain: string, why: string): Row => ({
    chain,
    why: `what this device keeps of it does not read (${why}), so nothing done here can be kept until it is reset from the box, which writes over that copy and whatever it held`,
    cure: "Reset",
  })
  // A copy this device keeps of which a part (`what`) did not read: that part is kept aside, the store works without it,
  // and the reset that writes over the copy removes it.
  export const aside = (chain: string, what: string): Row => ({
    chain,
    why: `part of what this device keeps of it did not read (${what}); that part is kept aside and the store works without it, and writing over this copy from the box removes it`,
    cure: "Reset",
  })
  export const NOTHING: Left = { objects: NONE, chains: [], rows: [] }
  export const UNSEEN: Seen = {}

  const sorted = (xs: Iterable<string>): string[] => [...new Set(xs)].sort()
  const byName = (a: Excluded, b: Excluded): number => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0)
  const least = (a: string, b: string): string => (a <= b ? a : b)

  export const of = (xs: Iterable<Excluded>): T => [...xs].reduce<T>((t, x) => join(t, [{ ...x, resting: sorted(x.resting) }]), NONE)

  // A semilattice, NONE its identity: union by name, a name's resting unioned and the least why kept, so it commutes.
  export const join = (a: T, b: T): T => {
    if (b.length === 0) return a
    if (a.length === 0) return b
    const out = new Map(a.map(x => [x.name, x]))
    for (const y of b) {
      const x = out.get(y.name)
      out.set(y.name, x === undefined ? y : { name: y.name, why: least(x.why, y.why), resting: sorted([...x.resting, ...y.resting]) })
    }
    return [...out.values()].sort(byName)
  }

  // The same semilattice over chains: one per chain, its least why.
  const chainsJoin = (a: readonly Chain[], b: readonly Chain[]): readonly Chain[] => {
    if (b.length === 0) return a
    if (a.length === 0) return b
    const out = new Map(a.map(c => [c.chain, c.why]))
    for (const c of b) out.set(c.chain, least(out.get(c.chain) ?? c.why, c.why))
    return [...out.entries()].sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0)).map(([chain, why]) => ({ chain, why }))
  }

  // Rows as a set: each is kept whole, in one order.
  const rowsJoin = (a: readonly Row[], b: readonly Row[]): readonly Row[] => {
    if (b.length === 0) return a
    if (a.length === 0) return b
    const out = new Map([...a, ...b].map(r => [JSON.stringify([r.chain, r.why, r.cure]), r]))
    return [...out.entries()].sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0)).map(([, r]) => r)
  }

  export const left = (objects: T, chains: readonly Chain[] = [], rows: readonly Row[] = []): Left => ({
    objects,
    chains: chains.reduce<readonly Chain[]>((t, c) => chainsJoin(t, [c]), []),
    rows: rows.reduce<readonly Row[]>((t, r) => rowsJoin(t, [r]), []),
  })

  // The product of the three semilattices, NOTHING its identity.
  export const joinLeft = (a: Left, b: Left): Left => ({ objects: join(a.objects, b.objects), chains: chainsJoin(a.chains, b.chains), rows: rowsJoin(a.rows, b.rows) })

  export const empty = (l: Left): boolean => l.objects.length === 0 && l.chains.length === 0 && l.rows.length === 0

  export const same = (a: Left, b: Left): boolean => JSON.stringify(a) === JSON.stringify(b)

  // The objects left out because they rest on a name left out; never one of the names, which nothing parsed rests on.
  export const resting = (t: T): number => new Set(t.flatMap(x => x.resting)).size

  export const step = (s: Seen, e: Event): Seen => (same(s[e.reader] ?? NOTHING, e.left) ? s : { ...s, [e.reader]: e.left })

  export const all = (s: Seen): Left => Object.values(s).reduce(joinLeft, NOTHING)

  const objectsSaid = (t: T): string => {
    const one = t.length === 1
    return `${t.length} ${one ? "object" : "objects"} on this device could not be read and ${one ? "was" : "were"} left out (${resting(t)} resting on ${one ? "it" : "them"})`
  }

  const chainsSaid = (cs: readonly Chain[]): string =>
    cs.length === 1
      ? `the chain ${cs[0]?.chain ?? ""} could not be read, so what refers into it is unpriced until it reads`
      : `${cs.length} chains could not be read, so what refers into them is unpriced until they read`

  // A copy is said to need the owner only where the owner can do something for it: a reset that writes over it. A row
  // with no cure is a word about the copy, never a call.
  const rowsSaid = (rs: readonly Row[]): string => {
    const asking = rs.filter(r => r.cure === "Reset")
    const telling = rs.filter(r => r.cure === "None")
    return [
      ...(asking.length === 0 ? [] : [asking.length === 1 ? `this device's copy of ${asking[0]?.chain ?? ""} needs you` : `${asking.length} of this device's copies need you`]),
      ...(telling.length === 0 ? [] : [telling.length === 1 ? `a word on this device's copy of ${telling[0]?.chain ?? ""}` : `a word on ${telling.length} of this device's copies`]),
    ].join("; ")
  }

  // The one line: what was left out of what was read, then what could not be read at all, then what this device keeps.
  export const said = (l: Left): string => {
    const parts = [
      ...(l.objects.length === 0 ? [] : [objectsSaid(l.objects)]),
      ...(l.chains.length === 0 ? [] : [chainsSaid(l.chains)]),
      ...(l.rows.length === 0 ? [] : [rowsSaid(l.rows)]),
    ]
    const line = parts.join("; ")
    return line.charAt(0).toUpperCase() + line.slice(1)
  }

  export const decoder: d.Decoder<T> = d.map(d.arr(d.obj<Excluded>({ name: d.str, why: d.str, resting: d.arr(d.str) })), of)
}
export default excluded
