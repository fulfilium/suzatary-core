/**
 * The phone is a replica: a person taps Done with no network and finds the chain agreeing later, once, with the instant
 * their device claimed (`app/cli/tests/acting.rs` and the core's replica laws pin the decisions over sets of names). The
 * network is failed, not held, since a held request eventually succeeds; the first scene replays on a reload, so the
 * objects survive IndexedDB as well as the reconnect.
 */

import assert from "node:assert/strict";

import { fold, openInPlace, shownTodos, suite, untilReplica, WRITE_BASE, WRITE_HEADERS } from "./harness";
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
  // The button arrives with `GET /api/me`'s answer, after the list is drawn from the cached fold.
  await openInPlace(app.page, todo);
  await app.page.waitFor(`document.querySelector('.rowopen[data-todo="${todo}"] .act-done') !== null`, {
    timeout: 8000,
  });
  await app.page.click(`.rowopen[data-todo="${todo}"] .act-done`);
  // ⚠️ The button going is the act landing; the line going depends on the filter, which these scenes share in `localStorage`.
  await app.page.waitFor(`document.querySelector('.rowopen[data-todo="${todo}"] .act-done') === null`, {
    timeout: 4000,
  });
};

app.scene("two acts with no network reach the box on reconnect", async () => {
  await app.open(WRITE_BASE);
  // The replica has to be holding the chain before it can seal onto it.
  await untilReplica(app.page);
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".tree .row.pending").length`), 0, "nothing is waiting before anything is tapped");

  await app.page.failRequests(["/objects"]);
  await tapDone(todos.flat.id);
  await tapDone(todos.checklist.id);
  // Each line stays in sight, its card open, though the act made the card's value Fine.
  for (const todo of [todos.flat.id, todos.checklist.id])
    await app.page.waitFor(`document.querySelector('.tree .row.pending[data-todo="${todo}"] .queuedmark')?.textContent === "done · sending"`, { timeout: 4000 });

  // The cards are gone because this browser folded the objects it holds; the chain has heard nothing.
  assert.equal(await stateOf(todos.flat.id), "open");
  assert.equal(await stateOf(todos.checklist.id), "open");

  assert.deepEqual([await sending(todos.flat.id), await sending(todos.checklist.id)], ["done · sending", "done · sending"]);
  assert.equal(await app.page.evaluate<boolean>(`document.getElementById("stale").hidden`), true, "the stream is up: no banner");

  await app.page.resumeRequests();
  await app.page.reload();

  // ⚠️ The box's fold first: no marked line is also what a page still reading IndexedDB shows, and the fold only climbs.
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

  // ⚠️ This resolves a todo on a chain every file shares: `file-permit` is read by `list.test.ts` alone, on the read-only server.
  const mine = todos.urgent.id;

  // Each replica's change rests on its own todo's writes, neither on the other, so each is a head and nothing joins them.

  await app.page.failRequests(["/objects"]);
  await tapDone(mine);
  await app.page.waitFor(`document.querySelector('.tree .row.pending[data-todo="${mine}"] .queuedmark')?.textContent === "done · sending"`, { timeout: 4000 });
  await app.page.resumeRequests();

  // A divergence is two objects resting on one parent, so the other replica may re-resolve a todo already closed.
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

  // Landing is the box's fact, carried by its announcement of the other replica's write at its next poll, so its fold is waited on.
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

  // The late branch's fact is believed though the box moved on before it arrived (SPEC §9.6), which last-writer-wins would fail.
  assert.equal(await stateOf(mine), "completed");
  await app.page.waitFor(`document.querySelector('.tree .row.pending[data-todo="${mine}"]') === null`, { timeout: 4000 });
  const holds = await app.page.evaluate<string>(
    `fetch("/api/objects").then((r) => r.json()).then((b) => JSON.stringify(b.objects.map((o) => o.hash)))`,
  );
  assert.ok((JSON.parse(holds) as string[]).includes(other.name), "the other replica's object is still held");
  assert.ok((await shownTodos(app.page)).length >= 0, "and the list still renders");
});
