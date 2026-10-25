// Children kept by key: a node is rebuilt only when its signature moves, and moved by `reconcile`.

import { reconcile } from "./reconcile"

export namespace keyed {
  export type Item = { readonly key: string; readonly sig: string; readonly make: () => HTMLElement }
  export type List = { readonly draw: (items: readonly Item[]) => void; readonly node: (key: string) => HTMLElement | undefined }

  export const list = (parent: HTMLElement): List => {
    const nodes = new Map<string, HTMLElement>()
    const sigs = new Map<string, string>()
    let keys: string[] = []
    const draw = (items: readonly Item[]): void => {
      for (const { key, sig, make } of items) {
        if (sigs.get(key) === sig) continue
        const fresh = make()
        nodes.get(key)?.replaceWith(fresh)
        nodes.set(key, fresh)
        sigs.set(key, sig)
      }
      const next = items.map(i => i.key)
      for (const op of reconcile(keys, next)) {
        const at = nodes.get(op.key)
        if (op.op === "remove") {
          at?.remove()
          nodes.delete(op.key)
          sigs.delete(op.key)
        } else if (at !== undefined) {
          const before = op.before === null ? null : (nodes.get(op.before) ?? null)
          // A node already where it goes is left, so a list placed in pieces is not placed again.
          if (at.parentNode !== parent || at.nextSibling !== before) parent.insertBefore(at, before)
        }
      }
      keys = next
    }
    return { draw, node: k => nodes.get(k) }
  }
}
export default keyed
