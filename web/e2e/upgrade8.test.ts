/**
 * A PHONE LAST OPENED ON A BUILD WHOSE LINES' NODES CARRIED NO STEP: version 7 of the device's database kept the lines'
 * skeleton one node and one key per row in the `lines` store, and version 8's nodes carry each line's step, which the
 * stream chooses its rounds by. The upgrade drops the old skeleton and keeps the content by name, so the first pull
 * asks from nothing, every node it lands carries its step, and the page reads the same conversation with no content
 * asked of the box again. The keys dropped with it were what the blocks rendered from their words were found by, so the
 * upgrade clears every rendered block once. Earlier skeletons, version 6's whole row and its pieces, are dropped the same way.
 *
 * Its own file, so its browser profile is fresh and the origin holds nothing until the scene plants the old database.
 */

import assert from "node:assert/strict"

import { SIGNED_POST, WRITE_BASE, suite } from "./harness"

const app = await suite("upgrade8")

type Rows = {
  readonly version: number
  readonly nodes: readonly { readonly hash: string; readonly header: { readonly node: string; readonly step?: unknown } }[]
  readonly all: readonly (readonly [string, unknown])[]
  readonly content: readonly (readonly [string, unknown])[]
}

// The database's version, the lines' store's rows and its nodes, and the content store's rows.
const rows = `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const db = r.result
    const tx = db.transaction(["lines", "content"], "readonly")
    const keys = tx.objectStore("lines").getAllKeys()
    const values = tx.objectStore("lines").getAll()
    const ckeys = tx.objectStore("content").getAllKeys()
    const cvalues = tx.objectStore("content").getAll()
    tx.oncomplete = () => {
      db.close()
      const all = keys.result.map((k, i) => [k, values.result[i]])
      done({
        version: db.version,
        nodes: all.filter(([k]) => k.startsWith("node/")).map(([, n]) => n),
        all,
        content: ckeys.result.map((k, i) => [k, cvalues.result[i]]),
      })
    }
    tx.onerror = () => fail(String(tx.error))
  }
})`

const SAID = `[...document.querySelectorAll(".turns .turn-person .turntext")].some((t) => t.textContent.includes("the words that stay"))`

app.scene("a device holding version 7's lines' skeleton drops it and every rendered block, keeps the content, pulls the skeleton with its steps and reads the same", async () => {
  await app.open(WRITE_BASE)
  const id = await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Untitled"})).then((r) => r.json()).then((j) => j.id)`,
  )
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`)
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 15000 })
  await app.page.type(".composebox", "the words that stay")
  await app.page.click(".composesend")
  await app.page.waitFor(SAID, { timeout: 20000 })
  await app.page.waitFor(`${rows}.then((r) => r.nodes.length > 1 && r.content.length > 1)`, { timeout: 20000 })
  const now = await app.page.evaluate<Rows>(rows)
  // The same rows as version 7 kept them: every node without its step.
  const old = now.all.map(([k, v]) => [k, k.startsWith("node/") ? { ...(v as Rows["nodes"][number]), header: { ...(v as Rows["nodes"][number]).header, step: undefined } } : v] as const)
  await app.page.navigate(`${WRITE_BASE}/api/about`)
  const planted = await app.page.evaluate<boolean>(`new Promise((done, fail) => {
    const gone = indexedDB.deleteDatabase("suzatary")
    gone.onerror = () => fail(String(gone.error))
    gone.onsuccess = () => {
      const r = indexedDB.open("suzatary", 7)
      r.onupgradeneeded = () => (r.result.createObjectStore("chain"), r.result.createObjectStore("content"), r.result.createObjectStore("lines"))
      r.onerror = () => fail(String(r.error))
      r.onsuccess = () => {
        const tx = r.result.transaction(["lines", "content"], "readwrite")
        for (const [k, v] of ${JSON.stringify(old)}) tx.objectStore("lines").put(v, k)
        for (const [k, v] of ${JSON.stringify(now.content)}) tx.objectStore("content").put(v, k)
        tx.oncomplete = () => (r.result.close(), done(true))
        tx.onerror = () => fail(String(tx.error))
      }
    }
  })`)
  assert.equal(planted, true)
  // A block a build before this one rendered from the lines' words, which no key the device keeps can find any more.
  const block = await app.page.evaluate<boolean>(`new Promise((done, fail) => {
    const r = indexedDB.open("suzatary-rendered")
    r.onerror = () => fail(String(r.error))
    r.onsuccess = () => {
      const tx = r.result.transaction("rows", "readwrite")
      tx.objectStore("rows").put({ key: "answer old", html: "<p>words an older build rendered</p>", bytes: 2, used: 1 })
      tx.oncomplete = () => (r.result.close(), done(true))
      tx.onerror = () => fail(String(tx.error))
    }
  })`)
  assert.equal(block, true)
  const journal = await app.page.journal()
  // The first pull after the upgrade asks from nothing: an upgrade that kept the old nodes would ask since them.
  const release = await app.page.holdRequests(["/api/stores/lines/skeleton"])
  try {
    await app.page.navigate(`${WRITE_BASE}/?upgraded=8#/secretary/${id}`)
    await app.page.untilHeld(1)
    const first = app.page.heldUrls()[0] ?? ""
    assert.equal(new URL(first).searchParams.get("since") ?? "", "", `the first pull after the upgrade asks from nothing (${first})`)
  } finally {
    await release()
  }
  await app.page.waitFor(SAID, { timeout: 20000 })
  // The upgrade dropped the keys those blocks were found by, so it clears every block once.
  await app.page.waitFor(`new Promise((done, fail) => {
    const r = indexedDB.open("suzatary-rendered")
    r.onerror = () => fail(String(r.error))
    r.onsuccess = () => {
      const g = r.result.transaction("rows").objectStore("rows").get("answer old")
      g.onsuccess = () => (r.result.close(), done(g.result === undefined))
      g.onerror = () => fail(String(g.error))
    }
  })`, { timeout: 20000 })
  const pulled = await app.page.evaluate<Rows>(rows)
  assert.equal(pulled.version, 9)
  assert.ok(pulled.nodes.length >= now.nodes.length, "the skeleton is pulled again")
  assert.ok(pulled.nodes.every((n) => n.header.node !== "line" || n.header.step !== undefined), "every line's node carries its step")
  assert.ok(now.content.every(([k]) => pulled.content.some(([c]) => c === k)), "the content by name stays")
  const kept = new Set(now.content.map(([k]) => k))
  const asked = journal.lines().filter((l) => l.includes("/api/stores/lines/content") && [...kept].some((k) => l.includes(k)))
  assert.deepEqual(asked, [], "no content the device kept was asked of the box again")
})

