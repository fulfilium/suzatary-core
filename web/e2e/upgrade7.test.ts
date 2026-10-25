/**
 * A PHONE LAST OPENED ON A BUILD THAT KEPT THE LINES' SKELETON WHOLE: version 6 of the device's database held the
 * lines as one row, `store/lines`, in the rows' store, standing at the frontier the box last named. Version 7 keeps
 * them one node and one key per row in a store of their own; the core's worker moves the old row there on its first
 * restore and deletes it, and the page reads the same conversation from the moved rows.
 *
 * Its own file, so its browser profile is fresh and the origin holds nothing until the scene plants the old database.
 */

import assert from "node:assert/strict"

import { SIGNED_POST, WRITE_BASE, suite } from "./harness"

const app = await suite("upgrade7")

type Head = { readonly generation: number; readonly words: readonly unknown[]; readonly refused: readonly unknown[]; readonly frontier: unknown }
type Rows = {
  readonly version: number
  readonly head: Head | null
  readonly nodes: readonly { readonly hash: string }[]
  readonly keys: Readonly<Record<string, string>>
  readonly old: unknown
}

// The database's version, the lines' store read as its rows say (the head, the nodes, the keys), and the version 6
// row, or null.
const rows = `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const db = r.result
    const tx = db.transaction(["lines", "chain"], "readonly")
    const keys = tx.objectStore("lines").getAllKeys()
    const values = tx.objectStore("lines").getAll()
    const old = tx.objectStore("chain").get("objects:store/lines")
    tx.oncomplete = () => {
      db.close()
      const all = keys.result.map((k, i) => [k, values.result[i]])
      done({
        version: db.version,
        head: all.find(([k]) => k === "head")?.[1] ?? null,
        nodes: all.filter(([k]) => k.startsWith("node/")).map(([, n]) => n),
        keys: Object.fromEntries(all.filter(([k]) => k.startsWith("key/")).map(([k, v]) => [k.slice(4), v])),
        old: old.result ?? null,
      })
    }
    tx.onerror = () => fail(String(tx.error))
  }
})`

app.scene("a device holding the lines' skeleton whole moves it one node per row, deletes the old row, and reads the same", async () => {
  await app.open(WRITE_BASE)
  const id = await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Untitled"})).then((r) => r.json()).then((j) => j.id)`,
  )
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`)
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 15000 })
  await app.page.type(".composebox", "the words that move")
  await app.page.click(".composesend")
  await app.page.waitFor(`[...document.querySelectorAll(".turns .turn-person .turntext")].some((t) => t.textContent.includes("the words that move"))`, { timeout: 20000 })
  await app.page.waitFor(`${rows}.then((r) => r.head !== null && r.nodes.length > 1)`, { timeout: 20000 })
  const now = await app.page.evaluate<Rows>(rows)
  assert.ok(now.head !== null)
  const names = now.nodes.map((n) => n.hash).sort()
  // The same lines as version 6 kept them: the whole row in the rows' store at its frontier, and no store of their own.
  const whole = { replica: { generation: now.head.generation, objects: now.nodes, synced: names, refused: now.head.refused, words: now.head.words, frontier: now.head.frontier }, keys: now.keys }
  await app.page.navigate(`${WRITE_BASE}/api/about`)
  const planted = await app.page.evaluate<boolean>(`new Promise((done, fail) => {
    const gone = indexedDB.deleteDatabase("suzatary")
    gone.onerror = () => fail(String(gone.error))
    gone.onsuccess = () => {
      const r = indexedDB.open("suzatary", 6)
      r.onupgradeneeded = () => (r.result.createObjectStore("chain"), r.result.createObjectStore("content"))
      r.onerror = () => fail(String(r.error))
      r.onsuccess = () => {
        const tx = r.result.transaction("chain", "readwrite")
        tx.objectStore("chain").put(${JSON.stringify(whole)}, "objects:store/lines")
        tx.oncomplete = () => (r.result.close(), done(true))
        tx.onerror = () => fail(String(tx.error))
      }
    }
  })`)
  assert.equal(planted, true)
  // The first pull after the upgrade asks from what was moved: a move that did nothing would ask from nothing.
  const release = await app.page.holdRequests(["/api/stores/lines/skeleton"])
  try {
    await app.page.navigate(`${WRITE_BASE}/?upgraded=7#/secretary/${id}`)
    await app.page.untilHeld(1)
    const first = app.page.heldUrls()[0] ?? ""
    assert.ok(new URL(first).searchParams.has("since"), `the first pull after the upgrade asks since what was moved (${first})`)
  } finally {
    await release()
  }
  await app.page.waitFor(`[...document.querySelectorAll(".turns .turn-person .turntext")].some((t) => t.textContent.includes("the words that move"))`, { timeout: 20000 })
  await app.page.waitFor(`${rows}.then((r) => r.old === null)`, { timeout: 20000 })
  const moved = await app.page.evaluate<Rows>(rows)
  assert.equal(moved.version, 7)
  assert.equal(moved.old, null, "the version 6 row is deleted")
  const held = moved.nodes.map((n) => n.hash)
  assert.ok(names.every((n) => held.includes(n)), "every node of the old row is a row of its own")
  assert.ok(Object.keys(now.keys).every((k) => moved.keys[k] === now.keys[k]), "and every key")
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

app.scene("the pieces of a pull a version 6 device cut before its first landing are deleted, keys and all, though no row was kept", async () => {
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
        tx.objectStore("chain").put(${JSON.stringify(piece)}, "objects:store/lines/piece/000000000000")
        tx.objectStore("chain").put(${JSON.stringify(piece)}, "objects:store/inbox/piece/000000000000")
        tx.oncomplete = () => (r.result.close(), done(true))
        tx.onerror = () => fail(String(tx.error))
      }
    }
  })`)
  assert.equal(planted, true)
  assert.equal((await app.page.evaluate<string[]>(keysUnder("objects:store/"))).length, 2)
  await app.page.navigate(`${WRITE_BASE}/?cut=6#/secretary`)
  await app.page.waitFor(`${keysUnder("objects:store/")}.then((ks) => ks.length === 0)`, { timeout: 20000 })
})
