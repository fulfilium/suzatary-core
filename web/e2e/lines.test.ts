/**
 * THE LINES, IN A TAB (docs/WORDS-PLAN-2026-10-09.md §5): the conversations are read from
 * `secretary-lines`, whose objects hold their words locked, with the keys the pull hands over
 * kept beside them in one row. Its claims, each a scene:
 *
 * 1. A conversation reads from the lines: the row holds the objects and the keys, and the
 *    objects hold no word of it in the clear.
 * 2. Erase on a turn draws it by its shape, with the box's sentence, and the device forgets
 *    the blocks its words were rendered to.
 * 3. Forgetting a fact erases what it caused and offers the rest on a card, which the tab's
 *    tap erases.
 * 4. A device that kept the plaintext conversations is upgraded: that row goes once the lines
 *    are held, and nothing plaintext is left on the device.
 * 5. A conversation the device prefetched reads with no network, and one it lacks says so.
 */

import assert from "node:assert/strict"

import { PHONE, SIGNED_POST, WRITE_BASE, suite } from "./harness"

const app = await suite("lines")

/** Every row of every store of the database `name`, as one string: what the device holds of it. */
const held = (name: string): string => `new Promise((ok, no) => {
  const r = indexedDB.open(${JSON.stringify(name)})
  r.onerror = () => no(String(r.error))
  r.onsuccess = () => {
    const db = r.result
    const stores = [...db.objectStoreNames]
    if (stores.length === 0) return (db.close(), ok("{}"))
    const tx = db.transaction(stores, "readonly")
    const out = {}
    let left = stores.length
    for (const s of stores) {
      const keys = tx.objectStore(s).getAllKeys()
      const values = tx.objectStore(s).getAll()
      values.onsuccess = () => {
        out[s] = keys.result.map((k, i) => [k, values.result[i]])
        if (--left === 0) (db.close(), ok(JSON.stringify(out)))
      }
    }
  }
})`

/** The lines' row as the device keeps it, one node and one key per row beside a head row, or null before a head. */
const linesRow = `${held("suzatary")}.then((all) => {
  const rows = JSON.parse(all).lines ?? []
  if (!rows.some(([k]) => k === "head")) return null
  const objects = rows.filter(([k]) => k.startsWith("node/")).map(([, n]) => n)
  const keys = Object.fromEntries(rows.filter(([k]) => k.startsWith("key/")).map(([k, v]) => [k.slice(4), v]))
  return { replica: { objects }, keys }
})`

/** `write(store)` run in one read-write transaction of the store `store` of the database `name`. */
const put = (name: string, store: string, write: string): string => `new Promise((done, fail) => {
  const r = indexedDB.open(${JSON.stringify(name)})
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const tx = r.result.transaction(${JSON.stringify(store)}, "readwrite")
    ;(${write})(tx.objectStore(${JSON.stringify(store)}))
    tx.oncomplete = () => (r.result.close(), done(true))
    tx.onerror = () => fail(String(tx.error))
  }
})`

/** The rows of the database `name`'s store `store`, as `[key, value]` pairs. */
const rowsOf = (name: string, store: string): string =>
  `${held(name)}.then((all) => JSON.parse(all)[${JSON.stringify(store)}] ?? [])`

const said = (who: string): string =>
  `[...document.querySelectorAll(".turns .turn-${who} .turntext")].map((t) => t.textContent).join("\\n")`

