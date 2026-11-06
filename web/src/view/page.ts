// What a mounted page answers to: a destroy when it is left, and maybe a hash it can absorb. The box moving is heard
// by what the page reads, never told to the page.

import { el } from "./dom"
import { chevronIcon } from "./icons"
import type { Screen } from "../shell/layout"
import type { Route } from "../shell/route"
import address from "../shell/address"

export interface Mounted {
  readonly destroy: () => void
  // True when the page took the change on itself (the list selecting a todo on a wide screen) and stays mounted.
  readonly absorb?: (screen: Screen) => boolean
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
