// A cloud coding session under the proposal that started it: its states, its answers, its card.

import type { CodingFile, CodingSession } from "../wire/generated/wire"
import { clear, el } from "../view/dom"
import placeholder from "../view/placeholder"
import cardui from "./cardui"
import intents from "../prodrome/intents"

export namespace coding {
  /** The routine's token, settable once `[coding]` names a routine. */
  export const TOKEN = "SUZATARY_ROUTINE_TOKEN"
  export const TOKEN_HINT =
    "The token of the routine a tapped coding session fires. It is shown once when generated at claude.ai/code/routines; set it here, never in a chat."

  export type Session = CodingSession
  export type Answer = { type: "Drop"; reason: string }
  export type Tally = { files: number; added: number; removed: number }

  /** Asking the box; `null` for a login that may not. */
  export type Effects = {
    answer: ((a: Answer) => void) | null
  }

  type Of<K extends Session["state"]> = Extract<Session, { state: K }>
  export type Alg<R> = { [K in Session["state"]]: (s: Of<K>) => R }

  export const fold = <R>(alg: Alg<R>) => (s: Session): R =>
    s.state === "fired" ? alg.fired(s)
    : s.state === "reported" ? alg.reported(s)
    : s.state === "merged" ? alg.merged(s)
    : alg.dropped(s)

  // A monoid under addition, `zero` its identity.
  export const zero: Tally = { files: 0, added: 0, removed: 0 }
  export const combine = (...ts: Tally[]): Tally =>
    ts.reduce((a, b) => ({ files: a.files + b.files, added: a.added + b.added, removed: a.removed + b.removed }), zero)
  export const tally = (f: CodingFile): Tally => ({ files: 1, added: f.added ?? 0, removed: f.removed ?? 0 })
  export const total = (stat: readonly CodingFile[]): Tally => combine(...stat.map(tally))

  export const answers = fold<Answer["type"][]>({
    fired: () => ["Drop"],
    reported: () => ["Drop"],
    merged: () => [],
    dropped: () => [],
  })

  /** What a keyed redraw compares: a card changes exactly when this does. */
  export const signature = (s: Session | null): string =>
    s === null ? "" : s.state

  export const headline = fold<string>({
    fired: s => `Running since ${s.at}`,
    reported: s => `Reported on ${s.branch}`,
    merged: s => `Merged at ${s.sha.slice(0, 12)}`,
    dropped: s => `Dropped: ${s.reason}`,
  })


  const lines = (f: CodingFile): string =>
    f.added === null || f.removed === null ? "binary" : `+${f.added} −${f.removed}`

  const stat = (fs: readonly CodingFile[]): Node => {
    const t = total(fs)
    return el(
      "div",
      { class: "codingstat" },
      el("p", { class: "codingtotal" }, `${t.files} file${t.files === 1 ? "" : "s"}, +${t.added} −${t.removed}`),
      el("ul", { class: "codingfiles" }, fs.map(f => el("li", null, el("code", null, f.path), " ", el("span", { class: "codinglines" }, lines(f))))),
    )
  }

  const body = fold<(Node | null)[]>({
    fired: s => [el("p", null, el("a", { href: s.url, target: "_blank", rel: "noopener", class: "codingurl" }, s.url))],
    reported: s => [
      el("p", { class: "codingpath" }, el("code", null, s.path)),
      stat(s.stat),
      el("pre", { class: "proposalbody codingreport" }, s.report),
    ],
    merged: () => [],
    dropped: () => [],
  })

  // Whether an answer is pending is the intents'; whether the drop field is open, and its words, the card's register.
  const controls = (asked: intents.Shown, ui: cardui.Ui, redraw: () => void, send: (a: Answer) => void): Node[] => {
    const go = (e: cardui.Event): void => (ui.go(e), redraw())
    const sent = (a: Answer): void => (ui.go({ type: "Closed" }), send(a))
    const dropping = (): Node[] => {
      const why = cardui.field(ui, () => el("input", { type: "text", class: "dismissreason", placeholder: "Why not? (done by hand, wrong task…)" }))
      const confirm = () => (ui.get().text.trim() === "" ? why.focus() : sent({ type: "Drop", reason: ui.get().text.trim() }))
      return [why, el("button", { type: "button", class: "act act-drop confirming", onclick: confirm }, "Dismiss"), el("button", { type: "button", class: "act act-keep", onclick: () => go({ type: "Closed" }) }, "Keep")]
    }
    const idle = (): Node[] => [
      el("button", { type: "button", class: "act act-drop", onclick: () => go({ type: "Opened", step: { type: "Saying", verb: "drop" } }) }, "Dismiss"),
    ]
    return intents.fold<Node[]>(asked, {
      Absent: () => (cardui.is(ui.get(), "Saying", "drop") ? dropping() : idle()),
      Pending: () => [placeholder.inline()],
      Taken: () => [placeholder.inline()],
      Failed: f => [el("span", { class: "problem" }, f.lost ? `No answer (${f.e}); asking again never does it twice.` : f.e), ...idle()],
    })
  }

  /** The session's card; answers only for a login that may, and only those its state admits. */
  export const view = (s: Session, asked: intents.Shown, ui: cardui.Ui, fx: Effects): HTMLElement => {
    const row = el("div", { class: "proposalact-row" })
    const node = el(
      "section",
      { class: `codingsession codingsession-${s.state}`, "data-session": s.id },
      el("p", { class: "codinghead" }, headline(s)),
      ...body(s),
    )
    const answer = fx.answer
    if (answer === null || answers(s).length === 0) return node
    // The card says its register and the intent it draws as data, so what a reader sees can be asked of the page.
    const draw = (): void => {
      node.dataset["ui"] = cardui.print(ui.get())
      node.dataset["asked"] = asked.type
      clear(row)
      row.append(...controls(asked, ui, draw, answer))
    }
    draw()
    node.append(row)
    return node
  }
}
export default coding