async function converse(text: string): Promise<string> {
  await app.page.navigate(`${WRITE_BASE}/#/secretary/default`)
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 })
  const id = await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Untitled"})).then((r) => r.json()).then((j) => j.id)`,
  )
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`)
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 })
  await app.page.type(".composebox", text)
  await app.page.click(".composesend")
  return id
}

app.scene("a conversation is read from the lines, its keys held beside its objects", async () => {
  await app.resize(PHONE)
  await app.open(WRITE_BASE)
  await converse("give me a rendered answer")
  await app.page.waitFor(`document.querySelector(".turn-model .answerblock") !== null`, { timeout: 20000 })
  await app.page.waitFor(`${linesRow}.then((row) => row !== null && Object.keys(row.keys).length > 0)`, { timeout: 15000 })
  const row = await app.page.evaluate<{ replica: { objects: { hash: string }[] }; keys: Record<string, string> }>(linesRow)
  assert.ok(row.replica.objects.length > 0, "the lines' skeleton")
  assert.ok(Object.values(row.keys).every((hex) => /^[0-9a-f]{64}$/.test(hex)), "each key its 32 bytes")
  // The content the page read is kept by name beside the skeleton, each literal its words sealed.
  const kept = await app.page.evaluate<[string, string][]>(rowsOf("suzatary", "content"))
  assert.ok(kept.length > 0 && kept.every(([name]) => row.replica.objects.some((n) => n.hash === name)), "the content of nodes the skeleton names")
  const device = await app.page.evaluate<string>(held("suzatary"))
  assert.ok(!device.includes("compiled to HTML"), "nothing the device keeps holds the words in the clear")
  assert.ok(!device.includes("give me a rendered answer"))
  assert.equal(await app.page.evaluate<string>(held("suzatary")).then((all) => all.includes("objects:store/conversations")), false, "no row of the old store")
})

app.scene("Erase draws a turn by its shape and the device forgets the blocks its words rendered", async () => {
  await app.resize(PHONE)
  await app.open(WRITE_BASE)
  await converse("give me a rendered answer")
  await app.page.waitFor(`document.querySelector(".turn-model .answerblock") !== null`, { timeout: 20000 })
  await app.page.waitFor(`${held("suzatary-rendered")}.then((all) => all.includes("compiled to HTML"))`, { timeout: 15000 })
  await app.page.click(".turn-model .turnmorebutton")
  await app.page.click(".turn-model .turnerase .act-erase")
  await app.page.waitFor(`document.querySelector(".turn-model .turnerase .act-erase.confirming") !== null`, { timeout: 5000 })
  // The box's own sentence for what an erasure does, read from the module, as its card says it.
  await app.page.waitFor(`document.querySelector(".turn-model .turnerase .proposalconfirm").textContent.includes("what they referred to stay")`, { timeout: 5000 })
  await app.page.click(".turn-model .turnerase .act-erase.confirming")
  await app.page.waitFor(`document.querySelector(".turn-model.gone .turngone") !== null`, { timeout: 20000 })
  assert.match(await app.page.evaluate<string>(`document.querySelector(".turn-model.gone .turngone").textContent`), /^\[.+ erased these words\]$/)
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".turn-model.gone .answerblock, .turn-model.gone .turnreply, .turn-model.gone .act-erase") === null`), true, "no words, no Reply, no Erase")
  assert.ok(!(await app.page.evaluate<string>(said("model"))).includes("compiled to HTML"))
  await app.page.waitFor(`${held("suzatary-rendered")}.then((all) => !all.includes("compiled to HTML"))`, { timeout: 15000 })
  assert.ok(!(await app.page.evaluate<string>(held("suzatary"))).includes("compiled to HTML"), "nothing the device holds says it")
})

app.scene("forgetting a fact erases what it caused, and the tab's tap erases what it offers", async () => {
  await app.resize(PHONE)
  await app.open(WRITE_BASE)
  await converse("remember that I take calls after ten")
  await app.page.waitFor(`document.querySelector(".receipt .act.forget") !== null`, { timeout: 20000 })
  await app.page.click(".receipt .act.forget")
  await app.page.waitFor(`document.querySelector(".receipt")?.dataset.state === "forgotten"`, { timeout: 20000 })
  await app.page.waitFor(`document.querySelector(".proposal-act-erase .act-erase") !== null`, { timeout: 20000 })
  await app.page.click(".proposal-act-erase .act-erase")
  await app.page.waitFor(`document.querySelector(".proposal-act-erase .act-erase.confirming") !== null`, { timeout: 5000 })
  assert.match(await app.page.evaluate<string>(`document.querySelector(".proposal-act-erase .proposalconfirm").textContent`), /for good/)
  await app.page.click(".proposal-act-erase .act-erase.confirming")
  await app.page.waitFor(`document.querySelector(".turn-person.gone .turngone") !== null`, { timeout: 20000 })
  assert.ok(!(await app.page.evaluate<string>(said("person"))).includes("calls after ten"), "the turn that said it is erased")
  assert.ok(!(await app.page.evaluate<string>(said("model"))).includes("Noted."), "and the round's prose")
  await app.page.waitFor(`${held("suzatary")}.then((all) => !all.includes("calls after ten"))`, { timeout: 15000 })
})

