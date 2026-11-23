/**
 * The rows before the reading. Todos draws from the box's fold as soon as it has it, priced by the core in its
 * worker, and moves to the reading of the objects this browser holds when that lands, without a row moving.
 *
 * PROVEN RATHER THAN TIMED: the replica's objects are held at the network, so the reading cannot be made; rows on
 * screen then are drawn from the box's fold and nothing else. Released, the reading lands (`data-at` is set) and the
 * same todos stand in the same order.
 */

import assert from "node:assert/strict";

import { BASE, chainKept, shownTodos, suite } from "./harness";

const app = await suite("firstrows");

app.scene("Todos draws its rows from the box's fold before the local fold, and the fold keeps their keys and order", async () => {
  const release = await app.page.holdRequests(["/objects"]);
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`);
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".tree").getAttribute("data-at")`),
    "",
    "no reading yet: these rows are the box's",
  );
  const keys = `[...document.querySelectorAll(".tree [data-key]")].map((n) => n.getAttribute("data-key"))`;
  const before = await app.page.evaluate<string[]>(keys);
  const todos = await shownTodos(app.page);
  await release();
  await app.page.waitFor(`(document.querySelector(".tree")?.getAttribute("data-at") ?? "") !== ""`);
  assert.deepEqual(await shownTodos(app.page), todos, "the same todos, in the same order");
  assert.deepEqual(await app.page.evaluate<string[]>(keys), before, "and every line keeps its key");
});

// The project chain is one the Todos page is not reading: its pull is kept on the device, so a load after it asks the
// box only for what came after the heads it holds, never for the whole chain again.
const kept = `${chainKept("suzatary")}.then((n) => n > 0)`;

app.scene("a chain the page is not reading is kept on the device, and a warm load asks only for what came after its heads", async () => {
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(kept, { timeout: 15000 });
  await app.page.reload();
  // The core's worker pulls the chains, so its own record of what it fetched is read.
  const pulls = `performance.getEntriesByType("resource").map((e) => e.name).filter((n) => n.includes("/api/suzatary/objects"))`;
  let urls: string[] = [];
  for (let waited = 0; urls.length === 0 && waited < 15000; waited += 200) {
    urls = await app.page.inWorker<string[]>("prodromework", pulls).catch(() => []);
    if (urls.length === 0) await new Promise((next) => setTimeout(next, 200));
  }
  assert.ok(urls.length > 0, "the worker pulled the chain the page is not reading");
  for (const url of urls) assert.match(url, /[?&]since=/, `${url} asks for the whole chain`);
});
