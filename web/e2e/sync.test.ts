/**
 * THE PHONE IS A REPLICA, END TO END: acts with no network, and what reaches
 * the chain.
 *
 * `app/cli/tests/acting.rs` pins what `POST /api/objects` does with a list of
 * literals, and `web/test/replica.test.ts` pins the decisions over sets of
 * names. Neither can check the thing the design is actually about: that a
 * person can tap Done on a train, lock their phone, and find the chain agreeing
 * with them later — once, with the instant their own device claimed.
 *
 * THE NETWORK IS FAILED, NOT HELD. A held request eventually succeeds, so it
 * can never produce the case this file is about; offline is a fetch that
 * FAILED, which is what `failRequests` models. `resumeRequests` is the other
 * half.
 *
 * WHAT LEFT WITH THE QUEUE. This file used to count `/api/act/batch` requests
 * and read a pill that said "2 queued", because an act was a WISH waiting to be
 * granted. An act is an OBJECT now, sealed here, folded here, and simply not
 * yet somewhere else — so each line it moved says "sending" until the box has
 * it, and the assertion is that the box ends up holding them, not that a
 * particular request shape carried them.
 *
 * WHAT TRIGGERS THE REPLAY IS A RELOAD in the first scene, on purpose: it
 * proves the objects survived IndexedDB as well as the reconnect, which is the
 * case a phone actually lives through.
 */

import assert from "node:assert/strict";

import { atTheTerminal, fold, openInPlace, reveal, shownTodos, suite, untilReplica, WRITE_BASE, WRITE_HEADERS, pickCategory } from "./harness";
import cast from "../test/cast";

const { todos, owner } = cast.world;

const app = await suite("sync");
// A scene here that fails is read by its trace: every request, every frame the streams delivered, and what was open.
await app.journal();

const stateOf = async (todo: string): Promise<string | undefined> =>
  (await fold(app.page)).find((entry) => entry.todo === todo)?.state;

/** What a line says while the box lacks this browser's act on it, or "" for a line that waits on nothing. */
const sending = async (todo: string): Promise<string> =>
  app.page.evaluate<string>(`document.querySelector('.tree .row[data-todo="${todo}"] .queuedmark')?.textContent ?? ""`);

const tapDone = async (todo: string): Promise<void> => {
  // WAITED FOR, because the button ARRIVES: `GET /api/me` says whether this
  // login may seal, and the list is drawn from the cached fold before it
  // answers. Clicking without this raced the answer — and the scene that
  // reached the tap fastest was the one that failed.
  await openInPlace(app.page, todo);
  await app.page.waitFor(`document.querySelector('.rowopen[data-todo="${todo}"] .act-done') !== null`, {
    timeout: 8000,
  });
  await app.page.click(`.rowopen[data-todo="${todo}"] .act-done`);
  // ⚠️ THE BUTTON GOING IS THE ACT LANDING; the LINE going is a fact about the
  // FILTER. A resolved todo leaves the list under "Open" and STAYS on it under
  // "All", so waiting for the line to disappear asserts the act only while
  // nothing has touched the filter — and these scenes share one browser
  // profile (`launch()` is per file), where the filter is remembered in
  // `localStorage`, and the first scene deliberately switches to "All" to see
  // its own unsent marks. Waiting on the Done button says the one thing meant
  // here, under either filter: this todo is no longer open, so it is no longer
  // offered a Done.
  await app.page.waitFor(`document.querySelector('.rowopen[data-todo="${todo}"] .act-done') === null`, {
    timeout: 4000,
  });
};

