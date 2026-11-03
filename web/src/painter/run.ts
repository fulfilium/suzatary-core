// A Rust program running in the tab: its module steps the model and answers patches and effects, the painter applies
// the patches, and the host's handler performs each effect and answers it with an input. Nothing here knows the page.

import painter from "./painter"

export namespace run {
  // A page's class, as `viewtree::export!` makes it.
  export type Module = {
    readonly first: () => Uint8Array
    readonly input: (json: string) => Uint8Array
    readonly event: (id: number, on: number, words: string) => Uint8Array
    readonly effects: () => string
    readonly free: () => void
  }
  // Performs one effect; what it comes to, now or later, is an input sent back.
  export type Handler<I, E> = (effect: E, send: (input: I) => void) => void
  export type T<I> = { readonly send: (input: I) => void; readonly destroy: () => void }

  // `effects` is the module's JSON of the program's own `Effect` sum, printed by serde from the type the host's handler
  // is generated from, so it is read as that type without a decoder of its own.
  const parse = <E>(json: string): readonly E[] => JSON.parse(json)

  export const start = <I, E>(root: HTMLElement, page: Module, foreign: painter.Foreign, handle: Handler<I, E>): T<I> => {
    let live = true
    const step = (name: string, patches: () => Uint8Array): void => {
      if (!live) return
      const t0 = performance.now()
      const bytes = patches()
      dom.apply(bytes)
      const effects = parse<E>(page.effects())
      performance.measure("painter:step", { start: t0, end: performance.now(), detail: { step: name, bytes: bytes.length, effects: effects.length } })
      for (const e of effects) handle(e, send)
    }
    const send = (input: I): void => step("input", () => page.input(JSON.stringify(input)))
    const dom = painter.create(root, foreign, (id, on, words) => step("event", () => page.event(id, on, words)))
    step("first", () => page.first())
    return { send, destroy: () => ((live = false), dom.destroy(), page.free()) }
  }
}
export default run
