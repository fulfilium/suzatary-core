// A Rust program in the tab: its patches painted, each effect handed to the host, one input at a time.

import type { Input } from "../wire/generated/pages"
import deliver from "./deliver"
import mailbox from "./mailbox"
import painter from "./painter"
import scroll from "./scroll"

export namespace run {
  export type Module = {
    readonly first: () => Uint8Array
    readonly input: (json: string) => Uint8Array
    readonly reply: (json: string, text: Uint8Array) => Uint8Array
    readonly event: (id: number, on: number, words: string) => Uint8Array | undefined
    readonly effects: () => string
    readonly free: () => void
  }
  export type Handler<I, E> = (effect: E, send: (input: I) => void) => void
  export type T<I> = { readonly send: (input: I) => void; readonly destroy: () => void }

  const named = (input: Input<unknown, unknown, unknown>): string =>
    input.type === "Context" ? input.type : `${input.type} ${JSON.stringify(input.ask)}`

  // Serde prints the `Effect` sum the handler's type is generated from, so it needs no decoder.
  const parse = <E>(json: string): readonly E[] => JSON.parse(json)

  // A step with an effect that `keeps` holds the box `box` finds (a page's own once drawn) at its distance from the end
  // across its patches, before any of its effects is handled (a `Focus` of the same step moves the box after); only such
  // a step reads layout before its patches, so the others' patches are laid out by the frame.
  export const start = <I extends Input<unknown, unknown, unknown>, E>(
    root: HTMLElement,
    page: Module,
    foreign: painter.Foreign,
    blocks: painter.Blocks,
    handle: Handler<I, E>,
    keeps: (effect: E) => boolean,
    gone: ((n: Node) => void) | undefined,
    box: () => Element,
    refresh?: painter.Refresh,
    layer: HTMLElement | null = null,
  ): T<I> => {
    let live = true
    const inputs = mailbox.of<I>(input => void step(named(input), () => deliver(page, input)))
    const step = (name: string, patches: () => Uint8Array | undefined): boolean =>
      inputs.turn(() => {
        if (!live) return false
        const t0 = performance.now()
        const bytes = patches()
        if (bytes === undefined) return false
        const effects = parse<E>(page.effects())
        const kept = effects.some(keeps) ? scroll.anchor(box()) : null
        dom.apply(bytes)
        scroll.settle(box())
        if (kept !== null) scroll.keep(kept)
        performance.measure("painter:step", { start: t0, end: performance.now(), detail: { step: name, bytes: bytes.length, effects: effects.length } })
        for (const e of effects) handle(e, send)
        return true
      })
    const send = inputs.send
    const dom = painter.create(root, foreign, blocks, (id, on, words) => step("event", () => page.event(id, on, words)), gone, refresh, layer)
    step("first", () => page.first())
    return { send, destroy: () => ((live = false), dom.destroy(), page.free()) }
  }
}
export default run
