import { test } from "node:test"
import assert from "node:assert/strict"

import app from "../src/app/app"
import time from "../src/app/time"
import type login from "../src/app/login"
import stale from "../src/shell/stale"
import chrome from "../src/shell/chrome"
import nav from "../src/shell/nav"
import type shell from "../src/app/shell"
import excluded from "../src/prodrome/excluded"
import onscreen from "../src/app/store"
import { forAll, route, type Rng } from "./gen"

const event = (r: Rng): app.Event =>
  r.pick<() => app.Event>([
    () => ({ type: "Went", route: route(r) }),
    () => ({ type: "Pinned", chain: r.pick(["life", "suzatary"]) }),
    () => ({ type: "Met", me: r.pick<login.Met>([{ type: "Box" }, { type: "Reads", login: "reader@example.invalid" }, { type: "Writes", login: "ada@example.invalid", actor: "ada" }]), served: r.pick([[], ["life"], ["life", "suzatary"]]), categories: [], zone: time.UTC }),
    () => ({ type: "Waiting", waiting: { count: r.int(5), unplaced: {} } }),
    () => ({ type: "More", open: r.bool() }),
    () => ({ type: "Opened" }),
    () => ({ type: "Heard", heads: r.pick([["a1"], ["a1", "b2"]]) }),
    () => ({ type: "GaveUp" }),
    () => ({ type: "Build", line: r.pick<shell.Line>([{ type: "None" }, { type: "Ready", version: "0.18.0" }]) }),
  ])()
const seen = (r: Rng): chrome.Seen => ({
  fold: r.pick<() => onscreen.Fold>([
    () => onscreen.UNLOADED,
    () => ({ type: "Shown", entries: [], at: "t", by: { type: "Box", heads: ["a1"], fetchedAt: r.int(1e9), next: null, fresh: r.bool() } }),
    () => ({ type: "Shown", entries: [], at: "t", by: { type: "Local", heads: ["a1"], next: null, fresh: r.bool() } }),
    () => ({ type: "Shown", entries: [], at: "t", by: { type: "Cached", fetchedAt: r.int(1e9) } }),
  ])(),
  excluded: r.pick([excluded.NOTHING, excluded.left([{ name: "a1", why: "not held", resting: ["b2"] }]), excluded.left([], [{ chain: "suzatary", why: "a hole" }])]),
  theme: r.pick(["auto", "light", "dark"] as const),
})
const walk = (r: Rng): app.App => r.array(r.int(30), () => event(r)).reduce(app.step, app.initial(route(r), r.bool(), "life"))

test("law: a drawn shell is a function of App — plain data, the same for equal Apps, whatever else the tab holds", () => {
  forAll(300, r => {
    const a = walk(r)
    const s = seen(r)
    const drawn = chrome.of(a, s)
    assert.deepEqual(JSON.parse(JSON.stringify(drawn)), drawn, "the chrome is data: no node, no function")
    assert.deepEqual(chrome.of(structuredClone(a), structuredClone(s)), drawn)
    for (let i = 0; i < 5; i += 1) app.dispatch(event(r))
    assert.deepEqual(chrome.of(a, s), drawn, "the tab's own App moving changes nothing drawn from another")
  })
})

test("a part is painted only when its value moved", () => {
  const painted: string[] = []
  const paint = chrome.painter({
    nav: () => void painted.push("nav"),
    panel: () => void painted.push("panel"),
    live: () => void painted.push("live"),
    stale: () => void painted.push("stale"),
    excluded: () => void painted.push("excluded"),
    signing: () => void painted.push("signing"),
    build: () => void painted.push("build"),
  })
  // A login that writes; leaving for the conversation page takes the panel away on a wide screen only.
  const a = app.step(app.initial({ type: "inbox" }, false, "life"), { type: "Met", me: { type: "Writes", login: "ada@example.invalid", actor: "ada" }, served: ["life"], categories: [], zone: time.UTC })
  const fold = { fold: onscreen.UNLOADED, excluded: excluded.NOTHING, theme: "auto" as const }
  const loaded: onscreen.Fold = { type: "Shown", entries: [], at: "t", by: { type: "Cached", fetchedAt: 0 } }
  paint(chrome.of(a, fold))
  paint(chrome.of(app.step(a, { type: "Waiting", waiting: { count: 0, unplaced: {} } }), fold))
  paint(chrome.of(app.step(a, { type: "Went", route: { type: "conversation", id: "default", refs: [] } }), fold))
  paint(chrome.of(a, { fold: loaded, excluded: excluded.NOTHING, theme: "auto" }))
  paint(chrome.of(a, { fold: loaded, excluded: excluded.left([{ name: "a1", why: "not held", resting: [] }]), theme: "auto" }))
  assert.deepEqual(painted, ["nav", "panel", "live", "stale", "excluded", "signing", "build", "nav", "nav", "stale", "excluded"])
})

test("the banner is down while the fold is loaded and heard, and always for this browser's own fold while the stream is up", () => {
  forAll(300, r => {
    const a = walk(r)
    const s = seen(r)
    const b = chrome.of(a, s).stale
    const f = s.fold
    const live = app.live(a)
    const down = !onscreen.loaded(f) || (f.by.type === "Local" ? live !== "off" : onscreen.fresh(f) && live !== "off")
    assert.equal(b.type === "Hidden", down)
    if (b.type === "Shown" && f.type === "Shown")
      assert.equal(b.why.type, f.by.type === "Local" ? "Offline" : onscreen.fresh(f) ? "Unheard" : "Old")
  })
})

test("an act on its way raises no banner, and no banner names an object", () => {
  forAll(200, r => {
    const pending: onscreen.Fold = { type: "Shown", entries: [], at: "t", by: { type: "Local", heads: ["2f09bafc200d", "4405434d438f"], next: null, fresh: false } }
    assert.deepEqual(stale.of(r.pick(["on", "unknown"] as const), pending), { type: "Hidden" })
    const b = stale.of("off", pending)
    assert.ok(b.type === "Shown")
    assert.ok(!/[0-9a-f]{8}/.test(stale.said(b.why)), stale.said(b.why))
  })
})

test("law: one door to the secretary on each width: the panel beside every wide page but its own, and no floating Ask", () => {
  forAll(300, r => {
    const a = walk(r)
    const drawn = chrome.of(a, seen(r))
    assert.equal(drawn.panel.type === "Beside", a.wide && nav.activeFor(app.route(a), a.chain) !== "secretary")
  })
})
