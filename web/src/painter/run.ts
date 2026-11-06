// A Rust program running in the tab: its module steps the model and answers patches and effects, the painter applies
// the patches, and the host's handler performs each effect and answers it with an input. Nothing here knows the page.

import painter from "./painter"
import scroll from "./scroll"

export namespace run {
  export type Module = {
    readonly first: () => Uint8Array
    readonly input: (json: string) => Uint8Array
    readonly event: (id: number, on: number, words: string) => Uint8Array | undefined
    readonly effects: () => string
    readonly free: () => void
  }
  // `stepped.fromEnd`: the scroll box's distance from its end before the step's patches.
  export type Handler<I, E> = (effect: E, send: (input: I) => void, stepped: Stepped) => void
  export type Stepped = { readonly fromEnd: number }
  export type T<I> = { readonly send: (input: I) => void; readonly destroy: () => void }

  // Serde prints the `Effect` sum the handler's type is generated from, so it needs no decoder.
  const parse = <E>(json: string): readonly E[] => JSON.parse(json)

  // A `Keep` restores the place of the scroll box `box` finds (one a page draws itself is there once drawn).
  export const start = <I, E>(
    root: HTMLElement,
    page: Module,
    foreign: painter.Foreign,
    blocks: painter.Blocks,
    handle: Handler<I, E>,
    gone: ((n: Node) => void) | undefined,
    box: () => Element,
    refresh?: painter.Refresh,
  ): T<I> => {
    let live = true
    const step = (name: string, patches: () => Uint8Array | undefined): boolean => {
      if (!live) return false
      const t0 = performance.now()
      const b = box()
      const fromEnd = b.scrollHeight - b.scrollTop
      const bytes = patches()
      if (bytes === undefined) return false
      dom.apply(bytes)
      scroll.settle(box())
      const effects = parse<E>(page.effects())
      performance.measure("painter:step", { start: t0, end: performance.now(), detail: { step: name, bytes: bytes.length, effects: effects.length } })
      for (const e of effects) handle(e, send, { fromEnd })
      return true
    }
    const send = (input: I): void => void step("input", () => page.input(JSON.stringify(input)))
    const dom = painter.create(root, foreign, blocks, (id, on, words) => step("event", () => page.event(id, on, words)), gone, refresh)
    step("first", () => page.first())
    return { send, destroy: () => ((live = false), dom.destroy(), page.free()) }
  }
}
export default run
