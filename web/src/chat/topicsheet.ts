// The sheet a topic is written through: a rename of a topic in the bar, or a topic added to one round, the topics in use
// offered first. What is typed is checked by the core's own rule as it is typed, so a name the type refuses is said
// before anything is written.

import type { StreamRound, TopicInUse } from "../wire/generated/wire"
import type secretaryState from "../pages/secretary.state"
import { el } from "../view/dom"

export namespace topicsheet {
  export type Effects = {
    readonly check: (text: string) => void
    readonly write: (topic: string) => void
    readonly close: () => void
  }
  export type Open = Exclude<secretaryState.Sheet, { readonly type: "Closed" }>

  const inUse = (bar: readonly TopicInUse[], topic: string): boolean => bar.some(t => t.topic === topic)

  // What a write of what is typed would do, or why there is none.
  const said = (s: Open, bar: readonly TopicInUse[], on: readonly string[]): string => {
    switch (s.check.type) {
      case "Blank":
        return s.type === "Adding" ? "Pick one, or write a new one." : ""
      case "Breaks":
        return s.check.why
      case "Fits": {
        const t = s.check.topic
        return s.type === "Adding" ? (on.includes(t) ? "This round has it." : inUse(bar, t) ? `Adds “${t}”.` : `“${t}” is a new topic.`)
          : t === s.topic ? "That is its name now."
          : inUse(bar, t) ? `“${s.topic}” and “${t}” become one topic, “${t}”, on every round of either.`
          : `Every round on “${s.topic}” keeps it, renamed “${t}”.`
      }
    }
  }

  const ready = (s: Open, on: readonly string[]): string | null =>
    s.check.type !== "Fits" ? null
    : s.type === "Renaming" ? (s.check.topic === s.topic ? null : s.check.topic)
    : on.includes(s.check.topic) ? null
    : s.check.topic

  export const view = (s: Open, bar: readonly TopicInUse[], round: StreamRound | undefined, fx: Effects): HTMLElement => {
    const renaming = s.type === "Renaming"
    const on = round?.topics ?? []
    const field = el("input", {
      type: "text",
      class: "topicfield",
      "aria-label": renaming ? "New name" : "Topic",
      value: renaming ? s.topic : "",
      autocapitalize: "none",
      autocomplete: "off",
      spellcheck: "false",
    })
    field.addEventListener("input", () => fx.check(field.value))
    const topic = ready(s, on)
    const form = el(
      "form",
      { class: "topicform" },
      field,
      el("button", { type: "submit", class: "act act-topicwrite", disabled: topic === null }, renaming ? "Rename" : "Add"),
      el("button", { type: "button", class: "act", onclick: fx.close }, "Cancel"),
    )
    form.addEventListener("submit", e => {
      e.preventDefault()
      if (topic !== null) fx.write(topic)
    })
    const offered = renaming ? [] : bar.filter(t => !on.includes(t.topic))
    const sheet = el(
      "div",
      { class: "topicsheet", role: "dialog", "aria-label": renaming ? `Rename ${s.topic}` : "Add a topic" },
      el("h2", { class: "topicsheethead" }, renaming ? `Rename “${s.topic}”` : "Add a topic"),
      offered.length === 0
        ? null
        : el("div", { class: "chiprow topicoffers" }, offered.map(t => el("button", { type: "button", class: "chipbtn", onclick: () => fx.write(t.topic) }, t.topic))),
      form,
      el("p", { class: s.check.type === "Breaks" ? "topicsaid problem" : "topicsaid legend", role: "status" }, said(s, bar, on)),
    )
    sheet.addEventListener("keydown", e => {
      if (e.key === "Escape") fx.close()
    })
    return sheet
  }
}
export default topicsheet
