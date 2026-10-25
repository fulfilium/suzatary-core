/**
 * SETTINGS AS WHAT IS CONNECTED, END TO END (`docs/SETUPAGENT-PLAN-2026-10-07.md`
 * §8 and §12, the guided setup's stage 2, `overview`).
 *
 * The fixture is `--settings`: the model's token and one
 * calendar link on the `ics` plugin (`fixture.rs::SETTINGS_CALENDAR_LINK`),
 * a calendar that synced once and has since lost its link, each declared
 * over an ISOLATED `secretspec` store (`scripts/web-e2e.sh`'s
 * `secrets_home`/`secrets_store`, never this box's `pass`) that the
 * `--settings` servers share. Nothing stores either, so each server's box
 * files their cards by itself at start, with no model asked.
 *
 * THE PLAN'S SCENES 1, 8 AND 2, THEN TWO KEPT FROM THE PAGE BEFORE IT:
 *
 * 1. Only what is connected is drawn, grouped: no Notifications heading on a
 *    deployment with no channel, only the offer, and no key name anywhere on
 *    the page, its cards included, nor in `/api/overview`'s answer or a
 *    drawn card's row.
 * 8. A read-only login sees the lines and the cards, and no control at all.
 * 2. The calendar's line is failing and draws its fix card inline; storing
 *    the link from it makes the line healthy, and the CDP network log shows
 *    the value in the tap's request and nowhere else.
 * -  The answered card says what it stored in words on its conversation page.
 * -  A key revoked at the terminal is answered with the wipe, and the device
 *    deletes what it holds and comes back with a new key; a wipe a reload cut
 *    short is finished by the next load.
 */

import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { atTheTerminal, BASE, PHONE, suite, WRITE_BASE } from "./harness";

const app = await suite("overview");

// Its own, made here: a file run without the shots file, or in another shard, has no other maker of it.
const SHOTS = process.env["SUZATARY_SHOTS"] ?? join(tmpdir(), "suzatary-shots");
mkdirSync(SHOTS, { recursive: true });

/** What the fixture declares and nothing stores, named once by `fixture.rs`; never drawn on a line. */
const NAMES = ["CLAUDE_CODE_OAUTH_TOKEN", "PLUGIN_ICS_WORK"];
/** What the owner reads instead of the link's name: the template's label for it. */
const LINK_LABEL = "your Work calendar's secret link";

/** The storage's record of a wipe begun and not ended (`web/src/app/wipe.ts`). */
const WIPING = "suzatary.wiping";

/** A fixture value — never a real one. */
const FIXTURE_VALUE = "https://calendar.example.test/private-fixture/basic.ics";

const calendarLine = '.settingsline[data-line="credential:ics/work"]';
/** The box's own card for the link, drawn on its line. */
const boxCard = `${calendarLine} .proposal.proposal-act-secret[data-conversation="setup"]`;

/** Open `#/settings` and wait until the calendar's line has drawn: it waits on `GET /api/overview` and `GET /api/me`. */
async function openSettings(base: string): Promise<void> {
  await app.page.navigate(`${base}/#/settings`);
  await app.page.reload();
  await app.page.waitFor(`document.querySelector('${calendarLine}') !== null`, { timeout: 15000 });
}


