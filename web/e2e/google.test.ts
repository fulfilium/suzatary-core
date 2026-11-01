/**
 * CONNECT A GOOGLE ACCOUNT FROM SETTINGS, END TO END (`connectlist`, and the guided setup's stage 10,
 * `authcode`).
 *
 * `scripts/web-e2e.sh` starts this file's server with `--settings --connected google` and
 * `--google-fixture` aimed at `suzatary-dev fake-google`: every request the box makes to Google
 * reaches the fake, which checks the redirect, the S256 challenge and that a code is redeemed
 * once, and signs in the account each consent names. The fixture's own account is a Gmail
 * account read through lieer (`sync = "gmail"`).
 *
 * ONE SCENE, as a person who never read the code would walk it. Settings lists "A Google account"
 * first under Mail; its tap opens a conversation of its own, whose first line names each step;
 * the reply with an address makes the secretary file the Connect card; its tap writes the
 * account; on Settings, the box asks for its Google Cloud client once, whose card takes the
 * downloaded file in its write-only field; then for the account's sign-in, whose button goes to Google through the box;
 * the fake consents and sends the browser back to the box's callback, which keeps the grant, reads
 * the account's calendars at once and lands on the Inbox; Settings then says the account synced.
 * The CDP log shows the client's secret only in the tap that carried it, and the grant in no answer.
 *
 * Then the lieer account: the same tap and reply change nothing until its sign-in with Google is
 * kept and has read its mail through the fake's Gmail profile; its Settings line reads as before,
 * lieer's, with the sign-in card on it. Then its sync moves, with no second mail account and no
 * second client card.
 */

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

import { suite, trustThisDevice } from "./harness";

const app = await suite("google");

const GOOGLE = process.env["E2E_BASE_GOOGLE"] ?? "";
const INSTANCE = process.env["E2E_GOOGLE_INSTANCE"] ?? "";
const STORE = process.env["E2E_GOOGLE_STORE"] ?? "";
const FAKE = process.env["E2E_FAKE_GOOGLE"] ?? "";

const FRESH = "ada@example.test";
/** The fixture's own account, read through lieer. */
const LIEER = "her@example.invalid";
const SECRET = "GOCSPX-fake-client-secret";
const REFRESH = "1//fake-refresh-1";

const client = (redirect: string): string =>
  JSON.stringify({
    web: {
      client_id: "1234-fake.apps.googleusercontent.com",
      project_id: "suzatary-fake",
      client_secret: SECRET,
      redirect_uris: [redirect],
    },
  });

const stored = (): string => (existsSync(STORE) ? readFileSync(STORE, "utf8") : "");
const instance = (): string => readFileSync(INSTANCE, "utf8");

/** The selector of a waiting card of `kind` the box filed, as Settings draws it on its line. */
const cardOf = (kind: string): string => `.settingsbody .proposal.proposal-act-${kind}[data-conversation="setup"]:not(.proposal-acted)`;

/** The account's line on Settings. */
const lineOf = (account: string): string => `.settingsline[data-line="account:${account}"]`;

/**
 * Settings' Google offer, tapped, and the conversation of its own it opens, answered with `account`; its Connect card
 * tapped there, and Settings opened again, where the box's next cards stand on their lines.
 */
