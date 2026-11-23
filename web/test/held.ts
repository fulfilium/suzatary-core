// Stores and chains the core holds, filled the one way one is: the box's answer holding the objects, taken as a piece
// and landed. The test's own reading of an object's parents and prodrome only names what a box serving it would say.

import type { ObjectsPayload, StorePayload, StoredObject } from "../src/wire/generated/wire"
import type { ChainStore, Chains, Core, ObjectStore } from "../src/prodrome/core"
import { holdIn } from "../src/prodrome/core"

const NAME = /[0-9a-f]{64}/g
const GENESIS = /^Change\(genesis='([0-9a-f]{64})', /

// The prodrome an object is in: a change's genesis, or a `Genesis`'s own name.
const prodromeOf = (o: StoredObject): string | null => (o.literal.startsWith("Genesis(") ? o.hash : (GENESIS.exec(o.literal)?.[1] ?? null))

// What an object names before its event: its parents and its prodrome's genesis.
export const restsOn = (o: StoredObject): string[] => {
  const cut = o.literal.indexOf("event=")
  return [...(cut === -1 ? o.literal : o.literal.slice(0, cut)).matchAll(NAME)].map(m => m[0])
}

// The prodromes `objects` are in, as a box serving them names them.
export const held = (objects: readonly StoredObject[]): string[] => [...new Set(objects.flatMap(o => prodromeOf(o) ?? []))].sort()

// The objects no other of them rests on, as a box holding them names its heads.
export const headsOf = (objects: readonly StoredObject[]): string[] => {
  const named = new Set(objects.flatMap(restsOn))
  return objects.map(o => o.hash).filter(h => !named.has(h))
}

// The box's answer of a store holding `objects`, under `untrusted` and `root`.
export const answerOf = (objects: readonly StoredObject[], untrusted: readonly string[] = [], root: string | null = null): ObjectsPayload => ({
  objects: [...objects],
  heads: headsOf(objects),
  untrusted: [...untrusted],
  root,
  geneses: held(objects),
  epoch: 1,
  since: "Known",
  frontier: { digest: "0".repeat(64), count: objects.length },
})

// A store the core holds, filled with `objects`.
export const storeOf = <S extends ObjectStore>(store: S, objects: readonly StoredObject[]): S => {
  const answer: StorePayload = { ...answerOf(objects), keys: {}, rest: 0, size: 0, weight: 0, erased: [] }
  store.piece(JSON.stringify(answer), 1)
  store.land().free()
  return store
}

// A chain held whole: `objects` under the policy `untrusted`, as a pull of them leaves it.
export type HeldChain = { readonly name: string; readonly store: ChainStore }
export const heldChain = (core: Core, name: string, objects: readonly StoredObject[], untrusted: readonly string[], root: string | null = null): HeldChain => {
  const store = new core.Chain()
  store.piece(JSON.stringify(answerOf(objects, untrusted, root)), 1)
  store.land().free()
  return { name, store }
}

// Each of `chains` held in `set` as the core's worker holds it: a chain held again replaces its last holding.
export const holdAll = (set: Chains, chains: readonly HeldChain[]): Chains => {
  for (const c of chains) holdIn(set, c.name, c.store)
  return set
}
