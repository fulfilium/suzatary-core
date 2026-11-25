/**
 * Switching chains shows the other chain's list, a todo added there lands at the box on that chain and not on `life`,
 * and IndexedDB keeps one replica row per chain. The second chain is empty, so a switcher that only changed a label
 * would still show six todos. It drives `E2E_BASE_WRITE`, because a scene moves a tip and the read-only files share
 * the other server.
 */

import assert from "node:assert/strict";

import { PHONE, WRITE_BASE, chainKept, shownTodos, suite, switchChain, untilReplica, pickCategory } from "./harness";

const app = await suite("chains");

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
  // A `Reset` puts the fold back to `Unloaded`, so the page never shows the other chain's todos meanwhile.
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length === 0`, { timeout: 10000 });
  assert.deepEqual(await openOn("suzatary"), [], "and the box agrees the chain is empty");

  // The other chain's cache and replica were never evicted, only not read.
  await switchChain(app.page, "life");
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length >= 5`, { timeout: 10000 });
});

// The list's title is read from the chains the box serves, which land after the first paint, so it is drawn again then.
app.scene("a cold link to a todo on the second chain, at a desk, titles the list beside it with that chain", async () => {
  await app.open(WRITE_BASE);
  await switchChain(app.page, "suzatary");
  await app.resize([1440, 900]);
  // Cold: a new document, so the chains the box serves land after the list is first drawn.
  await app.page.navigate("about:blank");
  await app.page.navigate(`${WRITE_BASE}/#/todo/ada`);
  await app.page.waitFor(`document.querySelector("#view.tiled .todospane .pagetitle")?.textContent.trim() === "suzatary"`, { timeout: 15000 });
  await app.resize(PHONE);
  await app.page.navigate(`${WRITE_BASE}/#/`);
  await switchChain(app.page, "life");
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length >= 5`, { timeout: 10000 });
});

app.scene("a todo added on suzatary lands there and not in life", async () => {
  await app.open(WRITE_BASE);
  const lifeBefore = await openOn("life");

  await switchChain(app.page, "suzatary");
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length === 0`, { timeout: 10000 });
  // An act is appended to a replica that names the chain's genesis, so the replica must have pulled this chain first;
  // a browser cannot make a chain's first object, so the fixture's upkeep chain is seeded by hand.
  await untilReplica(app.page, "suzatary");
  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await app.page.type(".draftbody", "Fix the flaky guards test");
  await pickCategory(app.page, "other");
  await app.page.click(".addbtn");

  const id = "fixtheflakyguardstest";
  await app.page.waitFor(`document.querySelector('.tree .row[data-todo="${id}"]') !== null`, { timeout: 12000 });

  // Checked at the box: a write that only reached this browser would look identical on screen.
  await app.page.waitFor(
    `fetch("/api/suzatary/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}"))`,
    { timeout: 15000 },
  );
  assert.deepEqual(await openOn("life"), lifeBefore, "nothing may have appeared in Ada's own list");

  // The selection is persisted, so the scene switches back before the next one.
  await switchChain(app.page, "life");
});

app.scene("the two replicas are independent rows", async () => {
  await app.open(WRITE_BASE);
  await untilReplica(app.page, "life");
  await switchChain(app.page, "suzatary");
  await untilReplica(app.page, "suzatary");

  // One row per chain: a single row would be overwritten by the switch, which is how acts get lost.
  assert.ok((await app.page.evaluate<number>(chainKept("life"))) > 0, "life's replica is still held")
  assert.ok((await app.page.evaluate<number>(chainKept("suzatary"))) > 0, "and the one being read")
  await switchChain(app.page, "life");
});
