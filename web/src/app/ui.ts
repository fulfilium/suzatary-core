// The ui tier's registers, each declared once with where its writes go. A `Tab` one has one writer, this tab, so it is
// a plain value in memory; a `Device` one is this browser's storage, writes that carry their past, joined across its
// tabs; an `Owner` one too, and its writes are what another device would be handed (`receive`).

import register from "../algebra/register"
import kv from "./kv"
import lo from "../algebra/lo"
import d from "../wire/decoder"

export namespace ui {
  export type Declared<A> =
    | { readonly name: string; readonly tier: "Tab"; readonly initial: A }
    | { readonly name: string; readonly tier: "Device" | "Owner"; readonly initial: A; readonly value: d.Decoder<A> }
  // A declared register, and a `Tab` one's value in memory, by key (a stored one has nothing in memory).
  export type Kind<A> = Declared<A> & { readonly memory: Map<string, A> }

  export const kind = <A>(d: Declared<A>): Kind<A> => ({ ...d, memory: new Map() })

  export type Cell<A> = {
    readonly read: () => register.Reading<A>
    readonly get: () => A
    // A write that saw what the register holds now: for a writer that drew from the reading it is replacing.
    readonly set: (a: A) => void
    // What a writer saw, and a write resting on it: answers what the writer sees after it. A write resting on a
    // reading that has since moved lands beside the newer writes, as a conflict.
    readonly seen: () => register.T<A>
    readonly write: (seen: register.T<A>, a: A) => register.T<A>
    readonly hear: (f: () => void) => () => void
  }

  export type Ui = {
    readonly cell: <A>(kind: Kind<A>, key?: string) => Cell<A>
    // Writes another replica made, joined in; what the transport between devices hands over.
    readonly receive: <A>(kind: Kind<A>, key: string, ws: register.T<A>) => void
    // The writes this replica holds, for that transport to hand to another.
    readonly held: <A>(kind: Kind<A>, key?: string) => register.T<A>
    // A stored key moved under this replica (another tab wrote it): who listens to it reads again.
    readonly heard: (stored: string | null) => void
  }

  const PREFIX = "suzatary.ui."
  // A name holds no "/", so a stored key is the slot after the prefix and reads back as one.
  const slot = (name: string, key: string): string => `${name}/${key}`
  const same = <A>(a: A, b: A): boolean => JSON.stringify(a) === JSON.stringify(b)

  // One replica's registers over a store: `by` names the writer, one per tab.
  export const over = (store: kv.Kv, by: string): Ui => {
    const ears = new Map<string, Set<() => void>>()
    const tell = (s: string): void => [...(ears.get(s) ?? [])].forEach(f => f())
    const held = <A>(kind: Kind<A>, key = ""): register.T<A> => {
      if (kind.tier === "Tab") return register.EMPTY
      return lo.withDefault(d.json(register.stored(kind.value))(store.get(PREFIX + slot(kind.name, key))), register.EMPTY)
    }
    // A register that reads as its initial value and nothing else is no register: its key goes, history and all. A
    // `Device` one kept is cut to its frontier's replicas, so a tab that wrote and closed leaves nothing behind: this
    // storage is joined only with new writes (a writer's basis is never joined in), so nothing it superseded arrives
    // again. An `Owner` one keeps its whole past, which another device's writes are read against (`receive`).
    const keep = <A>(kind: Kind<A>, key: string, r: register.T<A>): void => {
      const g = register.read(r)
      if (g.type === "Value" && same(g.v, kind.initial)) store.drop(PREFIX + slot(kind.name, key))
      else store.set(PREFIX + slot(kind.name, key), JSON.stringify(kind.tier === "Device" ? register.cut(r) : r))
      tell(slot(kind.name, key))
    }
    const tab = <A>(kind: Kind<A>, key: string, a: A): void => {
      if (same(a, kind.memory.get(key) ?? kind.initial)) return
      if (same(a, kind.initial)) kind.memory.delete(key)
      else kind.memory.set(key, a)
      tell(slot(kind.name, key))
    }
    const stored = <A>(kind: Kind<A>, key: string, seen: register.T<A>, a: A): register.T<A> => {
      const now = held(kind, key)
      const mine = register.write(seen, by, a, register.latest(now, by))
      keep(kind, key, register.join(now, mine))
      return mine
    }
    return {
      cell: <A>(kind: Kind<A>, key = ""): Cell<A> => {
        const read = (): register.Reading<A> =>
          kind.tier === "Tab" ? (kind.memory.has(key) ? register.of([kind.memory.get(key) ?? kind.initial]) : register.of<A>([])) : register.read(held(kind, key))
        return {
          read,
          get: () => register.one(read(), kind.initial),
          // Writing what is read writes nothing; in a conflict every write settles it.
          set: a => {
            const g = read()
            if (g.type !== "Conflict" && same(a, g.type === "Value" ? g.v : kind.initial)) return
            if (kind.tier === "Tab") tab(kind, key, a)
            else stored(kind, key, held(kind, key), a)
          },
          seen: () => held(kind, key),
          write: (seen, a) => (kind.tier === "Tab" ? (tab(kind, key, a), register.EMPTY) : stored(kind, key, seen, a)),
          hear: f => {
            const s = slot(kind.name, key)
            const set = ears.get(s) ?? new Set()
            ears.set(s, set.add(f))
            return () => void set.delete(f)
          },
        }
      },
      receive: (kind, key, ws) => (kind.tier === "Tab" ? undefined : keep(kind, key, register.join(held(kind, key), ws))),
      held,
      heard: k => {
        if (k === null) return [...ears.keys()].forEach(tell)
        if (k.startsWith(PREFIX)) tell(k.slice(PREFIX.length))
      },
    }
  }

  const replica = (): string =>
    typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`

  // This tab's registers: a writer of its own, over this browser's storage.
  export const tab: Ui = over(kv.local, replica())
  export const cell = <A>(kind: Kind<A>, key = ""): Cell<A> => tab.cell(kind, key)

  // Another tab's write lands as a storage event; this tab reads the joined set when asked.
  export const install = (): void => window.addEventListener("storage", e => tab.heard(e.key))
}
export default ui
