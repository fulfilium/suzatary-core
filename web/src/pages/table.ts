// Every screen's page by its tag; the router never switches on it.

import type { Draw } from "../view/page"
import type { Screen } from "../shell/layout"
import { hrefOf } from "../shell/route"
import { el } from "../view/dom"
import frame from "../view/frame"
import reading from "../algebra/reading"
import program from "./program"
import time from "../app/time"
import app from "../app/app"

export namespace pages {
  type Screens = { readonly [K in Screen["type"]]: Extract<Screen, { readonly type: K }> }
  export type Table = { readonly [K in keyof Screens]: (s: Screens[K]) => Draw | Promise<Draw> }

  const unknown = (hash: string): frame.Page<string> => ({
    className: "unknownpage",
    title: "No such page",
    legend: h => `Nothing is routed at ${h}.`,
    reads: () => reading.constant(hash),
    view: () => el("p", null, el("a", { href: hrefOf({ type: "todos" }) }, "Back to the todos")),
  })

  export const table: Table = {
    // The list stays mounted for every list screen of its chain: a todo selected in its pane, a share into its sheet.
    todos: s => {
      const chain = s.chain === "" ? app.now().chain : s.chain
      return program.mount("TodosPage", { chain }, { absorbs: next => next.type === "todos" && (next.chain === "" || next.chain === chain) })
    },
    inbox: s => program.mount("InboxPage", { open: s.open ?? "" }),
    todo: s => program.mount("TodoPage", { chain: app.now().chain, id: s.id, pushed: true }),
    typst: () => program.mount("TypstPage", { chain: app.now().chain }, { absorbs: next => next.type === "typst" }),
    mail: s => program.mount("MailPage", { query: s.query, account: s.account }),
    message: s => program.mount("MessagePage", { id: s.id }),
    thread: s => program.mount("ThreadPage", { id: s.id }),
    about: () => program.mount("AboutPage", {}),
    settings: () => program.mount("SettingsPage", {}),
    memory: () => program.mount("MemoryPage", {}),
    briefings: () => program.mount("BriefingsPage", {}),
    briefing: s => program.mount("BriefingPage", { date: s.date }),
    // "" is today, on the owner's clock.
    calendar: s => program.mount("CalendarPage", { from: s.date === "" ? time.date(app.now().zone) : s.date }),
    secretary: s => program.mount("SecretaryPage", { topics: [...s.topics] }),
    conversation: s =>
      program.mount("ConversationPage", { at: { type: "Page", id: s.id, refs: [...s.refs], focus: s.focus === true, at: s.at ?? null } }),
    newtopic: () => program.mount("DraftPage", {}),
    unknown: s => root => frame.open(root, unknown(s.hash)),
  }

  // Whether a screen draws the shell's fold (`app/store`): its rows, or the titles of the todos it names.
  export type Fold = "Shown" | "Unshown"
  export const folds: { readonly [K in keyof Screens]: Fold } = {
    todos: "Shown",
    inbox: "Shown",
    todo: "Shown",
    typst: "Shown",
    mail: "Unshown",
    message: "Shown",
    thread: "Unshown",
    about: "Unshown",
    settings: "Unshown",
    memory: "Unshown",
    briefings: "Unshown",
    briefing: "Unshown",
    calendar: "Unshown",
    secretary: "Unshown",
    conversation: "Shown",
    newtopic: "Unshown",
    unknown: "Unshown",
  }

  export const load = <K extends Screen["type"]>(s: Screens[K] & { readonly type: K }): Draw | Promise<Draw> => table[s.type](s)
}
export default pages
