// The topic bar: the topics in use, the most rounds first, each a link that puts it in the filter or takes it out, so
// a combination is an address and Back moves it.

import type { TopicInUse } from "../wire/generated/wire"
import { el } from "../view/dom"
import { hrefOf } from "../shell/route"

export namespace topicbar {
  export type Effects = { readonly rename: (topic: string) => void }

  // Each topic once, sorted, so one filter has one print.
  export const filter = (topics: readonly string[]): readonly string[] => [...new Set(topics)].sort()

  export const toggled = (chosen: readonly string[], topic: string): readonly string[] =>
    chosen.includes(topic) ? chosen.filter(t => t !== topic) : filter([...chosen, topic])

  export const href = (chosen: readonly string[]): string => hrefOf({ type: "secretary", refs: [], topics: chosen })

  // A chosen topic no round carries (a link to a name renamed since) stays in the bar, so it can be taken out.
  export const shown = (topics: readonly TopicInUse[], chosen: readonly string[]): readonly TopicInUse[] => [
    ...topics,
    ...chosen.filter(c => !topics.some(t => t.topic === c)).map((topic): TopicInUse => ({ topic, rounds: 0, disputed: [] })),
  ]

  // The names in words: "a", "a and b", "a, b and c".
  export const and = (names: readonly string[]): string =>
    names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`

  const chip = (t: TopicInUse, chosen: readonly string[], writes: boolean, fx: Effects): HTMLElement => {
    const on = chosen.includes(t.topic)
    const disputed = t.disputed.length > 0
    return el(
      "span",
      { class: on ? "topicchip on" : "topicchip", "data-topic": t.topic },
      el(
        "a",
        { class: "topiclink", href: href(toggled(chosen, t.topic)), "aria-pressed": on ? "true" : "false", role: "button" },
        t.topic,
        el("span", { class: "topiccount", "aria-label": `${t.rounds} round${t.rounds === 1 ? "" : "s"}` }, String(t.rounds)),
      ),
      disputed ? el("span", { class: "topicdisputed", title: `Renamed apart, also called ${and(t.disputed)}: rename it to settle` }, "≠") : null,
      writes && (on || disputed)
        ? el("button", { type: "button", class: "topicrename", "aria-label": `Rename ${t.topic}`, onclick: () => fx.rename(t.topic) }, "Rename")
        : null,
    )
  }

  export const view = (topics: readonly TopicInUse[], chosen: readonly string[], writes: boolean, fx: Effects): HTMLElement | null => {
    const all = shown(topics, chosen)
    return all.length === 0 ? null : el("nav", { class: "topicbar", "aria-label": "Topics" }, all.map(t => chip(t, chosen, writes, fx)))
  }
}
export default topicbar
