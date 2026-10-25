/**
 * A PHONE LAST OPENED ON A BUILD BEFORE v1.11.0: it holds the replica in IndexedDB at version 2, in the form that build
 * kept (one word of the box as loose fields, refusals with no kind). This build reads no such row: the database's
 * upgrade drops every row it held, and the first load pulls each chain from the box again, so the page draws the box's
 * todos and the device keeps a row in this build's form. An act that row held and the box never took is gone with it.
 *
 * Its own file, so its browser profile is fresh and the origin holds nothing until the scene plants the old database.
 */

import assert from "node:assert/strict";

import { BASE, shownTodos, suite, untilReplica } from "./harness";

const app = await suite("upgrade");

/** An object the box never held: an act the old row kept and never sent. */
const UNSENT = "e".repeat(64);

/** The database as a version 2 build left it: one store, `chain`, and the life chain's row in that build's form. */
const plantVersion2 = `new Promise((done, fail) => {
  const r = indexedDB.open("suzatary", 2)
  r.onupgradeneeded = () => r.result.createObjectStore("chain")
  r.onerror = () => fail(String(r.error))
  r.onsuccess = () => {
    const tx = r.result.transaction("chain", "readwrite")
    tx.objectStore("chain").put({
      objects: [{ hash: ${JSON.stringify(UNSENT)}, literal: "an act this device sealed and never sent" }],
      synced: [],
      refused: [{ hash: ${JSON.stringify(UNSENT)}, reason: "a refusal kept before refusals had a kind" }],
      boxHeads: [],
      untrusted: [],
      root: null,
      fetchedAt: 5,
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

type Kept = { readonly version: number; readonly row: { readonly objects: { readonly hash: string }[]; readonly words?: unknown; readonly boxHeads?: unknown; readonly generation?: unknown } | null };

app.scene("a device holding a version 2 replica drops it on its first load and pulls the chain again from the box", async () => {
  // A page of the origin that is not the app, so nothing opens the database before the old one is planted.
  await app.page.navigate(`${BASE}/api/about`);
  assert.equal(await app.page.evaluate<boolean>(plantVersion2), true);
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 15000 });
  await untilReplica(app.page);
  const now = await app.page.evaluate<Kept>(kept);
  assert.equal(now.version, 7);
  assert.ok(now.row !== null, "the chain is held again");
  assert.ok(!now.row.objects.some((o) => o.hash === UNSENT), "the old row's unsent act is not carried over");
  assert.ok(Array.isArray(now.row.words) && now.row.boxHeads === undefined, "the row is in this build's form");
  assert.equal(now.row.generation, 0);
  assert.ok((await shownTodos(app.page)).length > 0, "and the list draws the box's todos");
});
