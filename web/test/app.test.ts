import { test } from "node:test"
import assert from "node:assert/strict"

import app from "../src/app/app"
import shell from "../src/app/shell"
import login from "../src/app/login"
import { hrefOf } from "../src/shell/route"
import { forAll, route, type Rng } from "./gen"

const CHAINS = ["life", "suzatary", "other", "..", "Life"]
const ADA = login.of({ login: "ada@example.invalid", actor: "ada" })

// One generator per arm, keyed by the tag: an arm with no generator here is a type error.
const arms: { [K in app.Event["type"]]: (r: Rng) => Extract<app.Event, { type: K }> } = {
  Went: r => ({ type: "Went", route: route(r) }),
  Replaced: r => ({ type: "Replaced", route: route(r) }),
  Moved: r => ({ type: "Moved", at: r.int(4), route: route(r) }),
  Kept: r => ({ type: "Kept", kept: { scroll: r.int(900), focus: r.pick(["", "row/a", "row/b"]) } }),
  Resized: r => ({ type: "Resized", wide: r.bool() }),
  Pinned: r => ({ type: "Pinned", chain: r.pick(CHAINS) }),
  Met: r => ({ type: "Met", me: login.of({ login: r.pick(["", "ada@example.invalid", "grace@example.invalid"]), actor: r.pick([null, "ada", "grace"]) }), served: r.array(r.int(3), () => r.pick(["life", "suzatary"])), categories: r.array(r.int(3), () => r.pick(["work", "home"])), zone: { name: r.pick(["UTC", "America/New_York", "Europe/London", "Europe/Berlin", "Pacific/Chatham"]) } }),
  Unmet: () => ({ type: "Unmet" }),
  Keyed: r => ({ type: "Keyed", key: r.pick<app.Key>([{ type: "Unchecked" }, { type: "Trusted" }, { type: "Untrusted" }, { type: "Revoked" }, { type: "Unsupported", e: "no Ed25519" }]) }),
  Waiting: r => ({ type: "Waiting", waiting: { count: r.int(9) - 3 + r.pick([0, 0.5]), unplaced: {} } }),
  More: r => ({ type: "More", open: r.bool() }),
  Opened: () => ({ type: "Opened" }),
  Heard: r => ({ type: "Heard", heads: r.pick([["a1"], ["b2"], ["a1", "c3"]]) }),
  GaveUp: () => ({ type: "GaveUp" }),
  Undeleted: () => ({ type: "Undeleted" }),
  Build: r => ({
    type: "Build",
    line: r.pick<shell.Line>([{ type: "None" }, { type: "Updated", version: "0.17.0", revision: "b2" }, { type: "Ready", version: "0.18.0" }]),
  }),
}
const TAGS = Object.keys(arms) as app.Event["type"][]
const event = (r: Rng): app.Event => arms[r.pick(TAGS)](r)
const walk = (r: Rng, n = 30): app.App =>
  r.array(n, () => event(r)).reduce(app.step, app.initial(route(r), r.bool(), r.pick(["life", "suzatary"])))
// A walk of a device that deleted what every revoked key left: no `Undeleted`, after which nothing writes again.
const KEPT_TAGS = TAGS.filter(tag => tag !== "Undeleted")
const walkKept = (r: Rng, n = 30): app.App =>
  r.array(n, () => arms[r.pick(KEPT_TAGS)](r)).reduce(app.step, app.initial(route(r), r.bool(), r.pick(["life", "suzatary"])))

const frozen = <T>(v: T): T => {
  if (typeof v === "object" && v !== null) {
    for (const x of Object.values(v)) frozen(x)
    Object.freeze(v)
  }
  return v
}

test("law: step is pure — it reads only its arguments, never changes them, and answers the same twice", () => {
  forAll(300, r => {
    const a = frozen(walk(r))
    const e = frozen(event(r))
    const copy = structuredClone(a)
    assert.deepEqual(app.step(a, e), app.step(copy, e))
    assert.deepEqual(a, copy)
  })
})

test("law: every event the shell hears is one of the sum's arms, and step is total over each", () => {
  forAll(200, r => {
    for (const tag of TAGS) {
      const a = app.step(walk(r), arms[tag](r))
      assert.ok(a.waiting === null || (Number.isInteger(a.waiting.count) && a.waiting.count >= 0), `${tag}: a count is whole and never negative`)
      assert.match(a.chain, /^[a-z][a-z0-9-]{0,31}$/, `${tag}: the chain is always a slug`)
    }
  })
})

