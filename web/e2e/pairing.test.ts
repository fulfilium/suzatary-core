/**
 * THE FIRST DEVICE, TRUSTED WITH THE BOX'S CODE: a browser whose key nobody trusted sees This device first in Settings,
 * is sent there from its own card, and types the code `suzatary device pair` printed. A wrong code is refused with the
 * tries left; the right one trusts this device, its card leaves the Inbox, and the code then trusts nothing else. A key
 * trusted at the terminal instead leaves no card behind either.
 */

import assert from "node:assert/strict";

import { atTheTerminal, pairAtTheBox, PHONE, SIGNED_POST, suite, WRITE_BASE } from "./harness";

const app = await suite("pairing", PHONE);

const toast = `document.querySelector("#toast")?.textContent ?? ""`;
const keyLine = `document.querySelector(".settingskey")?.textContent ?? ""`;

// A key nobody trusted in this browser's place, as "Use a new key on this device" makes one: its public half.
const untrustedKey = async (): Promise<string> => {
  await app.page.navigate(`${WRITE_BASE}/manifest.webmanifest`);
  return app.page.evaluate<string>(`(async () => {
    const request = (r) => new Promise((ok, no) => { r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
    const opening = indexedDB.open("suzatary-keys", 1);
    opening.onupgradeneeded = () => opening.result.createObjectStore("keys");
    const db = await request(opening);
    const pair = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
    const raw = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
    const key = { public: Array.from(raw, (b) => b.toString(16).padStart(2, "0")).join(""), key: pair.privateKey };
    await request(db.transaction("keys", "readwrite").objectStore("keys").put(key, "device"));
    db.close();
    return key.public;
  })()`);
};

app.scene("an untrusted device is trusted with the code from the box, its card leaves, and the code works once", async () => {
  const key = await untrustedKey();

  await app.page.navigate(`${WRITE_BASE}/#/settings`);
  await app.page.waitFor(`(${keyLine}).includes("is not trusted yet")`, { timeout: 20000 });
  assert.equal(
    await app.page.evaluate<boolean>(`(() => { const here = document.querySelector(".settingshere"), ask = document.querySelector(".settingsask"); return here !== null && ask !== null && (here.compareDocumentPosition(ask) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0 })()`),
    true,
    "This device stands before everything else while it is untrusted",
  );

  // Its own card in the Inbox sends it to Settings, where the code is typed.
  await app.page.click(".settingshere .act-asktrust");
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  const mine = `[...document.querySelectorAll(".proposal-act-device.proposal-standing")].find((c) => c.textContent.includes(${JSON.stringify(key)}))`;
  await app.page.waitFor(`(${mine}) !== undefined`, { timeout: 20000 });
  assert.match(await app.page.evaluate<string>(`${mine}.querySelector(".readonly").textContent`), /^This device is not trusted yet\. Trust it →$/);
  assert.equal(await app.page.evaluate<string>(`${mine}.querySelector(".readonly a").getAttribute("href")`), "#/settings");

  const code = pairAtTheBox();
  await app.page.navigate(`${WRITE_BASE}/#/settings`);
  await app.page.waitFor(`document.querySelector(".settingshere .act-pair") !== null`, { timeout: 20000 });
  await app.page.click(".settingshere .act-pair");
  await app.page.waitFor(`document.querySelector(".settingshere .paircode") !== null`);
  await app.page.type(".settingshere .paircode", code === "ZZZZ-ZZZZ" ? "XXXX-XXXX" : "ZZZZ-ZZZZ");
  await app.page.click(".settingshere .act-paircode");
  await app.page.waitFor(`(${toast}).includes("4 tries left")`, { timeout: 20000 });

  await app.page.evaluate(`(() => { const f = document.querySelector(".settingshere .paircode"); f.value = ""; f.dispatchEvent(new Event("input")); return true })()`);
  await app.page.type(".settingshere .paircode", code.toLowerCase());
  await app.page.click(".settingshere .act-paircode");
  await app.page.waitFor(`(${toast}).includes("This device is trusted.")`, { timeout: 20000 });
  await app.page.waitFor(`(${keyLine}).includes("which the chain trusts")`, { timeout: 30000 });

  // The card it asked for is no longer waiting.
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  await app.page.waitFor(
    `[...document.querySelectorAll(".proposal-act-device.proposal-standing")].every((c) => !c.textContent.includes(${JSON.stringify(key)}))`,
    { timeout: 20000 },
  );

  // The code is spent: typed again, it is refused in plain words.
  const again = await app.page.evaluate<{ status: number; error: string }>(
    `${SIGNED_POST}("/api/act/pair", JSON.stringify({ code: ${JSON.stringify(code)} })).then(async (r) => ({ status: r.status, error: (await r.json()).error }))`,
  );
  assert.equal(again.status, 403);
  assert.match(again.error, /already used/);
});

app.scene("a key trusted at the terminal leaves no card waiting in the Inbox", async () => {
  const key = await untrustedKey();
  await app.page.navigate(`${WRITE_BASE}/#/settings`);
  await app.page.waitFor(`document.querySelector(".settingshere .act-asktrust") !== null`, { timeout: 20000 });
  await app.page.click(".settingshere .act-asktrust");
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  const standing = `[...document.querySelectorAll(".proposal-act-device.proposal-standing")].some((c) => c.textContent.includes(${JSON.stringify(key)}))`;
  await app.page.waitFor(standing, { timeout: 20000 });
  assert.match(atTheTerminal(["device", "trust", key]), /withdrew its card in the Inbox/);
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  await app.page.waitFor(`!(${standing})`, { timeout: 20000 });
});
