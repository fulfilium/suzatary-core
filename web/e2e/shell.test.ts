// The app shell and the beacon, in a browser: each build's files under its own name, a cold start on the build the box
// serves, a running page offered a new one and never reloaded without a tap, a tab's failure in the box's log.

import assert from "node:assert/strict"
import { copyFileSync, existsSync, readFileSync, readdirSync, renameSync, writeFileSync } from "node:fs"

import { DESKTOP, PHONE, WRITE_BASE, suite, trustThisDevice } from "./harness"
import shell from "../src/app/shell"

const need = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — scripts/web-e2e.sh sets it`)
  return v
}
const SHELL = need("E2E_BASE_SHELL")
const BUILD = need("E2E_SHELL_BUILD")
const LOG = need("E2E_WRITE_LOG")
const INSTANCE = need("E2E_SHELL_INSTANCE")
const DIST = need("E2E_SHELL_DIST")

const app = await suite("shell")

const controlledBy = (id: string) => `navigator.serviceWorker.controller?.scriptURL.includes("build=${id}") === true`
const cacheNames = () => app.page.evaluate<string[]>(`caches.keys()`)
const kept = (name: string) =>
  app.page.evaluate<string[]>(`caches.open(${JSON.stringify(name)}).then((c) => c.keys()).then((rs) => rs.map((r) => new URL(r.url).pathname))`)
const logged = (fragment: string) => readFileSync(LOG, "utf8").includes(fragment)
// This build's chunks, as its bundling named them: `<name>-<hash>.js`.
const CHUNKS = readdirSync(DIST).filter(shell.chunk)

app.scene("a stamped build keeps its whole shell in the cache named by it, and never the API", async () => {
  writeFileSync(BUILD, "e2e-1\n")
  await app.open(SHELL)
  await app.page.waitFor(controlledBy("e2e-1"), { timeout: 20000 })
  assert.deepEqual((await cacheNames()).sort(), [shell.HEARD, "suzatary-shell-e2e-1"])
  assert.deepEqual(await kept(shell.HEARD), ["/b/current"], "the box's build, as last heard")
  const paths = await kept("suzatary-shell-e2e-1")
  assert.ok(CHUNKS.length > 0, "the app is split into chunks")
  for (const f of ["/index.html", "/boot.js", ...["app.js", "prodrome.js", "prodrome_bg.wasm", "views.js", "views_bg.wasm", "style.css", "tokens.css", ...CHUNKS].map(f => `/b/e2e-1/${f}`)])
    assert.ok(paths.includes(f), `${f} is kept: ${paths.join(", ")}`)
  assert.ok(paths.some(p => p.startsWith("/b/e2e-1/fonts/")), "the fonts are kept under the build's name")
  // Every file the build ships is kept at install, kept on first use, or a path no build names: a page of an old build
  // loads only its own build's files, and the box has none of them once it moved on.
  const served = ["sw.js", "manifest.webmanifest", "icon.svg", "icon-192.png", "icon-512.png", "index.html", "boot.js"]
  // The build's directories (its fonts, its Typst kit, `.well-known`) have no extension; a file of it has one.
  const loose = readdirSync(DIST)
    .filter(f => !f.startsWith(".") && f.includes(".") && !f.endsWith(".br") && !f.endsWith(".map"))
    .filter(f => !served.includes(f) && !shell.FILES.includes(f) && !shell.KEPT.includes(f) && !shell.chunk(f))
  assert.deepEqual(loose, [], "a file the build ships that no build's worker keeps")
  for (const f of ["views.js", "views_bg.wasm", "pagework.js"]) assert.ok(paths.includes(`/b/e2e-1/${f}`), `${f} is kept: ${paths.join(", ")}`)
  // A serif face is kept on first use, never at install: one in the cache is one this page drew in (the list's muted
  // italic lines draw in the italic), and so one the document loaded.
  const loaded = await app.page.evaluate<string[]>(`[...document.fonts].filter((f) => f.status === "loaded" && f.family === "Source Serif 4").map((f) => f.style)`)
  for (const f of shell.SERIF)
    if (paths.includes(`/b/e2e-1/fonts/${f}`))
      assert.ok(loaded.includes(f.includes("italic") ? "italic" : "normal"), `${f} is kept only once a page drew in it: loaded ${loaded.join(", ")}`)
  assert.deepEqual(paths.filter(p => !p.startsWith("/b/e2e-1/") && p !== "/index.html" && p !== "/boot.js"), [], "every build file under its build's name")
  assert.deepEqual(paths.filter(p => p.startsWith("/api/") || p === "/events"), [], "the API is never the worker's")
  assert.equal(await app.page.evaluate<boolean>(`document.getElementById("updated").hidden`), true, "a device's first build is not news")
})

// THE API NEVER WAKES THE WORKER (Chromium's static routes, `shell.NETWORK`): a request the worker would only pass on
// is matched to the network by the browser and never dispatched to it, so the page's calls cannot reach an older worker
// as it gives way to a newer one. Resource Timing says which source a request was routed to.
app.scene("the API goes to the box without the worker, and a shell file and the build the box serves through it", async () => {
  await app.page.waitFor(controlledBy("e2e-1"), { timeout: 20000 })
  const routed = (path: string) =>
    app.page.evaluate<{ matched: string; worker: number }>(
      `fetch(${JSON.stringify(path)}, { cache: "no-store" }).then((r) => r.text()).then(() => performance.getEntriesByType("resource").filter((e) => new URL(e.name).pathname === ${JSON.stringify(path)}).at(-1)).then((e) => ({ matched: e.workerMatchedSourceType, worker: e.workerStart }))`,
    )
  assert.equal((await routed("/api/about")).matched, "network", "the API is routed past the worker")
  const shellFile = await routed("/b/e2e-1/tokens.css")
  assert.equal(shellFile.matched, "", "a shell file matches no route")
  assert.ok(shellFile.worker > 0, "and the worker is given it")
  assert.equal((await routed("/b/current")).matched, "", "the build the box serves is the worker's to answer as last heard")
})

app.scene("today's list fetches no Typst; the first Typst view keeps the module in the build's cache", async () => {
  await app.open(SHELL)
  await app.page.waitFor(controlledBy("e2e-1"), { timeout: 20000 })
  const fetchedTypst = () => app.page.evaluate<boolean>(`performance.getEntriesByType("resource").some((e) => e.name.endsWith("/typst_bg.wasm"))`)
  assert.equal(await fetchedTypst(), false, "today's list asked for typst_bg.wasm")
  assert.ok(!(await kept("suzatary-shell-e2e-1")).includes("/b/e2e-1/typst_bg.wasm"), "the install precached typst_bg.wasm")
  await app.page.navigate(`${SHELL}/#/typst`)
  await app.page.waitFor(`document.querySelector(".typstframe .tview") !== null`, { timeout: 30000 })
  await app.page.waitFor(`caches.open("suzatary-shell-e2e-1").then((c) => c.match("/b/e2e-1/typst_bg.wasm")).then((r) => r !== undefined)`, { timeout: 20000 })
  await app.page.navigate(`${SHELL}/#/`)
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`)
})

app.scene("a second visit takes the core and typst-wasm from the build's cache, not the box", async () => {
  await app.page.waitFor(controlledBy("e2e-1"), { timeout: 20000 })
  const seen = await app.page.answeredBy(["/prodrome_bg.wasm", "/typst_bg.wasm"])
  await app.page.navigate("about:blank")
  await app.page.navigate(`${SHELL}/#/typst`)
  await app.page.waitFor(`document.querySelector(".typstframe .tview") !== null`, { timeout: 30000 })
  assert.deepEqual(
    seen().map(r => `${new URL(r.url).pathname} ${r.by}`).sort(),
    ["/b/e2e-1/prodrome_bg.wasm cache-storage", "/b/e2e-1/typst_bg.wasm cache-storage"],
  )
  await app.page.navigate(`${SHELL}/#/`)
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`)
})

// AN OPEN TAB OF BUILD N, ONCE BUILD N+1 IS LIVE, STILL RUNS BUILD N: the box serves no file of a build it moved from,
// so every file a page of N loads after the deploy (the Rust pages' module and its rehearsal worker among them) is
// N's, from N's cache. A tab that lost one anyway (a cache the browser let go) moves to the box's build at the page it
// asked for (`newbuild.lost`), and a page that cannot load is said in words, never drawn as the error.
app.scene("a tab of an old build loads its Rust pages' module from its build's cache after a deploy, and draws them", async () => {
  await app.page.waitFor(controlledBy("e2e-1"), { timeout: 20000 })
  await app.page.evaluate(`(window.e2eMark = true)`)
  writeFileSync(BUILD, "e2e-1b\n")
  try {
    assert.equal((await fetch(`${SHELL}/b/e2e-1/views.js`)).status, 404, "the box serves no file of a build it moved from")
    for (const f of ["views.js", "views_bg.wasm", "pagework.js"])
      assert.equal(await app.page.evaluate<number>(`fetch("/b/e2e-1/${f}").then((r) => r.status)`), 200, `${f}, from e2e-1's cache`)
    await app.page.evaluate(`(location.hash = "#/secretary", true)`)
    await app.page.waitFor(`document.querySelector("#view .secretarystream") !== null`, { timeout: 30000 })
    assert.equal(await app.page.evaluate<boolean>(`window.e2eMark === true`), true, "the tab was not reloaded")
    assert.equal(await app.page.evaluate<boolean>(`document.querySelector("#view .problem") === null`), true, "no problem is drawn")
  } finally {
    writeFileSync(BUILD, "e2e-1\n")
  }
  await app.page.navigate(`${SHELL}/#/`)
  await app.page.waitFor(`${controlledBy("e2e-1")} && document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 })
})

app.scene("a page whose code will not load says so in words and offers to load it again, never the error", async () => {
  // A page no worker serves, so its own requests are the ones failed: the Rust pages' module, and the worker that
  // would otherwise take the page over and fetch the module itself.
  await app.page.evaluate(`navigator.serviceWorker.getRegistrations().then((rs) => Promise.all(rs.map((r) => r.unregister())))`)
  await app.page.navigate("about:blank")
  await app.page.failRequests(["/views.js", "/views_bg.wasm", "/sw.js"])
  try {
    await app.page.navigate(`${SHELL}/#/secretary`)
    await app.page.waitFor(`document.querySelector("#view .problem .retry") !== null`, { timeout: 30000 })
    const said = await app.page.evaluate<string>(`document.getElementById("view").textContent`)
    assert.match(said, /This page could not be loaded/, said)
    assert.equal(/TypeError|import|views\.js/.test(said), false, said)
  } finally {
    await app.page.resumeRequests()
  }
  // Loaded again at the same address, the page draws, and its build's worker serves it once more.
  await app.page.evaluate(`(document.querySelector("#view .problem .retry").click(), true)`)
  await app.page.waitFor(`location.hash === "#/secretary" && document.querySelector("#view .secretarystream") !== null`, { timeout: 30000 })
  await app.page.navigate(`${SHELL}/#/`)
  await app.page.waitFor(`${controlledBy("e2e-1")} && document.querySelectorAll(".tree .row").length > 0`, { timeout: 30000 })
})

