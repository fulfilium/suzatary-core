// The only module that writes `location` or `history`: every entry carries its place in the log, so the browser's own
// back and forward move the log's pointer, and the hash it shows is always an address's print.

import app from "../app/app"
import routelog from "./routelog"
import kept from "../view/kept"
import { type Address, type Route, bare, hrefOfAddress, parseAddress } from "./route"

export namespace address {
  const markOf = (state: unknown): number | null => {
    const at = typeof state === "object" && state !== null && "at" in state ? state.at : undefined
    return typeof at === "number" && Number.isInteger(at) && at >= 0 ? at : null
  }

  const keep = (): void => app.dispatch({ type: "Kept", kept: kept.read() })

  const mark = (push: boolean): void => {
    const l = app.now().log
    const href = hrefOfAddress(routelog.address(l))
    if (push) window.history.pushState({ at: l.at }, "", href)
    else window.history.replaceState({ at: l.at }, "", href)
  }

  // To `to`, as a new entry of the history.
  export const went = (to: Address): void => {
    const was = app.now().log
    keep()
    app.dispatch({ type: "Went", to })
    if (app.now().log.at !== was.at) mark(true)
  }

  // To `to`, in the place of the entry the reader is at.
  export const replaced = (to: Address): void => {
    app.dispatch({ type: "Replaced", to })
    mark(false)
  }

  // To a page's own address, its workspace's tiles read from it.
  export const go = (route: Route): void => went(bare(route))
  export const replace = (route: Route): void => replaced(bare(route))

  // Back within the log; a place opened cold has nothing behind it, so back is the fallback, in its stead.
  export const back = (fallback: Route): void => {
    if (!routelog.canBack(app.now().log)) return replace(fallback)
    keep()
    window.history.back()
  }

  // A document load, so the box answers the path (a consent it sends on); what the page keeps is kept first, for the way back.
  export const leave = (href: string): void => {
    keep()
    window.location.assign(href)
  }

  // The browser moved (its back or forward, a hash typed or set): the log reads the entry it moved to.
  const moved = (state: unknown): void => {
    const to = parseAddress(window.location.hash)
    const at = markOf(state)
    if (at === null) {
      // An entry with no mark naming where the reader is (the hash set to the place it already shows) is marked.
      if (routelog.same(to, app.address(app.now()))) return mark(false)
      keep()
      app.dispatch({ type: "Went", to })
      return mark(false)
    }
    if (at === app.now().log.at && routelog.same(to, app.address(app.now()))) return
    keep()
    app.dispatch({ type: "Moved", at, to })
    if (app.now().log.at !== at) mark(false)
  }

  // The address a click follows inside this document, or null: a same-document link the reader did not ask the browser
  // to open otherwise (a new tab, a download).
  export const followed = (e: MouseEvent): string | null => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return null
    const a = e.target instanceof Element ? e.target.closest("a[href]") : null
    const href = a?.getAttribute("href") ?? ""
    if (!href.startsWith("#") || (a?.getAttribute("target") ?? "") !== "" || a?.hasAttribute("download") === true) return null
    return href
  }

  // A part of a page that opens a page and is no link (a card) asks for it with this event, from its own element: the
  // tile that holds it opens the page from itself, as for a link, and outside every tile it is the address.
  const OPEN = "suzatary:open"
  export const open = (from: Element, href: string): void =>
    void from.dispatchEvent(new CustomEvent<string>(OPEN, { bubbles: true, cancelable: true, detail: href }))
  const asked = (e: Event): string | null => (e instanceof CustomEvent && typeof e.detail === "string" && !e.defaultPrevented ? e.detail : null)
  export const hearOpens = (on: EventTarget, heard: (href: string, e: Event) => void): void =>
    on.addEventListener(OPEN, e => {
      const href = asked(e)
      if (href !== null) heard(href, e)
    })

  // A link outside every tile (the bar, the brand) is a write of the address register.
  const clicked = (e: MouseEvent): void => {
    const href = followed(e)
    if (href === null) return
    e.preventDefault()
    went(parseAddress(href))
  }

  // The first place is the address the tab was opened at: a deep link is an address printed.
  export const install = (): Address => {
    window.history.scrollRestoration = "manual"
    const first = parseAddress(window.location.hash)
    window.addEventListener("popstate", e => moved(e.state))
    window.addEventListener("hashchange", () => moved(window.history.state))
    document.addEventListener("click", clicked)
    hearOpens(document, href => went(parseAddress(href)))
    return first
  }

  // Once the log holds the first place, its entry is marked as the log's.
  export const settle = (): void => mark(false)
}
export default address
