// The one script the page names: it picks a build and imports that build's app, which draws the page (`app/loader.ts`
// decides which). A start runs the build the worker last heard the box serve, answered from its cache with no wait on
// the network, and the build of the worker itself when that one does not load, so a page runs offline. The box's first
// read starts here too, while the app is on its way. Every file the app loads after resolves against its own URL, so a
// page never asks for another build's file.

import shell from "./app/shell"
import primed from "./app/primed"
import loader from "./app/loader"
import { remembered } from "./app/chain"
import pushtoken from "./app/pushtoken"

// Said on the document as it starts, so a harness can tell a page booting from one that runs no app.
document.documentElement.dataset["booting"] = ""

// The Android app's push token, held for the app's module, which may be imported after the app sends it.
pushtoken.hold()

// The list's fold, the first thing the app reads on the chain this browser last read.
primed.start(primed.fold(remembered()))

const at = (b: string, file: string): string => new URL(shell.under(b, file), document.baseURI).href

const link = (attributes: Record<string, string>): void => {
  const l = document.createElement("link")
  for (const [k, v] of Object.entries(attributes)) l.setAttribute(k, v)
  document.head.appendChild(l)
}

void loader.start(
  {
    // A box that names no build gives no answer, never `dev`; nor does one that has not answered in a few seconds, so a
    // network that connects and never answers starts the worker's build.
    served: () =>
      fetch(new URL(shell.CURRENT, document.baseURI).href, { cache: "no-store", signal: AbortSignal.timeout(4000) })
        .then(r => (r.ok ? r.text() : ""))
        .then(t => (t.trim() === "" ? null : t.trim())),
    worker: () => {
      const c = "serviceWorker" in navigator ? navigator.serviceWorker.controller : null
      return c === null ? null : shell.named(shell.buildOf(c.scriptURL))
    },
    // The app links its stylesheets once it runs (`shell/page.ts`); asked for here, they arrive beside it.
    preload: (b, chunks) => {
      for (const f of ["tokens.css", "style.css"]) link({ rel: "preload", as: "style", href: at(b, f) })
      for (const f of ["app.js", ...chunks]) link({ rel: "modulepreload", href: at(b, f) })
    },
    // No bound: an import given up on still runs once it arrives, and would run beside the build tried after it. A URL,
    // not a template literal, so the bundler leaves the app out of the loader.
    load: b =>
      import(at(b, "app.js")).then(
        () => true,
        () => false,
      ),
    // The page holds nothing of a build's, so the loader draws its own line.
    said: text => {
      document.documentElement.dataset["started"] = "refused"
      const p = document.createElement("p")
      p.className = "problem"
      p.textContent = text
      document.body.replaceChildren(p)
    },
  },
  __PRELOAD__,
)
