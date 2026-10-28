/**
 * A PHONE LAST OPENED ON A BUILD THAT KEPT THE PAGE'S STORES IN THE ROWS' STORE: the inbox and the topic stores were
 * each a replica under `pages/<name>`, or, before that, beside an empty set of keys under `store/<name>`, with the pieces
 * of a cut pull beside them. This build keeps each store one node per row in a store of its own; the core's worker
 * carries both old rows into it whole the first time it restores the store (a topic this tab wrote and the box has not
 * taken is in them) and deletes them with their pieces.
 *
 * Its own file, so its browser profile is fresh and the origin holds nothing until the scene plants the old rows.
 */

import assert from "node:assert/strict"

import { WRITE_BASE, suite } from "./harness"

const app = await suite("pagerows")

type Held = {
  readonly generation: number
  readonly objects: readonly { readonly hash: string; readonly literal: string }[]
  readonly synced: readonly string[]
  readonly refused: readonly never[]
  readonly words: readonly never[]
  readonly frontier: { readonly type: "Nothing" }
}

// The inbox's nodes as this build keeps them, and the keys of the rows' store a build before kept it under.
const rows = `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const db = r.result
    const tx = db.transaction(["inbox", "chain"], "readonly")
    const nodes = tx.objectStore("inbox").getAll(IDBKeyRange.bound("node/", "node/\\uffff"))
    const old = tx.objectStore("chain").getAllKeys()
    tx.oncomplete = () => (db.close(), done({ nodes: nodes.result, old: old.result.filter((k) => /^objects:(pages|store)\\/inbox/.test(String(k))) }))
    tx.onerror = () => fail(String(tx.error))
  }
})`
type Rows = { readonly nodes: readonly { readonly hash: string; readonly literal: string }[]; readonly old: readonly string[] }

// `f` over the inbox's store and the rows' store, written in one transaction.
const writing = (f: string): string => `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const tx = r.result.transaction(["inbox", "chain"], "readwrite")
    ;(${f})(tx.objectStore("inbox"), tx.objectStore("chain"))
    tx.oncomplete = () => (r.result.close(), done(true))
    tx.onerror = () => fail(String(tx.error))
  }
})`

app.scene("a device holding the inbox in the rows' store carries it whole to the inbox's own store, and deletes the old rows", async () => {
  await app.open(WRITE_BASE)
  await app.page.navigate(`${WRITE_BASE}/#/inbox`)
  await app.page.waitFor(`${rows}.then((r) => r.nodes.length > 0)`, { timeout: 20000 })
  const nodes = (await app.page.evaluate<Rows>(rows)).nodes
  // The same inbox as the builds before kept it: the replica under `pages/inbox`, and beside no keys under the store's
  // name, with a piece; and nothing in the inbox's own store.
  const held: Held = { generation: 0, objects: nodes, synced: nodes.map(n => n.hash), refused: [], words: [], frontier: { type: "Nothing" } }
  await app.page.navigate(`${WRITE_BASE}/api/about`)
  await app.page.evaluate(
    writing(`(inbox, s) => {
      inbox.clear()
      s.put(${JSON.stringify(held)}, "objects:pages/inbox")
      s.put({ replica: ${JSON.stringify(held)}, keys: {} }, "objects:store/inbox")
      s.put({ replica: ${JSON.stringify(held)}, keys: {} }, "objects:store/inbox/piece/000000000000")
    }`),
  )
  // The box answers nothing, so what the inbox's store holds is what was carried.
  const release = await app.page.holdRequests(["/api/stores/"])
  try {
    await app.page.navigate(`${WRITE_BASE}/#/inbox`)
    await app.page.waitFor(`${rows}.then((r) => r.old.length === 0 && r.nodes.length > 0)`, { timeout: 20000 })
  } finally {
    await release()
  }
  const carried = (await app.page.evaluate<Rows>(rows)).nodes
  assert.deepEqual(new Set(carried.map(o => o.hash)), new Set(nodes.map(o => o.hash)), "the replica moved whole")
})
