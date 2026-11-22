// A start against the worker, both in node: a warm start runs its app with the network never answering, the box's build
// is asked again behind the answer and believed at the next start, and the loader tries each build once.

import { test } from "node:test"
import assert from "node:assert/strict"

import worker from "../src/app/worker"
import loader from "../src/app/loader"
import shell from "../src/app/shell"

const SCOPE = "https://box.example.invalid/"
const CHUNKS = ["chain-AAAA1111.js", "chunk-BBBB2222.js"]
const MANIFEST: shell.Manifest = { "typst_bg.wasm": `typst_bg-${"a".repeat(64)}.wasm` }

// The browser's CacheStorage, in memory: what a worker keeps between starts.
const storage = () => {
  const all = new Map<string, Map<string, Response>>()
  const urlOf = (u: RequestInfo | URL): string => (typeof u === "string" ? u : u instanceof URL ? u.href : u.url)
  const cache = (m: Map<string, Response>) => ({
    match: (u: RequestInfo | URL) => Promise.resolve(m.get(urlOf(u))?.clone()),
    put: (u: RequestInfo | URL, r: Response) => Promise.resolve(void m.set(urlOf(u), r.clone())),
    keys: () => Promise.resolve([...m.keys()].map(u => new Request(u))),
    delete: (u: RequestInfo | URL) => Promise.resolve(m.delete(urlOf(u))),
  })
  return {
    open: (name: string) => {
      const m = all.get(name) ?? new Map<string, Response>()
      all.set(name, m)
      return Promise.resolve(cache(m))
    },
    keys: () => Promise.resolve([...all.keys()]),
    delete: (name: string) => Promise.resolve(all.delete(name)),
  }
}

// The network as a test drives it: every ask is written down, and answered by `answer` while it is set; with none, an
// ask never settles, as a box behind a dead connection never answers.
const network = () => {
  const asked: string[] = []
  let answer: ((path: string) => Response) | null = null
  const fetch = (input: RequestInfo | URL, _init?: RequestInit): Promise<Response> => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const path = url.slice(SCOPE.length)
    asked.push(path)
    const a = answer
    return a === null ? new Promise<Response>(() => {}) : Promise.resolve(a(path))
  }
  return { fetch, asked, answer: (a: ((path: string) => Response) | null) => void (answer = a) }
}

type Listener = (e: Event) => void

// The open pages, each answering the worker's question with the build it runs, and the registration's active worker.
type Others = { readonly pages: readonly string[]; readonly active: string | null }
const page = (build: string) => ({
  postMessage: (_: unknown, [port]: Transferable[]) => {
    if (port instanceof MessagePort) port.postMessage(build), port.close()
  },
})
const worked = (build: string | null) => (build === null ? null : { scriptURL: `${SCOPE}${shell.script(build).slice(2)}` })

// A worker of build `id` installed against a box serving `id`, and the page's way to ask it for a URL.
// `kept` is the device's CacheStorage, which a worker installed before this one may have filled.
const installed = async (id: string, chunks: readonly string[] = CHUNKS, kept = storage(), others: Others = { pages: [], active: null }) => {
  const net = network()
  Object.defineProperty(globalThis, "caches", { value: kept, configurable: true })
  Object.defineProperty(globalThis, "fetch", { value: net.fetch, configurable: true })
  const listeners = new Map<string, Listener>()
  const scope: worker.Scope = {
    location: { href: `${SCOPE}${shell.script(id).slice(2)}` },
    registration: { scope: SCOPE, installing: null, waiting: null, active: worked(others.active) },
    skipWaiting: () => Promise.resolve(),
    clients: { claim: () => Promise.resolve(), matchAll: () => Promise.resolve(others.pages.map(page)) },
    addEventListener: (type, f) => void listeners.set(type, f),
  }
  worker.run(scope, id, chunks, MANIFEST)
  net.answer(path => new Response(path === shell.CURRENT ? id : `the file ${path}`))
  const waits: Promise<unknown>[] = []
  const install = Object.assign(new Event("install"), { waitUntil: (p: Promise<unknown>) => void waits.push(p) })
  listeners.get("install")?.(install)
  await Promise.all(waits)
  net.asked.splice(0)
  const behind: Promise<unknown>[] = []
  // What the page's request gets: the worker's answer, or the network's where the worker leaves it be.
  const ask = (path: string): Promise<Response> => {
    let answered: Promise<Response> | null = null
    const e = Object.assign(new Event("fetch"), {
      request: new Request(`${SCOPE}${path}`),
      respondWith: (r: Promise<Response>) => void (answered = r),
      waitUntil: (p: Promise<unknown>) => void behind.push(p),
    })
    listeners.get("fetch")?.(e)
    return answered ?? net.fetch(`${SCOPE}${path}`)
  }
  // The worker takes over: it claims the pages and sweeps.
  const activate = (): Promise<unknown> => {
    const sweeps: Promise<unknown>[] = []
    listeners.get("activate")?.(Object.assign(new Event("activate"), { waitUntil: (p: Promise<unknown>) => void sweeps.push(p) }))
    return Promise.all(sweeps)
  }
  return { net, ask, behind, kept, activate }
}

const started = (ask: (path: string) => Promise<Response>, own: string | null) => {
  const preloaded: [string, readonly string[]][] = []
  const said: string[] = []
  const run = loader.start(
    {
      served: () => ask(shell.CURRENT).then(r => r.text()).then(t => (t === "" ? null : t)),
      worker: () => own,
      preload: (b, chunks) => void preloaded.push([b, chunks]),
      load: b => ask(shell.under(b, "app.js")).then(r => r.ok),
      said: t => void said.push(t),
    },
    CHUNKS,
  )
  return { run, preloaded, said }
}

