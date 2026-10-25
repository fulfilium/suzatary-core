// A replica made the one way one is: from what it holds, read by `replicaOf`; a box word only when the test names one.

import type { RefusedObject, StoredObject } from "../src/wire/generated/wire"
import type { HeldChain } from "../src/prodrome/core"
import type { Frontier, Replica } from "../src/prodrome/replica"
import { NO_FRONTIER, prodromeOf, replicaOf } from "../src/prodrome/replica"

// The prodromes `objects` are in, as a box serving them names them.
export const held = (objects: readonly StoredObject[]): string[] => [...new Set(objects.flatMap(o => prodromeOf(o) ?? []))].sort()

export type Over = {
  readonly objects?: readonly StoredObject[]
  readonly synced?: readonly string[]
  readonly refused?: readonly RefusedObject[]
  readonly generation?: number
  readonly boxHeads?: readonly string[]
  readonly untrusted?: readonly string[]
  readonly root?: string | null
  readonly fetchedAt?: number
  /** The prodromes the box said it holds; by default those of `objects`, as a box holding them would say. */
  readonly geneses?: readonly string[]
  readonly epoch?: number
  readonly frontier?: Frontier
}

export const replicaWith = (o: Over = {}): Replica =>
  replicaOf({
    generation: o.generation ?? 0,
    objects: o.objects ?? [],
    synced: o.synced ?? [],
    refused: o.refused ?? [],
    words:
      o.boxHeads === undefined && o.untrusted === undefined && o.root === undefined && o.fetchedAt === undefined
        ? []
        : [{ epoch: o.epoch ?? 1, heads: o.boxHeads ?? [], untrusted: o.untrusted ?? [], root: o.root ?? null, fetchedAt: o.fetchedAt ?? 1, geneses: o.geneses ?? held(o.objects ?? []) }],
    frontier: o.frontier ?? NO_FRONTIER,
  })

// A chain held whole: `objects` under the policy `untrusted`, as a pull of them would leave it.
export const heldChain = (name: string, objects: readonly StoredObject[], untrusted: readonly string[]): HeldChain => ({
  name,
  replica: replicaWith({ objects, untrusted }),
})

/** What an answer from a box that knew the frontier asked from says of it: known, and a frontier of nothing. */
export const KNOWN = { since: "Known", frontier: { digest: "0".repeat(64), count: 0 } } as const