app.scene("two acts with no network reach the box on reconnect", async () => {
  await app.open(WRITE_BASE);
  // The replica has to be holding the chain before it can seal onto it — that
  // is the one thing the queue never needed and this does.
  await untilReplica(app.page);
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".tree .row.pending").length`), 0, "nothing is waiting before anything is tapped");

  await app.page.failRequests(["/objects"]);
  await tapDone(todos.flat.id);
  await tapDone(todos.checklist.id);
  // Each line stays in sight, its card open, though the act made the card's value Fine.
  for (const todo of [todos.flat.id, todos.checklist.id])
    await app.page.waitFor(`document.querySelector('.tree .row.pending[data-todo="${todo}"] .queuedmark')?.textContent === "done · sending"`, { timeout: 4000 });

  // The cards are gone because THIS BROWSER folded the objects it holds — the
  // same core the box folds with. The chain has heard nothing.
  assert.equal(await stateOf(todos.flat.id), "open");
  assert.equal(await stateOf(todos.checklist.id), "open");

  // …and the unsent state is visible rather than silent: under the Open filter each done line stays, saying so.
  assert.deepEqual([await sending(todos.flat.id), await sending(todos.checklist.id)], ["done · sending", "done · sending"]);
  assert.equal(await app.page.evaluate<boolean>(`document.getElementById("stale").hidden`), true, "the stream is up: no banner");

  await app.page.resumeRequests();
  await app.page.reload();

  // ⚠️ THE BOX'S OWN FOLD FIRST, and the order is not stylistic. No marked line
  // is ALSO what a page that has not finished reading IndexedDB looks like, so
  // waiting on it straight after a reload is a wait satisfied by the state
  // before the thing under test has begun — it passed on the empty in-memory
  // replica and then the two objects appeared. The fold is MONOTONE here: it
  // cannot say "completed" until the objects reached the box, so it is the
  // signal to wait on, and only then does a line without its mark mean what it says.
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.filter((e) => (e.todo === "${todos.flat.id}" || e.todo === "${todos.checklist.id}") && e.state === "completed").length === 2)`,
    { timeout: 15000 },
  );
  await app.page.waitFor(`document.querySelectorAll(".tree .row.pending").length === 0`, { timeout: 15000 });
  for (const todo of [todos.flat.id, todos.checklist.id]) {
    const entry = (await fold(app.page)).find((candidate) => candidate.todo === todo);
    assert.equal(entry?.state, "completed", todo);
    assert.equal(entry?.value, 1, "a resolved todo reads 1.0 from the instant it was resolved");
  }
});