app.scene("a device that kept the plaintext conversations is upgraded, and nothing plaintext remains", async () => {
  const OLD = "the old plaintext words of a line"
  await app.resize(PHONE)
  await app.open(WRITE_BASE)
  await converse("hello there")
  await app.page.waitFor(`${linesRow}.then((row) => row !== null && row.replica.objects.length > 0)`, { timeout: 20000 })
  // A row as a build before the lines kept it, and a block it rendered, put where that build put them.
  const hash = "e".repeat(64)
  const plain = {
    generation: 0,
    objects: [{ hash, literal: `Change(genesis='${"f".repeat(64)}', deps=(), event=Said(conversation='default', at=datetime(2026, 9, 2, 12, 0, 0), actor='assistant', held=0, turn='{"who":"person","text":"${OLD}","references":[]}', by=None))` }],
    synced: [hash],
    refused: [],
    words: [],
  }
  await app.page.evaluate(`new Promise((ok, no) => {
    const put = (name, store, write) => new Promise((done, fail) => {
      const r = indexedDB.open(name)
      r.onerror = () => fail(String(r.error))
      r.onsuccess = () => {
        const tx = r.result.transaction(store, "readwrite")
        write(tx.objectStore(store))
        tx.oncomplete = () => (r.result.close(), done(true))
        tx.onerror = () => fail(String(tx.error))
      }
    })
    put("suzatary", "chain", (s) => s.put(${JSON.stringify(plain)}, "objects:store/conversations"))
      .then(() => put("suzatary-rendered", "rows", (s) => s.put({ key: "answer old", html: "<p>${OLD}</p>", bytes: 2, used: 1 })))
      .then(ok, no)
  })`)
  assert.ok((await app.page.evaluate<string>(held("suzatary"))).includes(OLD), "the old row is on the device")
  // A new load: the build reads what the device holds afresh.
  await app.page.navigate(`${WRITE_BASE}/?upgraded=1#/secretary/default`)
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 })
  await app.page.waitFor(`${held("suzatary")}.then((all) => !all.includes("objects:store/conversations"))`, { timeout: 20000 })
  assert.ok(!(await app.page.evaluate<string>(held("suzatary"))).includes(OLD), "the old row is gone")
  assert.ok(!(await app.page.evaluate<string>(held("suzatary-rendered"))).includes(OLD), "and what it rendered")
  assert.ok((await app.page.evaluate<{ replica: { objects: unknown[] } } | null>(linesRow))?.replica.objects.length !== 0, "the lines stay")
})

