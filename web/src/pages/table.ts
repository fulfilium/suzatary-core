// Every route's page by its tag; the router never switches on it.

import type { Draw } from "../view/page"
import { hrefOf, type Route } from "../shell/route"
import { el } from "../view/dom"
import frame from "../view/frame"
import reading from "../algebra/reading"
import program from "./program"
import time from "../app/time"
import app from "../app/app"
import { listTitle } from "../app/chain"

export namespace pages {
  type Routes = { readonly [K in Route["type"]]: Extract<Route, { readonly type: K }> }
  export type Table = { readonly [K in keyof Routes]: (s: Routes[K], at: program.Around) => Draw | Promise<Draw> }

  const unknown = (hash: string): frame.Page<string> => ({
    className: "unknownpage",
    title: "No such page",
    legend: h => `Nothing is routed at ${h}.`,
    reads: () => reading.constant(hash),
    view: () => el("p", null, el("a", { href: hrefOf({ type: "todos" }) }, "Back to the todos")),
  })

  export const table: Table = {
    // A share opens the list with its sheet up; the list reads the route itself (`Local::Route`).
    todos: (_, at) => list(at),
    add: (_, at) => list(at),
    inbox: (s, at) => program.mount("InboxPage", { open: s.open ?? "" }, at),
    todo: (s, at) => program.mount("TodoPage", { chain: app.now().chain, id: s.id }, at),
    typst: (_, at) => program.mount("TypstPage", { chain: app.now().chain }, at),
    mail: (s, at) => program.mount("MailPage", { query: s.query, account: s.account }, at),
    message: (s, at) => program.mount("MessagePage", { id: s.id }, at),
    thread: (s, at) => program.mount("ThreadPage", { id: s.id }, at),
    about: (_, at) => program.mount("AboutPage", {}, at),
    settings: (_, at) => program.mount("SettingsPage", {}, at),
    memory: (_, at) => program.mount("MemoryPage", {}, at),
    briefings: (_, at) => program.mount("BriefingsPage", {}, at),
    briefing: (s, at) => program.mount("BriefingPage", { date: s.date }, at),
    // "" is today, on the owner's clock.
    calendar: (s, at) => program.mount("CalendarPage", { from: s.date === "" ? time.date(app.now().zone) : s.date }, at),
    secretary: (s, at) => program.mount("SecretaryPage", { topics: [...s.topics] }, at),
    conversation: (s, at) =>
      program.mount("ConversationPage", { at: { type: "Page", id: s.id, refs: [...s.refs], focus: s.focus === true, at: s.at ?? null } }, at),
    newtopic: (_, at) => program.mount("DraftPage", {}, at),
    conversations: (_, at) => program.mount("ConversationsPage", {}, at),
    aside: (s, at) => program.mount("ConversationPage", { at: { type: "Panel", about: s.about } }, at),
    unknown: s => root => frame.open(root, unknown(s.hash)),
  }

  const list = (at: program.Around): Promise<Draw> => {
    const a = app.now()
    return program.mount("TodosPage", { chain: a.chain, title: listTitle(a.chain, a.served) }, at)
  }

  // The routes the list draws, its title among it (`listTitle`, read when the page is made).
  export const lists = (r: Route): boolean => r.type === "todos" || r.type === "add"

  // The pages that take a move to another route of theirs on themselves, reading it (`Local::Route`), and stay
  // mounted: the list a share opens and the address it puts back, and the Typst list's open todo.
  export const absorbs = (was: Route, now: Route): boolean => (lists(was) && lists(now)) || (was.type === "typst" && now.type === "typst")

  // Whether a route's page draws the shell's fold (`app/store`): its rows, or the titles of the todos it names.
  export type Fold = "Shown" | "Unshown"
  export const folds: { readonly [K in keyof Routes]: Fold } = {
    todos: "Shown",
    add: "Shown",
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
    conversations: "Unshown",
    aside: "Unshown",
    unknown: "Unshown",
  }

  export const load = <K extends Route["type"]>(s: Routes[K] & { readonly type: K }, at: program.Around): Draw | Promise<Draw> =>
    table[s.type](s, at)
}
export default pages
