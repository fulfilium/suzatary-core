// The rehearsal worker drives each page's class through its rehearsal (`Hosted::rehearsal`) as `painter/run.ts` does.
// V8 compiles a wasm function at its first call, into the module every thread shares, so a visit finds compiled what
// the rehearsal's inputs reach; what only a later reading reaches compiles on the page's thread.

import deliver, { type Written, written } from "../painter/deliver"
import time from "../app/time"
import rehearsal from "./rehearsal"

type Glue = typeof import("./views.js")

// The worker's own scope, typed by what this file uses of it (the program's lib is the DOM's).
type Scope = {
  addEventListener: (t: "message", f: (e: MessageEvent<rehearsal.Said>) => void) => void
  postMessage: (told: rehearsal.Told) => void
}
const scope: Scope = self

type Rehearsal = { readonly flags: unknown; readonly inputs: readonly Written[] }

const rehearse = async (module: WebAssembly.Module): Promise<void> => {
  const glue: Glue = await import("./views.js")
  await glue.default({ module_or_path: module })
  // The stream first: it is the page a visit most often opens on, and its first reading the heaviest.
  for (const Page of [glue.SecretaryPage, glue.TodosPage, glue.ConversationPage, glue.InboxPage, glue.NeedsYouPage, glue.SettingsPage]) {
    const text = Page.rehearsal()
    if (text === undefined) continue
    // A page whose rehearsal is refused, its flags included, costs only its own warmth: the next is still rehearsed.
    let page: InstanceType<typeof Page> | undefined
    try {
      const { flags, inputs }: Rehearsal = JSON.parse(text)
      page = new Page(JSON.stringify(flags))
      page.first()
      page.effects()
      page.subs()
      for (const input of inputs) {
        deliver(page, written(input))
        page.effects()
        page.subs()
      }
    } catch (e) {
      console.warn("rehearsal", e)
    } finally {
      page?.free()
    }
  }
}

scope.addEventListener("message", e => {
  const said = e.data
  switch (said.type) {
    case "Zone":
      return scope.postMessage({ type: "Offsets", zone: said.zone, offsets: time.offsets({ name: said.zone }, ...rehearsal.span(said.now)) })
    case "Module":
      return void rehearse(said.module)
        .catch((err: unknown) => console.warn("rehearsal", err))
        .finally(() => {
          scope.postMessage({ type: "Rehearsed" })
          close()
        })
  }
})
