/**
 * A secret asked for in the Inbox: the card the fixture's `a_secret_ask` seeds asks for the coding routine's token over
 * the write server's isolated `secretspec` store, because the scene proves the secretary's card and the box's card on
 * the Settings line are the same tap. The card names the key in words over a masked field, Store over an empty field
 * sends nothing, and the tap settles the decision with the value in exactly one request and no response; the key may
 * already resolve when the scene starts (`overview.test.ts` checks the fresh start). The model's credential is asked
 * for only by its sign-in card, never by a Secret card.
 */

import assert from "node:assert/strict";

import { WRITE_BASE, suite } from "./harness";

const app = await suite("tapsecret");

/** Named once, by the shell, as `E2E_SETTINGS_KEY`. */
const SETTINGS_KEY = process.env["E2E_SETTINGS_KEY"] ?? "SUZATARY_ROUTINE_TOKEN";
/** The template's words for it, which the card says instead of the name. */
const SETTINGS_LABEL = "the coding routine's token";

/** Never a real value, nor the literal another scene types, so a log line naming one is never taken for the other. */
const FIXTURE_VALUE = "fixture-secretask-value";

const CARD = '.inboxpage .proposal.proposal-act-secret[data-conversation="secretask"]';

async function openInbox(): Promise<void> {
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  await app.page.reload();
  await app.page.waitFor(`document.querySelector('${CARD}') !== null`, { timeout: 15000 });
}

app.scene("the Inbox asks for a declared secret, and Store performs the same tap the box's card does", async () => {
  await openInbox();

  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${CARD} .proposalact').textContent`),
    new RegExp(SETTINGS_LABEL),
    "the head names the declared key in words",
  );
  assert.equal(
    (await app.page.evaluate<string>(`document.querySelector('${CARD}').textContent`)).includes(SETTINGS_KEY),
    false,
    "the stored name is not drawn",
  );

  const field = `${CARD} .proposalsecretfield input`;
  // A text field the stylesheet masks, never `type=password`, which a password manager takes for a sign-in and keeps.
  assert.deepEqual(
    await app.page.evaluate<string[]>(`(() => {
      const f = document.querySelector('${field}');
      return [f.type, f.getAttribute('autocomplete'), f.getAttribute('spellcheck'), getComputedStyle(f).webkitTextSecurity];
    })()`),
    ["text", "off", "false", "disc"],
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${field}').closest("label").querySelector("span").textContent`),
    "The coding routine's token",
    "the field is labelled on screen by what it is, never a bare placeholder",
  );
  assert.equal(await app.page.evaluate<string>(`document.querySelector('${field}').placeholder`), "Paste it here");

  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll('${CARD} .proposalsecretfield button').length`),
    1,
    "one button, not a two-step confirm",
  );

  // ⚠️ The capture starts before any tap, so its count proves Store over an empty field or spaces alone sent nothing:
  // the browser holds the form back while the field holds no character other than a space.
  const stopCapture = await app.page.captureNetwork(["/tap"]);
  await app.page.click(`${CARD} .act-store`);
  await app.page.type(field, "   ");
  await app.page.click(`${CARD} .act-store`);
  await app.page.evaluate(`document.querySelector('${field}').value = ""`);

  await app.page.type(field, FIXTURE_VALUE);
  await app.page.click(`${CARD} .act-store`);
  // Settling the decision drops the card from the Inbox, which is a queue of open decisions.
  await app.page.waitFor(`document.querySelector('${CARD}') === null`, { timeout: 10000 });

  const traffic = await stopCapture();
  assert.equal(traffic.length, 1, `one tap went, the one with the value: ${JSON.stringify(traffic.map((exchange) => exchange.url))}`);
  const withValue = traffic.filter((exchange) => exchange.requestBody.includes(FIXTURE_VALUE));
  assert.equal(
    withValue.length,
    1,
    `exactly one request carried the value: ${JSON.stringify(traffic.map((exchange) => exchange.url))}`,
  );
  assert.match(withValue[0]?.url ?? "", /\/tap$/, "the one request that carried it is the tap");
  for (const exchange of traffic) {
    assert.equal(
      exchange.responseBody.includes(FIXTURE_VALUE),
      false,
      `a response carried the value: ${exchange.url}`,
    );
  }

  // The conversation page draws every proposal whatever its state, so the answered card is checked there.
  await app.page.navigate(`${WRITE_BASE}/#/secretary/secretask`);
  await app.page.waitFor(`document.querySelector('.proposal.proposal-acted') !== null`, {
    timeout: 15000,
  });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('.proposal .proposaloutcome').textContent`),
    /stored/,
    "the outcome names what happened, never the value",
  );
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll('.proposal input').length`),
    0,
    "an acted secret card draws no field",
  );

  // The Settings page then lists the same name as set: one route, one store, whichever surface asked.
  await app.page.navigate(`${WRITE_BASE}/#/settings`);
  await app.page.reload();
  await app.page.waitFor(
    `document.querySelector('.settingsline[data-line="routine"]:not(.settingsline-failing)') !== null`,
    { timeout: 15000 },
  );
});