app.scene("an inbox row a build before the keys kept is taken again from the box, and nothing says it does not read", async () => {
  await app.resize(PHONE)
  await app.open(WRITE_BASE)
  await app.page.navigate(`${WRITE_BASE}/#/inbox`)
  const rowAt = (key: string): string => `${rowsOf("suzatary", "chain")}.then((rows) => rows.find(([k]) => k === "${key}")?.[1] ?? null)`
  const nodes = `${rowsOf("suzatary", "inbox")}.then((rows) => rows.filter(([k]) => k.startsWith("node/")).map(([, v]) => v))`
  const oldRow = rowAt("objects:store/inbox")
  await app.page.waitFor(`${nodes}.then((ns) => ns.length > 0)`, { timeout: 20000 })
  // The row as the build before the keys wrote it: the bare replica, under the store's name, and the inbox's own store
  // emptied.
  const objects = await app.page.evaluate<unknown[]>(nodes)
  const bare = { generation: 0, objects, synced: [], refused: [], words: [], frontier: { type: "Nothing" } }
  await app.page.evaluate(put("suzatary", "inbox", `(s) => s.clear()`))
  await app.page.evaluate(put("suzatary", "chain", `(s) => s.put(${JSON.stringify(bare)}, "objects:store/inbox")`))
  assert.equal(await app.page.evaluate<boolean>(`${oldRow}.then((row) => row !== null && row.replica === undefined)`), true, "the old shape is on the device")
  await app.page.navigate(`${WRITE_BASE}/?bare=1#/inbox`)
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 })
  await app.page.waitFor(`${nodes}.then((ns) => ns.length > 0)`, { timeout: 20000 })
  assert.equal(await app.page.evaluate<unknown>(oldRow), null, "the old row is gone")
  assert.equal(
    await app.page.evaluate<boolean>(`document.querySelector('#excluded li[data-row="store/inbox"]') === null`),
    true,
    "no line says the inbox does not read",
  )
})

app.scene("a key a closed tab left on the device is forgotten with the blocks it fed, by the next tab that reads the row", async () => {
  await app.resize(PHONE)
  await app.open(WRITE_BASE)
  // With no marks an earlier scene's erasure left: the scripted answer is the same words, whose blocks a mark keeps off
  // the device for good.
  await app.page.evaluate(put("suzatary-rendered", "rows", `(s) => s.clear()`))
  await converse("give me a rendered answer")
  await app.page.waitFor(`${held("suzatary-rendered")}.then((all) => all.includes("compiled to HTML"))`, { timeout: 20000 })
  // What a tab that is then closed keeps: the answer's key and the blocks it rendered.
  const keys = await app.page.evaluate<Record<string, string>>(`${linesRow}.then((row) => row.keys)`)
  const blocks = await app.page.evaluate<[string, unknown][]>(`${rowsOf("suzatary-rendered", "rows")}.then((rows) => rows.filter(([, v]) => v.html.includes("compiled to HTML")))`)
  await app.page.click(".turn-model .turnmorebutton")
  await app.page.click(".turn-model .turnerase .act-erase")
  await app.page.click(".turn-model .turnerase .act-erase.confirming")
  await app.page.waitFor(`document.querySelector(".turn-model.gone .turngone") !== null`, { timeout: 20000 })
  await app.page.waitFor(`${held("suzatary-rendered")}.then((all) => !all.includes("compiled to HTML"))`, { timeout: 15000 })
  const erased = await app.page.evaluate<string[]>(`${linesRow}.then((row) => ${JSON.stringify(Object.keys(keys))}.filter((k) => !(k in row.keys)))`)
  assert.ok(erased.length > 0, "the erased answer's key went")
  // The closed tab's late write lands: its key in the row, its blocks in the cache, each also under the name an
  // older build gave it, which no purge of this build computes.
  await app.page.evaluate(
    put("suzatary", "lines", `(s) => Object.entries(${JSON.stringify(keys)}).forEach(([name, hex]) => s.put(hex, "key/" + name))`),
  )
  await app.page.evaluate(
    put("suzatary-rendered", "rows", `(s) => ${JSON.stringify(blocks)}.forEach(([, v]) => (s.put(v), s.put({ ...v, key: "older-answer " + v.key.split(" ")[1] })))`),
  )
  assert.ok((await app.page.evaluate<string>(held("suzatary-rendered"))).includes("compiled to HTML"), "the blocks are back on the device")
  await app.page.navigate(`${WRITE_BASE}/?closed=1#/secretary/default`)
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 })
  await app.page.waitFor(`${held("suzatary-rendered")}.then((all) => !all.includes("compiled to HTML"))`, { timeout: 20000 })
  await app.page.waitFor(`${linesRow}.then((row) => ${JSON.stringify(erased)}.every((k) => !(k in row.keys)))`, { timeout: 15000 })
})

