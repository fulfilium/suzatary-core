// The secretary beside a page is the pinned leaf `#/aside` (`app/pages/src/desk.rs`, `Dock`). It stands by itself only
// on a page about one thing when the conversation holds talk about it (`talk.ts`). A press holds for that page alone,
// in this tab; nothing about it is kept on the device. The width is read only as the chrome the window has.

import nav from "../shell/nav"
import { referenceInView } from "./inview"
import { print } from "./refs"
import type { Route } from "../shell/route"
import type { Dock } from "../wire/generated/pages"

export namespace aside {
  // The whole conversation drawn because nothing in it is about the page is `Quiet`; `Unheard` until the device read it.
  export type Talk = "Quiet" | "Talking"
  export type Heard = Talk | "Unheard"

  export type Press = "open" | "closed"

  export type Presses = { readonly [key: string]: Press }

  // The window's chrome: a phone's header has no door for a page of many things, whose own door is the bar's Secretary.
  export type Chrome = "Wide" | "Phone"

  // Where a person stands, as the secretary beside the pages reads it: on the secretary's own pages; on the secretary
  // itself; on a page of many things, keyed by its destination, so a calendar's next week is still the same page; or on
  // a page about one thing, keyed by its reference.
  type Standing =
    | { readonly type: "Own" }
    | { readonly type: "Itself"; readonly about: string }
    | { readonly type: "Many"; readonly key: string }
    | { readonly type: "One"; readonly key: string }

  // `named` is the reference the page in view names itself by, where it does: a message's is its row's hash, whatever
  // id its address used, so the talk is asked about the text the rounds carry.
  const standing = (route: Route, chain: string, named: string | null = null): Standing => {
    if (route.type === "aside") return { type: "Itself", about: route.about }
    const at = nav.activeFor(route)
    if (at === "secretary") return { type: "Own" }
    const r = referenceInView(route, chain)
    return r.type === "one" ? { type: "One", key: named ?? print(r.reference) } : { type: "Many", key: at }
  }

  // The one door that shows the secretary, the header's, and the presses its show makes: on a page about one thing
  // under either chrome, on a page of many things under the wide one; none elsewhere.
  export type Door = { readonly type: "None" } | { readonly type: "Header"; readonly shows: Presses }

  // The say on the secretary beside the page in view, its door, and the reference whose talk the say read (none where a
  // press or the page decides).
  export type Said = { readonly dock: Dock; readonly door: Door; readonly asks: string | null }

  const ABSENT: Dock = { type: "Absent" }
  const NONE: Door = { type: "None" }

  const many = (key: string, chrome: Chrome, presses: Presses): Said => {
    switch (chrome) {
      case "Phone":
        return { dock: ABSENT, door: NONE, asks: null }
      case "Wide":
        return { dock: presses[key] === "open" ? { type: "Held", about: "" } : ABSENT, door: { type: "Header", shows: { ...presses, [key]: "open" } }, asks: null }
    }
  }

  /** The one rule: held where a press opened it, or, on a page about one thing that no press closed, where its talk
   * shows it; absent elsewhere, on the secretary's own pages, and on a page of many things under a phone's chrome,
   * whose door is the bar's Secretary; the secretary itself as it stands. */
  export const of = (route: Route, chrome: Chrome, chain: string, presses: Presses, talk: (about: string) => Heard, named: string | null = null): Said => {
    const s = standing(route, chain, named)
    const shows = (key: string): Presses => ({ ...presses, [key]: "open" })
    switch (s.type) {
      case "Own":
        return { dock: ABSENT, door: NONE, asks: null }
      case "Itself":
        return { dock: { type: "Held", about: s.about }, door: NONE, asks: null }
      case "Many":
        return many(s.key, chrome, presses)
      case "One": {
        const press = presses[s.key]
        const held = press === undefined ? talk(s.key) === "Talking" : press === "open"
        return { dock: held ? { type: "Held", about: s.key } : ABSENT, door: { type: "Header", shows: shows(s.key) }, asks: press === undefined ? s.key : null }
      }
    }
  }

  /** The secretary about `about` hidden from the page in view: the press is its reference's, or, for the whole
   * conversation, the page's of many things it stood beside, never a page it was not about. */
  export const hidden = (about: string, route: Route, chain: string, presses: Presses): Presses => {
    if (about !== "") return { ...presses, [about]: "closed" }
    const s = standing(route, chain)
    switch (s.type) {
      case "Own":
      case "Itself":
      case "One":
        return presses
      case "Many":
        return { ...presses, [s.key]: "closed" }
    }
  }

  /** The secretary about `about` arrived at by a new entry of the history: the person asked for it beside the page it is
   * about. */
  export const went = (about: string, presses: Presses): Presses => (about === "" || presses[about] === "open" ? presses : { ...presses, [about]: "open" })
}
export default aside
