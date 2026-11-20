// An input handed to a page's class. An answer's text crosses beside its input (`Hosted::reply`), so the page parses it
// once into the type it reads, and nothing prints it again.

import type { Answer, Input, Reading } from "../wire/generated/pages"

export type Page = { readonly input: (json: string) => Uint8Array; readonly reply: (json: string, text: Uint8Array) => Uint8Array }

export const deliver = (page: Page, input: Input<unknown, unknown, unknown>): Uint8Array =>
  input.type === "Held" && input.reading.type === "Val" && input.reading.v.type === "Done"
    ? page.reply(JSON.stringify({ type: "Held", ask: input.ask, behind: input.behind }), input.reading.v.body)
    : input.type === "Answered" && input.answer.type === "Done"
      ? page.reply(JSON.stringify({ type: "Answered", ask: input.ask }), input.answer.body)
      : page.input(JSON.stringify(input))

// An input as a rehearsal file or a page's law writes it, an answer's body its JSON value.
type In = Input<unknown, unknown, unknown>
type WrittenAnswer = Exclude<Answer, { readonly type: "Done" }> | { readonly type: "Done"; readonly body: unknown }
export type Written =
  | Exclude<In, { readonly type: "Held" | "Answered" }>
  | (Omit<Extract<In, { readonly type: "Held" }>, "reading"> & { readonly reading: Reading<WrittenAnswer> })
  | (Omit<Extract<In, { readonly type: "Answered" }>, "answer"> & { readonly answer: WrittenAnswer })

const utf8 = new TextEncoder()

const answer = (a: WrittenAnswer): Answer => (a.type === "Done" ? { type: "Done", body: utf8.encode(JSON.stringify(a.body)) } : a)

// The input it is, its answer's body printed into the text a real answer crosses as.
export const written = (i: Written): In => {
  switch (i.type) {
    case "Held":
      return { ...i, reading: i.reading.type === "Val" ? { type: "Val", v: answer(i.reading.v) } : i.reading }
    case "Answered":
      return { ...i, answer: answer(i.answer) }
    case "Context":
    case "Heard":
    case "Opened":
    case "Lost":
      return i
  }
}
export default deliver
