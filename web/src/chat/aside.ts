// The panel opens by itself only on a page about one thing when the conversation holds talk about it. A press holds for
// that page alone, in this tab; nothing about the panel is kept on the device.

import app from "../app/app"
import nav from "../shell/nav"
import { referenceInView } from "./inview"
import { print, type Reference } from "./refs"

export namespace aside {
  // A page of many things is keyed by its destination, so a calendar's next week is still the same page.
  export type Page =
    | { readonly type: "Many"; readonly key: string }
    | { readonly type: "One"; readonly key: string; readonly reference: Reference }

  /** Never beside the secretary's own pages. */
  export type Placement = { readonly type: "Absent" } | { readonly type: "Beside"; readonly page: Page }

  // The whole conversation drawn because nothing in it is about the page is `Quiet`.
  export type Talk = "Quiet" | "Talking"

  export type Press = "open" | "closed"

  export type Presses = { readonly [key: string]: Press }

  export const placement = (a: app.App): Placement => {
    const at = nav.activeFor(app.route(a), a.chain)
    if (!a.wide || at === "secretary") return { type: "Absent" }
    const r = referenceInView(app.screen(a), a.chain)
    return { type: "Beside", page: r.type === "one" ? { type: "One", key: print(r.reference), reference: r.reference } : { type: "Many", key: at } }
  }

  export const shown = (page: Page, presses: Presses, talk: Talk): boolean => {
    const said: Press | "unpressed" = presses[page.key] ?? "unpressed"
    switch (said) {
      case "open":
        return true
      case "closed":
        return false
      case "unpressed":
        return page.type === "One" && talk === "Talking"
    }
  }

  export const pressed = (page: Page, presses: Presses, talk: Talk): Presses => ({
    ...presses,
    [page.key]: shown(page, presses, talk) ? "closed" : "open",
  })
}
export default aside
