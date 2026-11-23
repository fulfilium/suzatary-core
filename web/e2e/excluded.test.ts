/**
 * A READING IS TOTAL OVER WHAT THIS DEVICE HOLDS (Prodrome SPEC law 44).
 *
 * The case that asked for it was the owner's phone: its inbox replica held a
 * card printed before `Remember` carried its `text`, and the conversations
 * page said "did not load" for the one print it could not parse. Here the
 * same print is put into the inbox this browser keeps, as the frames a pull
 * that brought it would have left, and the app is loaded again: the page
 * draws what it could read, and the shell's one line
 * says what was left out, opened to the name and why.
 */

import assert from "node:assert/strict";

import { WRITE_BASE, suite, untilReplica } from "./harness";

const app = await suite("excluded");

// A card as an older schema printed it, `Remember` with no `text`, in the inbox's own prodrome: a print of a
// prodrome the box does not name is dropped at the next pull, and is not this case.
const UNREAD =
  "Change(genesis='GENESIS', deps=(), event=Asked(proposal='p00000000000000000000000000000001', " +
  "at=datetime(2026, 9, 2, 12, 3, 26), actor='assistant', act=Remember(kind='person'), why='her signature says so', " +
  "sources=('mail:a1b2c3d4e5f6',), asked=None, serves=None, spec=None))";

/** The inbox's frames and snapshot this browser keeps, by their keys in the database's `replica` store. */
const INBOX = `IDBKeyRange.bound("inbox/", "inbox/\\uffff")`;

/**
 * The inbox this browser keeps, with a print of `template` in the store's own prodrome added as a pull that brought it
 * would add it: the core's frames of the box's answer holding it beside the rest, and of its landing, appended after the
 * last frame kept. The print's name is answered.
 */
function hold(template: string): Promise<string> {
  return app.page.evaluate<string>(`(async () => {
    const core = await import("/b/dev/prodrome.js");
    await core.default({ module_or_path: "/b/dev/prodrome_bg.wasm" });
    const held = await (await fetch("/api/stores/inbox/objects")).json();
    const genesis = held.objects.find((o) => o.literal.startsWith("Genesis(")).hash;
    const literal = ${JSON.stringify(template)}.replace("GENESIS", genesis);
    const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(literal));
    const hash = [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");
    const store = new core.Proposals();
    const piece = store.piece(JSON.stringify({ ...held, objects: [...held.objects, { hash, literal }] }), Date.now());
    const landing = store.land();
    const frames = [piece, landing.delta];
    landing.free();
    store.free();
    return new Promise((resolve, reject) => {
      const request = indexedDB.open("suzatary");
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const db = request.result;
        const tx = db.transaction("replica", "readwrite");
        const replica = tx.objectStore("replica");
        const keys = replica.getAllKeys(${INBOX});
        const snapshot = replica.get("inbox/snapshot");
        tx.oncomplete = () => { db.close(); resolve(hash); };
        tx.onerror = () => reject(tx.error);
        keys.onsuccess = () => snapshot.onsuccess = () => {
          const ats = keys.result.filter((k) => k.startsWith("inbox/log/")).map((k) => Number(k.slice("inbox/log/".length)));
          let at = Math.max(snapshot.result?.through ?? 0, ...ats);
          for (const frame of frames) replica.put(frame, "inbox/log/" + String((at += 1)).padStart(12, "0"));
        };
      };
    });
  })()`);
}

/** Until the inbox this browser keeps holds what a pull landed. */
function untilInbox(): Promise<void> {
  return app.page.waitFor(
    `new Promise((resolve) => {
      const request = indexedDB.open("suzatary");
      request.onerror = () => resolve(false);
      request.onsuccess = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains("replica")) { db.close(); resolve(false); return; }
        const query = db.transaction("replica", "readonly").objectStore("replica").getAllKeys(${INBOX});
        query.onerror = () => { db.close(); resolve(false); };
        query.onsuccess = () => { db.close(); resolve(query.result.length > 0); };
      };
    })`,
    { timeout: 15000 },
  );
}

/**
 * These copies spoiled, each a row of a store of the database by its key, from a document of the same origin that runs
 * no app, as a broken write or an older build would leave them: an earlier build's row of a chain as a row no build
 * reads; a snapshot of the stores of objects' form that does not read (`"unread"`); a snapshot of another form, as an
 * earlier build leaves it (`"earlier"`).
 */
async function spoil(copies: readonly (readonly [string, string, ("row" | "unread" | "earlier")])[]): Promise<void> {
  await app.page.navigate(`${WRITE_BASE}/.well-known/assetlinks.json`);
  await app.page.evaluate(`new Promise((resolve, reject) => {
    const request = indexedDB.open("suzatary");
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const db = request.result;
      const copies = ${JSON.stringify(copies)};
      const tx = db.transaction([...new Set(copies.map(([store]) => store))], "readwrite");
      const snapshot = (bytes) => ({ through: 1, bytes: new Uint8Array(bytes) });
      for (const [store, key, value] of copies) tx.objectStore(store).put(value === "unread" ? snapshot([2, 0, 0, 0, 255, 255, 255, 255]) : value === "earlier" ? snapshot([1, 2, 3, 4, 5]) : { objects: "not a list" }, key);
      tx.oncomplete = () => { db.close(); resolve(true); };
      tx.onerror = () => reject(tx.error);
    };
  })`);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
}

