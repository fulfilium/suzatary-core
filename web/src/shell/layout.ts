// Where a hash lands, given the width: a todo is a pane beside the list at 62rem and up, a page below it.

import share from "../todo/share"
import type { Route } from "./route"

/** The same edge `style.css` breaks at; `main.ts` watches `matchMedia` at exactly this width. */
export const WIDE_MIN_PX = 992

export const isWide = (width: number): boolean => width >= WIDE_MIN_PX

/**
 * What is mounted: the route itself, except the list. `#/`, `#/add` and a todo on a wide screen are
 * all the list of the reader's chain, carrying the todo in the pane and the body the add sheet opens
 * with.
 */
export type Screen =
  | { readonly type: "todos"; readonly selected: string; readonly prefill: string }
  | Exclude<Route, { readonly type: "todos" | "add" }>

const list = (selected = "", prefill = ""): Screen => ({ type: "todos", selected, prefill })

export const screenFor = (route: Route, wide: boolean): Screen =>
  route.type === "todos" ? list()
  : route.type === "add" ? list("", share.body(route.text, route.title))
  : route.type === "todo" && wide ? list(route.id)
  : route

// "Ask the secretary" with something to ask is the default conversation, so the place written is that one and the
// log never holds a route that only forwards.
export const forwarded = (route: Route): Route =>
  route.type === "secretary" && (route.refs.length > 0 || route.focus === true)
    ? { type: "conversation", id: "default", refs: route.refs, ...(route.focus === true ? { focus: true } : {}) }
    : route
