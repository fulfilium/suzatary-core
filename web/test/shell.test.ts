// shell's laws: the route keeps the API off the worker, a build's file is named by its build, and a cache belongs to one
// build until no page runs it.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { readdirSync } from "node:fs"

import shell from "../src/app/shell"
import { SLUG, forAll } from "./gen"
import type { Rng } from "./gen"

const SCOPE = "https://box.example.invalid/"
// A build's chunks, as its bundling names them.
const CHUNKS = ["toast-7QK2LS4A.js", "chunk-ZP3XUKDE.js"]
const FILES = shell.files(CHUNKS)
const build = (r: Rng) => r.string(SLUG, 1 + r.int(12))
const path = (r: Rng) =>
  r.pick([
    () => r.pick(shell.ROOT),
    () => shell.under(build(r), r.pick(FILES)),
    () => shell.under(build(r), `${r.string(SLUG, 1 + r.int(8)).replace(/^[^a-z]/, "x")}-${r.string("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567", 8)}.js`),
    () => shell.under(build(r), r.pick(shell.KEPT)),
    () => r.pick(FILES),
    () => shell.CURRENT,
    () => `api/${r.string(SLUG, 1 + r.int(10))}`,
    () => `api/${r.string(SLUG, 3)}/fold`,
    () => "events",
    () => r.string(SLUG + "/.", r.int(20)),
  ])()

test("law: a build's file is routed under the build its name says, its chunks with its shell, the page and its loader are the worker's own, the box's build is the worker's to answer, and nothing else is the shell's", () =>
  forAll(500, r => {
    const p = path(r)
    const url = `${SCOPE}${p}${r.bool() ? `?${r.string(SLUG, 4)}` : ""}`
    const routed = shell.route(url, SCOPE, r.pick(["GET", "POST", "HEAD"]))
    if (p.startsWith("api/") || p === "events") assert.deepEqual(routed, { type: "Pass" })
    const get = shell.route(url, SCOPE, "GET")
    const m = /^b\/([^/]+)\/(.+)$/.exec(p)
    const [, b = "", file = ""] = m ?? []
    assert.equal(get.type === "Root", shell.ROOT.includes(p === "" ? "index.html" : p), url)
    assert.equal(get.type === "Current", p === shell.CURRENT, url)
    assert.equal(get.type === "Shell", m !== null && p !== shell.CURRENT && (shell.FILES.includes(file) || shell.chunk(file)), url)
    assert.equal(get.type === "Kept", m !== null && shell.KEPT.includes(file), url)
    if (get.type === "Shell" || get.type === "Kept") assert.deepEqual([get.build, get.file], [b, file], "the build its name says")
    assert.deepEqual(shell.route(url, SCOPE, "POST"), { type: "Pass" })
  }))

test("any build's chunk is routed to that build's cache, whichever build's worker serves the tab", () => {
  assert.deepEqual(shell.route(`${SCOPE}b/old/chain-ABCD2345.js`, SCOPE, "GET"), { type: "Shell", build: "old", file: "chain-ABCD2345.js" })
  for (const f of ["chain-abcd2345.js", "chain-ABCD234.js", "chain-ABCD2345.js.map", "-ABCD2345.js", "../x-ABCD2345.js"]) assert.ok(!shell.chunk(f), f)
})

test("a build's file under no build's name is no file of the shell's: a page loads only its own build's names", () => {
  for (const f of [...FILES, ...shell.KEPT]) assert.deepEqual(shell.route(`${SCOPE}${f}`, SCOPE, "GET"), { type: "Pass" }, f)
})

test("the scope itself is the page, and another origin is never the shell's", () => {
  assert.deepEqual(shell.route(SCOPE, SCOPE, "GET"), { type: "Root", file: "index.html" })
  assert.deepEqual(shell.route(`${SCOPE}?x=1`, SCOPE, "GET"), { type: "Root", file: "index.html" })
  assert.deepEqual(shell.route(`${SCOPE}boot.js`, SCOPE, "GET"), { type: "Root", file: "boot.js" })
  assert.deepEqual(shell.route("https://elsewhere.example.invalid/b/b1/app.js", SCOPE, "GET"), { type: "Pass" })
})

test("law: a page's or a worker's own file names its build, and a file named by none is a dev build's", () =>
  forAll(300, r => {
    const id = build(r)
    const f = r.pick([...FILES, ...shell.KEPT])
    assert.deepEqual(shell.ownBuild(`${SCOPE}${shell.under(id, f)}`), shell.build(id))
    assert.equal(shell.named(shell.build(id)), id === "" ? shell.DEV : id)
    assert.deepEqual(shell.ownBuild(`${SCOPE}app.js`), { type: "Dev" })
  }))