const HELD = `new Promise((resolve) => {
  const request = indexedDB.open("suzatary");
  request.onerror = () => resolve(null);
  request.onsuccess = () => {
    const query = request.result.transaction("replica", "readonly").objectStore("replica").getAllKeys(${INBOX});
    query.onsuccess = () => resolve({ kept: query.result, hidden: document.getElementById("excluded")?.hidden ?? null, line: document.getElementById("excluded")?.textContent ?? null });
  };
})`;

app.scene("a print this device cannot read is left out, the page draws, and one line says so", async () => {
  await app.open(WRITE_BASE);
  await app.page.evaluate(`location.hash = "#/secretary"`);
  await app.page.waitFor(`document.querySelector(".column.secretarystream:not(.pagewaiting) .streamrounds") !== null`, { timeout: 15000 });
  await untilInbox();
  assert.equal(await app.page.evaluate<boolean>(`document.getElementById("excluded").hidden`), true, "nothing is left out yet");

  // Written from a document of the same origin that runs no app, so no pull of the page before can write the row
  // back without the print.
  await app.page.navigate(`${WRITE_BASE}/.well-known/assetlinks.json`);
  const name = await hold(UNREAD);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
  await app.page.waitFor(`document.querySelector(".column.secretarystream:not(.pagewaiting) .streamrounds") !== null`, { timeout: 15000 });
  await app.page.waitFor(`document.getElementById("excluded").hidden === false`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector("#excluded summary").textContent`),
    "1 object on this device could not be read and was left out (0 resting on it).",
  );
  await app.page.click("#excluded summary");
  const opened = await app.page.evaluate<string>(`document.querySelector("#excluded li").textContent`);
  assert.ok(opened.startsWith(name), `the line opens to the name, got ${opened}`);
  assert.match(opened, /failed to parse: Asked\.act: Remember\(\.\.\.\) is missing text/);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".secretarystream .problem").length`),
    0,
    "the page drew what it could read",
  );
}, HELD);

const row = (copy: string): string => `#excluded li[data-row="store/chain/${copy}"]`;

/** Whether the device still keeps an earlier build's row of `chain`. */
const carryKept = (chain: string): string => `new Promise((resolve) => {
  const request = indexedDB.open("suzatary");
  request.onsuccess = () => {
    const query = request.result.transaction("chain", "readonly").objectStore("chain").get("carry:${chain}");
    query.onsuccess = () => { request.result.close(); resolve(query.result !== undefined); };
    query.onerror = () => resolve(true);
  };
  request.onerror = () => resolve(true);
})`;

app.scene("a chain's copy an earlier build kept that does not read is named, with the one reset that writes over it", async () => {
  await app.open(WRITE_BASE);
  await spoil([["chain", "carry:life", "row"]]);
  await app.page.navigate(`${WRITE_BASE}/#/`);
  await app.page.waitFor(`document.querySelector('${row("life")}') !== null`, { timeout: 15000 });
  assert.match(await app.page.evaluate<string>(`document.querySelector("#excluded summary").textContent`), /needs you/i);
  await app.page.click("#excluded summary");
  const said = await app.page.evaluate<string>(`document.querySelector('${row("life")}').textContent`);
  assert.match(said, /does not read/, "it says why");
  assert.match(said, /writes over that copy/, "and that the reset writes over it");
  await app.page.click(`${row("life")} button`);
  await app.page.waitFor(`document.querySelector('${row("life")}') === null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<boolean>(carryKept("life")), false, "the reset wrote over it");
  await untilReplica(app.page, "life");
});

/**
 * A reading that moves what is left out draws the line again, and a reader who opened it finds it open: the reset of
 * one chain's row moves it, and the other chain's reset is still there to tap. A part of a log that does not read in
 * this build's form is set aside, never deleted, and named with the reset that removes it; an earlier build's copy of a
 * store the box alone writes goes without a word, and the store is taken again from the box.
 */
app.scene("the line stays open while a reset moves it; a log that does not read is set aside and named, an earlier build's inbox goes unsaid", async () => {
  await app.open(WRITE_BASE);
  await untilInbox();
  await untilReplica(app.page, "life");
  await spoil([
    ["chain", "carry:life", "row"],
    ["chain", "carry:suzatary", "row"],
    ["replica", "chain/life/snapshot", "unread"],
    ["replica", "inbox/snapshot", "earlier"],
  ]);
  await app.page.navigate(`${WRITE_BASE}/#/`);
  await app.page.waitFor(`document.querySelector('${row("life")}') !== null && document.querySelector('${row("suzatary")}') !== null`, { timeout: 15000 });
  await app.page.waitFor(`[...document.querySelectorAll('${row("life")}')].some((li) => /kept aside/.test(li.textContent))`, { timeout: 15000 });
  await untilInbox();
  await app.page.click("#excluded summary");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll('#excluded li[data-row="store/inbox"]').length`), 0, "the inbox's earlier copy is named nowhere");
  await app.page.click(`${row("life")} button`);
  await app.page.waitFor(`document.querySelector('${row("life")}') === null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector("#excluded details").open`), true, "drawn again, the line is as the reader left it");
  await app.page.click(`${row("suzatary")} button`);
  await app.page.waitFor(`document.querySelector('${row("suzatary")}') === null`, { timeout: 15000 });
  await untilReplica(app.page, "life");
  await untilInbox();
});
