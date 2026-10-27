// What a reading of the lines needs of the device: the nodes of the skeleton it reads, and their content. A reading is
// whole or it is not made: every node it needs is held, or it names the nodes still to fetch, never a fold over part of
// what it needs. Pure: the skeleton is a replica of nodes, the content a test by name.

import type { Replica } from "./replica"
import type { SkeletonNode } from "../wire/generated/wire"

export namespace lines {
  // The whole store, for the list and a search; or some conversations, each of which reads its own lines and what they
  // rest on: one for a window, those whose rounds the stream draws.
  export type Scope = { readonly type: "Whole" } | { readonly type: "Conversations"; readonly ids: readonly string[] }

  // Every node the reading needs is held; or the nodes whose content is not.
  export type Need = { readonly type: "Whole" } | { readonly type: "Fetching"; readonly nodes: readonly SkeletonNode[] }

  export type Has = (name: string) => boolean

  export const WHOLE: Scope = { type: "Whole" }
  export const one = (id: string): Scope => ({ type: "Conversations", ids: [id] })

  // The nodes `wanted` names and everything they rest on, parents first, by one pass back over the skeleton's order.
  const closure = (nodes: readonly SkeletonNode[], wanted: (n: SkeletonNode) => boolean): SkeletonNode[] => {
    const under = new Set<string>()
    for (const n of [...nodes].reverse()) {
      if (!under.has(n.hash) && !wanted(n)) continue
      under.add(n.hash)
      for (const p of n.parents) under.add(p)
    }
    return nodes.filter(n => under.has(n.hash))
  }

  // A conversation's lines are a register of their own: no line of one rests on a line of another, and an erasure
  // erases only its own conversation's words, so its closure reads as the whole store reads it.
  export const nodesOf = (skeleton: Replica<SkeletonNode>, scope: Scope): readonly SkeletonNode[] => {
    if (scope.type === "Whole") return skeleton.objects
    const ids = new Set(scope.ids)
    return closure(skeleton.objects, n => n.header.node === "line" && ids.has(n.header.conversation))
  }

  // Whether the skeleton holds any line of `id`, read off the headers alone.
  export const holds = (skeleton: Replica<SkeletonNode>, id: string): boolean =>
    skeleton.objects.some(n => n.header.node === "line" && n.header.conversation === id)

  export const need = (skeleton: Replica<SkeletonNode>, has: Has, scope: Scope): Need => {
    const lacking = nodesOf(skeleton, scope).filter(n => !has(n.hash))
    return lacking.length > 0 ? { type: "Fetching", nodes: lacking } : { type: "Whole" }
  }

  // The nodes of every conversation a line naming one of `keys` is in, and what they rest on.
  export const naming = (skeleton: Replica<SkeletonNode>, keys: readonly string[]): readonly SkeletonNode[] => {
    const named = new Set(keys)
    const ids = new Set(skeleton.objects.flatMap(n => (n.header.node === "line" && n.header.keys.some(k => named.has(k)) ? [n.header.conversation] : [])))
    return ids.size === 0 ? [] : closure(skeleton.objects, n => n.header.node === "line" && ids.has(n.header.conversation))
  }
}
export default lines
