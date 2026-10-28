/**
 * A PHONE LAST OPENED ON A BUILD OF DATABASE VERSION 3: it holds each replica with the box's words, and a word that
 * named no prodrome kept `null` for "did not say". This build carries such a row rather than dropping it: the upgrade
 * leaves out that word and keeps the rest, so the row's generation (and every object it holds, an unsent act included)
 * is still there, and the first load pulls the box's word again. The row planted holds no object, so the page reads the
 * box's chain alone; its generation is what only a carried row keeps.
 *
 * Its own file, so its browser profile is fresh and the origin holds nothing until the scene plants the old database.
 */

import assert from "node:assert/strict";

import { BASE, shownTodos, suite, untilReplica } from "./harness";

const app = await suite("upgrade3");

/** A generation only a carried row holds: a dropped one starts again at 0. */
const GENERATION = 7;

/** The database as a version 3 build left it: the life chain's row with one word that said no geneses. */
const plantVersion3 = `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary", 3)
  r.onupgradeneeded = () => r.result.createObjectStore("chain")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const tx = r.result.transaction("chain", "readwrite")
    tx.objectStore("chain").put({
      generation: ${GENERATION},
      objects: [],
      synced: [],
      refused: [],
      words: [{ epoch: 1, heads: [], untrusted: [], root: null, fetchedAt: 5, geneses: null }],
    }, "objects:life")
    tx.oncomplete = () => (r.result.close(), done(true))
    tx.onerror = () => fail(String(tx.error))
  }
})`;

/** The database's version and the life chain's row as the device keeps it now. */
const kept = `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const db = r.result
    const g = db.transaction("chain", "readonly").objectStore("chain").get("objects:life")
    g.onsuccess = () => (db.close(), done({ version: db.version, row: g.result ?? null }))
    g.onerror = () => fail(String(g.error))
  }
})`;

type Kept = {
  readonly version: number;
  readonly row: { readonly generation: number; readonly words: { readonly geneses: unknown }[] } | null;
};

app.scene("a device holding a version 3 replica keeps its row, drops the word that said nothing, and hears the box again", async () => {
  // A page of the origin that is not the app, so nothing opens the database before the old one is planted.
  await app.page.navigate(`${BASE}/api/about`);
  assert.equal(await app.page.evaluate<boolean>(plantVersion3), true);
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 15000 });
  await untilReplica(app.page);
  const now = await app.page.evaluate<Kept>(kept);
  assert.equal(now.version, 9);
  assert.ok(now.row !== null, "the chain is held");
  assert.equal(now.row.generation, GENERATION, "the row is carried, not dropped");
  assert.ok(now.row.words.length > 0 && now.row.words.every((w) => Array.isArray(w.geneses)), "every word names the box's prodromes");
  assert.ok((await shownTodos(app.page)).length > 0, "and the list draws the box's todos");
});
