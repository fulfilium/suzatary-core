/**
 * THE MODEL BOOTSTRAP, END TO END (the guided setup's stage 0): connecting the first model is
 * the one step no model guides, so the box asks for it itself.
 *
 * `scripts/web-e2e.sh` starts this file's server with `--model unanswered`: the register says
 * its model's last call failed, so the box files the "Connect a model" card at start, and its
 * `PATH` holds a fake `claude` that draws what `claude setup-token` draws. No real sign-in is
 * ever reached.
 *
 * ONE SCENE: Settings says no model answers and draws the card; Connect starts the sign-in,
 * whose link (the box's own reading of the CLI) is the code card's button; the code typed into
 * its field keeps the subscription's token, and the CDP log shows the code in the tap's request
 * alone and the token in no request or response.
 */

import assert from "node:assert/strict";

import { suite, trustThisDevice } from "./harness";

const app = await suite("bootstrap");

const BOOTSTRAP = process.env["E2E_BASE_BOOTSTRAP"] ?? "";
const INSTANCE = process.env["E2E_BOOTSTRAP_INSTANCE"];

/** What the fake `claude` accepts, and the token it prints for it: never a real one. */
const CODE = "e2e-code#e2e-state";
const TOKEN = "sk-ant-oat01-E2Efake-token-never-real";

const connect = '.settingsbody .proposal.proposal-act-sign_in[data-conversation="setup"]';
const code = '.settingsbody .proposal.proposal-act-sign_in_code[data-conversation="setup"]';

app.scene("with no model answering, Settings connects one by signing in", async () => {
  await trustThisDevice(app.page, BOOTSTRAP, INSTANCE);
  await app.page.navigate(`${BOOTSTRAP}/#/settings`);
  // The card draws before the page knows this device may act, and its button once it does.
  await app.page.waitFor(`document.querySelector('${connect} .act-connect') !== null`, { timeout: 15000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".settingsunanswered").textContent`),
    /No model answers/,
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${connect} .proposaltag').textContent`),
    /^App$/,
    "the box filed it, with no model asked",
  );

  await app.page.click(`${connect} .act-connect`);
  await app.page.click(`${connect} .act-connect.confirming`);
  await app.page.waitFor(`document.querySelector('${code} .proposallink') !== null`, { timeout: 30000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${code} .proposallink').getAttribute("href")`),
    /^https:\/\/claude\.com\/cai\/oauth\/authorize\?/,
    "the button opens the link the CLI drew",
  );

  const stopCapture = await app.page.captureNetwork(["/tap", "/api/"]);
  await app.page.type(`${code} .proposalsecretfield input`, CODE);
  await app.page.click(`${code} .act-finish`);
  await app.page.waitFor(`document.querySelector(".settingsunanswered") === null`, { timeout: 30000 });

  const traffic = await stopCapture();
  const carrying = traffic.filter((exchange) => exchange.requestBody.includes(CODE));
  assert.equal(carrying.length, 1, `one request carried the code: ${JSON.stringify(traffic.map((e) => e.url))}`);
  assert.match(carrying[0]?.url ?? "", /\/tap$/, "the one that carried it is the code card's tap");
  for (const exchange of traffic) {
    assert.equal(exchange.requestBody.includes(TOKEN), false, `a request carried the token: ${exchange.url}`);
    assert.equal(exchange.responseBody.includes(TOKEN), false, `a response carried the token: ${exchange.url}`);
    assert.equal(exchange.responseBody.includes(CODE), false, `a response carried the code: ${exchange.url}`);
  }
  const pageText = await app.page.evaluate<string>(`document.body.textContent`);
  assert.equal(pageText.includes(CODE) || pageText.includes(TOKEN), false, "neither reached the page");
});
