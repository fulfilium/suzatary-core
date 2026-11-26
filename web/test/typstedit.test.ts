// The editor's laws: its runs partition the source, an answer about another text changes nothing, an escaped title
// opens in its prose, and the tab's restricted project refuses and admits what the box's does (one implementation,
// the Prodrome's `Project.restricted()`, checked against the box's own vectors).

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"
import { pathToFileURL } from "node:url"

import type { Core } from "../src/prodrome/core"
import edit from "../src/typst/edit"
import { HOSTILE, forAll } from "./gen"

type Restricted = { set: (key: string, text: string) => string | undefined; compile: (main: string, state: string) => string }
type Glue = {
  initSync: (m: { module: BufferSource }) => unknown
  highlight: (source: string) => string
  Project: { restricted: () => Restricted }
}

const built = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-test.sh`)
  return v
}

const core = createRequire(import.meta.url)(`${built("SUZATARY_PRODROME_WASM")}/nodejs/prodrome.js`) as Core
const glue = (await import(pathToFileURL(`${built("SUZATARY_TYPST_WASM")}/web/typst.js`).href)) as Glue
glue.initSync({ module: readFileSync(`${built("SUZATARY_TYPST_WASM")}/web/typst_bg.wasm`) })

const check = (source: string): edit.Checked => {
  const p = glue.Project.restricted()
  const refused = p.set(edit.MAIN, source)
  return refused === undefined ? edit.checked(p.compile(edit.MAIN, "{}")) : edit.failed(refused)
}

const SOURCE = "ab*c_ #[]$\\\n éx🙂"

test("the runs partition the source, whatever the spans and marks", () =>
  forAll(300, r => {
    const source = r.string(SOURCE, r.int(30))
    const spans = edit.spans(glue.highlight(source))
    const marks = r.array(r.int(3), (): edit.Diagnostic => {
      const from = r.int(source.length + 2)
      return { message: "x", hints: [], from, to: from + r.int(4) }
    })
    const runs = edit.runs(source, spans, marks)
    assert.equal(runs.map(x => x.text).join(""), source)
    assert.ok(runs.every(x => x.text !== ""))
  }))

test("an answer about a text the editor no longer holds changes nothing", () =>
  forAll(200, r => {
    const now = r.string(SOURCE, 1 + r.int(10))
    const other = `${now}x`
    const s = edit.step(edit.of(""), { type: "Typed", source: now })
    for (const e of [
      { type: "Shaded", source: other, spans: [] },
      { type: "Checked", source: other, checked: edit.failed("late") },
      { type: "Offered", source: other, offer: { from: 0, items: [{ label: "x", insert: "x", cursor: 1, detail: null }] } },
      { type: "Hovered", source: other, text: "late" },
    ] satisfies edit.Event[])
      assert.equal(edit.step(s, e), s)
  }))

test("a failed compile keeps the last page that compiled, marked stale", () => {
  const ok = edit.step(edit.step(edit.of(""), { type: "Typed", source: "*a*" }), { type: "Checked", source: "*a*", checked: { type: "Html", html: "<p>a</p>", warnings: [] } })
  assert.equal(edit.stale(ok), false)
  const broken = edit.step(edit.step(ok, { type: "Typed", source: "*a* ]" }), { type: "Checked", source: "*a* ]", checked: check("*a* ]") })
  assert.equal(broken.shown, "<p>a</p>")
  assert.equal(edit.stale(broken), true)
  assert.match(edit.problems(broken).map(p => edit.line(broken.source, p)).join(), /^1:5: unexpected closing bracket/)
})

test("an escaped title opens in its prose, and markup a writer wrote opens unchanged", () => {
  for (const prose of ["File the *permit* renewal", "Pay $40 to ada@x.test", ...HOSTILE]) {
    const stored = prose.replace(/[\\[\]#$@*_<>`]/g, c => `\\${c}`)
    const opened = edit.opened(stored, core.prose)
    assert.equal(opened.text, prose)
    assert.equal(opened.wasProse, stored !== prose)
  }
  assert.deepEqual(edit.opened("Renew *passport*", core.prose), { text: "Renew *passport*", wasProse: false })
})

// The box's vectors: the same texts the Rust door is tested against, admitted or refused alike in the tab.
const vectors = JSON.parse(readFileSync("app/api/tests/web/closed-world.json", "utf8")) as { text: string; admitted: boolean }[]

test("the tab's restricted project admits exactly what the box's does", () => {
  assert.ok(vectors.length > 5)
  for (const v of vectors) {
    const c = check(v.text)
    const clean = c.type === "Html" && core.suspicious(c.html) === undefined
    assert.equal(clean, v.admitted, `${v.text}: ${JSON.stringify(c)}`)
  }
})

test("a refusal names where in the text typst stopped", () => {
  const c = check("Renew #html.elem(\"b\")[x]")
  assert.equal(c.type, "Failed")
  assert.deepEqual(c.type === "Failed" ? c.diagnostics.map(x => edit.line("Renew #html.elem(\"b\")[x]", x)) : [], ["1:8: unknown variable: html"])
})

test("a compile says what the HTML export dropped, in the owner's terms, and a clean text says nothing", () => {
  const held = (source: string): edit.State => {
    const s = edit.step(edit.of(""), { type: "Typed", source })
    return edit.step(s, { type: "Checked", source, checked: check(source) })
  }
  const lines = (source: string): readonly string[] => edit.dropped(held(source)).map(w => edit.line(source, w))
  assert.deepEqual(lines("a #circle(radius: 1cm) b"), ["1:4: circle is not shown: shapes have no HTML form"])
  assert.deepEqual(lines("#place(top)[x]"), ["1:2: place is not shown: placed content has no HTML form"])
  assert.deepEqual(lines("#grid(columns: 2)[a][b]"), ["1:2: grid is not shown: it has no HTML form"])
  assert.deepEqual(lines("*a* and _b_"), [])
  assert.equal(held("#circle()").check?.checked.type, "Html")
  assert.deepEqual(edit.dropped(held("#circle() ]")), [])
  assert.equal(edit.problems(held("#circle() ]")).length, 1)
})
