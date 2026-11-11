/**
 * A CONNECTION'S CARDS, END TO END (the guided setup's stage 4, scenes 4 and 6 of its §12).
 *
 * `scripts/web-e2e.sh` starts this file's server with `--settings --connected calendar`: the
 * fixture connected a calendar through the instance file's own editor, so the box asks for its
 * link as one of its own cards, and its removal stands as a card in the `connect` conversation,
 * as the setup agent will file it.
 *
 * THREE SCENES, in order. A link served from an origin the calendar may not reach is stored
 * nowhere: the CDP log shows it in the tap's request alone, the card the box files names only
 * the origin, and its Allow writes the origin into the instance file. Then Remove takes the
 * calendar's link out of the instance file, and the box's card asking for it leaves the Inbox.
 * Last, the secretary, asked to add a Family calendar, files a Connect card whose tap adds it.
 */

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

import { SIGNED_POST, suite, trustThisDevice } from "./harness";

const app = await suite("connections");

const CONNECT = process.env["E2E_BASE_CONNECT"] ?? "";
const INSTANCE = process.env["E2E_CONNECT_INSTANCE"] ?? "";
const STORE = process.env["E2E_CONNECT_STORE"] ?? "";

const NAME = "PLUGIN_ICS_HOME";
/** What the card calls the link: the card says a secret in the owner's words, never its name. */
const WORDS = "your Home calendar's secret link";
const ORIGIN = "https://cal.example.test";
const LINK = `${ORIGIN}/private-e2e-9b1c/basic.ics`;

const instance = (): string => readFileSync(INSTANCE, "utf8");
const stored = (): string => (existsSync(STORE) ? readFileSync(STORE, "utf8") : "");

/** The box's open card asking for the calendar's link, by its index, or -1 while there is none. */
const linkCard = `[...document.querySelectorAll('.inboxpage .proposal.proposal-act-secret[data-conversation="setup"]')]
  .filter(card => (card.textContent ?? '').includes("${WORDS}"))
  .map(card => Number(card.getAttribute('data-proposal')))
  .concat([-1])[0]`;

app.scene("a link from an origin the calendar may not reach is stored nowhere, and asks to allow the origin", async () => {
  await trustThisDevice(app.page, CONNECT, INSTANCE);
  await app.page.navigate(`${CONNECT}/#/inbox`);
  await app.page.waitFor(`${linkCard} >= 0`, { timeout: 20000 });
  const index = await app.page.evaluate<number>(linkCard);
  const card = `.inboxpage .proposal[data-conversation="setup"][data-proposal="${index}"]`;
  // The box's own cards are one Setup row in the Inbox, opened in place.
  await app.page.click(".inboxpage .setupgroup > summary");
  await app.page.waitFor(`document.querySelector(".inboxpage .setupgroup").open === true`);

  const stopCapture = await app.page.captureNetwork(["/tap", "/api/"]);
  await app.page.type(`${card} .proposalsecretfield input`, LINK);
  await app.page.click(`${card} .act-store`);
  const grant = '.inboxpage .proposal.proposal-act-grant[data-conversation="setup"]';
  await app.page.waitFor(`document.querySelector('${grant}') !== null`, { timeout: 20000 });
  const traffic = await stopCapture();

  const carrying = traffic.filter((exchange) => exchange.requestBody.includes(LINK));
  assert.equal(carrying.length, 1, `one request carried the link: ${JSON.stringify(traffic.map((e) => e.url))}`);
  assert.match(carrying[0]?.url ?? "", /\/tap$/, "the one that carried it is the card's tap");
  for (const exchange of traffic) {
    assert.equal(exchange.responseBody.includes("private-e2e-9b1c"), false, `a response carried the link: ${exchange.url}`);
  }
  assert.equal(stored().includes(LINK), false, "nothing was stored");
  const said = await app.page.evaluate<string>(`document.querySelector('${grant}').textContent`);
  assert.match(said, /https:\/\/cal\.example\.test/, "the card names the origin");
  assert.equal(said.includes("private-e2e-9b1c"), false, "and never the link");
  assert.equal(instance().includes(`"${ORIGIN}"`), false, "nothing was allowed before the tap");

  await app.page.click(`${grant} .act-allow`);
  await app.page.click(`${grant} .act-allow.confirming`);
  await app.page.waitFor(`document.querySelector('${grant}') === null`, { timeout: 20000 });
  assert.match(instance(), /"https:\/\/cal\.example\.test"/, "the tap allowed the origin in the instance file");
});

app.scene("removing the calendar takes its link out of the instance file, and the box stops asking for it", async () => {
  await app.page.navigate(`${CONNECT}/#/inbox`);
  await app.page.reload();
  const removal = '.inboxpage .proposal.proposal-act-disconnect[data-conversation="connect"]';
  await app.page.waitFor(`document.querySelector('${removal} .act-remove') !== null`, { timeout: 20000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${removal}').textContent`),
    /todos, cards and conversations/,
    "the card says what stays",
  );
  await app.page.click(`${removal} .act-remove`);
  await app.page.click(`${removal} .act-remove.confirming`);
  await app.page.waitFor(`document.querySelector('${removal}') === null`, { timeout: 20000 });
  assert.equal(instance().includes(NAME), false, "the link's table is gone");

  await app.page.reload();
  await app.page.waitFor(`document.querySelector('.inboxpage') !== null`, { timeout: 20000 });
  await app.page.waitFor(`${linkCard} === -1`, { timeout: 20000 });
});

// THE SECRETARY ADDS WHAT SETTINGS OFFERS (audit S1): asked in a round of the owner's own words, it files a Connect
// card from the box's catalog, and the tap writes the calendar into the instance file; the box then asks for its link.
app.scene("asked to add a calendar, the secretary files a Connect card whose tap adds it", async () => {
  await app.page.navigate(`${CONNECT}/#/secretary/default`);
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  const id = await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Setup"})).then((r) => r.json()).then((j) => j.id)`,
  );
  await app.page.navigate(`${CONNECT}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  await app.page.type(".composebox", "connect my Family calendar");
  await app.page.click(".composesend");
  const card = `.proposal.proposal-act-connect[data-conversation="${id}"]`;
  await app.page.waitFor(`document.querySelector('${card} .act-connect') !== null`, { timeout: 20000 });
  assert.equal(instance().includes("PLUGIN_ICS_FAMILY"), false, "nothing is written before the tap");

  await app.page.click(`${card} .act-connect`);
  await app.page.click(`${card} .act-connect.confirming`);
  await app.page.waitFor(`document.querySelector('.proposal.proposal-acted.proposal-act-connect[data-conversation="${id}"]') !== null`, { timeout: 20000 });
  assert.match(instance(), /PLUGIN_ICS_FAMILY/, "the tap wrote the calendar");

  await app.page.navigate(`${CONNECT}/#/inbox`);
  await app.page.reload();
  const asked = `[...document.querySelectorAll('.inboxpage .proposal.proposal-act-secret[data-conversation="setup"]')]
    .some(card => (card.textContent ?? '').includes("your Family calendar's secret link"))`;
  await app.page.waitFor(asked, { timeout: 20000 });
});