// A page that has not reloaded still holds this mark; a reload, of any kind, drops it.
const MARK = `(window.e2eMark = true)`
const marked = `window.e2eMark === true`
const line = `document.getElementById("updated")`
const lineSays = () => app.page.evaluate<string>(`${line}.hidden ? "" : ${line}.textContent`)
// The page to the background and back, as a phone's app switcher does; the browser's own is never hidden headless.
const shown = (state: "hidden" | "visible") =>
  `(Object.defineProperty(document, "visibilityState", { value: ${JSON.stringify(state)}, configurable: true }), document.dispatchEvent(new Event("visibilitychange")), true)`
const deploy = (id: string): void => writeFileSync(BUILD, `${id}\n`)
// Every stream a document opens, kept where a scene can cut it. Headless Chromium's offline mode leaves an open
// stream open, so the cut is the browser's own: the socket closed and its error told, then the network back.
const STREAMS = `(() => {
  const Real = window.EventSource
  window.e2eStreams = []
  window.EventSource = class extends Real {
    constructor(url, init) { super(url, init); window.e2eStreams.push(this) }
  }
})()`
// The stream cut and back: the page's stream gives up, the network comes back, it opens anew, and the page asks
// which build the box serves.
const reconnect = async (): Promise<void> => {
  await app.page.evaluate(`(window.e2eStreams.forEach((s) => { s.close(); s.dispatchEvent(new Event("error")) }), true)`)
  await app.page.evaluate(`(window.dispatchEvent(new Event("online")), true)`)
}
const landedOn = (id: string) => `${controlledBy(id)} && document.readyState === "complete" && !(${marked})`
// Waits for the page to run `id` under its worker; a page that did not says where it stands.
const land = async (id: string): Promise<void> => {
  try {
    await app.page.waitFor(landedOn(id), { timeout: 30000 })
  } catch (e) {
    const at = await app.page.evaluate<string>(`JSON.stringify({ reloaded: !(${marked}), worker: navigator.serviceWorker.controller?.scriptURL ?? null, apps: performance.getEntriesByType("resource").map((e) => new URL(e.name).pathname).filter((p) => p.endsWith("/app.js") || p === "/b/current") })`)
    throw new Error(`the page did not land on ${id}: ${at}`, { cause: e })
  }
}

