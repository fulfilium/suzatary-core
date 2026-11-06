// The rehearsal worker: handed the views module the page compiled, it runs each page's rehearsal on it, so V8 compiles
// the functions a first visit calls here and the page's thread finds them compiled (compiled code is the module's, shared
// by every thread that instantiates it). Whatever happens it then says it is done and closes: a rehearsal that fails
// costs the page nothing but that warmth.

type Glue = typeof import("./views.js")

// The worker's own scope, typed by what this file uses of it (the program's lib is the DOM's).
type Scope = {
  addEventListener: (t: "message", f: (e: MessageEvent<WebAssembly.Module>) => void) => void
  postMessage: (said: null) => void
}
const scope: Scope = self

const rehearse = async (module: WebAssembly.Module): Promise<void> => {
  const glue: Glue = await import("./views.js")
  await glue.default({ module_or_path: module })
  // The stream first: it is the page a visit most often opens on, and its first reading the heaviest.
  for (const page of [glue.SecretaryPage, glue.ConversationPage, glue.InboxPage, glue.NeedsYouPage, glue.SettingsPage]) {
    try {
      page.rehearse()
    } catch (e) {
      console.warn("rehearsal", e)
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