async function begin(account: string): Promise<void> {
  await app.page.navigate(`${GOOGLE}/#/settings`);
  const google = `.settingsoffers[data-group="mail"] .settingsoffer-google`;
  await app.page.waitFor(`document.querySelector('${google}') !== null`, { timeout: 20000 });
  await app.page.click(google);
  await app.page.waitFor(`location.hash.startsWith("#/secretary/") && !location.hash.startsWith("#/secretary/setup")`, { timeout: 20000 });
  const opening = "Connect a Google account (its mail and calendars). First, make the box's Google Cloud client";
  await app.page.waitFor(`document.body.textContent.includes(${JSON.stringify(opening)})`, { timeout: 20000 });
  assert.match(
    await app.page.evaluate<string>(`document.body.textContent`),
    /Then tap the account's Sign in with Google card[^.]*\. To begin, reply with its email address[^.]*\. Each step after it stands on its line in Settings/,
    "the first line says each step and what to reply",
  );
  await app.page.type(".composebox", account);
  await app.page.click(".composesend");
  const connect = ".proposal.proposal-act-connect:not(.proposal-acted)";
  await app.page.waitFor(`document.querySelector('${connect} .act-connect') !== null`, { timeout: 20000 });
  await app.page.click(`${connect} .act-connect`);
  await app.page.click(`${connect} .act-connect.confirming`);
  await app.page.waitFor(`document.querySelector('${connect}') === null`, { timeout: 20000 });
  await app.page.navigate(`${GOOGLE}/#/settings`);
}

/** The account's sign-in card tapped and followed to the fake Google and back, which keeps its grant; the card says so. */
async function signIn(account: string): Promise<void> {
  const signIn = cardOf("google_sign_in");
  await app.page.waitFor(`document.querySelector('${signIn} .act-sign_in') !== null`, { timeout: 20000 });
  assert.ok((await app.page.evaluate<string>(`document.querySelector('${signIn}').textContent`)).includes(account), "the card names the account");
  await app.page.click(`${signIn} .act-sign_in`);
  await app.page.click(`${signIn} .act-sign_in.confirming`);
  const button = `${signIn} a.proposallink`;
  await app.page.waitFor(`document.querySelector('${button}') !== null`, { timeout: 20000 });
  const href = await app.page.evaluate<string>(`document.querySelector('${button}').href`);
  const index = await app.page.evaluate<string>(`document.querySelector('${signIn}').getAttribute('data-proposal')`);
  assert.ok(href.startsWith(`${GOOGLE}/api/oauth/google/consent/`), `the button is the box's own path: ${href}`);
  assert.ok(FAKE !== "" && !href.startsWith(FAKE), "the page never holds Google's own link");

  // The fake consents at once and sends the browser back to the box's callback, which keeps the grant, reads the
  // calendars and lands on the Inbox. Only the box's two Google routes are captured: the Inbox it lands on opens its
  // event streams under /api/, and a stream never ends for the capture to stop. The page leaving is already at the
  // Inbox, complete, so the wait asks for a document without its mark.
  const back = await app.page.captureNetwork(["/api/oauth/google/"]);
  await app.page.evaluate(`(() => { window.__e2eLeaving = true; location.href = ${JSON.stringify(href)}; return true; })()`);
  await app.page.waitFor(
    `window.__e2eLeaving === undefined && location.origin === ${JSON.stringify(GOOGLE)} && location.hash === "#/inbox" && document.readyState === "complete"`,
    { timeout: 20000 },
  );
  for (const exchange of await back()) assert.equal(exchange.responseBody.includes(REFRESH), false, `an answer carried the grant: ${exchange.url}`);
  const drawn = await app.page.evaluate<string>(`fetch("/api/secretary/conversations/setup/drawn?indices=${index}").then((r) => r.text())`);
  const row = (JSON.parse(drawn) as { proposals: { state: string; outcome: string }[] }).proposals[0];
  assert.equal(row?.state, "acted", drawn);
  assert.ok(row?.outcome.includes(`signed ${account} in with Google`), row?.outcome);
}

/** Settings' line for `account`, once it says the account is signed in with Google and its calendars synced. */
async function healthy(account: string): Promise<void> {
  await app.page.navigate(`${GOOGLE}/#/settings`);
  const line = lineOf(account);
  await app.page.waitFor(
    `document.querySelector('${line} .settingshealth')?.textContent.startsWith("Synced ") === true`,
    { timeout: 20000 },
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${line} .settingsmeta').textContent`),
    /^Google: mail and calendars by one sign-in with Google, signed in\.$/,
  );
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${line}.settingsline-failing') !== null`), false);
}

