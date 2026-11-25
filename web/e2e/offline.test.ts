/**
 * The last fold is kept in `localStorage`, drawn before the fetch, and left under a line that dates it when the fetch
 * fails; a browser also holding the chain folds it with the core and says so in another sentence. No service worker
 * caches data, since an old chain served silently would show a number with no provenance, and every fixture is a dev
 * build whose document is itself a request, so the server is not killed: `/api/…` and `/events` fail at the network
 * while the bundle keeps loading. The database is emptied for the first offline, so neither passes by a race.
 */

import assert from "node:assert/strict";

import { BASE, chainKept, chainLanded, shownTodos, suite } from "./harness";

const app = await suite("offline");

const sleep = (ms: number): Promise<void> => new Promise((done) => setTimeout(done, ms));

/** How much of the chain this browser keeps, its log's entries: 0 for none, -1 for a database that will not answer. */
function heldObjects(): Promise<number> {
  return app.page.evaluate<number>(chainKept("life"));
}

/** Its own loop rather than a `waitFor`, because the count it answers is the scene's assertion. */
async function untilHeld(): Promise<number> {
  for (let waited = 0; waited < 30000; waited += 200) {
    const held = await heldObjects();
    if (held > 0) return held;
    await sleep(200);
  }
  throw new Error("the objects were never cached");
}

// The list asks the box for its fold once per tab even when the tab's own fold is fresh first, since only the box's
// fold is kept as the dated copy an offline reload draws.
app.scene("a warm tab whose own fold is fresh before the list mounts still keeps the box's fold", async () => {
  await app.open(BASE);
  await app.page.waitFor(`window.localStorage.getItem("suzatary.fold.life") !== null`);
  await untilHeld();
  await app.page.evaluate(`(window.localStorage.removeItem("suzatary.fold.life"), true)`);
  const release = await app.page.holdRequests(["views_bg.wasm"]);
  await app.page.reload();
  await app.page.untilHeld();
  // The replica restored and folded by the tab's own core, about now.
  await sleep(4000);
  await release();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 30000 });
  await app.page.waitFor(`window.localStorage.getItem("suzatary.fold.life") !== null`, { timeout: 20000 });
});

