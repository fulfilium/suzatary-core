// Every act a page takes, in data: a closed vocabulary, what of the history holds each, and how each reaches the box.
// A monotone act is sealed into the tab's own replica; every other is a request the box answers under its lock.

import type { AttachmentRow, ConversationPayload, FactId, ProposalRow } from "../wire/generated/wire"
import { acknowledgeProposal, dismissProposal, dropSession, editQuestion, eraseTurns, fixProposal, forgetFact, regenerate, renameConversation, retractSummary, say, stopAnswer, tapProposal } from "../wire/api"
import { parsed } from "../chat/refs"
import type vouch from "./vouch"
import type topics from "./topics"

export namespace asks {
  // `from` is the signature the card saw; any later climb of the machine holds the ask.
  export type Proposal = { readonly conversation: string; readonly index: number; readonly from: string }
  // Each performed at most once under the inbox's lock.
  export type Answer =
    | { readonly type: "Tap"; readonly note: string; readonly always: boolean; readonly endorsement?: vouch.Endorsement }
    | { readonly type: "Dismiss"; readonly reason: string }
    | { readonly type: "Acknowledge" }
    | { readonly type: "Drop"; readonly reason: string }
    | { readonly type: "Fix" }
  // The box's line for the question is at or past `after`.
  export type Question = {
    readonly conversation: string
    readonly after: number
    readonly text: string
    readonly references: readonly string[]
    readonly files: readonly AttachmentRow[]
  }

  export type Ask =
    | { readonly type: "Seal"; readonly chain: string; readonly event: string }
    // `at` makes the same write asked again after the history took the first a new ask.
    | { readonly type: "Topic"; readonly write: topics.Write; readonly at: string }
    | { readonly type: "Answer"; readonly at: Proposal; readonly answer: Answer }
    | { readonly type: "Question"; readonly kind: "Say" | "Edit"; readonly question: Question }
    | { readonly type: "Round"; readonly kind: "Stop" | "Regenerate"; readonly conversation: string; readonly after: number }
    | { readonly type: "Rename"; readonly conversation: string; readonly title: string }
    | { readonly type: "Forget"; readonly fact: FactId }
    | { readonly type: "Retract"; readonly node: string }
    | { readonly type: "Erase"; readonly conversation: string; readonly turns: readonly number[] }

  type Of<K extends Ask["type"]> = Extract<Ask, { type: K }>
  export type Alg<R> = { readonly [K in Ask["type"]]: (a: Of<K>) => R }

  export const fold = <R>(alg: Alg<R>) => (a: Ask): R => {
    switch (a.type) {
      case "Seal": return alg.Seal(a)
      case "Topic": return alg.Topic(a)
      case "Answer": return alg.Answer(a)
      case "Question": return alg.Question(a)
      case "Round": return alg.Round(a)
      case "Rename": return alg.Rename(a)
      case "Forget": return alg.Forget(a)
      case "Retract": return alg.Retract(a)
      case "Erase": return alg.Erase(a)
    }
  }

  // `null` for a conversation the tab does not hold.
  export type Reading = (conversation: string) => ConversationPayload | null

  export const signature = (p: ProposalRow): string =>
    `${p.state}:${p.tries}:${p.coding?.state ?? ""}`

  export const about = (conversation: string, index: number) => (a: Ask): boolean =>
    a.type === "Answer" && a.at.conversation === conversation && a.at.index === index

  const moved = (at: Proposal, row: ProposalRow | undefined): boolean => row !== undefined && signature(row) !== at.from

  // A request whose record no replica carries (a forgotten fact, a retracted summary) is held only by the box's answer.
  export const holds = (read: Reading): ((a: Ask) => boolean) =>
    fold<boolean>({
      Seal: () => false,
      Topic: () => false,
      Answer: a => moved(a.at, read(a.at.conversation)?.proposals.find(p => p.index === a.at.index)),
      Question: ({ question: q }) => read(q.conversation)?.turns.some(t => t.index >= q.after && t.who === "person" && t.text === q.text) === true,
      Round: a => (read(a.conversation)?.total ?? 0) > a.after,
      Rename: a => read(a.conversation)?.title === a.title,
      Forget: () => false,
      Retract: () => false,
      Erase: a => {
        const turns = read(a.conversation)?.turns ?? []
        return a.turns.every(n => turns.some(t => t.index === n && t.gone !== null))
      },
    })

  // Where an ask's record lands, which a settle reads.
  export const conversation = fold<string | null>({
    Seal: () => null,
    Topic: () => null,
    Answer: a => a.at.conversation,
    Question: a => a.question.conversation,
    Round: a => a.conversation,
    Rename: a => a.conversation,
    Forget: () => null,
    Retract: () => null,
    Erase: a => a.conversation,
  })

  const answered = ({ conversation: c, index: i }: Proposal, a: Answer, secret?: string): Promise<unknown> =>
    a.type === "Tap" ? tapProposal(c, i, a.note, secret, a.always, a.endorsement)
    : a.type === "Dismiss" ? dismissProposal(c, i, a.reason)
    : a.type === "Acknowledge" ? acknowledgeProposal(c, i)
    : a.type === "Fix" ? fixProposal(c, i)
    : dropSession(c, i, a.reason)

  const asked = (kind: "Say" | "Edit", { conversation: c, text, references, files, after }: Question): Promise<unknown> =>
    kind === "Say" ? say(c, text, parsed(references), files, after) : editQuestion(c, text, parsed(references), files)

  // A secret's value travels beside the ask, never in it, so no key, set or log holds it. A seal and a topic are the
  // shell's, since each writes the tab's own replica.
  export const send = (a: Exclude<Ask, { type: "Seal" | "Topic" }>, secret?: string): Promise<unknown> =>
    a.type === "Answer" ? answered(a.at, a.answer, secret)
    : a.type === "Question" ? asked(a.kind, a.question)
    : a.type === "Round" ? (a.kind === "Stop" ? stopAnswer(a.conversation) : regenerate(a.conversation))
    : a.type === "Rename" ? renameConversation(a.conversation, a.title)
    : a.type === "Erase" ? eraseTurns(a.conversation, a.turns)
    : a.type === "Retract" ? retractSummary(a.node)
    : forgetFact(a.fact)
}
export default asks
