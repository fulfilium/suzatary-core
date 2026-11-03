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

import { WRITE_BASE, suite } from "./harness";

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
 * no app, as a broken write or an older build would leave them: a chain's row as a row no build reads, a store's
 * snapshot (`"bytes"`) as bytes of no form the core reads. Then the secretary's page, which reads them.
 */
async function spoil(copies: readonly (readonly [string, string, ("row" | "bytes")?])[]): Promise<void> {
  await app.page.navigate(`${WRITE_BASE}/.well-known/assetlinks.json`);
  await app.page.evaluate(`new Promise((resolve, reject) => {
    const request = indexedDB.open("suzatary");
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const db = request.result;
      const copies = ${JSON.stringify(copies)};
      const tx = db.transaction([...new Set(copies.map(([store]) => store))], "readwrite");
      for (const [store, key, value] of copies) tx.objectStore(store).put(value === "bytes" ? { through: 1, bytes: new Uint8Array([1, 2, 3, 4, 5]) } : { objects: "not a list" }, key);
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
  assert.match(opened, /failed to parse: Asked\.act: Remember\(\.\.\.\) has no field text/);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".secretarystream .problem").length`),
    0,
    "the page drew what it could read",
  );
}, HELD);

app.scene("a chain's copy that does not read is named, with the one reset that writes over it", async () => {
  await app.open(WRITE_BASE);
  await spoil([["chain", "objects:life"]]);
  await app.page.waitFor(`document.querySelector('#excluded li[data-row="life"]') !== null`, { timeout: 15000 });
  assert.match(await app.page.evaluate<string>(`document.querySelector("#excluded summary").textContent`), /this device's copy of life needs you/i);
  await app.page.click("#excluded summary");
  const said = await app.page.evaluate<string>(`document.querySelector('#excluded li[data-row="life"]').textContent`);
  assert.match(said, /does not read/, "it says why");
  assert.match(said, /writes over that copy/, "and that the reset writes over it");
  await app.page.click(`#excluded li[data-row="life"] button`);
  // The row reads again, as the reset's generation, and fills from the box; its entry goes once it does (the line
  // stays for the print the scene above left in the inbox's row).
  await app.page.waitFor(`document.querySelector('#excluded li[data-row="life"]') === null`, { timeout: 15000 });
  await app.page.waitFor(
    `new Promise((resolve) => {
      const request = indexedDB.open("suzatary");
      request.onsuccess = () => {
        const query = request.result.transaction("chain", "readonly").objectStore("chain").get("objects:life");
        query.onsuccess = () => { const row = query.result; request.result.close(); resolve(Array.isArray(row?.objects) && row.objects.length > 0 && row.generation >= 1); };
        query.onerror = () => resolve(false);
      };
      request.onerror = () => resolve(false);
    })`,
    { timeout: 15000 },
  );
});

/**
 * A reading that moves what is left out draws the line again, and a reader who opened it finds it open: the reset of
 * one chain's copy moves it, and the other chain's reset is still there to tap. A store's copy of no form the core reads
 * is no row to reset: it is dropped and the store is taken again from the box, so the line never names it.
 */
app.scene("the line stays open while a reset moves it, and a store's copy that does not read is taken again unsaid", async () => {
  await app.open(WRITE_BASE);
  await untilInbox();
  await spoil([["chain", "objects:life"], ["chain", "objects:suzatary"], ["replica", "inbox/snapshot", "bytes"]]);
  const row = (copy: string): string => `#excluded li[data-row="${copy}"]`;
  await app.page.waitFor(`document.querySelector('${row("life")}') !== null && document.querySelector('${row("suzatary")}') !== null`, { timeout: 15000 });
  await app.page.click("#excluded summary");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll('#excluded li[data-row^="store/"]').length`), 0, "no store's copy is named");
  await app.page.click(`${row("life")} button`);
  await app.page.waitFor(`document.querySelector('${row("life")}') === null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector("#excluded details").open`), true, "drawn again, the line is as the reader left it");
  await app.page.click(`${row("suzatary")} button`);
  await app.page.waitFor(`document.querySelector('${row("suzatary")}') === null`, { timeout: 15000 });
  await untilInbox();
});
