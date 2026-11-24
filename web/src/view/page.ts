// The box moving is heard by what the page reads, never told to the page.

import { el } from "./dom"
import { chevronIcon } from "./icons"
import type { Route } from "../shell/route"
import address from "../shell/address"

export interface Mounted {
  readonly destroy: () => void
}

export const inert: Mounted = { destroy: () => {} }

// A page with its code loaded, drawn in one synchronous step.
export type Draw = (root: HTMLElement) => Mounted

/** Back, or the fallback route when the tab has nothing to go back to (a deep link opened cold). */
export const backButton = (label: string, fallback: Route): HTMLElement =>
  el("button", {
    type: "button",
    class: "back",
    onclick: () => address.back(fallback),
  }, chevronIcon(), label)