// Every key of the rows' store holding `prefix`, of whatever version the device holds.
const keysUnder = (prefix: string): string => `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const keys = r.result.transaction("chain", "readonly").objectStore("chain").getAllKeys()
    keys.onsuccess = () => (r.result.close(), done(keys.result.filter((k) => String(k).startsWith(${JSON.stringify(prefix)}))))
    keys.onerror = () => fail(String(keys.error))
  }
})`

app.scene("a version 6 device's whole lines' row and the pieces of a pull it cut are deleted, keys and all", async () => {
  await app.open(WRITE_BASE)
  await app.page.navigate(`${WRITE_BASE}/api/about`)
  // A first pull of the lines and of the inbox cut after one piece each: the pieces, with the keys of their words, and
  // no row.
  const piece = { replica: { generation: 0, objects: [], synced: [], refused: [], words: [], frontier: { type: "Nothing" } }, keys: { ["a".repeat(64)]: "b".repeat(64) } }
  const planted = await app.page.evaluate<boolean>(`new Promise((done, fail) => {
    const gone = indexedDB.deleteDatabase("suzatary")
    gone.onerror = () => fail(String(gone.error))
    gone.onsuccess = () => {
      const r = indexedDB.open("suzatary", 6)
      r.onupgradeneeded = () => (r.result.createObjectStore("chain"), r.result.createObjectStore("content"))
      r.onerror = () => fail(String(r.error))
      r.onsuccess = () => {
        const tx = r.result.transaction("chain", "readwrite")
        tx.objectStore("chain").put(${JSON.stringify(piece)}, "objects:store/lines")
        tx.objectStore("chain").put(${JSON.stringify(piece)}, "objects:store/lines/piece/000000000000")
        tx.objectStore("chain").put(${JSON.stringify(piece)}, "objects:store/inbox/piece/000000000000")
        tx.oncomplete = () => (r.result.close(), done(true))
        tx.onerror = () => fail(String(tx.error))
      }
    }
  })`)
  assert.equal(planted, true)
  assert.equal((await app.page.evaluate<string[]>(keysUnder("objects:store/"))).length, 3)
  await app.page.navigate(`${WRITE_BASE}/?cut=6#/secretary`)
  await app.page.waitFor(`${keysUnder("objects:store/")}.then((ks) => ks.length === 0)`, { timeout: 20000 })
})
