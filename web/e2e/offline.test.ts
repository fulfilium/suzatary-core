/**
 * Local-first, and the banner that makes it honest.
 *
 * The claim (`web/src/cache.ts`, `web/src/stale.ts`) is that the last fold is
 * kept in `localStorage`, drawn before the fetch, and — when the fetch fails —
 * left on screen under a line that DATES it. Every part of that is behaviour:
 * the pure tests can check what the cache holds and what sentence the banner
 * picks, and neither can check that a reload with no backend puts cards on the
 * screen instead of an empty page.
 *
 * WHY THE SERVER IS NOT KILLED, though that is the obvious way to write this.
 * A service worker for DATA was REJECTED here on purpose: a cache serving an
 * old chain silently would put a number on screen with no provenance. The
 * worker that exists keeps a STAMPED build's shell (`sw.ts`), and every
 * fixture here is a dev build, so with the server truly gone there is no app
 * at all — the document itself is a request — and "kill it and reload" would
 * test the browser's error page, not this design. What the design is about is an unreachable
 * BACKEND, so that is what this makes unreachable: every `/api/…` and `/events`
 * request fails at the network, which is exactly what the reader's phone sees
 * on a train, and the static bundle it already has keeps loading.
 *
 * TWO OFFLINES, and the difference is the point. A browser holding only the
 * last fold shows dated numbers and says so. A browser also holding the chain
 * (the core's replica of it, in its worker) folds it and shows this moment's
 * numbers, under a different sentence: what is uncertain then is the chain,
 * not the clock. Both are driven below, and the database is emptied for the
 * first, so neither passes by a race.
 */

import assert from "node:assert/strict";

import { BASE, chainKept, chainLanded, shownTodos, suite } from "./harness";

const app = await suite("offline");

const sleep = (ms: number): Promise<void> => new Promise((done) => setTimeout(done, ms));

/** How much of the chain this browser keeps, its log's entries: 0 for none, -1 for a database that will not answer. */
function heldObjects(): Promise<number> {
  return app.page.evaluate<number>(chainKept("life"));
}

/** Wait until this browser is holding the objects, and say HOW MANY — which is
 * why it is its own loop rather than a `waitFor`: the count is the assertion in
 * the scene below, not just the condition to stop on.
 *
 * (`waitFor` awaits a promise-shaped expression since the fix in `cdp.ts`. The
 * note that used to be here was the observation that it did not — worth
 * keeping the pointer: `Boolean(aPromise)` is `true` for every promise ever
 * made, and that made several waits elsewhere no waits at all.) */
async function untilHeld(): Promise<number> {
  for (let waited = 0; waited < 30000; waited += 200) {
    const held = await heldObjects();
    if (held > 0) return held;
    await sleep(200);
  }
  throw new Error("the objects were never cached");
}

// THE LIST ASKS THE BOX FOR ITS FOLD ONCE PER TAB, EVEN WHEN THE TAB'S OWN FOLD IS FRESH FIRST: only the box's fold
// carries what the box rendered and is kept as the dated copy an offline reload draws, and a Rust page mounts once its
// module loads, by when a warm tab's own fold is often fresh. The module is held until it is.
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
  // Emptied AFTER the caching has finished, under the running app: this scene is about the browser that has the fold
  // and NOT the objects, and the next one is about the browser that has both. The core in the worker lets the database
  // go when it is deleted, and whatever it writes after is a log that began again, which the next load reads as such.
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
  // The same todos. With no objects the tab holds no functions, so each todo
  // is its row's value, and a group of several is the core's conjunction of
  // those values, priced in its worker: never an average, and never left
  // without a number while the core answers.
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

  // The live dot says the same thing in the header, and says it as words for a
  // screen reader rather than only as a colour.
  await app.page.waitFor(`document.getElementById("live").getAttribute("aria-label") === "no live updates"`, {
    timeout: 20000,
  });
});

app.scene("holding the objects, the same reload folds them here instead", async () => {
  // The scene above left the network refused; this one has to fetch the chain
  // before it can hold it.
  await app.page.resumeRequests();
  await app.page.evaluate(
    `new Promise((resolve) => { const request = indexedDB.deleteDatabase("suzatary"); request.onsuccess = () => resolve(true); request.onerror = () => resolve(false); request.onblocked = () => resolve(false); })`,
  );
  // A real reload, not a navigate to the URL already showing: the page left by
  // the scene above has an EventSource that gave up, and the tip it never
  // heard is exactly what tells `keepObjects` which chain to hold.
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 15000 });
  await untilHeld();
  // Whole, read back through the core: heard from the box, with every object its landing names.
  await app.page.waitFor(chainLanded("life"), { timeout: 20000 });

  await app.page.failRequests(["/api/", "/events"]);
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 });
  await app.page.waitFor(`document.getElementById("stale").hidden === false`, { timeout: 20000 });

  // THE DIFFERENT SENTENCE. Not "stale since 11:32": these numbers are this moment's, computed here by the same core,
  // and what may have moved is elsewhere. It names no object (audit X1): the Chain page keeps those.
  // WAITED FOR, not asserted at once: the banner honestly says "stale" while the core is still being fetched and the
  // objects folded, and only then says this.
  await app.page.waitFor(`/^Offline:/.test(document.querySelector(".stalewhat").textContent)`, { timeout: 30000 });
  const said = await app.page.evaluate<string>(`document.querySelector(".stalewhat").textContent`);
  assert.match(said, /^Offline: what you see is what this device holds/, said);
  assert.equal(/[0-9a-f]{8}/.test(said), false, said);

  // And it is a LIST, not a sentence about one: the local fold produced the
  // same todos the server's did, with a number on the open ones.
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
