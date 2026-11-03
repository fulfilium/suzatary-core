/**
 * THE INBOX, END TO END (`docs/INBOXPRODROME-PLAN-2026-09-17.md` §5 commit 4).
 *
 * A DECISION IS A RECORD, and this file is the browser's own check of that:
 * the real `suzatary serve`, the real fold, a real triage card and a real
 * scripted proposal, so what these scenes read is what the page itself reads.
 *
 * THE SCENES:
 *
 * 1. A triage card appears in the Inbox, tagged as the triage agent's and
 *    drawn as its diff, and the chain has not moved.
 * 2. A viewer reads the Inbox and is offered nothing to press, and the tap
 *    route refuses it.
 * 3. A card is drawn closed, and asking Send opens it whole before the question.
 * 4. Tapping a CHANGESET proposal Confirms it and drops the count — a
 *    changeset, not an email, so it never touches the shared SMTP recorder
 *    `secretary.test.ts` asserts an exact count against.
 * 5. A reason being typed stays in its own card while the card above it goes.
 * 6. A send that failed with nothing sent stays in the Inbox with Try again,
 *    and trying again sends it once and completes it.
 * 7. A send that may have gone through stays in the Inbox with one answer,
 *    "I checked Sent"; a tap asked anyway answers with the record, and after
 *    the check it leaves and still sends nothing.
 * 8. A triage card and a changeset open on their one-line summary, the diff
 *    collapsed; tapping the triage card performs every act on it.
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";

import { fold, PROPOSED_BASE, SIGNED_POST, suite, WRITE_BASE } from "./harness";
import cast from "../test/cast";

const { proposed } = cast.world;

const app = await suite("inbox");

/** The todo the fixture's run proposes adding, and the one it proposes closing. */
const ADDED = proposed.id;
const CLOSED = proposed.id;

/**
 * Open the Inbox and wait until it has drawn something — a card, or the empty
 * note.
 */
// THE PAGE DRAWN AGAIN FROM NOTHING, IN THE SAME TAB: away and back by the hash, never a reload, so whatever a card
// shows after it came from what the tab holds outside the drawing (a card's register), not from the old nodes.
async function drawnAgain(base: string, card: string): Promise<void> {
  await app.page.navigate(`${base}/#/secretary`);
  await app.page.waitFor(`document.querySelector(".inboxpage") === null`, { timeout: 8000 });
  await app.page.navigate(`${base}/#/inbox`);
  await app.page.waitFor(`document.querySelector(${JSON.stringify(card)}) !== null`, { timeout: 15000 });
}

const valueOf = (selector: string): Promise<string> =>
  app.page.evaluate<string>(`document.querySelector(${JSON.stringify(selector)})?.value ?? "(none)"`);

async function openInbox(base: string): Promise<void> {
  await app.page.navigate(`${base}/#/inbox`);
  await app.page.reload();
  await app.page.waitFor(
    `document.querySelector(".inboxpage .proposal, .inboxpage .empty") !== null && document.querySelector('.inboxpage .proposals[data-box="Answered"]') !== null`,
    { timeout: 15000 },
  );
}

/** The waiting count the tab bar's own badge reports — `""` for none waiting,
 * never a number this page computed twice. */
const badge = (): Promise<string> =>
  app.page.evaluate<string | null>(`document.querySelector(".tabbar .tabbadge")?.textContent ?? null`).then(
    (found) => found ?? "",
  );

/**
 * A FRESH DATED CONVERSATION, minted over the route rather than the UI — the
 * same shape `secretary.test.ts`'s own `freshConversation` is, kept here so
 * this file's scenes do not depend on that one's DOM.
 */
async function freshConversation(base: string): Promise<string> {
  await app.page.navigate(`${base}/#/secretary/default`);
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  return app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Untitled"})).then((r) => r.json()).then((j) => j.id)`,
  );
}

/** Open a fresh conversation and say something, waiting for the proposal at
 * `index` to land. */