// The app put away and taken out again.
const resume = async (): Promise<void> => {
  await app.page.evaluate(shown("hidden"))
  await app.page.evaluate(shown("visible"))
}
const ready = `!${line}.hidden && ${line}.textContent.includes("is ready")`
const settle = `new Promise((ok) => setTimeout(() => ok(true), 1500))`

app.scene("a running page hears a new build on a reconnect and as it comes back, and is offered it, never reloaded; Reload moves it and says so once", async () => {
  await app.page.beforeEveryDocument(STREAMS)
  await app.page.reload()
  await app.page.waitFor(`${controlledBy("e2e-1")} && document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 })
  await app.page.evaluate(MARK)
  deploy("e2e-2")
  await reconnect()
  await app.page.waitFor(ready, { timeout: 30000 })
  assert.match(await lineSays(), /^Suzatary \S+ is readyReloadDismiss$/)
  await app.page.click(".updateddismiss")
  await resume()
  await app.page.waitFor(settle)
  assert.equal(await lineSays(), "", "a dismissed build is not offered again")
  assert.equal(await app.page.evaluate<boolean>(`${marked} && ${controlledBy("e2e-1")}`), true, "nothing moved under the reader")
  deploy("e2e-3")
  await resume()
  await app.page.waitFor(ready, { timeout: 30000 })
  await app.page.waitFor(settle)
  assert.equal(await app.page.evaluate<boolean>(marked), true, "coming back offers a newer build, and reloads nothing")
  await app.page.click(".updatedreload")
  await land("e2e-3")
  await app.page.waitFor(`caches.keys().then((ns) => ns.filter((n) => n.startsWith(${JSON.stringify(shell.PREFIX)})).join() === "suzatary-shell-e2e-3")`, { timeout: 20000 })
  await app.page.waitFor(`!${line}.hidden`, { timeout: 20000 })
  assert.match(await lineSays(), /^Updated to Suzatary \S+ \(e2e-3\)Dismiss$/)
  await app.page.click(".updateddismiss")
  assert.equal(await app.page.evaluate<boolean>(`${line}.hidden`), true)
  await app.page.reload()
  await app.page.waitFor(`${controlledBy("e2e-3")} && document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 })
  await app.page.waitFor(`fetch("/api/about").then(() => ${settle})`)
  assert.equal(await lineSays(), "", "said once: a plain reload says nothing")
})

