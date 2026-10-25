// One round of the stream: where it was asked and when, the question, the answer, and its topics as small chips a
// writer takes off one by one or adds to.

import type { StreamRound } from "../wire/generated/wire"
import { el } from "../view/dom"
import { crossIcon, plusIcon } from "../view/icons"
import { recent } from "../view/format"
import { hrefOf } from "../shell/route"
import { textWithReferences } from "./refs"
import { answerBlocks } from "./transcript"
import list from "./list"
import app from "../app/app"

export namespace streamround {
  export type Effects = {
    readonly add: (target: string) => void
    readonly remove: (target: string, topic: string) => void
  }

  // When it was asked, as the round says it on the owner's `today`.
  export const when = (r: StreamRound, today: string): string => recent(r.opening.at, today, app.now().zone)

  // What the drawing reads, so a round is drawn again only when one of these moved.
  export const sig = (r: StreamRound, writes: boolean, said: string): string =>
    JSON.stringify([r.title, r.opening.text, said, r.answer?.index ?? null, r.answer?.text ?? null, r.answer?.who === "model" ? r.answer.blocks.map(b => b.html) : [], r.topics, r.room, writes])

  const topic = (r: StreamRound, t: string, writes: boolean, fx: Effects): HTMLElement =>
    el(
      "span",
      { class: "roundtopic" },
      t,
      writes ? el("button", { type: "button", class: "roundtopicoff", "aria-label": `Take ${t} off this round`, onclick: () => fx.remove(r.target, t) }, crossIcon()) : null,
    )

  const answer = (r: StreamRound): HTMLElement | null =>
    r.answer === null ? null
    : r.answer.who === "model" && r.answer.blocks.length > 0 ? answerBlocks(r.answer)
    : el("div", { class: "turntext" }, r.answer.text)

  export const view = (r: StreamRound, writes: boolean, said: string, fx: Effects): HTMLElement =>
    el(
      "article",
      { class: "streamround", "data-target": r.target },
      el(
        "p",
        { class: "streammeta" },
        el("a", { href: hrefOf({ type: "conversation", id: r.conversation, refs: [], at: r.turn }) }, list.named(r.title)),
        el("span", { class: "streamwhen" }, said),
      ),
      el("div", { class: `turntext streamasked streamasked-${r.opening.who}` }, ...textWithReferences(r.opening.text, r.opening.who)),
      answer(r),
      r.topics.length === 0 && !writes
        ? null
        : el(
            "div",
            { class: "roundtopics" },
            r.topics.map(t => topic(r, t, writes, fx)),
            writes && r.room > 0
              ? el("button", { type: "button", class: "roundtopicadd", "aria-label": "Add a topic to this round", onclick: () => fx.add(r.target) }, plusIcon(), r.topics.length === 0 ? "Topic" : null)
              : null,
          ),
    )
}
export default streamround