async function propose(base: string, said: string, index: number): Promise<string> {
  const id = await freshConversation(base);
  await app.page.navigate(`${base}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  await app.page.type(".composebox", said);
  await app.page.click(".composesend");
  await app.page.waitFor(`document.querySelector('.proposal[data-proposal="${index}"]') !== null`, {
    timeout: 20000,
  });
  return id;
}

/** RUN THE AGENT'S WRITER AGAIN, from node. `scripts/web-e2e.sh` names the
 * binary and the fixture's instance file; there is nothing here to fake. */
function rerunTriage(...flags: string[]): { ran: boolean; refused: string } {
  const bin = process.env["E2E_TRIAGE_BIN"];
  const instance = process.env["E2E_TRIAGE_INSTANCE"];
  if (bin === undefined || instance === undefined) {
    throw new Error("E2E_TRIAGE_BIN/E2E_TRIAGE_INSTANCE are unset — scripts/web-e2e.sh sets them");
  }
  const said = execFileSync(bin, ["fixture-triage", "--instance", instance, ...flags], { encoding: "utf8" });
  return JSON.parse(said.trim()) as { ran: boolean; refused: string };
}

/** A route call over `fetch`, in the page — every route-level assertion below
 * asks the box directly rather than through a card, per this file's own note
 * on why scenes 3-5 do. */
async function post(base: string, path: string, body: unknown): Promise<{ status: number; body: unknown }> {
  await app.page.navigate(`${base}/#/`);
  const said = await app.page.evaluate<string>(`(async () => {
    const answer = await ${SIGNED_POST}(${JSON.stringify(path)}, ${JSON.stringify(JSON.stringify(body))});
    return JSON.stringify({ status: answer.status, body: await answer.json() });
  })()`);
  return JSON.parse(said) as { status: number; body: unknown };
}

/** The fixture's triage card: the first the triage agent filed. */
const TRIAGE = `.inboxpage .proposal[data-conversation="triage"]`;

app.scene("a triage card appears in the Inbox, tagged as the agent's and drawn as its diff", async () => {
  await openInbox(PROPOSED_BASE);
  await app.page.waitFor(`document.querySelector('${TRIAGE}') !== null`, { timeout: 10000 });
  assert.equal(await app.page.evaluate<string>(`document.querySelector('${TRIAGE} .proposaltag').textContent`), "Triage");
  // ITS SUMMARY FIRST: the acts in one line.
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${TRIAGE} .diffline').textContent`),
    `add ${proposed.id} · complete ${CLOSED}`,
  );
  await app.page.click(`${TRIAGE} .proposaldetails > summary`);
  await app.page.waitFor(`document.querySelector('${TRIAGE} .proposaldetails').open === true`, { timeout: 4000 });
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll('${TRIAGE} .proposedobject').length`), 2, "one card, two acts");
  const text = await app.page.evaluate<string>(`document.querySelector('${TRIAGE}').textContent`);
  assert.match(text, new RegExp(proposed.body), "the body is shown in full");
  assert.match(text, /the rain did it/, "and so is the note on the completion");

  // AND THE CHAIN HAS NOT MOVED. Nothing here is on it until a tap.
  const entries = await fold(app.page);
  assert.equal(entries.find((entry) => entry.todo === ADDED), undefined, "the added todo is not on the chain");
  assert.equal(await badge(), "1", "one card is waiting, and the bar says so");
});

app.scene("a viewer reads the Inbox and is offered nothing to press", async () => {
  await openInbox(PROPOSED_BASE);
  await app.page.waitFor(`document.querySelector('${TRIAGE}') !== null`, { timeout: 10000 });
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll('${TRIAGE} .act-confirm').length`),
    0,
    "a login that cannot tap is not offered a button that taps",
  );
  // …and the ROUTE refuses it, whatever the page drew.
  const refused = await post(PROPOSED_BASE, "/api/secretary/conversations/triage/proposals/0/tap", {});
  assert.equal(refused.status, 403);
});

// EVERY SCENE FROM HERE SCOPES ITS QUERIES BY `[data-conversation="<id>"]`,
// NEVER BY A BARE COUNT OF `.inboxpage .proposal`. Each `propose` mints its
// own conversation, but WRITE_BASE is shared across this whole file's scenes
// in the order node runs them (`--test-concurrency=1`, one server) — so a
// scene that leaves its own decision standing must not be read by a LATER
// scene's raw total, only by the row that names its own conversation.