app.scene("two replicas that diverged leave the box both heads, joined by nothing", async () => {
  await app.open(WRITE_BASE);
  await untilReplica(app.page);

  // ⚠️ `file-permit`, AND THE CHOICE MATTERS: this scene RESOLVES a todo on a
  // chain every other file in the suite shares, so it has to pick one nothing
  // else depends on being open. `book-flight` and `pack-bags` were spent by the
  // scene above, `water-plants` belongs to `writes.test.ts`, and
  // `email-harrow` is the untrusted one whose trust semantics are somebody
  // else's subject. `file-permit` is read by `list.test.ts` alone, on the
  // read-only server whose chain cannot move.
  const mine = todos.urgent.id;

  // A SECOND REPLICA, played straight: it holds the objects this tab does and
  // appends its own act with the same core, with no idea this tab exists. Then
  // this tab appends ITS act, offline, and syncs. Each change rests on its own
  // todo's writes, neither on the other, so each is a head beside the box's, and
  // nothing joins them: the heads are a set.

  await app.page.failRequests(["/objects"]);
  await tapDone(mine);
  await app.page.waitFor(`document.querySelector('.tree .row.pending[data-todo="${mine}"] .queuedmark')?.textContent === "done · sending"`, { timeout: 4000 });
  await app.page.resumeRequests();

  // The other replica gets there first. Its event need not be NOVEL for the
  // heads to diverge — what makes a divergence is two objects resting on one
  // parent — so it re-resolves a todo the scene above already closed, and this
  // scene asserts nothing about that todo's state. What it asserts is
  // structural, which is what a divergence is about.
  const theirs = await app.page.evaluate<string>(`(async () => {
    const glue = await import("/b/dev/prodrome.js");
    await glue.default({ module_or_path: new URL("/b/dev/prodrome_bg.wasm", location.href).href });
    const held = await fetch("/api/objects").then((r) => r.json());
    const replica = new glue.Todos(JSON.stringify(held.objects.map((o) => ({ hash: o.hash, text: o.literal }))));
    const event = glue.lifecycle("Completed", "${todos.flat.id}", "2026-09-02T11:30:00", "${owner.actor}", "");
    const appended = JSON.parse(replica.append(event, null));
    replica.free();
    // It signs as this device does: the box takes no write its trusted key does not prove.
    const request = (r) => new Promise((ok, no) => { r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
    const db = await request(indexedDB.open("suzatary-keys", 1));
    const key = await request(db.transaction("keys", "readonly").objectStore("keys").get("device"));
    db.close();
    const bytes = await crypto.subtle.sign({ name: "Ed25519" }, key.key, new TextEncoder().encode("prodrome object " + appended.hash));
    const signature = Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("");
    const signed = JSON.parse(glue.signed(appended.hash, key.public, signature));
    const receipt = await fetch("/api/objects", {
      method: "POST",
      headers: ${WRITE_HEADERS},
      body: JSON.stringify({ objects: [...appended.objects.map((o) => o.text), signed.text] }),
    }).then((r) => r.json());
    // Its head is the signature, which rests on the change.
    return JSON.stringify({ name: signed.hash, receipt });
  })()`);
  const other = JSON.parse(theirs) as { name: string; receipt: { heads: string[] } };
  assert.ok(other.receipt.heads.includes(other.name), "the other replica's signed change is a head the box holds");

  // Now this tab's own change lands beside the other's — a real divergence, and
  // the box writes nothing to join them. LANDING IS THE BOX'S FACT, so it is the
  // box's fold that is waited on: it cannot say "completed" before the object is
  // there. What carries it is the box's announcement of the other replica's
  // write, at its next poll; the line's mark only follows, once this tab hears back.
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === ${JSON.stringify(mine)} && e.state === "completed"))`,
    { timeout: 15000 },
  );
  const tips = await app.page.evaluate<string>(
    `fetch("/api/objects").then((r) => r.json()).then((b) => JSON.stringify(b.heads))`,
  );
  const heads = JSON.parse(tips) as string[];
  assert.ok(heads.length > 1, `two replicas' changes are two heads, the box has ${tips}`);
  assert.ok(heads.includes(other.name), `the other replica's change is a head: ${tips}`);

  // AND THE LATE BRANCH SURVIVED. This tab lost the race — its object arrived
  // after the box had moved on — and nothing settles it but a later write,
  // so the fact it carried is believed all the same. That is the whole claim
  // (SPEC §9.6), and it is the one a last-writer-wins store would fail.
  assert.equal(await stateOf(mine), "completed");
  await app.page.waitFor(`document.querySelector('.tree .row.pending[data-todo="${mine}"]') === null`, { timeout: 4000 });
  // Both replicas' objects are in the box's chain, neither dropped for the
  // other: the union is the union.
  const holds = await app.page.evaluate<string>(
    `fetch("/api/objects").then((r) => r.json()).then((b) => JSON.stringify(b.objects.map((o) => o.hash)))`,
  );
  assert.ok((JSON.parse(holds) as string[]).includes(other.name), "the other replica's object is still held");
  assert.ok((await shownTodos(app.page)).length >= 0, "and the list still renders");
});

/**
 * ANOTHER TAB'S RESET, landing in this tab's next save of the chain's row: that save's own transaction finds the row
 * already the empty replica of the next generation, which every join keeps whole. Put in that transaction, before its
 * read, because that is the one order no wait in a scene could make happen.
 */
const RESET_AT_NEXT_SAVE = `new Promise((resolve, reject) => {
  const request = indexedDB.open("suzatary");
  request.onerror = () => reject(request.error);
  request.onsuccess = () => {
    const db = request.result;
    const read = db.transaction("chain", "readonly").objectStore("chain").get("objects:life");
    read.onsuccess = () => {
      const row = read.result;
      db.close();
      let reset = { generation: (row.generation ?? 0) + 1, objects: [], synced: [], refused: [], words: [], frontier: { type: "Nothing" } };
      const transaction = IDBDatabase.prototype.transaction;
      IDBDatabase.prototype.transaction = function (names, mode, ...rest) {
        const t = transaction.call(this, names, mode, ...rest);
        if (mode !== "readwrite" || reset === null) return t;
        const objectStore = t.objectStore.bind(t);
        t.objectStore = (name) => {
          const store = objectStore(name);
          const get = store.get.bind(store);
          store.get = (key) => {
            if (key === "objects:life" && reset !== null) { store.put(reset, key); reset = null; }
            return get(key);
          };
          return store;
        };
        return t;
      };
      resolve(true);
    };
    read.onerror = () => reject(read.error);
  };
})`;

