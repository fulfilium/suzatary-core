// The page the app runs in, drawn by the app itself: `index.html` is the same file for every build (the worker keeps one
// and serves it to whichever build boots), so it holds only the loader, and everything a build reads by name, the
// stylesheets and every host element, is that build's own.

import { el } from "../view/dom"

export namespace page {
  const bar = (id: string, cls: string) => el("div", { id, class: cls, role: "status", "aria-live": "polite", hidden: true })

  // The sticky header, which covers the top of the page's own scroll box.
  let made: HTMLElement | undefined
  export const header = (): HTMLElement =>
    (made ??= el(
      "header",
      null,
      el("a", { class: "brand", href: "#/" }, "Suzatary"),
      // One bar of words: across the top on a wide screen, fixed to the foot on a phone (`shell/nav.ts`).
      el("nav", { id: "tabbar", class: "tabbar", "aria-label": "sections" }),
      // The door that shows the secretary beside the page (`main.ts`), where it is not drawn; it hides itself. Before
      // the live mark, so the mark stands at the header's end whether the door is drawn or not.
      el("button", { id: "beside", type: "button", class: "secretarytoggle", "aria-label": "Show the secretary", hidden: true }, "Secretary"),
      // Acts this browser has taken and not yet handed over, hidden at zero (`shell/pending.ts`).
      el("span", { id: "live", class: "live", title: "connecting…", "aria-label": "connecting…" }),
    ))

  // The body's hosts, each filled by the module named beside it.
  const body = (): Node[] => [
    // The first thing Tab reaches: nine pages of navigation before the content is a page nobody reaches with a keyboard.
    el("a", { class: "skip", href: "#view" }, "Skip to the page"),
    header(),
    // How far each store's sync has come, in bytes (`shell/syncing.ts`).
    bar("syncing", "syncbar"),
    // A refusal and a chain move are announced, not merely drawn: the only place the app says a write was refused.
    bar("toast", "toast"),
    el("div", { id: "needsyou", class: "needsyouhost", "aria-live": "assertive" }),
    // The cache is honest only if the page says it is showing one (`shell/stale.ts`).
    bar("stale", "stalebar"),
    // What this device's readings left out, never silently (`shell/leftout.ts`).
    bar("excluded", "excludedbar"),
    // Said, never failed: a browser that cannot sign reads only.
    bar("signing", "excludedbar signingbar"),
    // The build this page started on, or a newer one to load (`app/newbuild.ts`).
    bar("updated", "excludedbar updatedbar"),
    // tabindex=-1 so the router can put the keyboard here when a page has no heading of its own (`view/focus.ts`).
    el("main", { id: "view", tabindex: "-1" }),
    // What a page draws as the window's (a sheet, a scrim), above every tile (`shell/tiles.ts`).
    el("div", { id: "layers" }),
  ]

  // This build's stylesheets, beside its `app.js`; settled once each has loaded or failed, so the body is drawn styled.
  const styled = (base: string): Promise<void> =>
    Promise.all(
      ["tokens.css", "style.css"].map(
        f =>
          new Promise<void>(done => {
            const link = el("link", { rel: "stylesheet", href: new URL(f, base).href, onload: () => done(), onerror: () => done() })
            document.head.appendChild(link)
          }),
      ),
    ).then(() => {})

  // `base` is the URL of this build's `app.js`. A stylesheet already in the head is a build's that did not start (the
  // loader fell back), so it goes before this build's are linked.
  export const draw = async (base: string): Promise<void> => {
    for (const old of document.head.querySelectorAll('link[rel="stylesheet"]')) old.remove()
    await styled(base)
    document.body.replaceChildren(...body())
  }
}
export default page