// ASKING YES OPENS THE CARD (audit X2): a card is drawn closed, and Send's question is asked with every field on screen.
app.scene("a card is drawn closed, and asking Send opens it whole before the question", async () => {
  const id = await propose(WRITE_BASE, "propose the reply opened by its question", 0);
  await openInbox(WRITE_BASE);
  const card = `.inboxpage .proposal[data-conversation="${id}"]`;
  await app.page.waitFor(`document.querySelector('${card} .act-send') !== null`, { timeout: 10000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${card} .proposaldetails').open`), false);
  assert.deepEqual(
    await app.page.evaluate<string[]>(`[...document.querySelectorAll('${card} .proposalact-row button')].map((b) => b.textContent)`),
    ["Send", "Dismiss", "Reply"],
  );
  await app.page.click(`${card} .act-send`);
  await app.page.waitFor(`document.querySelector('${card} .proposalconfirm') !== null`, { timeout: 4000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${card} .proposaldetails').open`), true, "the message is on screen");
  await app.page.click(`${card} .act-cancelsend`);
  await app.page.waitFor(`document.querySelector('${card} .proposalconfirm') === null`, { timeout: 4000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${card} .proposaldetails').open`), true, "No leaves it as it was");
  const gone = await app.page.evaluate<number>(
    `${SIGNED_POST}("/api/secretary/conversations/${id}/proposals/0/dismiss", JSON.stringify({ reason: "e2e cleanup" })).then((r) => r.status)`,
  );
  assert.equal(gone, 200);
});

// A CHANGESET, NOT AN EMAIL: tapping Confirm applies real chain acts and never
// touches `$SUZATARY_E2E_SENT`, which `secretary.test.ts` asserts an EXACT
// count against — a second sender there would fail a test this file did not
// touch. A tap is a tap whichever act it performs; this one just leaves the
// shared fixture's one recorded-mail count alone.
app.scene("tapping a proposal in the Inbox confirms it and drops the count", async () => {
  const id = await propose(WRITE_BASE, "propose an inbox tap test changeset", 0);
  await openInbox(WRITE_BASE);
  const row = `.inboxpage .proposal[data-conversation="${id}"]`;
  await app.page.click(`${row} .act-confirm`);
  await app.page.waitFor(`document.querySelector(".inboxpage .proposalconfirm") !== null`, { timeout: 4000 });
  // THE CONFIRM STEP AND ITS NOTE OUTLIVE A REDRAW: they are the card's register, not its nodes.
  await app.page.type(`${row} .noteline`, "applied from");
  await drawnAgain(WRITE_BASE, row);
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${row} .proposalconfirm') !== null`), true, "the confirm step survives");
  await app.page.type(`${row} .noteline`, " the Inbox");
  assert.equal(await valueOf(`${row} .noteline`), "applied from the Inbox", "and so do the words, typed on from where they were");
  // The count the page drew just now: the shell's startup badge counts standing proposals only, and may be what
  // showed before the page was drawn again.
  const before = await badge();
  await app.page.click(`${row} .act-confirm.confirming`);
  await app.page.waitFor(`document.querySelector('${row}') === null`, { timeout: 15000 });
  // The card goes with the tap's own register; the badge is drawn when the box's answer lands, a later draw, so
  // the scene waits for the count to move rather than reading it at one instant.
  await app.page
    .waitFor(`(document.querySelector(".tabbar .tabbadge")?.textContent ?? "") !== ${JSON.stringify(before)}`, { timeout: 15000 })
    .catch(() => undefined);
  const after = await badge();
  assert.notEqual(after, before, `tapping settled the decision the proposal made: ${before} -> ${after}`);
  const entries = await fold(app.page);
  assert.ok(entries.some((entry) => entry.todo === "e2einboxtaptest"), "the tap really applied the changeset");
});

app.scene("a reason being typed stays in its own card while the card above it goes", async () => {
  const above = await propose(WRITE_BASE, "propose the reply that moves away", 0);
  const below = await propose(WRITE_BASE, "propose the reply being answered", 0);
  await openInbox(WRITE_BASE);
  const card = (id: string): string => `.inboxpage .proposal[data-conversation="${id}"]`;
  await app.page.waitFor(`document.querySelector('${card(above)}') !== null && document.querySelector('${card(below)}') !== null`, { timeout: 15000 });
  await app.page.click(`${card(below)} .act-dismiss`);
  await app.page.waitFor(`document.querySelector('${card(below)} .dismissreason') !== null`, { timeout: 4000 });
  await app.page.type(`${card(below)} .dismissreason`, "not yet, the");
  // The other card goes from under the one being typed in: the rows the Inbox draws move.
  const gone = await app.page.evaluate<number>(
    `${SIGNED_POST}("/api/secretary/conversations/${above}/proposals/0/dismiss", JSON.stringify({ reason: "e2e reorder" })).then((r) => r.status)`,
  );
  assert.equal(gone, 200);
  await app.page.waitFor(`document.querySelector('${card(above)}') === null`, { timeout: 15000 });
  await app.page.type(`${card(below)} .dismissreason`, " deadline");
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${card(below)} .dismissreason').value`),
    "not yet, the deadline",
    "every character is in the field it was typed into",
  );
  assert.deepEqual(
    await app.page.evaluate<string[]>(`[...document.querySelectorAll('.inboxpage .dismissreason')].map((f) => f.value).filter((v) => v !== "")`),
    ["not yet, the deadline"],
    "and in no other",
  );
  await app.page.click(`${card(below)} .act-dismiss.confirming`);
  await app.page.waitFor(`document.querySelector('${card(below)}') === null`, { timeout: 15000 });
});

// THE RECORDER FAILS ON CUE: `--smtp-fixture <file>` reads `<file>.failures`
// before each send and fails the next one as the first line says (`not_sent`
// or `unknown`), recording nothing. The scenes below count their own subject
// in the recorder rather than its lines, because it is shared.
const SENT = process.env["SUZATARY_E2E_SENT"] ?? "";

const sentWith = (subject: string): number =>
  (existsSync(SENT) ? readFileSync(SENT, "utf8") : "")
    .split("\n")
    .filter(Boolean)
    .filter((line) => (JSON.parse(line) as { subject?: string }).subject === subject).length;

/** Where a conversation's first proposal stands, read as the tab reads it: the inbox's objects through the module. */
async function decisionOf(conversation: string): Promise<string | undefined> {
  const proposals = await app.page.evaluate<{ state: { kind: string }; asked: string }[]>(
    `(async () => {
      const core = await import("/b/dev/prodrome.js");
      await core.default({ module_or_path: "/b/dev/prodrome_bg.wasm" });
      const held = await (await fetch("/api/stores/inbox/objects")).json();
      const inbox = new core.Proposals();
      inbox.piece(JSON.stringify(held), 0);
      inbox.land().free();
      const chains = new core.Chains();
      try {
        return JSON.parse(inbox.inbox(new Date().toISOString().slice(0, 19), chains)).proposals;
      } finally {
        chains.free();
        inbox.free();
      }
    })()`,
  );
  return proposals.find((p) => p.asked === `proposal:${conversation}/0`)?.state.kind;
}

/** Tap an answer on this card and confirm it. */
async function answer(row: string, verb: string): Promise<void> {
  await app.page.click(`${row} .act-${verb}`);
  await app.page.waitFor(`document.querySelector('${row} .proposalconfirm') !== null`, { timeout: 4000 });
  await app.page.click(`${row} .act-${verb}.confirming`);
}

app.scene("a send that sent nothing offers Try again, and trying again sends it once", async () => {
  assert.notEqual(SENT, "", "the suite names a recorder file");
  const id = await propose(WRITE_BASE, "propose the failing reply", 0);
  writeFileSync(`${SENT}.failures`, "not_sent\n");
  await openInbox(WRITE_BASE);
  const row = `.inboxpage .proposal[data-conversation="${id}"]`;
  await answer(row, "send");
  await app.page.waitFor(`document.querySelector('${row}.proposal-failed') !== null`, { timeout: 15000 });

  // STILL WAITING, AND STILL ANSWERABLE: the card says what failed and offers
  // the same send again, and the decision behind it is open.
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${row} .proposaloutcome').textContent`),
    /^Not sent, tried once: /,
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector('${row} .act-send').textContent`),
    "Try again",
  );
  assert.equal(sentWith("The failing reply"), 0, "nothing was recorded");
  assert.equal(await decisionOf(id), "open");

  await answer(row, "send");
  await app.page.waitFor(`document.querySelector('${row}') === null`, { timeout: 15000 });
  assert.equal(sentWith("The failing reply"), 1, "the second tap sent it, once");
  assert.equal(await decisionOf(id), "completed", "sending it closed its decision");
});

app.scene("a send that may have gone stays until he checks Sent, and never sends again", async () => {
  const id = await propose(WRITE_BASE, "propose the uncertain reply", 0);
  writeFileSync(`${SENT}.failures`, "unknown\n");
  await openInbox(WRITE_BASE);
  const row = `.inboxpage .proposal[data-conversation="${id}"]`;
  await answer(row, "send");
  await app.page.waitFor(`document.querySelector('${row}.proposal-unknown .act-acknowledge') !== null`, {
    timeout: 15000,
  });

  // STILL IN FRONT OF HIM, with one answer: never Send again, never Dismiss.
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll('${row} .act-send, ${row} .act-dismiss').length`),
    0,
    "a send that may have gone is never offered again",
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector('${row} .proposaloutcome').textContent`),
    /It may have been sent\. Check Sent; it is never sent again\./,
  );
  assert.equal(await decisionOf(id), "open", "it waits on his check");

  // A TAP ASKED ANYWAY ANSWERS WITH THE RECORD, AND THE RECORDER STAYS EMPTY.
  const tap = (): Promise<string> =>
    app.page.evaluate<string>(
      `${SIGNED_POST}("/api/secretary/conversations/${id}/proposals/0/tap", "{}").then(async (r) => r.status + " " + (await r.json()).proposals[0].state)`,
    );
  assert.equal(await tap(), "200 unknown");

  await app.page.click(`${row} .act-acknowledge`);
  await app.page.waitFor(`document.querySelector('${row}') === null`, { timeout: 15000 });
  assert.equal(await decisionOf(id), "completed", "checked, it leaves the Inbox");
  assert.equal(await tap(), "200 checked");
  assert.equal(sentWith("The uncertain reply"), 0);
});

/** Where the phone-width pictures of a folded and an opened diff go, when a run asks for them. */
const SHOTS = process.env["SUZATARY_DIFF_SHOTS"];
const shoot = async (selector: string, name: string): Promise<void> => {
  if (SHOTS === undefined) return;
  await app.page.evaluate(`document.querySelector('${selector}').scrollIntoView({ block: "center" })`);
  await app.page.screenshot(`${SHOTS}/${name}.png`);
};

app.scene("a triage card and a changeset open on their summary, and a tap performs every act of the card", async () => {
  const HELD = "e2esummarybatch";
  const ran = rerunTriage("--hold", HELD);
  assert.equal(ran.ran, true, `the triage run: ${ran.refused}`);
  const id = await propose(WRITE_BASE, "propose a summary changeset", 0);
  await openInbox(WRITE_BASE);

  const index = await app.page.evaluate<string>(
    `[...document.querySelectorAll('${TRIAGE}')].find((c) => c.textContent.includes("${HELD}"))?.getAttribute("data-proposal") ?? ""`,
  );
  assert.notEqual(index, "", "the triage run is drawn as a card");
  const card = `${TRIAGE}[data-proposal="${index}"]`;
  const changeset = `.inboxpage .proposal[data-conversation="${id}"]`;
  await app.page.waitFor(`document.querySelector('${card} .act-confirm') !== null`, { timeout: 10000 });
  await shoot(card, "triage-folded");
  await shoot(changeset, "changeset-folded");
  const text = (selector: string): Promise<string> =>
    app.page.evaluate<string>(`document.querySelector('${selector}')?.textContent ?? ""`);

  // THE TRIAGE CARD, FOLDED: the acts and the low price said, and Confirm offered.
  assert.equal(await text(`${card} .diffline`), `add ${HELD} · complete ${HELD}`);
  assert.equal(await text(`${card} .diffflag`), `At or under the 45% the review holds: ${HELD} at 40%`);
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${card} .proposaldetails').open`), false, "a card is drawn closed");

  // THE CHANGESET, FOLDED: grouped by act, the low price said, and Confirm still offered.
  assert.equal(await text(`${changeset} .diffline`), "add 2 todos · complete e2esummaryone");
  assert.equal(await text(`${changeset} .diffflag`), "At or under the 45% the review holds: e2esummaryone at 40%");
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${changeset} .proposaldetails').open`), false, "a card is drawn closed");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll('${changeset} .act-confirm').length`), 1);
  await app.page.click(`${changeset} .proposaldetails > summary`);
  await app.page.waitFor(`document.querySelector('${changeset} .proposaldetails').open === true`, { timeout: 4000 });
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll('${changeset} .proposedobject').length`), 3);
  await shoot(changeset, "changeset-open");

  // THE TAP PERFORMS THE WHOLE CARD: the todo is added and completed, as the owner.
  await app.page.click(`${card} .act-confirm`);
  await app.page.waitFor(`document.querySelector('${card} .proposalconfirm') !== null`, { timeout: 4000 });
  await app.page.click(`${card} .act-confirm.confirming`);
  await app.page.waitFor(`document.querySelector('${card}') === null`, { timeout: 15000 });
  const entries = await fold(app.page);
  assert.equal(entries.find((entry) => entry.todo === HELD)?.state, "completed", "both acts are on the chain");

  // Nothing of this scene is left standing for a later file.
  const dismissed = await post(WRITE_BASE, `/api/secretary/conversations/${id}/proposals/0/dismiss`, { reason: "e2e cleanup" });
  assert.equal(dismissed.status, 200, JSON.stringify(dismissed.body));
});