const apps = () => app.page.evaluate<string[]>(`performance.getEntriesByType("resource").map((e) => new URL(e.name).pathname).filter((p) => p.endsWith("/app.js"))`)

app.scene("a start runs the build last heard with nothing asked of the box first, offers the box's newer one, and the next start runs it", async () => {
  deploy("e2e-4")
  await app.page.evaluate(MARK)
  await app.page.reload()
  await app.page.waitFor(`document.documentElement.dataset["started"] === "app" && document.readyState === "complete"`, { timeout: 30000 })
  assert.deepEqual(await apps(), ["/b/e2e-3/app.js"], "the build heard before the deploy")
  await app.page.waitFor(ready, { timeout: 30000 })
  await app.page.reload()
  await app.page.waitFor(`${controlledBy("e2e-4")} && document.readyState === "complete"`, { timeout: 30000 })
  assert.deepEqual(await apps(), ["/b/e2e-4/app.js"])
  await app.page.waitFor(`!${line}.hidden`, { timeout: 20000 })
  assert.match(await lineSays(), /^Updated to Suzatary \S+ \(e2e-4\)/)
})

const DRAFT = "a draft typed before the deploy"
const composer = `document.querySelector(".composebox")`
// The composer is drawn again as the page learns this device's key is trusted; words typed into the one it replaced
// would be typed into nothing, so the draft is typed into a box that has stood still, and seen there.
const typeDraft = async (): Promise<void> => {
  for (;;) {
    await app.page.waitFor(`(window.e2eBox = ${composer}) !== null`, { timeout: 20000 })
    await app.page.waitFor(settle)
    if (await app.page.evaluate<boolean>(`window.e2eBox === ${composer}`)) break
  }
  await app.page.type(".composebox", DRAFT)
  await app.page.waitFor(`document.activeElement === ${composer} && ${composer}.value === ${JSON.stringify(DRAFT)}`, { timeout: 5000 })
}

