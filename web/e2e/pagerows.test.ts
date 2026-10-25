/**
 * A PHONE LAST OPENED ON A BUILD THAT KEPT THE PAGE'S STORES WITH KEYS: the inbox and the topic stores lock no words,
 * yet a build before this one kept each as a replica beside an empty set of keys, under `store/<name>` in the rows'
 * store. This build keeps each as its replica alone, under `pages/<name>`; the first restore moves the old row there
 * whole (a topic this tab wrote and the box has not taken is in it) and deletes it with its pieces.
 *
 * Its own file, so its browser profile is fresh and the origin holds nothing until the scene plants the old row.
 */

import assert from "node:assert/strict"

import { WRITE_BASE, suite } from "./harness"

const app = await suite("pagerows")

type Row = { readonly objects: readonly { readonly hash: string }[]; readonly synced: readonly string[] }

// The inbox's row as this build keeps it, and the row a build before kept, or null for either that is not there.
const rows = `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const db = r.result
    const tx = db.transaction("chain", "readonly")
    const now = tx.objectStore("chain").get("objects:pages/inbox")
    const old = tx.objectStore("chain").get("objects:store/inbox")
    tx.oncomplete = () => (db.close(), done({ now: now.result ?? null, old: old.result ?? null }))
    tx.onerror = () => fail(String(tx.error))
  }
})`

// `f` over the rows' store, written in one transaction.
const writing = (f: string): string => `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const tx = r.result.transaction("chain", "readwrite")
    ;(${f})(tx.objectStore("chain"))
    tx.oncomplete = () => (r.result.close(), done(true))
    tx.onerror = () => fail(String(tx.error))
  }
})`

app.scene("a device holding the inbox with keys moves its replica whole to the row without them, and deletes the old", async () => {
  await app.open(WRITE_BASE)
  await app.page.navigate(`${WRITE_BASE}/#/inbox`)
  await app.page.waitFor(`${rows}.then((r) => r.now !== null && r.now.synced.length > 0)`, { timeout: 20000 })
  const kept = (await app.page.evaluate<{ now: Row; old: unknown }>(rows)).now
  assert.equal((await app.page.evaluate<{ now: Row; old: unknown }>(rows)).old, null, "this build writes no row under the old name")
  // The same inbox as the build before kept it: the replica beside no keys, under the store's name, with a piece.
  await app.page.navigate(`${WRITE_BASE}/api/about`)
  await app.page.evaluate(
    writing(`(s) => {
      s.delete("objects:pages/inbox")
      s.put({ replica: ${JSON.stringify(kept)}, keys: {} }, "objects:store/inbox")
      s.put({ replica: ${JSON.stringify(kept)}, keys: {} }, "objects:store/inbox/piece/000000000000")
    }`),
  )
  await app.page.navigate(`${WRITE_BASE}/#/inbox`)
  await app.page.waitFor(`${rows}.then((r) => r.old === null && r.now !== null)`, { timeout: 20000 })
  const moved = (await app.page.evaluate<{ now: Row; old: unknown }>(rows)).now
  assert.deepEqual(new Set(moved.objects.map(o => o.hash)), new Set(kept.objects.map(o => o.hash)), "the replica moved whole")
  assert.ok(!("keys" in moved), "the row holds no keys")
  const piece = await app.page.evaluate<unknown>(`new Promise((done) => {
    const r = indexedDB.open("suzatary")
    r.onsuccess = () => { const g = r.result.transaction("chain").objectStore("chain").get("objects:store/inbox/piece/000000000000"); g.onsuccess = () => (r.result.close(), done(g.result ?? null)) }
  })`)
  assert.equal(piece, null, "the old row's pieces went with it")
})