app.scene("the list draws and the badge is counted with no request for the lines, which are pulled when the secretary opens", async () => {
  // The popup reads the proposals from the inbox, which the boot may pull; the lines wait for a page that reads them.
  await app.resize(PHONE)
  // The lines' pulls, never their stream, which a capture would wait on forever.
  const LINES = ["/api/stores/lines/skeleton", "/api/stores/lines/content", "/api/stores/lines/objects"]
  const ofLines = (url: string): boolean => LINES.some(l => url.includes(l))
  // With no lines kept, so the pull this scene counts is a whole pull of the store: a row an earlier scene left
  // would make it a 304 with no body, whatever order the files run in.
  await app.open(WRITE_BASE)
  await app.page.evaluate(
    put("suzatary", "lines", `(s) => s.clear()`),
  )
  // Nor any content an earlier scene's page kept, so what the secretary reads is fetched by name.
  await app.page.evaluate(put("suzatary", "content", `(s) => s.clear()`))
  // And with no HTTP cache, so no earlier scene's answer comes back as a 304 the capture cannot read.
  await app.page.cold(true)
  // From a blank page, so no request the last scene's page left in flight is counted.
  await app.page.navigate("about:blank")
  const booting = await app.page.captureNetwork([...LINES, "/api/stores/inbox/objects", "/api/waiting"])
  await app.open(WRITE_BASE)
  await app.page.waitFor(`performance.getEntriesByType("resource").some((e) => e.name.includes("/api/waiting"))`, { timeout: 15000 })
  const boot = await booting()
  assert.deepEqual(boot.filter(r => ofLines(r.url)).map(r => r.url), [], "the boot never pulls the lines' skeleton or content")
  // The lines are pulled by the core's worker, whose requests the page's own entries do not list: what it landed is read
  // from the device instead.
  assert.deepEqual(await app.page.evaluate<[string, unknown][]>(rowsOf("suzatary", "lines")), [], "and lands nothing of them")
  // The badge's count is the box's; an answer the tab already holds comes back as a 304, with no body.
  const counted = boot.filter(r => r.url.includes("/api/waiting"))
  assert.ok(counted.length > 0, "the badge is asked of the box")
  for (const r of counted.filter(r => r.responseBody !== "")) assert.equal(typeof (JSON.parse(r.responseBody) as { count: unknown }).count, "number")
  await app.page.evaluate(`void (location.hash = "#/secretary/default")`)
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 })
  await app.page.waitFor(`${linesRow}.then((row) => row !== null && row.replica.objects.length > 0)`, { timeout: 20000 })
  // Once the lines land, the page's placeholder gives way to its list.
  await app.page.waitFor(`document.querySelector(".chats .placeholder") === null`, { timeout: 15000 })
  // The content is fetched by name once the skeleton names what the page reads.
  await app.page.waitFor(`${rowsOf("suzatary", "content")}.then((rows) => rows.length > 0)`, { timeout: 15000 })
  // The pull ended whole: the row holds the lines, and no kept piece is left, since pieces are dropped only when a
  // pull lands whole. Read from the device, not from captured bodies, which a revalidated or streamed answer may lack.
  const left = await app.page.evaluate<[string, unknown][]>(rowsOf("suzatary", "lines"))
  assert.deepEqual(left.filter(([k]) => k.startsWith("piece/")).map(([k]) => k), [], "no piece is left: the pull ended whole")
  await app.page.cold(false)
})

