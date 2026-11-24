// What is folded away and opened on demand, its openness an `Open` the caller keeps so a redraw draws it as left.

import type { Attrs, Child } from "./dom"
import { el } from "./dom"

export namespace disclosure {
  export type Open = { readonly get: boolean; readonly set: (open: boolean) => void }
  export const of = (r: { readonly get: () => boolean; readonly set: (open: boolean) => void }): Open => ({ get: r.get(), set: r.set })

  // With no `Open` the button holds the state; a page that redraws passes one.
  export const view = (overview: Child, full: HTMLElement, open: Open | null): HTMLElement => {
    const shown = open?.get ?? false
    full.hidden = !shown
    const label = (o: boolean): string => (o ? "Hide the diff" : "Show the diff")
    const button = el("button", { type: "button", class: "diffbtn", "aria-expanded": String(shown) }, label(shown))
    button.addEventListener("click", (e: Event) => {
      e.stopPropagation()
      const now = button.getAttribute("aria-expanded") !== "true"
      button.setAttribute("aria-expanded", String(now))
      button.textContent = label(now)
      full.hidden = !now
      open?.set(now)
    })
    return el("div", { class: "diff" }, el("div", { class: "diffsummary" }, overview, button), full)
  }

  // A <details> drawn as `open` holds it, so a redraw keeps what the reader opened. The summary's click writes before
  // the browser toggles, so a redraw in the very next task already reads it; a tap on one a redraw has already taken
  // off the page writes nothing.
  export const details = (open: Open, attrs: Attrs, summary: Child, ...body: Child[]): HTMLElement => {
    const head = el("summary", null, summary)
    const d = el("details", { ...attrs, open: open.get }, head, ...body)
    head.addEventListener("click", e => (d.isConnected ? open.set(!d.open) : e.preventDefault()))
    d.addEventListener("toggle", () => (d.isConnected ? open.set(d.open) : undefined))
    return d
  }
}
export default disclosure