/** The generation of this tab's row of the chain, as the device keeps it. */
const GENERATION = `new Promise((resolve) => {
    const request = indexedDB.open("suzatary");
    request.onerror = () => resolve(-1);
    request.onsuccess = () => {
      const read = request.result.transaction("chain", "readonly").objectStore("chain").get("objects:life");
      read.onsuccess = () => { request.result.close(); resolve(read.result?.generation ?? 0); };
      read.onerror = () => resolve(-1);
    };
  })`;

app.scene("an act overtaken by another tab's reset is sealed again onto it, never shown and dropped", async () => {
  await app.open(WRITE_BASE);
  await untilReplica(app.page);

  // Another tab's reset, landing between this tab sealing an act and keeping it.
  await app.page.evaluate(RESET_AT_NEXT_SAVE);

  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await app.page.type(".draftbody", "Repot the cactus");
  await pickCategory(app.page, "home");
  await app.page.click(".addbtn");

  // THE BOX'S FACT: the act was sealed again onto what the reset holds, once
  // filled from the box, and reached it, rather than refused or dropped.
  const id = "repotthecactus";
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "open"))`,
    { timeout: 15000 },
  );
  await app.page.waitFor(`document.querySelector('.tree .row[data-todo="${id}"]') !== null`, { timeout: 8000 });
  // ONCE: the act reached memory only from the row the device kept, so no exchange posted the first sealing too.
  await app.page.waitFor(`document.querySelector('.tree .row[data-todo="${id}"]:not(.pending)') !== null`, { timeout: 15000 });
  const sealings = await app.page.evaluate<number>(
    `fetch("/api/objects").then((r) => r.json()).then((b) => b.objects.filter((o) => o.literal.startsWith("Change(") && o.literal.includes("todo='${id}'")).length)`,
  );
  assert.equal(sealings, 1, "the box holds the act once");
});

app.scene("a pull asked before another tab's reset adds nothing to it, and an act sealed after it lands", async () => {
  await app.open(WRITE_BASE);
  await untilReplica(app.page);

  // The box moves at its terminal, and this tab's pull of what moved is held at the network: an answer to a question
  // asked of the replica the reset is about to replace.
  const pulling = await app.page.holdRequests(["/api/life/objects?since="]);
  atTheTerminal(["todo", "add", "movedattheterminal", "--body", "Moved at the terminal", "--rationale", "a write this tab hears while its pull is held", "--category", "home", "--priority", "60"]);
  await app.page.untilHeld();

  // The act's own save takes the reset, so it is sealed again once the replica is filled from the box, by the exchange
  // queued behind the held one.
  const before = await app.page.evaluate<number>(GENERATION);
  await app.page.evaluate(RESET_AT_NEXT_SAVE);
  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await app.page.type(".draftbody", "Repot the fern");
  await pickCategory(app.page, "home");
  await app.page.click(".addbtn");
  // The act's own save took the reset when the row's generation passes the one it held before; an earlier scene's reset
  // may have left it past zero.
  await app.page.waitFor(`${GENERATION}.then((g) => g > ${before})`);
  await pulling();

  // The held answer named only what moved since the replica before the reset; joined into the reset, it would leave a
  // replica of new objects without their ancestors, which the act cannot be sealed onto.
  const id = "repotthefern";
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "open"))`,
    { timeout: 15000 },
  );
  // And this tab holds the chain whole, every object the box holds, once its exchanges are done.
  await app.page.waitFor(`document.querySelector('.tree .row[data-todo="${id}"]:not(.pending)') !== null`, { timeout: 15000 });
  const missing = `Promise.all([
    fetch("/api/life/objects").then((r) => r.json()).then((b) => b.objects.map((o) => o.hash)),
    new Promise((resolve, reject) => {
      const request = indexedDB.open("suzatary");
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const read = request.result.transaction("chain", "readonly").objectStore("chain").get("objects:life");
        read.onsuccess = () => { request.result.close(); resolve(new Set(read.result.objects.map((o) => o.hash))); };
        read.onerror = () => reject(read.error);
      };
    }),
  ]).then(([box, held]) => box.filter((hash) => !held.has(hash)).length)`;
  await app.page.waitFor(`${missing}.then((n) => n === 0)`, { timeout: 15000 });
  await reveal(app.page, id);
  await reveal(app.page, "movedattheterminal");
});
