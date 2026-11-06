// The secretary beside every page but its own at 62rem and up, open by the owner's rule (`aside.ts`). Below that width
// it is out of the document with no conversation mounted, not merely hidden.

import ui from "../app/ui"
import registers from "../app/registers"
import aside from "./aside"
import { el } from "../view/dom"
import { inert, type Mounted } from "../view/page"
import program from "../pages/program"
import d from "../wire/decoder"

const presses = ui.cell(registers.panel)

const told = d.obj({ talk: d.oneOf<aside.Talk>({ Quiet: null, Talking: null }) })

type Chrome = { readonly root: HTMLElement; readonly toggle: HTMLElement; readonly body: HTMLElement }

// `talk` is as the program last told.
type Beside = { page: aside.Page; talk: aside.Talk; readonly mounted: Promise<Mounted>; readonly unheard: () => void }

// The module's one mutable cell; `beside` is replaced whole on every mount.
const cell: { chrome: Chrome | null; beside: Beside | null } = { chrome: null, beside: null }

const apply = (c: Chrome, b: Beside): void => {
  const open = aside.shown(b.page, presses.get(), b.talk)
  c.root.classList.toggle("panelclosed", !open)
  c.toggle.setAttribute("aria-expanded", open ? "true" : "false")
  c.toggle.textContent = open ? "Hide" : "Secretary"
}

const chromeOf = (): Chrome => {
  if (cell.chrome !== null) return cell.chrome
  const toggle = el("button", { type: "button", class: "panelcollapse" })
  const body = el("div", { class: "panelhost" })
  const root = el("aside", { class: "secretarypanel", "aria-label": "Secretary" }, toggle, body)
  document.body.appendChild(root)
  cell.chrome = { root, toggle, body }
  return cell.chrome
}

const unmount = (): void => {
  const b = cell.beside
  if (b === null) return
  b.unheard()
  void b.mounted.then(m => m.destroy())
  cell.beside = null
}

window.addEventListener("pagehide", unmount)

const mount = (c: Chrome, page: aside.Page): void => {
  // The program draws the panel's scroll box; it scrolls itself, never the page under it.
  const box = (): Element => c.body.querySelector(".panelscroll") ?? c.body
  const b: Beside = {
    page,
    talk: "Quiet",
    // A conversation swapped out while the module loads is never drawn.
    mounted: program
      .mount("ConversationPage", { at: { type: "Panel", about: page.type === "One" ? page.key : "" } }, {
        box,
        told: said => {
          const t = told(said)
          if (t.type === "Val" && cell.beside === b && b.talk !== t.v.talk) (b.talk = t.v.talk, apply(c, b))
        },
      })
      .then(draw => (cell.beside === b ? draw(c.body) : inert)),
    unheard: presses.hear(() => (cell.beside === b ? apply(c, b) : undefined)),
  }
  c.toggle.onclick = () => presses.set(aside.pressed(b.page, presses.get(), b.talk))
  cell.beside = b
  apply(c, b)
}

/** Remounts only when what the conversation is about moved, so moving between pages of many things keeps it. */
export function updatePanel(placement: aside.Placement): void {
  if (placement.type === "Absent") {
    unmount()
    cell.chrome?.root.remove()
    return
  }
  const c = chromeOf()
  if (!c.root.isConnected) document.body.appendChild(c.root)
  const about = (page: aside.Page): string => (page.type === "One" ? page.key : "")
  const b = cell.beside
  if (b !== null && about(b.page) === about(placement.page)) {
    b.page = placement.page
    return apply(c, b)
  }
  unmount()
  mount(c, placement.page)
}
