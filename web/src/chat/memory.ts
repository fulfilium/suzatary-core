/** The secretary's memory: its page's body, and the receipt line a round draws. */

import type { FactId, FactKind, FactRow, FactSaid, MemoryPayload, Receipt, RecordPayload, RecordSummary, SaidOn } from "../wire/generated/wire"
import empty from "../view/empty"
import type { Child } from "../view/dom"
import { el } from "../view/dom"
import marks from "../view/marks"
import { hrefOf } from "../shell/route"
import { toast } from "../view/toast"
import intentsync from "../prodrome/intentsync"
import lo from "../algebra/lo"

export namespace memory {
  export type Group = { kind: FactKind; label: string; facts: FactRow[] }
  /** Forgetting, for a login that writes; `null` offers no Forget at all. */
  export type Forget = ((id: FactId) => void) | null

  /** Retracting, for a login that writes; `null` offers no Wrong at all. */
  export type Retract = ((node: string) => void) | null

  /** A summary retracted is one the next reading no longer shows, so `after` reads again; a refusal says so. */
  export const retracts = (writes: boolean, after: () => unknown): Retract =>
    writes ? node => void intentsync.asked({ type: "Retract", node }).then(after, (e: unknown) => toast(`Could not retract the summary: ${lo.reason(e)}`)) : null

  /** A fact forgotten is a fact the next reading no longer holds, so `after` reads again; a refusal names the fact. */
  export const forgets = (writes: boolean, after: () => unknown): Forget =>
    writes ? id => void intentsync.asked({ type: "Forget", fact: id }).then(after, (e: unknown) => toast(`Could not forget fact ${id}: ${lo.reason(e)}`)) : null

  const LABELS: { readonly [K in FactKind]: string } = {
    owner: "About you",
    person: "People",
    preference: "Preferences",
    standing: "Standing instructions",
  }
  const ORDER: readonly FactKind[] = ["owner", "person", "preference", "standing"]
  export const KINDS: readonly { kind: FactKind; label: string }[] = ORDER.map(kind => ({ kind, label: LABELS[kind] }))

  /** Facts by kind, in `KINDS`' order, oldest first, empty kinds left out. */
  export const group = (p: MemoryPayload): Group[] =>
    KINDS.map(k => ({ ...k, facts: p.facts.filter(f => f.kind === k.kind) })).filter(g => g.facts.length > 0)

  /** The round that said it: the conversation windowed at the turn, or the card a tap answered. */
  export const saidAt = (s: SaidOn): string =>
    s.kind === "turn"
      ? hrefOf({ type: "conversation", id: s.conversation, refs: [], at: s.turn })
      : hrefOf({ type: "conversation", id: s.conversation, refs: [`proposal:${s.conversation}/${s.proposal}`] })

  export const saidIn = (s: SaidOn): string =>
    s.kind === "turn" ? `said in ${s.conversation}, turn ${s.turn}` : `tapped in ${s.conversation}, card ${s.proposal}`

  const said = (s: SaidOn): Node => el("a", { class: "memorysaid", href: saidAt(s) }, saidIn(s))

  /** Where a fact comes from: the round that said it, or the facts it replaced and where each of theirs was said. */
  const provenance = (s: FactSaid): Node =>
    s.how === "said"
      ? said(s.on)
      : el(
          "span",
          { class: "memoryprovenance" },
          el("a", { class: "memorymerged", href: saidAt(s.round) }, `merged from ${s.from.map(id => `fact ${id}`).join(", ")}`),
          s.origins.map(said),
        )

  const forgetButton = (id: FactId, forget: (id: FactId) => void): Node =>
    el("button", { type: "button", class: "act forget", onclick: () => forget(id) }, "Forget")

  const fact = (f: FactRow, forget: Forget): Node =>
    el(
      "li",
      { class: "memoryfact", "data-fact": f.id },
      el("p", { class: "memoryfacttext" }, f.fact),
      el(
        "div",
        { class: "memoryfactfoot" },
        provenance(f.said),
        forget === null ? null : forgetButton(f.id, forget),
      ),
    )

  const card = (g: Group, forget: Forget): Node =>
    el(
      "section",
      { class: "memorygroup", "data-kind": g.kind },
      el(
        "div",
        { class: "memoryhead" },
        el("span", { class: "grouplabel" }, g.label),
        el("span", { class: "groupcount" }, g.facts.length === 1 ? "1 fact" : `${g.facts.length} facts`),
      ),
      el("ul", { class: "memoryfacts" }, g.facts.map(f => fact(f, forget))),
    )

  export const legend = (p: MemoryPayload): string =>
    `${p.facts.length} of ${p.cap} facts, background for every conversation.`

  const summary = (r: RecordSummary, retract: Retract): Node =>
    el(
      "li",
      { class: "memoryfact recordsummary", "data-node": r.node },
      el("p", { class: "memoryfacttext" }, r.text),
      el(
        "div",
        { class: "memoryfactfoot" },
        el("span", { class: "memorysaid" }, r.lines === 1 ? "of 1 line" : `of ${r.lines} lines`),
        retract === null ? null : el("button", { type: "button", class: "act retract", onclick: () => retract(r.node) }, "Wrong"),
      ),
    )

  /** The summaries of their words a new conversation is told, each with Wrong: the box summarises those lines again. */
  const record = (r: RecordPayload, retract: Retract): Child =>
    r.summaries.length === 0
      ? null
      : el(
          "section",
          { class: "memorygroup recordgroup" },
          el(
            "div",
            { class: "memoryhead" },
            el("span", { class: "grouplabel" }, "Summaries of what you said"),
            el("span", { class: "groupcount" }, r.summaries.length === 1 ? "1 summary" : `${r.summaries.length} summaries`),
          ),
          el("ul", { class: "memoryfacts" }, r.summaries.map(s => summary(s, retract))),
        )

  export const view = (p: MemoryPayload, r: RecordPayload, forget: Forget, retract: Retract): Child => [
    p.facts.length === 0 ? empty.note("Nothing remembered yet.") : group(p).map(g => card(g, forget)),
    record(r, retract),
    marks.callout(
      "Forgetting erases",
      el("p", null, "A forgotten fact leaves the memory's file, not only this list. A conversation that quoted it keeps its words. A summary marked Wrong is deleted and written again from your lines."),
    ),
  ]

  /** The line a round shows for what it remembered, never folded into its trace; its words only while the memory holds them. */
  export const receipt = (r: Receipt, forget: Forget): HTMLElement =>
    el(
      "p",
      { class: "receipt", "data-fact": r.fact, "data-state": r.state },
      el("a", { href: hrefOf({ type: "memory" }) }, "Remembered"),
      ": ",
      r.state === "standing"
        ? el("span", { class: "receipttext" }, r.text)
        : el("span", { class: "receiptfact" }, `fact ${r.fact}`),
      r.state === "forgotten" ? [marks.diamond(), el("span", { class: "receiptstate" }, "forgotten")]
      : r.state === "merged" ? [marks.diamond(), el("span", { class: "receiptstate" }, `merged into fact ${r.into}`)]
      : forget === null ? null
      : [marks.diamond(), forgetButton(r.fact, forget)],
    )
}
export default memory