app.scene("a reload with no backend draws the cached fold under a dated banner", async () => {
  await app.open(BASE);
  const before = await shownTodos(app.page);
  await app.page.waitFor(`window.localStorage.getItem("suzatary.fold.life") !== null`);
  // Emptied after the caching has finished, for a browser with the fold and not the objects; the worker's core lets the
  // database go when it is deleted.
  await untilHeld();
  // The banner is down while the fold is fresh and the stream is up: a page that always said "stale" would say nothing.
  assert.equal(await app.page.evaluate<boolean>(`document.getElementById("stale").hidden`), true);
  assert.equal(
    await app.page.evaluate<boolean>(
      `new Promise((resolve) => { const request = indexedDB.deleteDatabase("suzatary"); request.onsuccess = () => resolve(true); request.onerror = () => resolve(false); request.onblocked = () => resolve(false); })`,
    ),
    true,
    "the database is deleted under the running app",
  );

  await app.page.failRequests(["/api/", "/events"]);
  await app.page.reload();

  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 15000 });
  // With no objects each todo is its row's value, and a group is the core's conjunction of them, never an average.
  assert.deepEqual(
    [...(await shownTodos(app.page))].sort(),
    [...before].sort(),
    "the cached fold should draw exactly the todos last seen",
  );
  // Priced once the core in the worker answers, which reads the device's chains first.
  await app.page.waitFor(`document.querySelectorAll(".tree .group .mark.unlinked").length === 0`, { timeout: 15000 });

  await app.page.waitFor(`document.getElementById("stale").hidden === false`, { timeout: 15000 });
  const said = await app.page.evaluate<string>(`document.querySelector(".stalewhat").textContent`);
  assert.match(said, /^Stale since \d\d:\d\d on \d{4}-\d\d-\d\d/, `the banner must date the fold, it said ${said}`);
  assert.match(said, /the last fold this browser saw/);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".staleretry").length`),
    1,
    "someone told their data is old is owed the chance to ask again",
  );

  // The live dot says the same in words for a screen reader, not only as a colour.
  await app.page.waitFor(`document.getElementById("live").getAttribute("aria-label") === "no live updates"`, {
    timeout: 20000,
  });
});

app.scene("holding the objects, the same reload folds them here instead", async () => {
  // The scene above left the network refused; this one has to fetch the chain before it can hold it.
  await app.page.resumeRequests();
  await app.page.evaluate(
    `new Promise((resolve) => { const request = indexedDB.deleteDatabase("suzatary"); request.onsuccess = () => resolve(true); request.onerror = () => resolve(false); request.onblocked = () => resolve(false); })`,
  );
  // A real reload: the page left above has an EventSource that gave up, and the tip it never heard tells `keepObjects`
  // which chain to hold.
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 15000 });
  await untilHeld();
  // Whole, read back through the core: heard from the box, with every object its landing names.
  await app.page.waitFor(chainLanded("life"), { timeout: 20000 });

  await app.page.failRequests(["/api/", "/events"]);
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 });
  await app.page.waitFor(`document.getElementById("stale").hidden === false`, { timeout: 20000 });

  // Waited for, since the banner says "stale" until the core is fetched and the objects folded; it names no object,
  // which the Chain page keeps.
  await app.page.waitFor(`/^Offline:/.test(document.querySelector(".stalewhat").textContent)`, { timeout: 30000 });
  const said = await app.page.evaluate<string>(`document.querySelector(".stalewhat").textContent`);
  assert.match(said, /^Offline: what you see is what this device holds/, said);
  assert.equal(/[0-9a-f]{8}/.test(said), false, said);

  const lines = await shownTodos(app.page);
  assert.ok(lines.length > 0, "the local fold drew no lines");
  // And holding the functions, the tab composes every group: no group is left without a value.
  await app.page.waitFor(`document.querySelectorAll(".tree .group .mark.unlinked").length === 0`, { timeout: 15000 });
  const priced = await app.page.evaluate<number>(`document.querySelectorAll(".tree .row svg.pie").length`);
  assert.ok(priced > 0, "the local fold priced nothing");
});

app.scene("a todo page this tab fetched keeps its page; one it never fetched is plain text", async () => {
  await app.page.resumeRequests();
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 1`, { timeout: 15000 });
  const [fetched, never] = await shownTodos(app.page);
  assert.ok(fetched !== undefined && never !== undefined, "two todos to open");

  // Opened once with the box answering: typst's page, "Why this number" in it.
  await app.page.evaluate(`location.hash = "#/todo/${fetched}"`);
  await app.page.waitFor(`document.querySelector(".todoview .reading .whytree") !== null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".offlinenote") === null`), true);

  // The box stops answering for todo pages; the list stays where it is.
  await app.page.failRequests(["/api/life/todo/", "/api/todo/"]);
  await app.page.evaluate(`location.hash = "#/"`);
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 15000 });

  await app.page.evaluate(`location.hash = "#/todo/${fetched}"`);
  await app.page.waitFor(`document.querySelector(".todoview .offlinenote") !== null`, { timeout: 15000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".todoview .offlinenote").textContent`),
    /as this tab last fetched it/,
  );
  assert.equal(
    await app.page.evaluate<boolean>(`document.querySelector(".todoview .reading .whytree") !== null`),
    true,
    "the fetched page is kept whole",
  );

  await app.page.evaluate(`location.hash = "#/todo/${never}"`);
  await app.page.waitFor(
    `document.querySelector(".todoview .todoid")?.textContent === ${JSON.stringify(never)} && document.querySelector(".todoview .offlinenote") !== null`,
    { timeout: 15000 },
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".todoview .offlinenote").textContent`),
    /never fetched this todo's page/,
  );
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".todoview .whytree") === null`), true);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".todoview .todobody .rich").length`),
    0,
    "a page never fetched is plain text, not a rendering",
  );
  await app.page.resumeRequests();
});
