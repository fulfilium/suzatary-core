/**
 * A SECRET, ASKED FOR IN THE INBOX (`docs/SECRETASK-PLAN-2026-09-21.md` §3
 * commit 3): a card whose act is `kind: "secret"` — the standing proposal
 * `app/cli/src/commands/fixture.rs::a_secret_ask` seeds directly through the
 * real conversation store rather than driving the scripted model — over
 * the SAME isolated `secretspec` store every `--settings` server shares
 * (`scripts/web-e2e.sh`'s `secrets_home`/`secrets_store`).
 *
 * It asks for `SECRET_ASK_KEY`, the coding routine's token, which the write
 * server's `[coding]` declares and its Settings line reads, because the whole
 * point of this scene is that the secretary's card and the box's own card on
 * the Settings line are the SAME tap. Not the model's credential: only its
 * sign-in card asks for that, never a Secret card. The key MAY already resolve when this scene starts, and the
 * assertions below never depend on it starting unset: `overview.test.ts`
 * checks the fresh-start transition, on the calendar's link.
 *
 * ONE SCENE: the card shows the sentence and a password field, Store is
 * disabled until something is typed, and tapping it does what any proposal's
 * tap does in the Inbox (`inbox.test.ts`'s own "tapping a proposal … drops
 * the count" scene) — SETTLES THE DECISION, so the card leaves the Inbox
 * entirely rather than sitting there re-drawn as `Acted`
 * (`docs/INBOXPRODROME-PLAN-2026-09-17.md` §5(a)). The rendering rule this
 * card obeys once answered — "stored", and no field — is checked instead on
 * the CONVERSATION page, which draws every proposal whatever its state
 * (`proposal.ts`'s own header: "ONE implementation, mounted in two places").
 * The Settings page then lists the same name as resolving, and the captured
 * network (`cdp.ts`'s `captureNetwork`) shows the value in exactly ONE
 * request body — the tap — and in no response body anywhere in the scene.
 */

import assert from "node:assert/strict";

import { WRITE_BASE, suite } from "./harness";

const app = await suite("tapsecret");

/** The name the card asks for — named once by the shell as `E2E_SETTINGS_KEY`, read here rather
 * than duplicated. */
const SETTINGS_KEY = process.env["E2E_SETTINGS_KEY"] ?? "SUZATARY_ROUTINE_TOKEN";
/** The template's words for it, which the card says instead of the name. */
const SETTINGS_LABEL = "the coding routine's token";

/** A fixture value — never a real one, and never the literal another scene
 * types, so a screenshot or a log line naming one is never mistaken for the
 * other. */
const FIXTURE_VALUE = "fixture-secretask-value";

const CARD = '.inboxpage .proposal.proposal-act-secret[data-conversation="secretask"]';

async function openInbox(): Promise<void> {
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  await app.page.reload();
  await app.page.waitFor(`document.querySelector('${CARD}') !== null`, { timeout: 15000 });
}

app.scene("the Inbox asks for a declared secret, and Store performs the same tap the box's card does", async () => {
  await openInbox();

  // THE HEAD NAMES THE KEY IN WORDS ("Store the coding routine's token"), and so does the field's label; the stored name is
  // nowhere on the card.
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
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${field}').getAttribute('type')`),
    "password",
  );
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector('${field}').getAttribute('autocomplete')`),
    "off",
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${field}').closest("label").querySelector("span").textContent`),
    "The coding routine's token",
    "the field is labelled on screen by what it is, never a bare placeholder",
  );
  assert.equal(await app.page.evaluate<string>(`document.querySelector('${field}').placeholder`), "Paste it here");

  // NOTHING TYPED YET: ONE BUTTON, AND IT IS DISABLED — there is no confirm
  // step behind it, so it must not be pressable on an empty field.
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll('${CARD} .proposalsecretfield button').length`),
    1,
    "one button, not a two-step confirm",
  );
  assert.equal(
    await app.page.evaluate<boolean>(`document.querySelector('${CARD} .act-store').disabled`),
    true,
    "Store is disabled until something is typed",
  );

  // ⚠️ THE CAPTURE STARTS BEFORE THE VALUE IS EVER TYPED, so the request that
  // carries it is provably the one this scene drives, not an artefact of an
  // earlier scene's own traffic.
  const stopCapture = await app.page.captureNetwork(["/tap"]);

  await app.page.type(field, FIXTURE_VALUE);
  assert.equal(
    await app.page.evaluate<boolean>(`document.querySelector('${CARD} .act-store').disabled`),
    false,
    "typing enables Store",
  );
  await app.page.click(`${CARD} .act-store`);
  // SETTLING THE DECISION DROPS IT FROM THE INBOX — the same rule a
  // changeset's tap obeys there (`inbox.test.ts`), never a `proposal-acted`
  // card left sitting in a QUEUE OF OPEN DECISIONS.
  await app.page.waitFor(`document.querySelector('${CARD}') === null`, { timeout: 10000 });

  const traffic = await stopCapture();
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

  // THE CONVERSATION PAGE STILL DRAWS THE SAME CARD, ANSWERED: "stored",
  // and no field left to type into again — `proposal.ts`'s one
  // implementation, checked here rather than in the Inbox, which had
  // already dropped the row above.
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

  // THE SETTINGS PAGE THEN LISTS THE SAME NAME AS SET — one route, one
  // store, whichever surface asked for it (§3 rule 3).
  await app.page.navigate(`${WRITE_BASE}/#/settings`);
  await app.page.reload();
  await app.page.waitFor(
    `document.querySelector('.settingsline[data-line="routine"]:not(.settingsline-failing)') !== null`,
    { timeout: 15000 },
  );
});