test("law: said twice is said once — every arm is idempotent, and a repeat returns the same App", () => {
  forAll(300, r => {
    for (const tag of TAGS) {
      const e = arms[tag](r)
      const once = app.step(walk(r), e)
      assert.equal(app.step(once, e), once, tag)
    }
  })
})

const PLACES = ["Went", "Replaced", "Moved", "Kept", "Resized"] as const

test("law: a list pins its chain, a new place closes the sheet, and only the route register's arms move the log", () => {
  forAll(300, r => {
    const a = walk(r)
    const b = app.step(a, arms[r.pick(PLACES)](r))
    const s = app.screen(b)
    if (s.type === "todos" && s.chain !== "") assert.equal(b.chain, s.chain)
    if (hrefOf(app.route(b)) !== hrefOf(app.route(a))) assert.equal(b.more, false)
    for (const tag of TAGS.filter(t => !(PLACES as readonly string[]).includes(t))) assert.equal(app.step(a, arms[tag](r)).log, a.log, tag)
  })
})

test("law: only a route change or More moves the sheet, so an answer landing after More was pressed leaves it open", () => {
  forAll(300, r => {
    const open = app.step(walk(r), { type: "More", open: true })
    for (const tag of TAGS.filter(t => !(PLACES as readonly string[]).includes(t) && t !== "More")) assert.equal(app.step(open, arms[tag](r)).more, true, tag)
    assert.equal(app.step(open, { type: "Went", route: app.route(open) }).more, true, "the same route is not a change")
    assert.equal(app.step(open, arms.Kept(r)).more, true, "keeping what the view had is not a change")
  })
})

test("a stream given up turns the live dot off, and an open turns it on", () => {
  forAll(200, r => {
    const off = app.step(walk(r), { type: "GaveUp" })
    assert.equal(app.live(off), "off")
    assert.equal(app.live(app.step(off, { type: "Opened" })), "on")
  })
})

test("law: a chain switch starts a new connection's heads and keeps what the live dot said", () => {
  forAll(200, r => {
    const a = app.step(app.step(walk(r), { type: "Opened" }), { type: "Heard", heads: ["a1"] })
    const other = a.chain === "life" ? "suzatary" : "life"
    const b = app.step(a, { type: "Pinned", chain: other })
    assert.equal(b.chain, other)
    assert.deepEqual(b.stream.heads, [])
    assert.deepEqual(b.stream.link, a.stream.link)
    assert.equal(app.step(a, { type: "Pinned", chain: "../x" }), a, "a name that is not a slug is ignored")
  })
})

test("who writes and the build's line are App's, read through its derived values", () => {
  const a = app.initial({ type: "todos" }, false, "life")
  assert.equal(app.writes(a), false)
  const b = app.step(a, { type: "Met", me: ADA, served: ["life", "suzatary"], categories: [], zone: { name: "America/New_York" } })
  assert.deepEqual(b.zone, { name: "America/New_York" }, "the owner's zone is the box's answer")
  assert.equal(app.actor(b), "ada")
  assert.deepEqual(b.served, ["life", "suzatary"])
  assert.equal(app.actor(app.step(b, { type: "Unmet" })), null)
  assert.deepEqual(app.reach(b), { type: "Writes" }, "a writer whose key is not read yet is offered the box")
  // A writer whose browser cannot sign reads, says why, and is still who it is.
  const u = app.step(b, { type: "Keyed", key: { type: "Unsupported", e: "no Ed25519" } })
  assert.equal(app.writes(u), false)
  assert.equal(app.actor(u), "ada")
  assert.match(app.unsigning(u) ?? "", /cannot sign what it writes \(no Ed25519\)/)
  assert.equal(app.step(a, { type: "Keyed", key: { type: "Untrusted" } }), a, "a reader has no key to read")
  assert.equal(app.step(u, { type: "Met", me: ADA, served: ["life", "suzatary"], categories: [], zone: { name: "UTC" } }).who, u.who, "met again, still unsigning")
  const ready: shell.Line = { type: "Ready", version: "0.18.0" }
  const c = app.step(a, { type: "Build", line: ready })
  assert.deepEqual(c.build, ready)
  assert.equal(app.step(c, { type: "Build", line: { type: "Ready", version: "0.18.0" } }), c, "the same line said twice wakes nobody")
  assert.deepEqual(app.step(c, { type: "Build", line: { type: "None" } }).build, { type: "None" })
  assert.equal(app.apiBase(b), "/api/life")
})

