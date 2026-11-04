/**
 * TWO LISTS, AND THEY ARE NOT EACH OTHER.
 *
 * The fixture deployment serves two chains: `life`, the six-todo chain every
 * other scene reads, and `suzatary`, which is EMPTY. Empty is what makes these
 * scenes evidence rather than decoration — a switcher that changed a label and
 * nothing else would still show six todos, and here it must show none.
 *
 * Three claims, each one a way the naive implementation goes wrong:
 *
 * 1. Switching shows the OTHER chain's list. Not the same list under a new
 *    name, and not a stale one: the store is reset rather than marked stale,
 *    because these entries are not old, they are about a different store.
 * 2. A todo added on `suzatary` lands in `events-suzatary/` AND NOT IN
 *    `events/`. Checked at the box, through both chains' own routes, because
 *    "it appeared in the list" is what a purely local write also looks like.
 * 3. The two replicas are independent: IndexedDB holds a row per chain, and
 *    the row for the chain nobody is looking at is not the one being written.
 *
 * This file drives `E2E_BASE_WRITE`, which has a chain pair of its own, because
 * scene 2 MOVES a tip and every read-only file shares the other server.
 */

import assert from "node:assert/strict";

import { WRITE_BASE, shownTodos, suite, switchChain, untilReplica, pickCategory } from "./harness";

const app = await suite("chains");

/** The open todos the BOX says a named chain holds, asked through that chain's
 * own route — the only opinion that counts about where an object landed. */
const openOn = async (chain: string): Promise<string[]> =>
  app.page.evaluate<string[]>(
    `fetch("/api/${chain}/fold")
       .then((r) => r.json())
       .then((b) => b.entries.filter((e) => e.state === "open").map((e) => e.todo).sort())`,
  );

app.scene("switching to suzatary shows an empty list", async () => {
  await app.open(WRITE_BASE);
  assert.ok((await shownTodos(app.page)).length >= 5, "the life chain opens with its own todos");

  await switchChain(app.page, "suzatary");
  // The empty list, not the other one's: a `Reset` puts the fold back to
  // `Unloaded`, so the page says it is loading and then says there is nothing —
  // never six todos belonging to a store nobody asked about.
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length === 0`, { timeout: 10000 });
  assert.deepEqual(await openOn("suzatary"), [], "and the box agrees the chain is empty");

  // And back, without losing anything: the other chain's cache and replica were
  // never evicted, they were just not being read.
  await switchChain(app.page, "life");
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length >= 5`, { timeout: 10000 });
});

app.scene("a todo added on suzatary lands there and not in life", async () => {
  await app.open(WRITE_BASE);
  const lifeBefore = await openOn("life");

  await switchChain(app.page, "suzatary");
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length === 0`, { timeout: 10000 });
  // AN ACT NEEDS A GENESIS. Authoring is the box's (`POST /api/<chain>/author`)
  // but the object is this browser's, appended to a replica that names the
  // chain's genesis — so the replica has to have pulled THIS chain before
  // anything can be written to it. That is also why the real `events-suzatary/` is seeded by
  // hand: a browser cannot make a chain's first object.
  await untilReplica(app.page, "suzatary");
  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await app.page.type(".draftbody", "Fix the flaky guards test");
  await pickCategory(app.page, "other");
  await app.page.click(".addbtn");

  const id = "fixtheflakyguardstest";
  await app.page.waitFor(`document.querySelector('.tree .row[data-todo="${id}"]') !== null`, { timeout: 12000 });

  // THE CLAIM, checked at the box and in both directions. A write that only
  // reached this browser would look identical on screen.
  await app.page.waitFor(
    `fetch("/api/suzatary/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}"))`,
    { timeout: 15000 },
  );
  assert.deepEqual(await openOn("life"), lifeBefore, "nothing may have appeared in Ada's own list");

  // BACK TO `life` BEFORE THE NEXT SCENE. The selection is PERSISTED — that is
  // the design — so a scene that switches and does not switch back leaves the
  // browser it shares with the rest of this file looking at the other list.
  await switchChain(app.page, "life");
});

app.scene("the two replicas are independent rows", async () => {
  await app.open(WRITE_BASE);
  await untilReplica(app.page, "life");
  await switchChain(app.page, "suzatary");
  await untilReplica(app.page, "suzatary");

  // ONE ROW PER CHAIN. `life`'s objects are still under its own key while the
  // reader is looking at the other list — a single row would have been
  // overwritten by the switch, which is how acts get lost.
  const keys = await app.page.evaluate<string[]>(
    `new Promise((resolve) => {
      const request = indexedDB.open("suzatary");
      request.onerror = () => resolve([]);
      request.onsuccess = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains("chain")) { db.close(); resolve([]); return; }
        const query = db.transaction("chain", "readonly").objectStore("chain").getAllKeys();
        query.onerror = () => { db.close(); resolve([]); };
        query.onsuccess = () => { const found = [...query.result]; db.close(); resolve(found); };
      };
    })`,
  );
  assert.ok(keys.includes("objects:life"), `life's replica should still be held, keys were ${keys.join(", ")}`);
  assert.ok(keys.includes("objects:suzatary"), `and the one being read, keys were ${keys.join(", ")}`);
  assert.ok(!keys.includes("objects"), "the single-chain key is not written any more");
  await switchChain(app.page, "life");
});