app.scene("Reload keeps the composer's draft", async () => {
  await trustThisDevice(app.page, SHELL, INSTANCE)
  await app.page.navigate(`${SHELL}/#/secretary/default`)
  await app.page.waitFor(controlledBy("e2e-4"), { timeout: 20000 })
  await typeDraft()
  await app.page.evaluate(MARK)
  deploy("e2e-5")
  await resume()
  await app.page.waitFor(ready, { timeout: 30000 })
  assert.equal(await app.page.evaluate<boolean>(marked), true, "nothing reloaded by itself")
  await app.page.click(".updatedreload")
  await land("e2e-5")
  await app.page.waitFor(`${composer}?.value === ${JSON.stringify(DRAFT)}`, { timeout: 20000 })
})

app.scene("the line sits across the top at phone and desktop width", async () => {
  for (const [name, size] of [["phone", PHONE], ["desktop", DESKTOP]] as const) {
    await app.page.viewport(size[0], size[1])
    await app.page.navigate(`${SHELL}/#/`)
    await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 })
    const box = await app.page.evaluate<{ left: number; right: number; top: number; view: number; header: number }>(`(() => {
      const bar = ${line}
      bar.hidden = false
      bar.replaceChildren(Object.assign(document.createElement("span"), { className: "updatedwhat", textContent: "Updated to Suzatary 0.0.0 (e2e)" }), Object.assign(document.createElement("button"), { className: "updateddismiss", textContent: "Dismiss" }))
      const r = bar.getBoundingClientRect()
      return { left: r.left, right: r.right, top: r.top, view: document.documentElement.clientWidth, header: document.querySelector("header").getBoundingClientRect().bottom }
    })()`)
    assert.ok(box.left >= 0 && box.right <= box.view, `${name}: inside the page: ${JSON.stringify(box)}`)
    assert.ok(box.top >= box.header - 1, `${name}: under the header: ${JSON.stringify(box)}`)
    assert.equal(await app.page.evaluate<number>(`document.documentElement.scrollWidth - document.documentElement.clientWidth`), 0, `${name}: no sideways scroll`)
    const shots = process.env["SUZATARY_E2E_SHOTS"]
    if (shots !== undefined && shots !== "") await app.page.screenshot(`${shots}/updated-${name}.png`)
    await app.page.viewport(PHONE[0], PHONE[1])
    await app.page.reload()
  }
})

