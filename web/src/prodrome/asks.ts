// Every act a page takes, in data: a closed vocabulary, what of the history holds each, and how each reaches the box.
// A monotone act is sealed into the tab's own replica; every other is a request the box answers under its lock.

import type { AttachmentRow, ConversationPayload, FactId, ProposalRow } from "../wire/generated/wire"
import { acknowledgeProposal, dismissProposal, dropSession, editQuestion, eraseTurns, forgetFact, regenerate, renameConversation, retractSummary, say, stopAnswer, tapProposal } from "../wire/api"
import { parsed } from "../chat/refs"
import type vouch from "./vouch"
import type topics from "./topics"

export namespace asks {
  // A proposal the way its card saw it: `from` is its signature then, and any later climb of the machine holds the ask.
  export type Proposal = { readonly conversation: string; readonly index: number; readonly from: string }
  // The owner's answers to one proposal, each performed at most once under the inbox's lock.
  export type Answer =
    | { readonly type: "Tap"; readonly note: string; readonly always: boolean; readonly endorsement?: vouch.Endorsement }
    | { readonly type: "Dismiss"; readonly reason: string }
    | { readonly type: "Acknowledge" }
    | { readonly type: "Drop"; readonly reason: string }
  // A question the way its asker saw the conversation: `after` turns stood, so the box's line for it is at or past `after`.
  export type Question = {
    readonly conversation: string
    readonly after: number
    readonly text: string
    readonly references: readonly string[]
    readonly files: readonly AttachmentRow[]
  }

  export type Ask =
    | { readonly type: "Seal"; readonly chain: string; readonly event: string }
    // `at` is when it was asked, so the same write asked again after the history took the first is a new ask.
    | { readonly type: "Topic"; readonly write: topics.Write; readonly at: string }
    | { readonly type: "Answer"; readonly at: Proposal; readonly answer: Answer }
    | { readonly type: "Question"; readonly kind: "Say" | "Edit"; readonly question: Question }
    | { readonly type: "Round"; readonly kind: "Stop" | "Regenerate"; readonly conversation: string; readonly after: number }
    | { readonly type: "Rename"; readonly conversation: string; readonly title: string }
    | { readonly type: "Forget"; readonly fact: FactId }
    // A summary of the record the owner says is wrong, by its node's name.
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

  // What a reading of the replicas answers about one conversation; `null` for one the tab does not hold.
  export type Reading = (conversation: string) => ConversationPayload | null

  // What moves when the box records an answer to a proposal: its machine and the session it started.
  export const signature = (p: ProposalRow): string =>
    `${p.state}:${p.tries}:${p.coding?.state ?? ""}`

  // Every ask about one proposal's card.
  export const about = (conversation: string, index: number) => (a: Ask): boolean =>
    a.type === "Answer" && a.at.conversation === conversation && a.at.index === index

  const moved = (at: Proposal, row: ProposalRow | undefined): boolean => row !== undefined && signature(row) !== at.from

  // The proposal's row decides alone, so a card asks it of the row it draws.
  export const holdsRow = (row: ProposalRow) => (a: Ask): boolean => a.type === "Answer" && moved(a.at, row)

  // Whether the history holds the act: a monotone act once sealed, a request once the box's record of it is read.
  // A request whose record no replica carries (a forgotten fact, a retracted summary) is held only by the box's own answer.
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

  // The conversation an ask's record lands in, which a settle reads; none for a todo's seal, a fact or a summary.
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
    : dropSession(c, i, a.reason)

  const asked = (kind: "Say" | "Edit", { conversation: c, text, references, files, after }: Question): Promise<unknown> =>
    kind === "Say" ? say(c, text, parsed(references), files, after) : editQuestion(c, text, parsed(references), files)

  // How a request reaches the box: the route that names it. A secret's value travels beside the ask, never in it,
  // so no key, set or log holds it; a seal and a topic are the shell's, since each writes the tab's own replica.
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