app.scene("a Google account connects from Settings in one walk, and a lieer account moves onto the same sign-in", async () => {
  await trustThisDevice(app.page, GOOGLE, INSTANCE);

  await begin(FRESH);
  assert.match(instance(), /email = "ada@example\.test"[^[]*sync = "google"/, "the tap wrote the account");

  // The box's Google Cloud client, asked for once, on Settings: its words are the steps in Google's console.
  const clientCard = cardOf("google_client");
  await app.page.waitFor(`document.querySelector('${clientCard} .proposalsecretfield input') !== null`, { timeout: 20000 });
  const said = await app.page.evaluate<string>(`document.querySelector('${clientCard}').textContent`);
  const redirect = `${GOOGLE}/api/oauth/google/callback`;
  assert.ok(said.includes(redirect), `the card names the exact redirect: ${said}`);
  assert.match(said, /In production/, "the card says to publish the app");
  assert.match(said, /7 days/, "and what Testing costs");

  const stopCapture = await app.page.captureNetwork(["/tap", "/api/"]);
  await app.page.type(`${clientCard} .proposalsecretfield input`, client(redirect));
  await app.page.click(`${clientCard} .act-store`);
  await app.page.waitFor(`document.querySelector('${cardOf("google_sign_in")} .act-sign_in') !== null`, { timeout: 20000 });
  const traffic = await stopCapture();
  const carrying = traffic.filter((exchange) => exchange.requestBody.includes(SECRET));
  assert.equal(carrying.length, 1, `one request carried the client: ${JSON.stringify(traffic.map((e) => e.url))}`);
  assert.match(carrying[0]?.url ?? "", /\/tap$/, "the one that carried it is the card's tap");
  for (const exchange of traffic) assert.equal(exchange.responseBody.includes(SECRET), false, `an answer carried it: ${exchange.url}`);
  assert.ok(stored().includes(SECRET), "the client's secret is kept");

  await signIn(FRESH);
  assert.ok(stored().includes(`GOOGLE_GRANT_ADA_AT_EXAMPLE_TEST=mail,calendar ${REFRESH}`), "the grant is kept under the account's name, with the parts Google granted");
  await healthy(FRESH);

  // The lieer account: the same walk, which moves it only once its grant is kept and has read its mail. Until then its
  // Settings line reads as before, lieer's, with the sign-in card on it; the client is not asked for again.
  const accounts = (): number => instance().split("[[mail.accounts]]").length - 1;
  const before = accounts();
  const lieer = lineOf(LIEER);
  const lineSays = `[...document.querySelectorAll('${lieer} .settingsmeta, ${lieer} .settingshealth')].map((p) => p.textContent).join(" | ")`;
  await app.page.navigate(`${GOOGLE}/#/settings`);
  await app.page.waitFor(`document.querySelector('${lieer}') !== null`, { timeout: 20000 });
  const lieerSaid = await app.page.evaluate<string>(lineSays);
  await begin(LIEER);
  assert.equal(accounts(), before, "no second mail account");
  assert.match(instance(), /email = "her@example\.invalid"[^[]*sync = "gmail"/, "the Connect tap changed nothing");
  await app.page.waitFor(`document.querySelector('${lieer} .proposal.proposal-act-google_sign_in') !== null`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<string>(lineSays), lieerSaid, "until the grant reads its mail, the line reads as before");
  assert.match(lieerSaid, /Gmail, through lieer's own grant\./);
  await signIn(LIEER);
  await app.page.navigate(`${GOOGLE}/#/settings`);
  await app.page.waitFor(`document.querySelector('${lieer}') !== null`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll('${cardOf("google_client")}').length`), 0, "one client serves every account");
  assert.match(stored(), /GOOGLE_GRANT_HER_AT_EXAMPLE_INVALID=mail,calendar /, "its grant is kept under its own name");
  assert.match(instance(), /email = "her@example\.invalid"[^[]*sync = "google"/, "its sync moved once the grant read its mail");
  await healthy(LIEER);
});