test("a warm start runs its app with the network never answering: nothing it waits on goes to the box", async () => {
  const w = await installed("b1")
  w.net.answer(null)
  const s = started(w.ask, "b1")
  // A start that waited on the network would never settle: it fails here instead of hanging the suite.
  const waited = new Promise<"waited">(ok => setTimeout(() => ok("waited"), 2000))
  assert.deepEqual(await Promise.race([s.run, waited]), { type: "Ran", build: "b1" })
  assert.deepEqual(w.net.asked, [shell.CURRENT], "the one ask of the box is the build, behind the answer")
  assert.deepEqual(s.preloaded, [["b1", CHUNKS]], "its own build's chunks, asked for beside its app")
})

test("the box's build heard behind one start is the build the next start runs", async () => {
  const w = await installed("b1")
  w.net.answer(path => new Response(path === shell.CURRENT ? "b2" : "", { status: path === shell.CURRENT ? 200 : 404 }))
  assert.equal(await (await w.ask(shell.CURRENT)).text(), "b1", "answered as last heard")
  await Promise.all(w.behind)
  assert.equal(await (await w.ask(shell.CURRENT)).text(), "b2", "and heard again behind the answer")
})

test("a box that refuses behind the answer leaves the build heard as it was", async () => {
  const w = await installed("b1")
  w.net.answer(() => new Response("down", { status: 502 }))
  await w.ask(shell.CURRENT)
  await Promise.all(w.behind)
  assert.equal(await (await w.ask(shell.CURRENT)).text(), "b1")
})

test("a build's chunks are kept with its shell at install, under the build's name", async () => {
  const w = await installed("b1")
  w.net.answer(null)
  for (const c of CHUNKS) assert.equal(await (await w.ask(shell.under("b1", c))).text(), `the file ${shell.under("b1", c)}`)
  assert.deepEqual(w.net.asked, [], "every chunk from the cache")
})

test("an old build's chunk the box no longer serves is the box's 404 when the worker never kept it", async () => {
  const w = await installed("b2")
  w.net.answer(() => new Response("", { status: 404 }))
  assert.equal((await w.ask(shell.under("b1", CHUNKS[0] ?? ""))).status, 404)
})

test("the loader tries the box's build, then the worker's, each once, and says why when none starts", async () => {
  const fx = (served: string | null, own: string | null, loads: readonly string[]) => {
    const tried: string[] = []
    const preloaded: [string, readonly string[]][] = []
    const said: string[] = []
    const run = loader.start(
      {
        served: () => (served === null ? Promise.reject(new Error("offline")) : Promise.resolve(served)),
        worker: () => own,
        preload: (b, chunks) => void preloaded.push([b, chunks]),
        load: b => (tried.push(b), Promise.resolve(loads.includes(b))),
        said: t => void said.push(t),
      },
      CHUNKS,
    )
    return run.then(r => ({ r, tried, preloaded, said }))
  }
  const fell = await fx("b2", "b1", ["b1"])
  assert.deepEqual(fell.r, { type: "Ran", build: "b1" })
  assert.deepEqual(fell.tried, ["b2", "b1"])
  assert.deepEqual(fell.preloaded, [["b2", []], ["b1", CHUNKS]], "another build's chunk names are not this loader's to know")
  const cold = await fx("b2", null, ["b2"])
  assert.deepEqual(cold.preloaded, [["b2", CHUNKS]], "with no worker, the loader is the box's build")
  const once = await fx("b1", "b1", [])
  assert.deepEqual(once.tried, ["b1"])
  assert.deepEqual(once.said, [loader.UNLOADED])
  const none = await fx(null, null, [])
  assert.deepEqual(none.r, { type: "Refused", said: loader.NAMELESS })
})

test("a newer build's worker serves a tab still on an older build that build's chunk from that build's cache", async () => {
  const old = await installed("b1")
  const now = await installed("b2", ["chain-CCCC3333.js"], old.kept)
  now.net.answer(() => new Response("", { status: 404 }))
  const c = CHUNKS[0] ?? ""
  const r = await now.ask(shell.under("b1", c))
  assert.equal(r.status, 200, "the old build's kept chunk, not the box's 404")
  assert.equal(await r.text(), `the file ${shell.under("b1", c)}`)
  assert.deepEqual(now.net.asked, [], "nothing asked of the box")
})

test("the worker's sweep drops a content file its build does not name only once every open page and the active worker run its build", async () => {
  const named = MANIFEST["typst_bg.wasm"]
  const older = `typst_bg-${"b".repeat(64)}.wasm`
  const cases: readonly [Others, readonly string[]][] = [
    [{ pages: ["b2"], active: "b2" }, [named]],
    [{ pages: [], active: null }, [named]],
    [{ pages: ["b2", "b1"], active: "b2" }, [named, older]],
    // A newer worker that took over while the pages answered.
    [{ pages: ["b2"], active: "b3" }, [named, older]],
  ]
  for (const [others, left] of cases) {
    const kept = storage()
    const c = await kept.open(shell.CONTENT)
    for (const f of [named, older]) await c.put(`${SCOPE}${shell.contentKey(f)}`, new Response(f))
    const w = await installed("b2", CHUNKS, kept, others)
    await w.activate()
    const files = (await c.keys()).map(r => r.url.slice(`${SCOPE}${shell.contentKey("")}`.length))
    assert.deepEqual(files, left, JSON.stringify(others))
  }
})