app.scene("a conversation the device prefetched reads with no network, and one it lacks says so", async () => {
  await app.resize(PHONE)
  // The core's worker prefetches and reads the lines, so it is attached as the page is.
  await app.page.attachWorkers()
  await app.open(WRITE_BASE)
  const kept = await converse("the prefetched one")
  const lacked = await converse("the one this device lacks")
  await app.page.waitFor(`document.querySelector(".turns .turn-person") !== null`, { timeout: 15000 })
  // No content on the device, and a new load that opens the second conversation alone: the first's lines come by
  // prefetch or not at all.
  await app.page.evaluate(put("suzatary", "content", `(s) => s.clear()`))
  await app.page.navigate(`${WRITE_BASE}/?prefetch=1#/secretary/${lacked}`)
  await app.page.waitFor(`document.querySelector(".turns .turn-person") !== null`, { timeout: 15000 })
  // Every line of both conversations on the device: the content store holds each node the skeleton names for them.
  const linesOf = (ids: readonly string[]): string =>
    `${linesRow}.then((row) => row === null ? [] : row.replica.objects.filter((n) => n.header.node === "line" && ${JSON.stringify(ids)}.includes(n.header.conversation)).map((n) => n.hash))`
  const contentNames = `${rowsOf("suzatary", "content")}.then((rows) => rows.map(([k]) => k))`
  await app.page.waitFor(
    `Promise.all([${linesOf([kept, lacked])}, ${contentNames}]).then(([lines, held]) => lines.length > 0 && lines.every((h) => held.includes(h)))`,
    { timeout: 20000 },
  )
  // A device that did not prefetch the second: its lines' content gone, its skeleton kept.
  const dropped = await app.page.evaluate<string[]>(linesOf([lacked]))
  await app.page.evaluate(put("suzatary", "content", `(s) => ${JSON.stringify(dropped)}.forEach((h) => s.delete(h))`))
  await app.page.failRequests(["/api/", "/events"])
  try {
    await app.page.navigate(`${WRITE_BASE}/?offline=1#/secretary/${kept}`)
    await app.page.waitFor(`[...document.querySelectorAll(".turns .turn-person .turntext")].some((t) => t.textContent.includes("the prefetched one"))`, { timeout: 15000 })
    assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".transcriptcontent .nothere").hidden`), true, "the prefetched conversation reads whole")
    await app.page.evaluate(`void (location.hash = "#/secretary/${lacked}")`)
    await app.page.waitFor(`document.querySelector(".transcriptcontent .nothere")?.hidden === false`, { timeout: 15000 })
    assert.match(
      await app.page.evaluate<string>(`document.querySelector(".transcriptcontent .nothere").textContent`),
      new RegExp(`^Not on this device yet: ${dropped.length} lines? of this conversation \\(`),
    )
    assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".turns .turn-person") === null`), true, "never an empty conversation drawn as read")
  } finally {
    await app.page.resumeRequests()
  }
})

// A FRACTION IS A PIE (WEB-SPEC §3.20): the line under the header draws a sync's share as the neutral pie, named in
// words. The lines' content is held at the box, so the line stands while it is read.
app.scene("a store's sync is drawn as a neutral pie, named in words", async () => {
  await app.open(WRITE_BASE)
  await app.page.evaluate(
    put("suzatary", "lines", `(s) => s.clear()`),
  )
  await app.page.evaluate(put("suzatary", "content", `(s) => s.clear()`))
  await app.page.cold(true)
  await app.page.navigate("about:blank")
  const release = await app.page.holdRequests(["/api/stores/lines/content"])
  try {
    await app.open(WRITE_BASE)
    await app.page.evaluate(`void (location.hash = "#/secretary/default")`)
    await app.page.untilHeld()
    await app.page.waitFor(`document.querySelector("#syncing .syncline svg.pie") !== null`, { timeout: 15000 })
    const [role, name, kind, words, bars] = await app.page.evaluate<[string, string, string, string, number]>(
      `(() => {
         const p = document.querySelector("#syncing .syncline svg.pie")
         return [p.getAttribute("role"), p.getAttribute("aria-label"), p.getAttribute("class"), p.closest(".syncline").textContent, document.querySelectorAll("progress").length]
       })()`,
    )
    assert.equal(role, "img")
    assert.match(name, /^\d+% synced$/, "the pie is named by its fraction in words")
    assert.equal(kind, "pie pieneutral", "progress wears the neutral accent, never the fulfillment scale")
    assert.match(words, /^Syncing conversations · /)
    assert.equal(bars, 0, "no fraction is drawn as a bar")
  } finally {
    await release()
    await app.page.cold(false)
  }
  await app.page.waitFor(`document.getElementById("syncing").hidden`, { timeout: 15000 })
})
