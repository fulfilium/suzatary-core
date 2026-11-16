// The rehearsal worker: handed the views module the page compiled, it drives each page's class through its rehearsal
// (`Hosted::rehearsal`) as the painter drives it (`painter/run.ts`): constructed, its first paint, each input with its
// effects, and freed. V8 compiles each wasm function on its first call, and compiled code is the module's, shared by every
// thread that instantiates it, so the page's thread finds compiled every function a first visit calls, the class's
// exports and its drop included. Whatever happens it then says it is done and closes: a rehearsal that fails costs the
// page nothing but that warmth.

type Glue = typeof import("./views.js")

// The worker's own scope, typed by what this file uses of it (the program's lib is the DOM's).
type Scope = {
  addEventListener: (t: "message", f: (e: MessageEvent<WebAssembly.Module>) => void) => void
  postMessage: (said: null) => void
}
const scope: Scope = self

type Rehearsal = { readonly flags: unknown; readonly inputs: readonly unknown[] }

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
        page.input(JSON.stringify(input))
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
