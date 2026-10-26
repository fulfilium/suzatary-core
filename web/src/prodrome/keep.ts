// Which lines' content this device keeps: the newest conversations, each whole, within a byte budget of its own. A
// prefetch fetches what the plan names and the device lacks; an eviction drops what the device holds outside the plan
// and outside what this page's readings read.

import type { Replica } from "./replica"
import type { SkeletonNode } from "../wire/generated/wire"

export namespace keep {
  // The origin's quota as `navigator.storage.estimate()` says it, or nothing where the browser does not say.
  export type Quota = { readonly type: "Unknown" } | { readonly type: "Known"; readonly bytes: number }

  // The live store is about 54 MB, so 256 MiB holds it whole with room to grow, and a tenth of the quota leaves the
  // rest of the origin's storage to the chains, the rendered blocks and the shell.
  export const MOST = 256 * 1024 * 1024
  export const UNKNOWN = 64 * 1024 * 1024

  export const budget = (q: Quota): number => (q.type === "Unknown" ? UNKNOWN : Math.min(MOST, Math.floor(q.bytes / 10)))

  export type Conversation = { readonly id: string; readonly nodes: readonly SkeletonNode[] }

  const newestFirst = ([a, x]: readonly [string, string], [b, y]: readonly [string, string]): number =>
    x < y ? 1 : x > y ? -1 : a < b ? -1 : a > b ? 1 : 0

  // Every conversation with what its lines rest on and then its lines, the one with the newest line first. A
  // conversation's lines rest on its own lines and on structure, so the walk from its lines' parents visits structure
  // alone, which is a few nodes.
  export const recent = (skeleton: Replica<SkeletonNode>): readonly Conversation[] => {
    const byHash = new Map(skeleton.objects.map(n => [n.hash, n]))
    const own = new Map<string, SkeletonNode[]>()
    const latest = new Map<string, string>()
    for (const n of skeleton.objects) {
      if (n.header.node !== "line") continue
      const c = n.header.conversation
      const lines = own.get(c) ?? []
      lines.push(n)
      own.set(c, lines)
      const was = latest.get(c)
      if (was === undefined || n.header.at > was) latest.set(c, n.header.at)
    }
    const under = (lines: readonly SkeletonNode[]): SkeletonNode[] => {
      const inside = new Set(lines.map(n => n.hash))
      const more = new Map<string, SkeletonNode>()
      const walk = lines.flatMap(n => n.parents)
      for (let p = walk.pop(); p !== undefined; p = walk.pop()) {
        const node = byHash.get(p)
        if (node === undefined || inside.has(p) || more.has(p)) continue
        more.set(p, node)
        walk.push(...node.parents)
      }
      return [...more.values(), ...lines]
    }
    return [...latest].sort(newestFirst).map(([id]) => ({ id, nodes: under(own.get(id) ?? []) }))
  }

  // The nodes of the newest conversations, each whole, in that order, up to the first that would pass `budget`.
  export const plan = (skeleton: Replica<SkeletonNode>, budget: number): readonly SkeletonNode[] => {
    const kept = new Set<string>()
    const out: SkeletonNode[] = []
    let bytes = 0
    for (const c of recent(skeleton)) {
      const more = c.nodes.filter(n => !kept.has(n.hash))
      const size = more.reduce((k, n) => k + n.size, 0)
      if (bytes + size > budget) break
      for (const n of more) {
        kept.add(n.hash)
        out.push(n)
      }
      bytes += size
    }
    return out
  }
}
export default keep