app.scene("only what is connected is drawn, grouped, and no line names a key", async () => {
  await openSettings(BASE);
  assert.deepEqual(
    await app.page.evaluate<string[]>(`[...document.querySelectorAll(".settingsbody h2")].map((h) => h.textContent)`),
    ["Models", "Mail", "Calendar", "Storage", "This box"],
    "a group nothing is connected to has no heading",
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".settingsoffered").textContent`),
    /Nothing connected for drive, notifications, coding sessions, other connections\./,
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${calendarLine} h3').textContent`),
    "Work",
    "a link is named for what the owner calls it",
  );
  const said = await app.page.evaluate<string>(`document.body.textContent`);
  const answered = await app.page.evaluate<string>(`fetch("/api/overview").then((r) => r.text())`);
  // Every card the page draws, as the box answers its row: the act a secret card carries is in words too.
  const rows = await app.page.evaluate<string>(
    `Promise.all([...document.querySelectorAll(".settingsline .proposal")].map((c) =>
      fetch("/api/secretary/conversations/" + c.dataset.conversation + "/drawn?indices=" + c.dataset.proposal).then((r) => r.text()))).then((rs) => rs.join("\\n"))`,
  );
  assert.match(rows, /"kind":"secret"/, "the page draws a secret card");
  for (const name of NAMES) {
    assert.equal(said.includes(name), false, `${name} is drawn on the page`);
    assert.equal(answered.includes(name), false, `${name} is in the overview's answer`);
    assert.equal(rows.includes(name), false, `${name} is in a card's row`);
  }
  assert.deepEqual(
    await app.page.evaluate<string[]>(
      `[...document.querySelectorAll('.settingsline[data-group="models"] h3')].map((h) => h.textContent)`,
    ),
    ["Chat, reviews and titles", "Compactor"],
    "the Models group lists which Claude model each job runs, and nothing to choose",
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('.settingsline[data-line="job:Chat, reviews and titles"] .settingshealth').textContent`),
    /^Answered /,
    "the model's line stands as its calls answered",
  );
});

app.scene("a terminal's command is folded behind its line, and the page names no path on the box", async () => {
  await openSettings(BASE);
  const repository = '.settingsline[data-line="repository"]';
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${repository} .settingshealth').textContent`),
    /^Set up at a terminal on the box: /,
    "the line says why in words",
  );
  assert.deepEqual(
    await app.page.evaluate<[boolean, string]>(
      `(() => { const d = document.querySelector('${repository} details.settingsatbox'); return [d.open, d.querySelector("summary").textContent] })()`,
    ),
    [false, "How to set this up at the box"],
    "the command waits, folded",
  );
  const shown = await app.page.evaluate<string>(
    `[...document.querySelectorAll(".settingsbody .settingsline")].map((l) => [...l.children].filter((c) => !c.matches("details")).map((c) => c.textContent).join(" ")).join("\\n")`,
  );
  assert.equal(/remote add|auth login|\/srv\/|\/tmp\/|\/home\//.test(shown), false, shown);
  const height = await app.page.evaluate<number>(`document.documentElement.scrollHeight`);
  await app.page.viewport(PHONE[0], height);
  await app.page.waitFor(`window.innerHeight === ${height}`);
  for (const theme of ["light", "dark"] as const) {
    await app.page.colorScheme(theme);
    await app.page.screenshot(join(SHOTS, `settings-390-${theme}.png`));
  }
  await app.page.colorScheme(null);
  await app.page.viewport(PHONE[0], PHONE[1]);
});

app.scene("a read-only login sees the lines and the cards, and no control", async () => {
  await openSettings(BASE);
  await app.page.waitFor(`document.querySelector(".settingshere h2")?.textContent === "This device"`);
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".settingsdevice").textContent`),
    /Rendered text kept here: \d+\.\d MB of 20\.0 MB .*Storage is|This browser cannot/,
  );
  await app.page.waitFor(`document.querySelector('${boxCard}') !== null`, { timeout: 15000 });
  assert.match(await app.page.evaluate<string>(`document.querySelector('${boxCard} .proposalact').textContent`), new RegExp(LINK_LABEL));
  assert.ok(await app.page.evaluate<boolean>(`document.querySelector('${boxCard} .readonly') !== null`));
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".act-ask") === null`), true, "no Ask");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".settingspage input").length`), 0, "no field");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".settingspage button").length`), 0, "no button");
});

app.scene("a calendar that lost its link draws its fix card, and storing it heals the line without returning it", async () => {
  await openSettings(WRITE_BASE);
  assert.ok(await app.page.evaluate<boolean>(`document.querySelector('${calendarLine}.settingsline-failing') !== null`), "it starts failing");
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${calendarLine} .settingshealth').textContent`),
    `Needs ${LINK_LABEL}.`,
  );
  await app.page.waitFor(`document.querySelector('${boxCard}') !== null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<string>(`document.querySelector('${boxCard} .proposaltag').textContent`), "Setup");
  assert.ok(await app.page.evaluate<boolean>(`document.querySelector(".act-ask") !== null`), "a writer is offered Ask");

  const stopCapture = await app.page.captureNetwork(["/tap", "/api/overview"]);
  await app.page.type(`${boxCard} .proposalsecretfield input`, FIXTURE_VALUE);
  await app.page.click(`${boxCard} .act-store`);
  await app.page.waitFor(`document.querySelector('${calendarLine}:not(.settingsline-failing)') !== null`, { timeout: 15000 });
  assert.match(await app.page.evaluate<string>(`document.querySelector('${calendarLine} .settingshealth').textContent`), /^Synced /);

  // ⚠️ NOT ONLY THE DOM: the tap's REQUEST carries the value (it has to, to store it); no response, and no other request, does.
  const traffic = await stopCapture();
  const carrying = traffic.filter((exchange) => exchange.requestBody.includes(FIXTURE_VALUE));
  assert.equal(carrying.length, 1, `one request carried the value: ${JSON.stringify(traffic.map((e) => e.url))}`);
  assert.match(carrying[0]?.url ?? "", /\/tap$/, "the one that carried it is the card's tap");
  for (const exchange of traffic) assert.equal(exchange.responseBody.includes(FIXTURE_VALUE), false, `a response carried the value: ${exchange.url}`);
  assert.equal((await app.page.evaluate<string>(`document.body.textContent`)).includes(FIXTURE_VALUE), false, "the value never reached the page");
});

app.scene("the answered card names the link on its conversation page, never the value", async () => {
  await app.page.navigate(`${WRITE_BASE}/#/secretary/setup`);
  // The conversation holds an earlier file's answered card too, so this waits for its own, as the replica brings it.
  await app.page.waitFor(
    `[...document.querySelectorAll(".proposal.proposal-acted .proposaloutcome")].some((node) => node.textContent.includes(${JSON.stringify(`stored ${LINK_LABEL}`)}))`,
    { timeout: 15000 },
  );
  const outcome = await app.page.evaluate<string>(
    `[...document.querySelectorAll(".proposal.proposal-acted .proposaloutcome")].map((node) => node.textContent).join(" ")`,
  );
  assert.match(outcome, new RegExp(`stored ${LINK_LABEL}`), `the outcome, in the box's words: ${outcome}`);
  assert.equal((await app.page.evaluate<string>(`document.body.textContent`)).includes(FIXTURE_VALUE), false, "never the value");
});

