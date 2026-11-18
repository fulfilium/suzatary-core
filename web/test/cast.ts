/** The made-up world every test and scene draws from: `suzatary_connect::cast`, as `web_vectors.rs` writes it. */

import { readFileSync } from "node:fs"

export namespace cast {
  export type Person = { name: string; email: string }
  export type Owner = { actor: string; name: string; email: string }
  export type Sub = { text: string; done: boolean }
  export type Todo = {
    id: string
    created: string
    body: string
    strong: string
    detail: string
    category: string
    rationale: string[]
    subtodos: Sub[]
  }
  export type Mail = { from: Person; subject: string; msgid: string; body: string }
  export type CastFile = { name: string; media: string; content: string }
  export type Notice = { mail: Mail; file: CastFile }
  export type Todos = { urgent: Todo; flat: Todo; checklist: Todo; resolved: Todo; high: Todo; agents: Todo }
  export type Cast = {
    owner: Owner
    counsel: Person
    accountant: Person
    friend: Person
    city: string
    todos: Todos
    mail: [Mail, Mail]
    proposed: Todo
    unpriced: Todo
    notice: Notice
  }

  export const world: Cast = JSON.parse(readFileSync("app/core/tests/web/cast.json", "utf8"))
}
export default cast
