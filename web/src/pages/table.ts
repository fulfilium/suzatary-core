// Every screen's page by its tag; the router never switches on it. The Typst list, chain and usage are lazy chunks: big
// and rarely landed on. The rest share most code with the shell, so splitting them gains little.

import type { Draw } from "../view/page"
import type { Screen } from "../shell/layout"
import { hrefOf } from "../shell/route"
import { el } from "../view/dom"
import frame from "../view/frame"
import reading from "../algebra/reading"
import todos from "./todos"
import todoview from "../todo/todoview"
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
    todos: s => root => todos.mount(root, s.selected, s.prefill),
    inbox: s => program.mount("InboxPage", { open: s.open ?? "" }),
    todo: s => root => frame.open(root, todoview.page(s.id, true)),
    typst: s => import("./typstlist").then(m => root => m.default.mount(root, s.open)),
    mail: s => program.mount("MailPage", { query: s.query, account: s.account }),
    message: s => program.mount("MessagePage", { id: s.id }),
    thread: s => program.mount("ThreadPage", { id: s.id }),
    chain: s => import("./chain").then(m => root => m.mount(root, s.focus)),
    usage: () => import("./usage").then(m => root => m.default.mount(root)),
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
    chain: "Unshown",
    usage: "Unshown",
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
