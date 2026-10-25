/**
 * A READING IS TOTAL OVER WHAT THIS DEVICE HOLDS (Prodrome SPEC law 44).
 *
 * The case that asked for it was the owner's phone: its inbox replica held a
 * card printed before `Remember` carried its `text`, and the conversations
 * page said "did not load" for the one print it could not parse. Here the
 * same print is put into the inbox row this browser keeps, and the app is
 * loaded again: the page draws what it could read, and the shell's one line
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

/** The inbox row this browser keeps (its replica and its keys), with `template` added in the row's own prodrome; the name is answered. */
function hold(template: string): Promise<string> {
  return app.page.evaluate<string>(`
    new Promise((resolve, reject) => {
      const request = indexedDB.open("suzatary");
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const db = request.result;
        const rows = db.transaction("chain", "readwrite").objectStore("chain");
        const read = rows.get("objects:store/inbox");
        read.onsuccess = () => {
          const row = read.result;
          const genesis = row.replica.objects.find((o) => o.literal.startsWith("Genesis(")).hash;
          const literal = ${JSON.stringify(template)}.replace("GENESIS", genesis);
          crypto.subtle.digest("SHA-256", new TextEncoder().encode(literal)).then((bytes) => {
            const hash = [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");
            row.replica.objects = [...row.replica.objects, { hash, literal }];
            const again = db.transaction("chain", "readwrite").objectStore("chain").put(row, "objects:store/inbox");
            again.onsuccess = () => { db.close(); resolve(hash); };
            again.onerror = () => { db.close(); reject(again.error); };
          });
        };
      };
    })`);
}

/** Until the inbox row this browser keeps holds the store's genesis. */
function untilInbox(): Promise<void> {
  return app.page.waitFor(
    `new Promise((resolve) => {
      const request = indexedDB.open("suzatary");
      request.onerror = () => resolve(false);
      request.onsuccess = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains("chain")) { db.close(); resolve(false); return; }
        const query = db.transaction("chain", "readonly").objectStore("chain").get("objects:store/inbox");
        query.onerror = () => { db.close(); resolve(false); };
        query.onsuccess = () => { const row = query.result; db.close(); resolve(row !== undefined && row.replica.objects.some((o) => o.literal.startsWith("Genesis("))); };
      };
    })`,
    { timeout: 15000 },
  );
}

/**
 * These copies spoiled, from a document of the same origin that runs no app, as a broken write or an older build would
 * leave them; then the secretary's page, which reads them.
 */
async function spoil(copies: readonly string[]): Promise<void> {
  await app.page.navigate(`${WRITE_BASE}/.well-known/assetlinks.json`);
  await app.page.evaluate(`new Promise((resolve, reject) => {
    const request = indexedDB.open("suzatary");
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const db = request.result;
      const rows = db.transaction("chain", "readwrite").objectStore("chain");
      for (const copy of ${JSON.stringify(copies)}) rows.put({ objects: "not a list" }, "objects:" + copy);
      rows.transaction.oncomplete = () => { db.close(); resolve(true); };
      rows.transaction.onerror = () => reject(rows.transaction.error);
    };
  })`);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
}

const HELD = `new Promise((resolve) => {
  const request = indexedDB.open("suzatary");
  request.onerror = () => resolve(null);
  request.onsuccess = () => {
    const query = request.result.transaction("chain", "readonly").objectStore("chain").get("objects:store/inbox");
    query.onsuccess = () => resolve({ held: (query.result?.replica?.objects ?? []).map((o) => o.literal.slice(0, 60)), hidden: document.getElementById("excluded")?.hidden ?? null, line: document.getElementById("excluded")?.textContent ?? null });
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
  await spoil(["life"]);
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
 * one copy moves it, and the other copy's reset is still there to tap.
 */
app.scene("a store's copy that does not read is named too, and the line stays open while a reset moves it", async () => {
  await app.open(WRITE_BASE);
  await spoil(["life", "store/inbox"]);
  const row = (copy: string): string => `#excluded li[data-row="${copy}"]`;
  await app.page.waitFor(`document.querySelector('${row("life")}') !== null && document.querySelector('${row("store/inbox")}') !== null`, { timeout: 15000 });
  await app.page.click("#excluded summary");
  assert.match(await app.page.evaluate<string>(`document.querySelector('${row("store/inbox")}').textContent`), /does not read/);
  await app.page.click(`${row("life")} button`);
  await app.page.waitFor(`document.querySelector('${row("life")}') === null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector("#excluded details").open`), true, "drawn again, the line is as the reader left it");
  await app.page.click(`${row("store/inbox")} button`);
  await app.page.waitFor(`document.querySelector('${row("store/inbox")}') === null`, { timeout: 15000 });
  await untilInbox();
});
