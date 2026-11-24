// An act: a button that runs one effect, disabled while it runs, and offered only to a login that writes.

import app from "../app/app"
import type { Child } from "./dom"
import { el } from "./dom"
import { toast } from "./toast"
import lo from "../algebra/lo"

export namespace act {
  // `Spent` keeps the button disabled until the page redraws.
  export type Outcome = { readonly type: "Again" } | { readonly type: "Spent" }

  export type T = {
    readonly label: Child
    readonly cls: string
    readonly run: () => Promise<Outcome>
    readonly aria?: string
    readonly failed?: (e: unknown) => void
  }

  export type Gate = { readonly type: "Writes" } | { readonly type: "Reads" }

  export const AGAIN: Outcome = { type: "Again" }
  export const SPENT: Outcome = { type: "Spent" }
  export const again = (p: Promise<unknown>): Promise<Outcome> => p.then(() => AGAIN)

  export const gate = (writes: boolean = app.writes(app.now())): Gate => (writes ? { type: "Writes" } : { type: "Reads" })
  export const offered = <A>(g: Gate, acts: () => A, otherwise: A): A => (g.type === "Writes" ? acts() : otherwise)

  // A double tap would be two writes, so a pressed act is disabled until it answers; inside a card's link it is not also a navigation.
  export const view = (a: T): HTMLElement => {
    const b = el("button", { type: "button", class: a.cls, "aria-label": a.aria ?? null }, a.label)
    b.addEventListener("click", (e: Event) => {
      e.preventDefault()
      e.stopPropagation()
      if (b.hasAttribute("disabled")) return
      b.setAttribute("disabled", "")
      void a.run().then(
        o => (o.type === "Again" ? b.removeAttribute("disabled") : undefined),
        (x: unknown) => (b.removeAttribute("disabled"), (a.failed ?? (e => toast(lo.reason(e))))(x)),
      )
    })
    return b
  }
}
export default act
