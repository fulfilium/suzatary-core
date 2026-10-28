// What a reading of the lines needs of the device: the nodes of the skeleton it reads. A reading is whole or it is not
// made (`engine.over`): every node it needs is held, or the nodes still lacking are fetched, never a fold over part of
// what it needs. Pure: the skeleton is a replica of nodes.

import type { Replica } from "./replica"
import type { SkeletonNode } from "../wire/generated/wire"

export namespace lines {
  // The whole store, for the list and a search; some conversations, each of which reads its own lines and what they
  // rest on, for a window; or the lines whose words the stream's drawn rounds read, and nothing they rest on.
  export type Scope =
    | { readonly type: "Whole" }
    | { readonly type: "Conversations"; readonly ids: readonly string[] }
    | { readonly type: "Objects"; readonly names: readonly string[] }

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
    if (scope.type === "Objects") {
      const names = new Set(scope.names)
      return skeleton.objects.filter(n => names.has(n.hash))
    }
    const ids = new Set(scope.ids)
    return closure(skeleton.objects, n => n.header.node === "line" && ids.has(n.header.conversation))
  }

  // Whether the skeleton holds any line of `id`, read off the headers alone.
  export const holds = (skeleton: Replica<SkeletonNode>, id: string): boolean =>
    skeleton.objects.some(n => n.header.node === "line" && n.header.conversation === id)

  // The nodes of every conversation a line naming one of `keys` is in, and what they rest on.
  export const naming = (skeleton: Replica<SkeletonNode>, keys: readonly string[]): readonly SkeletonNode[] => {
    const named = new Set(keys)
    const ids = new Set(skeleton.objects.flatMap(n => (n.header.node === "line" && n.header.keys.some(k => named.has(k)) ? [n.header.conversation] : [])))
    return ids.size === 0 ? [] : closure(skeleton.objects, n => n.header.node === "line" && ids.has(n.header.conversation))
  }
}
export default lines
