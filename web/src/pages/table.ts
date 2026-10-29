// Every screen's page, keyed by its tag: the router looks a screen up here and never switches on it. The four pages that
// carry most code of their own and are rarely where a start lands (the Typst list, the chain, usage and settings) are
// chunks of their own, loaded as each is first drawn, under the build's name as the app is (`scripts/build-web.sh`).
// The rest share most of their code with the shell, so splitting them would cut the app into many more files for
// little.

import type { Mounted } from "../view/page"
import type { Screen } from "../shell/layout"
import { hrefOf } from "../shell/route"
import { el } from "../view/dom"
import frame from "../view/frame"
import reading from "../algebra/reading"
import todos from "./todos"
import about from "./about"
import briefing from "./briefing"
import briefings from "./briefings"
import calendar from "./calendar"
import conversation from "./conversation"
import inbox from "./inbox"
import memory from "./memory"
import secretary from "./secretary"
import todoview from "../todo/todoview"
import { mount as mail } from "./mail"
import { mount as message } from "./message"
import { mount as thread } from "./thread"

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
    inbox: (root, s) => inbox.mount(root, s.open),
    todo: (root, s) => frame.mount(root, todoview.page(s.id, true)),
    typst: (root, s) => import("./typstlist").then(m => m.default.mount(root, s.open)),
    mail: (root, s) => mail(root, { query: s.query, account: s.account }),
    message: (root, s) => message(root, s.id),
    thread: (root, s) => thread(root, s.id),
    chain: (root, s) => import("./chain").then(m => m.mount(root, s.focus)),
    usage: root => import("./usage").then(m => m.default.mount(root)),
    about: root => about.mount(root),
    settings: root => import("./settings").then(m => m.default.mount(root)),
    memory: root => memory.mount(root),
    briefings: root => briefings.mount(root),
    briefing: (root, s) => briefing.mount(root, s.date),
    calendar: (root, s) => calendar.mount(root, s.date),
    secretary: (root, s) => secretary.mount(root, s.topics),
    conversation: (root, s) =>
      conversation.mount(root, s.id, { refs: s.refs, focus: s.focus === true, ...(s.at === undefined ? {} : { at: s.at }) }),
    newtopic: root => frame.mount(root, conversation.draft()),
    unknown: (root, s) => frame.mount(root, unknown(s.hash)),
  }

  // Whether a screen draws the fold the shell keeps (`app/store`): its rows, or the titles of the todos it names.
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