test("law: a place is read again exactly when the log's pointer moved to it, and gone to afresh otherwise", () => {
  forAll(300, r => {
    const a = walk(r)
    const there = app.step(a, { type: "Went", route: route(r) })
    if (there.log !== a.log) assert.equal(there.again, false)
    const back = app.step(there, { type: "Moved", at: there.log.at - 1, route: there.log.entries[there.log.at - 1]?.route ?? route(r) })
    if (back.log !== there.log) assert.equal(back.again, true)
    for (const tag of TAGS.filter(t => !(PLACES as readonly string[]).includes(t))) assert.equal(app.step(back, arms[tag](r)).again, back.again, tag)
  })
})

test("a renewed or newly trusted key writes again without a reload", () => {
  forAll(200, r => {
    const writer = app.step(walkKept(r), { type: "Met", me: ADA, served: ["life"], categories: [], zone: { name: "UTC" } })
    const stuck = app.step(writer, { type: "Keyed", key: r.pick<app.Key>([{ type: "Unsupported", e: "the key kept here does not read" }, { type: "Untrusted" }, { type: "Revoked" }]) })
    assert.equal(app.writes(stuck), false)
    const again = app.step(stuck, { type: "Keyed", key: { type: "Trusted" } })
    assert.equal(app.writes(again), true)
    assert.equal(app.actor(again), app.actor(writer))
    assert.equal(app.step(again, { type: "Met", me: ADA, served: ["life"], categories: [], zone: { name: "UTC" } }).who, again.who, "and the next Met keeps it writing")
  })
})

test("/api/me is parsed once: no login is this box's own node, a login with no actor reads, one with an actor writes", () => {
  assert.deepEqual(login.of({ login: "", actor: null }), { type: "Box" })
  assert.deepEqual(login.of({ login: "", actor: "ada" }), { type: "Box" }, "the box's node never writes, whatever an answer claims")
  assert.deepEqual(login.of({ login: "reader@example.invalid", actor: null }), { type: "Reads", login: "reader@example.invalid" })
  assert.deepEqual(ADA, { type: "Writes", login: "ada@example.invalid", actor: "ada" })
})

test("a tab that may not write says so and where to write from; one still asking says nothing", () => {
  const a = app.initial({ type: "todos" }, false, "life")
  const met = (me: login.Met) => app.step(a, { type: "Met", me, served: [], categories: [], zone: { name: "UTC" } })
  const said = (b: app.App): string => { const r = app.reach(b); return r.type === "Reads" ? r.said : "" }
  const toSettings = (b: app.App): boolean => { const r = app.reach(b); return r.type === "Reads" && r.settings }
  assert.deepEqual(app.reach(a), { type: "Waits" }, "nothing is claimed before the box answers")
  assert.match(said(app.step(a, { type: "Unmet" })), /box did not say who this login is/)
  assert.match(said(met({ type: "Box" })), /box's own browser.*never write.*Write from/)
  assert.match(said(met({ type: "Reads", login: "reader@example.invalid" })), /reader@example\.invalid.*not write.*Write from/)
  const keyed = (key: app.Key) => app.step(met(ADA), { type: "Keyed", key })
  assert.match(said(keyed({ type: "Unsupported", e: "no Ed25519" })), /cannot sign.*Write from/)
  assert.equal(said(keyed({ type: "Untrusted" })), "This device is not trusted to write yet: trust it from Settings → This device with the code from the box, or ask there, then tap its card on a trusted device.")
  assert.equal(toSettings(keyed({ type: "Untrusted" })), true, "an untrusted key is sent to Settings")
  assert.match(said(keyed({ type: "Revoked" })), /revoked.*Settings → This device/)
  assert.deepEqual(app.reach(keyed({ type: "Trusted" })), { type: "Writes" })
  forAll(300, r => {
    const w = walk(r)
    const answered = w.who.type !== "Asking" || w.held.type === "Undeleted"
    assert.equal(app.reach(w).type === "Reads", answered && !app.writes(w), "a line exactly where no box is drawn and the box has answered, or the device could not delete what it holds")
    assert.equal(app.reach(w).type === "Waits", !answered)
  })
})

test("a page that could not delete what a revoked key left says so, writes nothing, and no later key moves it", () => {
  forAll(100, r => {
    const a = app.step(app.step(walk(r), { type: "Undeleted" }), arms.Keyed(r))
    assert.equal(app.reach(a).type, "Reads")
    assert.match(app.unsigning(a) ?? "", /could not delete what it holds/)
  })
})
