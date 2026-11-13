// The one door between the route register and the browser's address bar: the only module that writes `location` or
// `history`. Every entry this tab writes carries its place in the log, so the browser's own back and forward move the
// log's pointer, and the hash it shows is always a route's print.

import app from "../app/app"
import routelog from "./routelog"
import kept from "../view/kept"
import { type Route, hrefOf, parseRoute } from "./route"

export namespace address {
  const markOf = (state: unknown): number | null => {
    const at = typeof state === "object" && state !== null && "at" in state ? state.at : undefined
    return typeof at === "number" && Number.isInteger(at) && at >= 0 ? at : null
  }

  const keep = (): void => app.dispatch({ type: "Kept", kept: kept.read() })

  const mark = (push: boolean): void => {
    const l = app.now().log
    const href = hrefOf(routelog.route(l))
    if (push) window.history.pushState({ at: l.at }, "", href)
    else window.history.replaceState({ at: l.at }, "", href)
  }

  export const go = (route: Route): void => {
    const was = app.now().log
    keep()
    app.dispatch({ type: "Went", route })
    if (app.now().log.at !== was.at) mark(true)
  }

  export const replace = (route: Route): void => {
    app.dispatch({ type: "Replaced", route })
    mark(false)
  }

  // Back within the log; a place opened cold has nothing behind it, so back is the fallback, in its stead.
  export const back = (fallback: Route): void => {
    if (!routelog.canBack(app.now().log)) return replace(fallback)
    keep()
    window.history.back()
  }

  // Out of the app, to a path the box serves as a document (a consent the box sends on): a document load, so the box
  // answers it. What the page keeps is kept first, for the way back.
  export const leave = (href: string): void => {
    keep()
    window.location.assign(href)
  }

  // The browser moved (its back or forward, a hash typed or set): the log reads the entry it moved to.
  const moved = (state: unknown): void => {
    const route = parseRoute(window.location.hash)
    const at = markOf(state)
    if (at === null) {
      // An entry with no mark naming where the reader is (the hash set to the place it already shows) is marked.
      if (routelog.same(route, app.route(app.now()))) return mark(false)
      keep()
      app.dispatch({ type: "Went", route })
      return mark(false)
    }
    if (at === app.now().log.at && routelog.same(route, app.route(app.now()))) return
    keep()
    app.dispatch({ type: "Moved", at, route })
    if (app.now().log.at !== at) mark(false)
  }

  // A same-document link is a write of the route register: the click goes through `go`, unless the reader asked the
  // browser for something else (a new tab, a download).
  const clicked = (e: MouseEvent): void => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
    const a = e.target instanceof Element ? e.target.closest("a[href]") : null
    const href = a?.getAttribute("href") ?? ""
    if (!href.startsWith("#") || (a?.getAttribute("target") ?? "") !== "" || a?.hasAttribute("download") === true) return
    e.preventDefault()
    go(parseRoute(href))
  }

  // The first place is the address the tab was opened at: a deep link is a route printed.
  export const install = (): Route => {
    window.history.scrollRestoration = "manual"
    const first = parseRoute(window.location.hash)
    window.addEventListener("popstate", e => moved(e.state))
    window.addEventListener("hashchange", () => moved(window.history.state))
    document.addEventListener("click", clicked)
    return first
  }

  // Once the log holds the first place, its entry is marked as the log's.
  export const settle = (): void => mark(false)
}
export default address
