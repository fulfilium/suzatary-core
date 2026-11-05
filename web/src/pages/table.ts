// Every screen's page by its tag; the router never switches on it. The Typst list, chain and usage are lazy chunks: big
// and rarely landed on. The rest share most code with the shell, so splitting them gains little.

import type { Mounted } from "../view/page"
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
  export type Table = { readonly [K in keyof Screens]: (root: HTMLElement, s: Screens[K]) => Promise<Mounted> }

  const unknown = (hash: string): frame.Page<string> => ({
    className: "unknownpage",
    title: "No such page",
    legend: h => `Nothing is routed at ${h}.`,
    reads: () => reading.constant(hash),
    view: () => el("p", null, el("a", { href: hrefOf({ type: "todos" }) }, "Back to the todos")),
  })

  export const table: Table = {
    todos: (root, s) => todos.mount(root, s.selected, s.prefill),
    inbox: (root, s) => program.mount(root, "InboxPage", { open: s.open ?? "" }),
    todo: (root, s) => frame.mount(root, todoview.page(s.id, true)),
    typst: (root, s) => import("./typstlist").then(m => m.default.mount(root, s.open)),
    mail: (root, s) => program.mount(root, "MailPage", { query: s.query, account: s.account }),
    message: (root, s) => program.mount(root, "MessagePage", { id: s.id }),
    thread: (root, s) => program.mount(root, "ThreadPage", { id: s.id }),
    chain: (root, s) => import("./chain").then(m => m.mount(root, s.focus)),
    usage: root => import("./usage").then(m => m.default.mount(root)),
    about: root => program.mount(root, "AboutPage", {}),
    settings: root => program.mount(root, "SettingsPage", {}),
    memory: root => program.mount(root, "MemoryPage", {}),
    briefings: root => program.mount(root, "BriefingsPage", {}),
    briefing: (root, s) => program.mount(root, "BriefingPage", { date: s.date }),
    // "" is today, on the owner's clock.
    calendar: (root, s) => program.mount(root, "CalendarPage", { from: s.date === "" ? time.date(app.now().zone) : s.date }),
    secretary: (root, s) => program.mount(root, "SecretaryPage", { topics: [...s.topics] }),
    conversation: (root, s) =>
      program.mount(root, "ConversationPage", { at: { type: "Page", id: s.id, refs: [...s.refs], focus: s.focus === true, at: s.at ?? null } }),
    newtopic: root => program.mount(root, "DraftPage", {}),
    unknown: (root, s) => frame.mount(root, unknown(s.hash)),
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

  export const mount = <K extends Screen["type"]>(root: HTMLElement, s: Screens[K] & { readonly type: K }): Promise<Mounted> =>
    table[s.type](root, s)
}
export default pages
