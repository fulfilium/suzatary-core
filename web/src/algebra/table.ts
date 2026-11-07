// A tabulation of pure functions, the Prodrome's `memo::Cache` (design §6.2): a set of (key, value) pairs read as a
// partial function where it is one, joined by union. A key holding two values is a finding, held as evidence and
// never chosen between; any entry may go at any time, so what a table holds changes what is computed, never what
// is answered. It holds no object of a history and is never in a checkpoint.

export namespace table {
  // A function's name and its argument's, printed with one space: names hold none, so no two keys print alike.
  export type Key = string

  export type Lookup<V> =
    | { readonly type: "Miss" }
    | { readonly type: "Hit"; readonly v: V }
    | { readonly type: "Finding"; readonly vs: readonly V[] }

  export const key = (fn: string, arg: string): Key => `${fn} ${arg}`
  // The function a key names, `key`'s first argument.
  export const fnOf = (k: Key): string => k.slice(0, Math.max(0, k.indexOf(" ")))

  export const fold = <V, R>(l: Lookup<V>, on: { readonly miss: () => R; readonly hit: (v: V) => R; readonly finding: (vs: readonly V[]) => R }): R =>
    l.type === "Hit" ? on.hit(l.v) : l.type === "Finding" ? on.finding(l.vs) : on.miss()

  // The value where the key holds exactly one; a finding is no answer.
  export const hit = <V>(l: Lookup<V>): V | undefined => (l.type === "Hit" ? l.v : undefined)

  // Which keys go, told of every key touched and asked for a victim after every write: it decides what is
  // remembered, never what anything means.
  export type Evict = {
    readonly touched: (k: Key) => void
    readonly forgot: (k: Key) => void
    readonly victim: (held: number) => Key | undefined
  }

  export const keep = (): Evict => ({ touched: () => {}, forgot: () => {}, victim: () => undefined })

  // At most `capacity` keys, the least recently touched first.
  export const lru = (capacity: number): Evict => {
    const recent = new Set<Key>()
    return {
      touched: k => (recent.delete(k), void recent.add(k)),
      forgot: k => void recent.delete(k),
      victim: held => (held > capacity ? recent.values().next().value : undefined),
    }
  }

  export type Table<V> = {
    readonly get: (k: Key) => Lookup<V>
    // True when the key now holds a second value for the first time: a finding.
    readonly insert: (k: Key, v: V) => boolean
    // The union into this table, answering every key it made a finding of.
    readonly join: (pairs: Iterable<readonly [Key, V]>) => Key[]
    readonly remove: (k: Key) => boolean
    readonly pairs: () => [Key, V][]
    readonly findings: () => Key[]
    readonly size: () => number
  }

  // `same` is the values' equality; a pure function's two answers at one key are equal by it.
  export const of = <V>(evict: Evict, same: (a: V, b: V) => boolean = Object.is): Table<V> => {
    const held = new Map<Key, V[]>()
    const add = (k: Key, v: V): boolean => {
      const vs = held.get(k) ?? []
      if (vs.some(w => same(w, v))) return false
      held.set(k, [...vs, v])
      return vs.length === 1
    }
    const remove = (k: Key): boolean => (evict.forgot(k), held.delete(k))
    const shrink = (): void => {
      let k = evict.victim(held.size)
      while (k !== undefined && remove(k)) k = evict.victim(held.size)
    }
    return {
      get: k => {
        const vs = held.get(k)
        if (vs === undefined) return { type: "Miss" }
        evict.touched(k)
        const [v] = vs
        return vs.length === 1 && v !== undefined ? { type: "Hit", v } : { type: "Finding", vs }
      },
      insert: (k, v) => {
        evict.touched(k)
        const found = add(k, v)
        shrink()
        return found
      },
      join: pairs => {
        const found = new Set<Key>()
        for (const [k, v] of pairs) {
          if (add(k, v)) found.add(k)
          evict.touched(k)
        }
        shrink()
        return [...found]
      },
      remove,
      pairs: () => [...held].flatMap(([k, vs]) => vs.map((v): [Key, V] => [k, v])),
      findings: () => [...held].flatMap(([k, vs]) => (vs.length > 1 ? [k] : [])),
      size: () => held.size,
    }
  }
}
export default table
