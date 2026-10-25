/**
 * A CONNECTION'S HEALTH AND ITS LINKS, END TO END (the first-setup audit's S6, S7, S8, S9, S13
 * and S17, `docs/AUDIT-SETUP-2026-10-17.md`).
 *
 * `scripts/web-e2e.sh` starts this file's server with `--settings --connections lapsed`: the Work calendar
 * declared with its link not stored, over a secret store of its own, the reference plugins
 * passed in `SUZATARY_PLUGINS` so a stored link is synced at its tap; and a Microsoft account
 * signed in to read and not to send.
 *
 * THREE SCENES, in order:
 *
 * 1. A link from an origin the calendar may not reach: the Allow card is drawn inside the
 *    link's own card on its line, and allowing it stores the link pasted once, which is checked
 *    at the tap; its host does not resolve, so the line says so in words and the box asks for
 *    the link again on the same line.
 * 2. A calendar added to the instance file by hand is drawn, and its link asked for, with no
 *    restart and no reload.
 * 3. The Microsoft account's consent to send is a card on its line, tapped like reading's.
 */

import assert from "node:assert/strict";
import { existsSync, readFileSync, writeFileSync } from "node:fs";

import { suite, trustThisDevice } from "./harness";

const app = await suite("links");

const LINKS = process.env["E2E_BASE_LINKS"] ?? "";
const INSTANCE = process.env["E2E_LINKS_INSTANCE"] ?? "";
const STORE = process.env["E2E_LINKS_STORE"] ?? "";

/** Served from an origin the fixture's calendar may not reach, at a host that does not resolve. */
const LINK = "https://cal.example.test/private-links-4d2e/basic.ics";

const stored = (): string => (existsSync(STORE) ? readFileSync(STORE, "utf8") : "");

const line = (key: string): string => `.settingsline[data-line="${key}"]`;
const work = line("credential:ics/work");
const linkCard = `${work} .proposal.proposal-act-secret`;
const allow = `${linkCard} .proposalallow .proposal.proposal-act-grant`;

async function openSettings(waitFor: string): Promise<void> {
  await app.page.navigate(`${LINKS}/#/settings`);
  await app.page.reload();
  await app.page.waitFor(`document.querySelector('${waitFor}') !== null`, { timeout: 20000 });
}

const text = (selector: string): Promise<string> => app.page.evaluate<string>(`document.querySelector('${selector}')?.textContent ?? ""`);

app.scene("a link from a new origin is allowed on its own card, sent once, and checked at the tap", async () => {
  await trustThisDevice(app.page, LINKS, INSTANCE);
  await openSettings(`${linkCard} .proposalsecretfield input`);

  const stopCapture = await app.page.captureNetwork(["/tap", "/api/"]);
  await app.page.type(`${linkCard} .proposalsecretfield input`, LINK);
  await app.page.click(`${linkCard} .act-store`);
  await app.page.waitFor(`document.querySelector('${allow} .act-allow') !== null`, { timeout: 20000 });
  assert.match(await text(`${linkCard} .proposaloutcome`), /first allow https:\/\/cal\.example\.test/, "the refusal is a step");
  assert.match(await text(allow), /https:\/\/cal\.example\.test/, "the Allow card names the origin");
  assert.equal(stored().includes(LINK), false, "nothing is stored before the origin is allowed");

  await app.page.click(`${allow} .act-allow`);
  await app.page.click(`${allow} .act-allow.confirming`);
  // The tab sends the link it kept with the Allow: no second paste.
  await app.page.waitFor(`document.querySelector('${work} .settingshealth')?.textContent.includes("could not be reached") === true`, {
    timeout: 30000,
  });
  const traffic = await stopCapture();
  assert.equal(stored().includes(LINK), true, "the link pasted once is stored");
  assert.match(readFileSync(INSTANCE, "utf8"), /"https:\/\/cal\.example\.test"/, "the origin is allowed in the instance file");
  const carrying = traffic.filter((exchange) => exchange.requestBody.includes(LINK));
  assert.equal(carrying.length, 2, `the refused tap and the one Allow sent: ${JSON.stringify(carrying.map((e) => e.url))}`);
  for (const exchange of carrying) assert.match(exchange.url, /\/tap$/);
  for (const exchange of traffic) assert.equal(exchange.responseBody.includes("private-links-4d2e"), false, `a response carried the link: ${exchange.url}`);

  // Checked at the tap, not at the timer: unreachable, in the owner's words, and asked for again on its line.
  const health = await text(`${work} .settingshealth`);
  assert.match(health, /its address could not be reached/, health);
  assert.equal(/\blost\b/.test(health), false, health);
  await app.page.waitFor(`document.querySelector('${linkCard}.proposal-standing .proposalsecretfield input') !== null`, { timeout: 20000 });
});

app.scene("a calendar added to the instance file by hand is drawn with no restart", async () => {
  await openSettings(work);
  const home = line("credential:ics/home");
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${home}') === null`), true);
  const declared = readFileSync(INSTANCE, "utf8");
  const one = `credentials = ["PLUGIN_ICS_WORK"]`;
  assert.ok(declared.includes(one), "the fixture declares its calendar inline");
  writeFileSync(INSTANCE, declared.replace(one, `credentials = ["PLUGIN_ICS_WORK", "PLUGIN_ICS_HOME"]`));
  await app.page.waitFor(`document.querySelector('${home} .proposal.proposal-act-secret') !== null`, { timeout: 30000 });
  assert.equal(await text(`${home} h3`), "Home");
});

app.scene("a Microsoft account's consent to send is a card on its line", async () => {
  const account = line("account:ada-work@example.test");
  const card = `${account} .proposal.proposal-act-auth_send`;
  await openSettings(card);
  assert.match(await text(`${account} .settingsmeta`), /reading signed in, sending not signed in/);
  await app.page.click(`${card} .act-run`);
  await app.page.click(`${card} .act-run.confirming`);
  // Answered, the card leaves the line, and its conversation says what the tap stored.
  await app.page.waitFor(`document.querySelector('${card}') === null`, { timeout: 20000 });
  await app.page.navigate(`${LINKS}/#/secretary/setup`);
  await app.page.waitFor(
    `[...document.querySelectorAll(".proposal.proposal-acted .proposaloutcome")].some((n) => n.textContent.includes("smtp grant stored for ada-work@example.test"))`,
    { timeout: 20000 },
  );
});
