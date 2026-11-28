// A Rust program in the tab: its patches painted, each effect handed to the host, one input at a time.

import type { Input, Slot, Sub, SubPatch } from "../wire/generated/pages"
import deliver from "./deliver"
import mailbox from "./mailbox"
import painter from "./painter"
import scroll from "./scroll"
import { measure } from "../app/perf"

export namespace run {
  export type Module = {
    readonly first: () => Uint8Array
    readonly input: (json: string) => Uint8Array
    readonly reply: (json: string, text: Uint8Array) => Uint8Array
    readonly event: (id: number, on: number, words: string) => Uint8Array | undefined
    readonly effects: () => string
    readonly subs: () => string
    readonly free: () => void
  }
  export type Handler<I, E> = (effect: E, send: (input: I) => void) => void
  export type Patched = SubPatch<Slot<unknown, unknown, unknown>, Sub>
  export type T<I> = { readonly send: (input: I) => void; readonly destroy: () => void }

  const named = (input: Input<unknown, unknown, unknown>): string =>
    input.type === "Context" ? input.type : `${input.type} ${JSON.stringify(input.ask)}`

  // Serde prints the `Effect` and `SubPatch` sums the handlers' types are generated from, so they need no decoder.
  const parse = <E>(json: string): readonly E[] => JSON.parse(json)

  // `page:subs` is the host taking and applying a step's subscription patches; `painter:step` is the whole step, `wasm` of it the
  // program's own (update, view, diff, subscriptions and their prints).
  // A step with an effect that `keeps` holds the box `box` finds (a page's own once drawn) at its distance from the end
  // across its patches, before any of its effects is handled (a `Focus` of the same step moves the box after); only such
  // a step reads layout before its patches, so the others' patches are laid out by the frame.
  export const start = <I extends Input<unknown, unknown, unknown>, E>(
    root: HTMLElement,
    page: Module,
    foreign: painter.Foreign,
    blocks: painter.Blocks,
    handle: Handler<I, E>,
    subscribe: (patch: Patched, send: (input: I) => void) => void,
    keeps: (effect: E) => boolean,
    gone: ((n: Node) => void) | undefined,
    box: () => Element,
    refresh?: painter.Refresh,
    layer: HTMLElement | null = null,
    // Whether the page is in the document; out of it, nothing is measured, since every box there is empty.
    drawn: () => boolean = () => true,
  ): T<I> => {
    let live = true
    const inputs = mailbox.of<I>(input => void step(named(input), () => deliver(page, input)))
    const step = (name: string, patches: () => Uint8Array | undefined): boolean =>
      inputs.turn(() => {
        if (!live) return false
        const t0 = performance.now()
        const bytes = patches()
        const wasm = performance.now() - t0
        if (bytes === undefined) return false
        const effects = parse<E>(page.effects())
        const measured = drawn()
        const kept = measured && effects.some(keeps) ? scroll.anchor(box()) : null
        dom.apply(bytes)
        if (measured) scroll.settle(box())
        if (kept !== null) scroll.keep(kept)
        // Its listeners change before any of its effects is performed, so an effect may name one the step starts.
        const s0 = performance.now()
        const patched = parse<Patched>(page.subs())
        patched.forEach(p => subscribe(p, send))
        measure("page:subs", s0, { patches: patched.length })
        measure("painter:step", t0, { step: name, bytes: bytes.length, effects: effects.length, wasm })
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
