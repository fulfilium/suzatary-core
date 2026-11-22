// The editor's Typst, in a worker the page can terminate: one restricted project (no `html`, no `plugin`, no file but
// the text), the same rule the box admits a text by, so a loop is a refused text and never a hung tab.

import edit from "./edit"

type Glue = typeof import("./typst.js")

const ready: Promise<{ glue: Glue; project: InstanceType<Glue["Project"]> }> = (async () => {
  const file = __CONTENT__["typst_bg.wasm"]
  const bytes = await fetch(new URL(`./${file}`, import.meta.url).href).then(r => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(`${file}: HTTP ${r.status} ${r.statusText}`))))
  const glue = await import("./typst.js")
  await glue.default({ module_or_path: bytes })
  return { glue, project: glue.Project.restricted() }
})()

const answer = async (ask: edit.Ask): Promise<edit.Answer> => {
  const { glue, project } = await ready
  const set = (source: string): string | undefined => project.set(edit.MAIN, source)
  switch (ask.type) {
    case "Shade": return { type: "Shade", spans: edit.spans(glue.highlight(ask.source)) }
    case "Check": {
      const refused = set(ask.source)
      return { type: "Check", checked: refused === undefined ? edit.checked(project.compile(edit.MAIN, "{}")) : edit.failed(refused) }
    }
    case "Complete": return { type: "Complete", offer: set(ask.source) === undefined ? edit.offer(project.complete(edit.MAIN, ask.cursor, ask.explicit)) : null }
    case "Hover": return { type: "Hover", text: set(ask.source) === undefined ? edit.hover(project.hover(edit.MAIN, ask.cursor)) : "" }
  }
}

// The DOM's types name `self` a window; a dedicated worker's scope answers on the same port it listens on.
const scope: { postMessage: (m: edit.Answered) => void; addEventListener: (t: "message", f: (e: MessageEvent<edit.Asked>) => void) => void } = self
scope.addEventListener("message", e => {
  const { id, ask } = e.data
  void answer(ask).then(
    a => scope.postMessage({ id, answer: a }),
    (x: unknown) => scope.postMessage({ id, answer: ask.type === "Check" ? { type: "Check", checked: edit.failed(String(x)) } : { type: "Shade", spans: [] } }),
  )
})