// Last in the file: it revokes this browser's key, which every writing scene before it signs with.
app.scene("a key revoked at the terminal is answered with the wipe, and the device deletes what it holds", async () => {
  const key = await app.trust();
  const shown = `(document.querySelector(".settingskey")?.getAttribute("data-key") ?? "")`;
  // The section is read again as the replica learns the key's standing, so it is waited for, never opened again.
  await openSettings(WRITE_BASE);
  await app.page.waitFor(`${shown} === ${JSON.stringify(key)}`, { timeout: 15000 });

  // A second tab of this device, open on the same replica, hears nothing from the box: its streams are held from the
  // start (a held request takes none of the host's six connections, which the first tab's streams already fill), and
  // every request once it has drawn. It is told of the wipe by the first tab's mark alone, and wipes too, so it never
  // writes the revoked device's replica back.
  const other = await app.tab();
  await other.holdRequests(["/events", "/stream"]);
  await other.navigate(`${WRITE_BASE}/#/settings`);
  await other.waitFor(`${shown} === ${JSON.stringify(key)}`, { timeout: 15000 });
  const otherOrigin = await other.evaluate<number>(`performance.timeOrigin`);
  const deaf = await other.holdRequests(["/api/"]);

  // This browser's own key revoked: its next signed request, the reload's, is answered with the wipe, which reloads the
  // page once more, and it comes back as a new device whose key the replica it pulls does not trust.
  await app.page.evaluate(`localStorage.setItem("suzatary.e2e.kept", "1")`);
  atTheTerminal(["device", "revoke", key]);
  await app.page.reload();
  await other.waitFor(`performance.timeOrigin !== ${otherOrigin} && document.readyState === "complete"`, { timeout: 15000 });
  await deaf();
  await other.close();
  await app.page.waitFor(`/^[0-9a-f]{64}$/.test(${shown}) && ${shown} !== ${JSON.stringify(key)}`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<string | null>(`localStorage.getItem("suzatary.e2e.kept")`), null, "its storage went with it");
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".settingskey").textContent.includes("not trusted yet")`), true);

  // Trusted at the terminal while the page stays open: the box moves first and the replica takes the key's object after,
  // and the section says so when the replica does, with no reload.
  const fresh = await app.page.evaluate<string>(shown);
  const origin = await app.page.evaluate<number>(`performance.timeOrigin`);
  atTheTerminal(["device", "trust", fresh]);
  await app.page.waitFor(`document.querySelector(".settingskey").textContent.includes("which the chain trusts")`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<number>(`performance.timeOrigin`), origin, "the page was not loaded again");

  // A wipe a reload cut short leaves its mark, and the next load finishes it before it signs anything: the key it had
  // goes with the storage.
  const cut = await app.page.evaluate<string>(shown);
  await app.page.evaluate(`localStorage.setItem("suzatary.e2e.kept", "1"), localStorage.setItem(${JSON.stringify(WIPING)}, "1")`);
  await app.page.reload();
  await app.page.waitFor(`/^[0-9a-f]{64}$/.test(${shown}) && ![${JSON.stringify(key)}, ${JSON.stringify(cut)}].includes(${shown})`, { timeout: 15000 });
  assert.deepEqual(
    await app.page.evaluate<(string | null)[]>(`["suzatary.e2e.kept", ${JSON.stringify(WIPING)}].map((k) => localStorage.getItem(k))`),
    [null, null],
    "the storage went, mark and all",
  );
});
