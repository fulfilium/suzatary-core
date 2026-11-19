// The rehearsal worker drives each page's class through its rehearsal (`Hosted::rehearsal`) as `painter/run.ts` does.
// V8 compiles a wasm function at its first call, into the module every thread shares. No page is drawn before the
// rehearsal ends (`program.module` awaits it), so a visit finds compiled what the rehearsal's inputs reach: each hold's
// `Loading` before its answer, and a stream reading holding a store's window of blocks' HTML. What only a later reading
// reaches compiles on the page's thread. It always says it is done and closes: a failed rehearsal costs only warmth.

import deliver, { type Written, written } from "../painter/deliver"

type Glue = typeof import("./views.js")

// The worker's own scope, typed by what this file uses of it (the program's lib is the DOM's).
type Scope = {
  addEventListener: (t: "message", f: (e: MessageEvent<WebAssembly.Module>) => void) => void
  postMessage: (said: null) => void
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
      for (const input of inputs) {
        deliver(page, written(input))
        page.effects()
      }
    } catch (e) {
      console.warn("rehearsal", e)
    } finally {
      page?.free()
    }
  }
}

scope.addEventListener("message", e =>
  void rehearse(e.data)
    .catch((err: unknown) => console.warn("rehearsal", err))
    .finally(() => {
      scope.postMessage(null)
      close()
    }),
)