app.scene("a tab on an old build loads only that build's files, though the box moved and its worker was stopped before its first Typst view", async () => {
  await app.page.navigate(`${SHELL}/#/`)
  await app.page.waitFor(`${controlledBy("e2e-5")} && document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 })
  assert.ok(!(await kept("suzatary-shell-e2e-5")).includes("/b/e2e-5/typst_bg.wasm"), "this build never drew a Typst view")
  await app.page.evaluate(MARK)
  deploy("e2e-6")
  // A browser stops an idle worker; nothing it held in memory is left to say which build this tab runs.
  await app.page.stopWorkers()
  const seen = await app.page.answeredBy([...shell.FILES, ...shell.KEPT])
  await app.page.evaluate(`(location.hash = "#/typst", true)`)
  // The Typst view's module and kit are asked for; the old build's are gone from the box and were never kept.
  await app.page.waitFor(`performance.getEntriesByType("resource").some((e) => e.name.endsWith("typst_bg.wasm") || e.name.endsWith("typst.js"))`, { timeout: 20000 })
  await app.page.waitFor(`new Promise((ok) => setTimeout(() => ok(true), 1500))`)
  // `b/current` is the loader's ask of the box's build, as the page loaded: a name, no build's file.
  const asked = await app.page.evaluate<string[]>(`performance.getEntriesByType("resource").map((e) => new URL(e.name).pathname).filter((p) => p.startsWith("/b/") && p !== "/b/current")`)
  assert.ok(asked.length > 0, "the page asked for its build's files")
  assert.deepEqual(asked.filter(p => !p.startsWith("/b/e2e-5/")), [], "nothing of another build")
  assert.deepEqual(seen().map(r => new URL(r.url).pathname).filter(p => !p.startsWith("/b/e2e-5/")), [], "nothing of another build answered")
  assert.equal(await app.page.evaluate<boolean>(marked), true, "the tab was not reloaded")
  await app.page.navigate(`${SHELL}/#/`)
})

// A fresh document, started until it runs `id` under its worker: the first start after a deploy runs the build heard
// before it, and hears the new one behind its answer.
const startOn = async (id: string): Promise<void> => {
  for (let k = 0; k < 3; k++) {
    await app.page.navigate("about:blank")
    await app.page.navigate(`${SHELL}/#/`)
    await app.page.waitFor(`document.documentElement.dataset["started"] === "app" && document.querySelectorAll(".tree .row").length > 0`, { timeout: 30000 })
    await app.page.waitFor(`fetch("/api/about").then(() => ${settle})`)
    if (await app.page.evaluate<boolean>(controlledBy(id))) return
  }
  await land(id)
}

app.scene("an old build's files are the box's 404 once it moved, and a tab on that build still draws a page from its cache", async () => {
  await startOn("e2e-6")
  await app.page.evaluate(MARK)
  deploy("e2e-6b")
  for (const f of ["views.js", "views_bg.wasm"])
    assert.equal((await fetch(`${SHELL}/b/e2e-6/${f}`)).status, 404, `the box serves no ${f} of a build it moved from`)
  await app.page.evaluate(`(location.hash = "#/typst", true)`)
  await app.page.waitFor(`document.querySelector(".typstpane") !== null`, { timeout: 30000 })
  assert.equal(await app.page.evaluate<boolean>(marked), true, "the tab was not reloaded")
  deploy("e2e-6")
})

app.scene("a tab whose build's worker a newer tab's worker took over still draws a page it never opened, from its own build's cache", async () => {
  await startOn("e2e-6")
  await app.page.evaluate(MARK)
  deploy("e2e-8")
  try {
    // What a second tab starting on the box's new build does, done from this one: the fixture serves plain HTTP/1.1, so
    // a second tab beside this one's open streams would wait on its six connections a host (the box serves HTTP/2).
    // The new build's worker installs and claims every tab.
    await app.page.evaluate(`navigator.serviceWorker.register(${JSON.stringify(shell.script("e2e-8"))}).then(() => true)`)
    await app.page.waitFor(controlledBy("e2e-8"), { timeout: 30000 })
    assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".typstpane") === null`), true, "the tab had not opened the page")
    await app.page.evaluate(`(location.hash = "#/typst", true)`)
    await app.page.waitFor(`document.querySelector(".typstpane") !== null`, { timeout: 30000 })
    assert.equal(await app.page.evaluate<boolean>(`${marked} && ${controlledBy("e2e-8")}`), true, "the same document, under the newer worker")
  } finally {
    deploy("e2e-6")
  }
})

// A build whose files differ from the one the worker kept: its page marks itself, and its app draws a host the old
// build's page never had. Undone after, so the scenes that follow serve the build they were built from.
const differing = (): (() => void) => {
  const files = ["index.html", "app.js"]
  for (const f of files) {
    copyFileSync(`${DIST}/${f}`, `${DIST}/${f}.kept`)
    // The server serves a file's .br before the file; the new build has none.
    if (existsSync(`${DIST}/${f}.br`)) renameSync(`${DIST}/${f}.br`, `${DIST}/${f}.br.kept`)
  }
  writeFileSync(`${DIST}/index.html`, readFileSync(`${DIST}/index.html.kept`, "utf8").replace("</head>", `<meta name="e2e-page" content="new" /></head>`))
  writeFileSync(`${DIST}/app.js`, `${readFileSync(`${DIST}/app.js.kept`, "utf8")}\ndocument.getElementById("view").before(Object.assign(document.createElement("div"), { id: "e2enewhost" }));\n`)
  return () => {
    for (const f of files) {
      renameSync(`${DIST}/${f}.kept`, `${DIST}/${f}`)
      if (existsSync(`${DIST}/${f}.br.kept`)) renameSync(`${DIST}/${f}.br.kept`, `${DIST}/${f}.br`)
    }
  }
}

app.scene("a cold start runs a new build that draws a host the old page lacks, under the old build's worker and page", async () => {
  await startOn("e2e-6")
  const undo = differing()
  try {
    deploy("e2e-7")
    // The first start runs the build heard before the deploy, and hears the new one behind its answer.
    await app.page.navigate("about:blank")
    await app.page.navigate(`${SHELL}/#/`)
    await app.page.waitFor(`document.documentElement.dataset["started"] === "app"`, { timeout: 30000 })
    assert.deepEqual(await apps(), ["/b/e2e-6/app.js"])
    await app.page.waitFor(ready, { timeout: 30000 })
    await app.page.navigate("about:blank")
    await app.page.navigate(`${SHELL}/#/`)
    await app.page.waitFor(`document.getElementById("e2enewhost") !== null && document.documentElement.dataset["started"] === "app"`, { timeout: 30000 })
    assert.equal(await app.page.evaluate<boolean>(`document.querySelector('meta[name="e2e-page"]') === null`), true, "the old build's worker served its own page")
    const files = await app.page.evaluate<string[]>(`performance.getEntriesByType("resource").map((e) => new URL(e.name).pathname).filter((p) => p.endsWith("/app.js") || p.endsWith(".css"))`)
    assert.deepEqual(files.sort(), ["/b/e2e-7/app.js", "/b/e2e-7/style.css", "/b/e2e-7/tokens.css"])
    await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 })
  } finally {
    undo()
  }
})

app.scene("a rejection nobody handled reaches the box's log as one line", async () => {
  await app.open(WRITE_BASE)
  await app.page.evaluate(`(Promise.reject(new Error("e2e beacon rejection")), true)`)
  await app.page.waitFor(`new Promise((ok) => setTimeout(() => ok(true), 200))`)
  const until = Date.now() + 10000
  while (!logged("e2e beacon rejection") && Date.now() < until) await new Promise(ok => setTimeout(ok, 100))
  const line = readFileSync(LOG, "utf8").split("\n").find(l => l.includes("e2e beacon rejection")) ?? ""
  assert.match(line, /client rejection from \S+ on build dev at todos \(.*Chrome\/\d+\.\d+\.\d+\.\d+/, line)
})

app.scene("a core that will not load is told to the box", async () => {
  await app.page.failRequests(["prodrome_bg.wasm"])
  try {
    await app.page.reload()
    const until = Date.now() + 15000
    while (!logged("client core from") && Date.now() < until) await new Promise(ok => setTimeout(ok, 100))
    assert.ok(logged("client core from"), "the core's failure is in the log")
  } finally {
    await app.page.resumeRequests()
  }
})