test("law: a build survives the worker's script URL, and dev is never a build", () =>
  forAll(300, r => {
    const id = build(r)
    const printed = new URL(shell.script(id), SCOPE).href
    assert.deepEqual(shell.buildOf(printed), id === "dev" ? { type: "Dev" } : { type: "Stamped", id })
    assert.deepEqual(shell.build("dev"), { type: "Dev" })
    assert.deepEqual(shell.build(""), { type: "Dev" })
    assert.deepEqual(shell.buildOf(`${SCOPE}sw.js`), { type: "Dev" })
  }))

test("law: a build's cache goes only once no open page runs it, and never while a page has not said", () =>
  forAll(300, r => {
    const id = build(r)
    const names = r.array(r.int(8), () => (r.bool() ? shell.cacheOf(build(r)) : r.string(SLUG, 6)))
    const all = r.bool() ? [...names, shell.cacheOf(id)] : names
    const pages = r.array(r.int(4), () => (r.bool(0.15) ? null : r.bool() ? id : build(r)))
    const gone = shell.unused(all, id, pages)
    if (pages.includes(null)) return assert.deepEqual(gone, [], "a page that has not said keeps every cache")
    assert.ok(!gone.includes(shell.cacheOf(id)), "the worker's own build stays")
    for (const p of pages) if (p !== null) assert.ok(!gone.includes(shell.cacheOf(p)), "a build an open page runs stays")
    assert.ok(gone.every(n => n.startsWith(shell.PREFIX)), "nothing that is not a shell goes")
    assert.deepEqual(gone, all.filter(n => n.startsWith(shell.PREFIX) && n !== shell.cacheOf(id) && !pages.some(p => p !== null && n === shell.cacheOf(p))))
  }))

test("law: a move to a new build is news once, and a device's first build is not news", () =>
  forAll(300, r => {
    const id = build(r)
    const last = r.array(r.int(3), () => build(r))
    const n = shell.news(last, { type: "Stamped", id })
    assert.deepEqual(n, last.includes(id) ? { type: "Known" } : last.length === 0 ? { type: "First", id } : { type: "New", id })
    assert.deepEqual(shell.news([...last, id], { type: "Stamped", id }), { type: "Known" }, "said once, then known")
    assert.deepEqual(shell.news(last, { type: "Dev" }), { type: "Known" }, "a dev build has no revision to name")
  }))

test("law: index.html, the same file for every build, holds only the loader and nothing a build reads", () => {
  const html = readFileSync("web/static/index.html", "utf8").replace(/<!--[\s\S]*?-->/g, "")
  const body = /<body>([\s\S]*)<\/body>/.exec(html)?.[1]?.replace(/\s+/g, " ").trim()
  assert.equal(body, `<script type="module" src="./boot.js"></script>`)
  const head = /<head>([\s\S]*)<\/head>/.exec(html)?.[1] ?? ""
  assert.ok(!/stylesheet|<script/.test(head), "no build's stylesheet or script in the head")
  assert.ok(!/\sid=/.test(html), "no host element a build could need")
})

test("the shell keeps every vendored font the stylesheet can ask for", () => {
  const vendored = readdirSync("web/static/fonts").filter(f => f.endsWith(".woff2")).sort()
  assert.deepEqual([...shell.FONTS].sort(), vendored)
})

test("the Typst module and kit are kept on first use, never precached with the shell", () => {
  for (const f of shell.KEPT) assert.ok(!FILES.includes(f), f)
  assert.ok(shell.KEPT.includes("typst_bg.wasm"))
})

test("law: what the browser sends to the box without waking the worker is what the worker passes anyway", () =>
  forAll(300, r => {
    const pattern = r.pick(shell.NETWORK)
    const rel = pattern.endsWith("*") ? pattern.slice(0, -1) + r.pick([build(r), `${build(r)}/${build(r)}`, `${build(r)}?since=${build(r)}`]) : pattern
    assert.deepEqual(shell.route(SCOPE + rel, SCOPE, r.pick(["GET", "POST"])), { type: "Pass" }, rel)
  }))

test("the two Source Serif files are kept as a page first draws in them, and the small faces with the shell", () => {
  for (const f of shell.SERIF) {
    assert.ok(shell.KEPT.includes(`fonts/${f}`), f)
    assert.ok(!FILES.includes(`fonts/${f}`), f)
  }
  for (const f of shell.FACES) assert.ok(FILES.includes(`fonts/${f}`), f)
  const css = readFileSync("web/static/style.css", "utf8")
  for (const f of shell.SERIF) assert.match(css, new RegExp(`font-display: swap;\\s*src: url\\(\\./fonts/${f.replace(/\./g, "\\.")}\\)`), `${f} swaps in, so no text waits on it`)
})
