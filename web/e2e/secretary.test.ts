/**
 * THE SECRETARY, END TO END (docs/ARCHITECTURE.md §10.3).
 *
 * The real `suzatary serve`, the real routes, the real conversation store on
 * disk — and two substitutions, both compiled out of the shipped binary:
 * `--model-fixture` answers a turn from a script instead of running `claude -p`,
 * and `--smtp-fixture` RECORDS what a tap would have sent instead of sending
 * it. Everything between those two is the program: a scripted reply that asks
 * for `propose_email` runs the real handler, writes a real proposal into a real
 * file, and the page draws it from the real route.
 *
 * The claims §10.3 makes that only a page can show, among them:
 *
 * 1. A proposal card shows the EXACT text — every recipient, the subject, the
 *    whole body — because the card is the confirmation CLAUDE.md requires.
 * 2. The writing login's tap is accepted, the fixture's recorder holds exactly
 *    what would have gone out, and a second tap sends nothing.
 * 3. DISMISSING hides it from the Inbox count and shows the reason.
 * 4. A URL in the box's own act turn is a link; in a model's prose it is not.
 * 5. The transcript keeps the reader's scroll and its drawn rounds as answers land.
 *
 * Every scene runs on `E2E_BASE_WRITE`, because each one moves a conversation.
 */

import assert from "node:assert/strict";
import { mkdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { DESKTOP, PHONE, SIGNED_POST, suite, topicRounds, WRITE_BASE } from "./harness";
import cast from "../test/cast";

const { todos, owner, counsel } = cast.world;

const app = await suite("secretary");

/** The subject the script proposes, spelled once so a scene and the recorder
 * are asserting on the same string. */
const SUBJECT = "The deed documents";

/**
 * A FRESH DATED CONVERSATION, minted directly over the route rather than
 * through the UI — `start` (POST) is untouched by the default-conversation
 * series (`docs/ONECHAT-PLAN-2026-09-13.md` §4, §6 commit 7), so this is
 * still the same write every `openConversation` call on the page makes. Doing
 * it over `fetch` rather than a button click keeps every scene's conversation
 * ISOLATED — the point of a dated id in the first place — without this file
 * depending on wherever the "Start a conversation" affordance lives on any
 * given page.
 *
 * ⚠️ LANDS ON `#/secretary/default` FIRST, NEVER THE BARE `#/secretary`:
 * `page.navigate` waits for `location.href` to equal exactly what it asked
 * for (`cdp.ts`'s own contract), and the bare hash now forwards itself away
 * before that can ever be true. `#/secretary/default` is a stable landing —
 * this is only here to give `evaluate` a same-origin document to run the
 * `fetch` from.
 */
async function freshConversation(base: string): Promise<string> {
  await app.page.navigate(`${base}/#/secretary/default`);
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  return app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Untitled"})).then((r) => r.json()).then((j) => j.id)`,
  );
}

/** Open a fresh conversation and say something, waiting for the model's
 * answer. */
async function converse(base: string, said: string): Promise<void> {
  const id = await freshConversation(base);
  await app.page.navigate(`${base}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  await app.page.type(".composebox", said);
  await app.page.click(".composesend");
}

app.scene("a proposal card shows the exact text that would be sent", async () => {
  await converse(WRITE_BASE, "propose the harrow reply");
  await app.page.waitFor(`document.querySelector(".proposal") !== null`, { timeout: 20000 });

  // EVERY FIELD, VERBATIM. This is the confirmation, so a scene that checked
  // only the subject would be checking the half that cannot go to the wrong
  // person.
  //
  // READ BY LABEL, NOT BY POSITION. The first attempt asked for
  // `.proposalfield:nth-of-type(1)` and matched NOTHING: `nth-of-type` counts
  // elements of the same TAG among their siblings, and the card's first div is
  // `.proposalact`, so the first field is the second div. A positional selector
  // would also have gone quietly wrong the day a proposal carried a `Cc`, which
  // the card draws only when it has one. The label is what a person reads, so
  // it is what this reads.
  const card = await app.page.evaluate<string>(
    `JSON.stringify((() => {
       const card = document.querySelector('.proposal');
       const fields = {};
       for (const field of card.querySelectorAll('.proposalfield')) {
         fields[field.querySelector('.proposallabel').textContent] =
           field.querySelector('.proposalvalue').textContent;
       }
       return {
         account: fields['From'],
         to: fields['To'],
         subject: fields['Subject'],
         body: card.querySelector('.proposalbody').textContent,
         state: card.className,
       };
     })())`,
  );
  const shown = JSON.parse(card) as Record<string, string>;
  assert.equal(shown["account"], owner.email);
  assert.equal(shown["to"], counsel.email);
  assert.equal(shown["subject"], "The deed documents");
  assert.match(shown["body"] ?? "", /the last one is scanned/);
  assert.match(shown["state"] ?? "", /proposal-standing/);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".proposal .act-send").length`),
    1,
    "a standing proposal offers exactly one Send",
  );
});

/**
 * DISMISSING IS THE OTHER ANSWER, and it is three facts: the card says so, the
 * Send button is gone, and the INBOX drops its row. The third is the one a
 * unit test cannot see.
 *
 * The Inbox is checked by this proposal's own row, never by how many rows it
 * draws: the box files cards of its own whenever it owes them (a secret that
 * does not resolve, a model that stops answering), and another round can file
 * a proposal meanwhile, so the count moves for reasons that are not this
 * dismissal. The scene files one such proposal from a second conversation
 * while this one is answered, and its row stays.
 */
app.scene("dismissing a proposal drops it from the Inbox and shows the reason", async () => {
  await converse(WRITE_BASE, "propose the reply to dismiss");
  await app.page.waitFor(`document.querySelector(".proposal .act-dismiss") !== null`, {
    timeout: 20000,
  });
  const conversation = await app.page.evaluate<string>(`location.hash`);
  const key = await app.page.evaluate<string>(`document.querySelector(".proposal").dataset.key`);
  const row = (k: string): string => `document.querySelector('.inboxpage .proposal[data-key="${k}"]') !== null`;

  // The Inbox holds it while it stands.
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  await app.page.waitFor(row(key), { timeout: 15000 });

  // A second writer: another conversation's round files a proposal of its own.
  const other = await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Untitled"})).then((r) => r.json()).then((j) => j.id)`,
  );
  assert.equal(
    await app.page.evaluate<number>(
      `${SIGNED_POST}("/api/secretary/conversations/${other}/say", JSON.stringify({say: "propose another reply meanwhile"})).then((r) => r.status)`,
    ),
    202,
  );

  await app.page.navigate(`${WRITE_BASE}/${conversation}`);
  await app.page.waitFor(`document.querySelector(".proposal .act-dismiss") !== null`, {
    timeout: 15000,
  });
  await app.page.click(".proposal .act-dismiss");
  // IT ASKS FOR THE REASON RATHER THAN FOR A CONFIRMATION: the reason is what
  // the next answer can use, and an empty one is refused before the request.
  await app.page.waitFor(`document.querySelector(".dismissreason") !== null`, { timeout: 4000 });
  await app.page.click(".proposal .act-dismiss.confirming");
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".proposal-dismissed").length`),
    0,
    "a dismissal with no reason is not sent",
  );
  await app.page.type(".dismissreason", "wrong person, this goes to Mira");
  await app.page.click(".proposal .act-dismiss.confirming");
  await app.page.waitFor(`document.querySelector(".proposal-dismissed") !== null`, {
    timeout: 15000,
  });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".proposaloutcome").textContent`),
    /^Dismissed: wrong person, this goes to Mira$/,
  );
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".proposal .act-send").length`),
    0,
    "an answered proposal offers no Send",
  );
  // AND THE SECRETARY SEES IT. The transcript carries an act turn, which is
  // what the next round reads. The box writes the act line after the inbox
  // records the dismissal, and the tab pulls the two stores apart, so the
  // card can read Dismissed a moment before the turn is drawn.
  await app.page.waitFor(`${said("act")}.includes("dismissed it: wrong person")`, { timeout: 15000 });

  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  await app.page.waitFor(row(`${other}/0`), { timeout: 20000 });
  await app.page.waitFor(`!(${row(key)})`, { timeout: 15000 });
});

/**
 * A CHANGESET CARD IS A DIFF (§10.3, "Changesets"), and it offers Confirm,
 * Dismiss and Reply — the same three answers a proposed email gets, drawn with
 * the same row a triage card's diff uses. Confirming APPLIES it: this is the
 * one scene that checks the acts actually land on the chain.
 */
app.scene("a changeset card shows the diff, and Confirm applies it", async () => {
  await converse(WRITE_BASE, "propose a test changeset");
  await app.page.waitFor(`document.querySelector(".proposal") !== null`, { timeout: 20000 });

  // ITS SUMMARY FIRST, the rows folded under the card's Details.
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposal .diffline").textContent`),
    "add e2echangesettest · complete e2echangesettest",
  );
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".proposal .proposaldetails").open`), false);
  await app.page.click(".proposal .proposaldetails > summary");
  await app.page.waitFor(`document.querySelector(".proposal .proposaldetails").open === true`, { timeout: 4000 });

  // THE DIFF: one row per act, an add and a complete.
  const rows = await app.page.evaluate<string>(
    `JSON.stringify([...document.querySelectorAll(".proposal .proposedobject")].map((row) => row.className))`,
  );
  const parsed = JSON.parse(rows) as string[];
  assert.equal(parsed.length, 2, `an add and a complete: ${rows}`);
  assert.ok(parsed.some((cls) => cls.includes("act-added")), rows);
  assert.ok(parsed.some((cls) => cls.includes("act-completed")), rows);

  // THE THREE ANSWERS: Confirm, Dismiss, Reply — no Send, which is the email
  // arm's word, and the one no every card says.
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".proposal .act-confirm").length`),
    1,
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposal .act-dismiss").textContent`),
    "Dismiss",
  );
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".proposal .act-replyto").length`),
    1,
  );
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".proposal .act-send").length`),
    0,
    "a changeset offers no Send",
  );

  await app.page.click(".proposal .act-confirm");
  await app.page.waitFor(`document.querySelector(".proposalconfirm") !== null`, { timeout: 4000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".proposalconfirm").textContent`),
    /Apply these 2 changes\?/,
  );
  await app.page.click(".proposal .act-confirm.confirming");
  await app.page.waitFor(`document.querySelector(".proposal-applied") !== null`, { timeout: 15000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".proposaloutcome").textContent`),
    /^Applied: 2 changes$/,
  );

  // AND BOTH ACTS ARE ON THE CHAIN.
  await app.page.navigate(`${WRITE_BASE}/#/todo/e2echangesettest`);
  await app.page.waitFor(`document.querySelector(".todoid") !== null`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".todoid").textContent`),
    "e2echangesettest",
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".chip.done")?.textContent ?? ""`),
    /completed/i,
  );
});

/**
 * A PROPOSED ACT, PERFORMED (`docs/SECRETARYACTS-PLAN-2026-09-14.md`, §6
 * commit 10): the box's own counterpart to a proposed email — the card names
 * the account and offers Run, with the same two-press confirm every other
 * proposal uses, and the sentence it shows is `Act::sentence`'s own words on
 * the wire (`wire::ProposedAct::Auth.sentence`), never a second composition
 * of them.
 *
 * `--model-fixture` now answers with `Acting::Recording` rather than
 * `Acting::NotYet` (commit 10 of the series): a tap performs the act on NO
 * THREAD AT ALL, so by the time the tap's own response lands the wire already
 * reports `acted` — no polling, no race. The card draws the `Done:` state and
 * the transcript carries the two act turns `begin_recording` wrote: the
 * announced sign-in sentence, whose `https://microsoft.com/devicelogin` is a
 * link because it is an ACT turn (`refs.ts`'s own rule, commit 6), and the
 * completion sentence.
 */
app.scene("a proposed act shows the account, and Run performs it in one round trip", async () => {
  await converse(WRITE_BASE, "propose an act");
  await app.page.waitFor(`document.querySelector(".proposal") !== null`, { timeout: 20000 });

  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposalact").textContent`),
    "Proposed act",
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposalbody").textContent`),
    `Authorise ${owner.email} to sync mail (IMAP).`,
  );
  assert.equal(
    await app.page.evaluate<string>(
      `[...document.querySelectorAll('.proposalfield')].find((f) => f.querySelector('.proposallabel').textContent === 'Account')?.querySelector('.proposalvalue').textContent ?? ''`,
    ),
    owner.email,
  );
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".proposal .act-run").length`),
    1,
    "a standing act proposal offers exactly one Run",
  );

  await app.page.click(".proposal .act-run");
  await app.page.waitFor(`document.querySelector(".proposalconfirm") !== null`, { timeout: 4000 });
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposalconfirm").textContent`),
    `Authorise ${owner.email} to sync mail (IMAP)?`,
  );
  await app.page.click(".proposal .act-run.confirming");
  await app.page.waitFor(`document.querySelector(".proposal-acted") !== null`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposaloutcome").textContent`),
    `Done: imap grant stored for ${owner.email}`,
  );

  // TWO OF THE BOX'S LINES, THE FIRST LINKED AND THE SECOND PLAIN PROSE (drawn as the box's, never as the owner's act). The outcome rides the inbox's store and the turn the
  // conversations', synced apart, so the card may say it is done before the completion turn has landed.
  await app.page.waitFor(`document.querySelectorAll(".turn-box").length >= 2`, { timeout: 15000 });
  const actTurns = await app.page.evaluate<number>(`document.querySelectorAll(".turn-box").length`);
  assert.equal(actTurns, 2, "the announced turn and the completion turn");
  assert.equal(
    await app.page.evaluate<string>(
      `document.querySelectorAll(".turn-box")[0].querySelector(".turntext a.actlink")?.getAttribute("href") ?? ""`,
    ),
    "https://microsoft.com/devicelogin",
    "the sign-in URL is linked because it is the box's line",
  );
  assert.match(
    await app.page.evaluate<string>(
      `document.querySelectorAll(".turn-box")[0].querySelector(".turntext").textContent`,
    ),
    /ABCD-EFGH/,
  );
  assert.equal(
    await app.page.evaluate<string>(
      `document.querySelectorAll(".turn-box")[1].querySelector(".turntext").textContent`,
    ),
    `imap grant stored for ${owner.email}`,
  );
});

/**
 * A MODEL TURN'S URL IS NEVER LINKED — `refs.ts`'s negative half of the
 * act-turn rule (commit 6), checked here in a real browser rather than only
 * under node: an `https://` token in the box's OWN act turns is a link
 * because the box wrote it, and the same token in a model's prose is not,
 * because a model has just read attacker-controlled mail.
 */
app.scene("a model turn's URL stays plain text, never a link", async () => {
  await converse(WRITE_BASE, "give me a link");
  await app.page.waitFor(
    `[...document.querySelectorAll(".turn-model")].some((t) => t.textContent.includes("https://"))`,
    { timeout: 15000 },
  );
  const modelTurnText = await app.page.evaluate<string>(
    `[...document.querySelectorAll(".turn-model")].find((t) => t.textContent.includes("https://")).textContent`,
  );
  assert.match(modelTurnText, /https:\/\/example\.test\/status/);
  const linkedInAModelTurn = await app.page.evaluate<boolean>(
    `[...document.querySelectorAll(".turn-model")].some((t) => t.querySelector("a.actlink") !== null)`,
  );
  assert.equal(linkedInAModelTurn, false, "a model turn's URL must not be an anchor");
});

app.scene("the tap is accepted for the writing login, and records what would go", async () => {
  await converse(WRITE_BASE, "propose the harrow reply");
  await app.page.waitFor(`document.querySelector(".proposal .act-send") !== null`, {
    timeout: 20000,
  });
  await app.page.click(".proposal .act-send");
  // THE CONFIRM STEP NAMES THE RECIPIENTS. A generic "are you sure" is a prompt
  // people press through; this one says who it goes to.
  await app.page.waitFor(`document.querySelector(".proposalconfirm") !== null`, { timeout: 4000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".proposalconfirm").textContent`),
    new RegExp(counsel.email.replace(/\./g, "\\.")),
  );
  await app.page.click(".proposal .act-send.confirming");
  await app.page.waitFor(`document.querySelector(".proposal-sent") !== null`, { timeout: 15000 });

  // NOTHING WAS SENT, AND THE RECORDER IS THE EVIDENCE. `$SUZATARY_E2E_SENT` is
  // the file the fixture postman appends to — one JSON line per message that
  // WOULD have gone — so this asserts on the message rather than on the absence
  // of one.
  const recorded = process.env["SUZATARY_E2E_SENT"] ?? "";
  assert.notEqual(recorded, "", "the suite names a recorder file");
  // COUNTED BY SUBJECT: the recorder is shared, and the Inbox's retry scenes
  // record their own messages under subjects of their own.
  const lines = readFileSync(recorded, "utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .filter((line) => (JSON.parse(line) as { subject?: string }).subject === SUBJECT);
  assert.equal(lines.length, 1, "exactly one message would have gone");
  const sent = JSON.parse(lines[0] ?? "{}") as Record<string, unknown>;
  assert.deepEqual(sent["to"], [counsel.email]);
  assert.equal(sent["subject"], SUBJECT);
  assert.match(String(sent["body"]), /the last one is scanned/);

  // AND A SECOND TAP SENDS NOTHING. The inbox's fold holds the completion, so
  // `submit` answers with the recorded response and never runs the send again.
  const again = await app.page.evaluate<string>(
    `${SIGNED_POST}(location.hash.replace("#/secretary/", "/api/secretary/conversations/") + "/proposals/0/tap", "{}")
       .then(async (r) => r.status + " " + (await r.json()).proposals[0].state)`,
  );
  assert.equal(again, "200 sent", "a second tap answers with what the first one did");
  assert.equal(
    readFileSync(recorded, "utf8")
      .split("\n")
      .filter(Boolean)
      .filter((line) => (JSON.parse(line) as { subject?: string }).subject === SUBJECT).length,
    1,
    "and no second email",
  );
});

app.scene("a tap whose answer is lost shows what the box did, not a frozen card", async () => {
  const lost = "The reply whose answer is lost";
  await converse(WRITE_BASE, "propose the reply whose answer is lost");
  await app.page.waitFor(`document.querySelector(".proposal-standing .act-send") !== null`, { timeout: 20000 });
  const conversation = (await app.page.evaluate<string>("location.hash")).replace("#/secretary/", "").split("?")[0] ?? "";
  const card = `.proposal[data-conversation="${conversation}"][data-proposal="0"]`;
  // On the Inbox nothing redraws a card but its own answer, so a lost answer froze it for good.
  // The tap reaches the box and its answer is lost on the way back, as on a phone whose network blinked.
  await app.page.loseResponses(["/tap"]);
  try {
    await app.page.navigate(`${WRITE_BASE}/#/inbox`);
    await app.page.waitFor(`document.querySelector(${JSON.stringify(`${card} .act-send`)}) !== null`, { timeout: 20000 });
    await app.page.click(`${card} .act-send`);
    await app.page.waitFor(`document.querySelector(".proposalconfirm") !== null`, { timeout: 4000 });
    // A toast keeps its text after it hides, and an earlier scene's "Sent to …" would satisfy the wait.
    await app.page.evaluate<boolean>(`(document.querySelector("#toast").textContent = "", true)`);
    await app.page.click(`${card} .act-send.confirming`);
    // "Sent to …" is the box's record read back; the answer the page lost would have said "sent to …".
    await app.page.waitFor(
      `(document.querySelector("#toast")?.textContent ?? "").startsWith("Sent to counsel@example.test")`,
      { timeout: 15000 },
    );
  } finally {
    await app.page.resumeRequests();
  }
  // The Inbox reloads after the toast, so the card leaves a moment later.
  await app.page.waitFor(`document.querySelector(${JSON.stringify(`${card}.proposal-standing`)}) === null`, {
    timeout: 8000,
  });
  const recorded = readFileSync(process.env["SUZATARY_E2E_SENT"] ?? "", "utf8")
    .split("\n")
    .filter(Boolean)
    .filter((line) => (JSON.parse(line) as { subject?: string }).subject === lost);
  assert.equal(recorded.length, 1, "it went once");
});


/**
 * A FINGER'S TARGETS ARE 44 PX TALL (the 2026-10-13 audit, X19): the brand, a history's hash links, and an answer's
 * Copy and Reply, which were links and buttons the height of their small words.
 */
app.scene("the brand, a history's hash links and an answer's Copy, Reply and its menu are each 44 px tall", async () => {
  await app.resize(PHONE);
  const heights = (selector: string): string =>
    `[...document.querySelectorAll(${JSON.stringify(selector)})].map((n) => n.getBoundingClientRect().height)`;
  const tall = async (selector: string): Promise<void> => {
    await app.page.waitFor(`document.querySelector(${JSON.stringify(selector)}) !== null`, { timeout: 15000 });
    const short = (await app.page.evaluate<number[]>(heights(selector))).filter((h) => h < 44);
    assert.deepEqual(short, [], `${selector}: every one is at least 44 px tall`);
  };
  await converse(WRITE_BASE, "give me a rendered answer");
  // Copy is behind the answer's "…", so the menu is opened and its items measured with it.
  await tall(".turn-model .turnmorebutton");
  await tall(".turnreply");
  await app.page.click(".turn-model .turnmorebutton");
  await app.page.waitFor(`document.querySelector(".turn-model .turnmore[open] .copyanswer") !== null`, { timeout: 15000 });
  await tall(".turn-model .turnmore[open] .copyanswer");
  await tall("header .brand");
  // The brand reaches its height in padding the header gives back, so the phone's header is no taller for it.
  assert.equal(
    await app.page.evaluate<number>(`Math.round(document.querySelector("header").getBoundingClientRect().height)`),
    32,
    "the phone header keeps its own 32 px",
  );
  await app.page.navigate(`${WRITE_BASE}/#/todo/${todos.urgent.id}`);
  // The history is closed until asked for; its hash links are measured open.
  await app.page.waitFor(`document.querySelector(".todoview .history") !== null`, { timeout: 15000 });
  if (!(await app.page.evaluate<boolean>(`document.querySelector(".todoview .history").open`))) await app.page.click(".todoview .history > summary");
  await app.page.waitFor(`document.querySelector(".todoview .history").open === true`);
  await tall(".timeline .hashlink");
});
/**
 * STREAMING NEVER SCROLLS A DRIFTED-UP BOX (`docs/CHAT-PLAN-2026-09-25.md`
 * §4.9, §5). The reader scrolls up; a question sent from elsewhere is
 * answered while they read, the place fills and the answer lands — and the
 * box stays exactly where they left it, the pill offering the way down.
 * (The fixture model answers whole, so what grows here is the place and the
 * landed round; `web/test/anchor.test.ts` pins the same law over every event.)
 */
app.scene("streaming never scrolls a drifted-up box", async () => {
  const id = await freshConversation(WRITE_BASE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  for (const n of [1, 2, 3, 4, 5]) {
    await app.page.type(".composebox", `give me a rendered answer ${n}`);
    await app.page.click(".composesend");
    await app.page.waitFor(`document.querySelectorAll(".turnanswer").length === ${n}`, { timeout: 20000 });
  }
  // THE SIXTH SEND SETTLES AFTER THE READER'S WHEEL, on a busy page. Its answer is drawn from the lines' pull, and the
  // send settles after the inbox's, which is held; the page's thread is kept busy while the wheel arrives, so the
  // compositor scrolls before the page hears the wheel, and the settled send then asks to place its question. That
  // order is what a loaded machine makes; here it is made on purpose.
  const settling = await app.page.holdRequests(["/api/stores/inbox/"]);
  await app.page.type(".composebox", "give me a rendered answer 6");
  await app.page.click(".composesend");
  await app.page.waitFor(`document.querySelectorAll(".turnanswer").length === 6`, { timeout: 20000 });
  const scroller = `(document.scrollingElement ?? document.documentElement)`;
  await app.page.evaluate(`void setTimeout(() => { const until = performance.now() + 400; while (performance.now() < until) {} }, 0)`);
  // The reader's own wheel: the box moved by anything else is not the reader drifting.
  await app.page.wheel(-1000000);
  await settling();
  await app.page.waitFor(`${scroller}.scrollTop === 0`);
  await app.page.scrollSettled();
  const before = await app.page.evaluate<number>(`${scroller}.scrollTop`);
  await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations/${id}/say", JSON.stringify({say: "give me a rendered answer"})).then((r) => String(r.status))`,
  );
  await app.page.waitFor(`document.querySelectorAll(".turnanswer").length === 7`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<number>(`${scroller}.scrollTop`), before, "the box did not move");
  // The answer is placed in the frame after it is drawn, and the pill with it.
  await app.page.waitFor(`document.querySelector(".newanswerpill").hidden === false`);
  assert.equal(await app.page.evaluate<number>(`${scroller}.scrollTop`), before, "the pill offers the way down instead, and the box stays");
});

/** The last question asked, its round at the top of the box just below the sticky header: held there by its send (`anchor.ts`, `ToTurn`). */
const QUESTION_AT_TOP = `Math.abs([...document.querySelectorAll(".round")].findLast((r) => r.querySelector(".turn-person") !== null).getBoundingClientRect().top - document.querySelector("body > header").getBoundingClientRect().bottom) <= 1`;

/**
 * THE PAGE MOVING THE BOX SETTLES NOTHING; THE READER'S SCROLL DOES. A question just sent is held at the top with
 * room beneath it. Content drawn above it (an older round, a banner) makes the browser anchor the box, and that
 * scroll is the page's: the question stays held. The reader's own scroll up past a screen above what the box shows
 * lets it go, and the room beneath goes with it: the reader stays drifted, offered the pill when an answer lands, never
 * pulled down.
 *
 * ⚠️ THE PAGE ANCHORS THE BOX AGAIN RIGHT AFTER THE WHEEL. The transcript counts a scroll within a moment of the
 * reader's input as theirs (`READER_MS`), so a scroll the page causes then, such as a block's HTML replacing its
 * source above, is counted as the reader's. The scene makes that happen on every run.
 */
app.scene("content drawn above a held question keeps it held, and the reader's scroll up drifts for good", async () => {
  const id = await freshConversation(WRITE_BASE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  for (const n of [1, 2, 3]) {
    await app.page.type(".composebox", `give me a rendered answer ${n}`);
    await app.page.click(".composesend");
    await app.page.waitFor(`document.querySelectorAll(".turnanswer").length === ${n}`, { timeout: 20000 });
  }
  // The third question is held once its send settles, which on a loaded box can be after its answer is drawn.
  await app.page.waitFor(QUESTION_AT_TOP, { timeout: 20000 });
  const held = `document.querySelector(".answerroom").hidden === false`;
  assert.equal(await app.page.evaluate<boolean>(held), true, "the question just sent has room beneath it");
  await app.page.scrollSettled();
  await app.page.evaluate<void>(`void document.querySelector(".turns").prepend(Object.assign(document.createElement("div"), { style: "height: 2000px" }))`);
  await app.page.scrollSettled();
  assert.equal(await app.page.evaluate<boolean>(held), true, "content drawn above keeps the question held");
  // Two and a half screens up: past a screen above what the box shows, however short the answer under the question.
  await app.page.wheel(-Math.round((await app.page.evaluate<number>("innerHeight")) * 2.5));
  await app.page.evaluate<void>(`void document.querySelector(".turns").prepend(Object.assign(document.createElement("div"), { style: "height: 50px" }))`);
  await app.page.waitFor(`document.querySelector(".answerroom").hidden === true`, { timeout: 8000 });
  await app.page.scrollSettled();
  // Where the held question's round stands on the screen, which moves as everything the reader sees moves. Not the
  // scroll offset: a bar above that comes or goes moves the offset, and the browser's anchoring keeps the view still.
  const shown = `document.querySelectorAll(".round")[2].getBoundingClientRect().top`;
  const before = await app.page.evaluate<number>(shown);
  await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations/${id}/say", JSON.stringify({say: "give me a rendered answer"})).then((r) => String(r.status))`,
  );
  await app.page.waitFor(`document.querySelectorAll(".turnanswer").length === 4`, { timeout: 20000 });
  await app.page.waitFor(`document.querySelector(".newanswerpill").hidden === false`, { timeout: 8000 });
  // Within a pixel: the scroll offset is whole pixels, the layout is not.
  const after = await app.page.evaluate<number>(shown);
  assert.ok(Math.abs(after - before) <= 1, `the reader stays where they went: ${before} then ${after}`);
});

/**
 * THE READER'S PLACE IS READ WITHOUT THE ROOM. A question held at the top has a window of empty room beneath it, and
 * the room goes as soon as the reader scrolls. The reader here scrolls up to a quarter of a screen above what the box
 * shows. With the room counted, that place is more than a screen above the box's end. Without it, the place is
 * within a screen, which is the bottom, so the hold goes to the newest and the next answer brings no pill.
 */
app.scene("a reader's scroll up from a held question is read against what the box shows, not the room under it", async () => {
  const id = await freshConversation(WRITE_BASE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  for (const n of [1, 2, 3]) {
    await app.page.type(".composebox", `give me a rendered answer ${n}`);
    await app.page.click(".composesend");
    await app.page.waitFor(`document.querySelectorAll(".turnanswer").length === ${n}`, { timeout: 20000 });
  }
  await app.page.waitFor(QUESTION_AT_TOP, { timeout: 20000 });
  await app.page.scrollSettled();
  const scroller = `(document.scrollingElement ?? document.documentElement)`;
  // How far the reader is above what the box shows (negative while the room reaches past it), and the wheel that
  // leaves them a quarter of a screen above it.
  const shows = `(${scroller}.scrollHeight - document.querySelector(".answerroom").offsetHeight - innerHeight - ${scroller}.scrollTop)`;
  const wheel = await app.page.evaluate<number>(`Math.round(innerHeight / 4 - ${shows})`);
  await app.page.wheel(-wheel);
  await app.page.waitFor(`document.querySelector(".answerroom").hidden === true`, { timeout: 8000 });
  await app.page.scrollSettled();
  const above = await app.page.evaluate<number>(shows);
  const height = await app.page.evaluate<number>("innerHeight");
  assert.ok(above > 0 && above <= height, `the reader is within a screen of what the box shows (${above} of ${height})`);
  await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations/${id}/say", JSON.stringify({say: "give me a rendered answer"})).then((r) => String(r.status))`,
  );
  // The pill is drawn in the same step that reads the window, so it is decided once the answer is on the page.
  await app.page.waitFor(`document.querySelectorAll(".turnanswer").length === 4`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".newanswerpill").hidden`), true, "no pill: the reader is with the newest");
});

/**
 * A FINISHED MODEL TURN, IN BLOCKS (`docs/CHAT-PLAN-2026-09-25.md` §4.5). A
 * heading, a paragraph and a `#panel` come back as three blocks of typst's
 * HTML, parsed by `rich.ts` alone, and the raw text stays underneath for
 * copy/select.
 */
app.scene("a heading, a paragraph and a panel render as three blocks, with the source in a details", async () => {
  await converse(WRITE_BASE, "give me a rendered answer");
  await app.page.waitFor(`document.querySelectorAll(".turnanswer .answerblock.rich").length === 3`, { timeout: 20000 });

  const blocks = await app.page.evaluate<{ kind: string; tag: string; text: string }[]>(
    `[...document.querySelectorAll(".turnanswer .answerblock")].map((b) => ({
       kind: b.dataset.kind, tag: b.firstElementChild?.tagName ?? "", text: b.textContent ?? "" }))`,
  );
  assert.deepEqual(
    blocks.map((b) => [b.kind, b.tag]),
    [["heading", "H2"], ["prose", "P"], ["prose", "BLOCKQUOTE"]],
    "typst's own elements, one block each",
  );
  assert.match(blocks[0]?.text ?? "", /A rendered answer/);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".turnanswer strong").length`),
    1,
    "*typst* is typst's <strong>, not asterisks",
  );

  const details = await app.page.evaluate<{ open: boolean; text: string }>(
    `(() => {
       const details = document.querySelector(".turnsource");
       return { open: details.open, text: details.querySelector(".turntext").textContent ?? "" };
     })()`,
  );
  assert.equal(details.open, false, "closed by default — the blocks are the reading, the text is for copy");
  assert.match(details.text, /#panel/);
});

/**
 * THE TRANSCRIPT AS A PLACE (`docs/CHATSHAPE-PLAN-2026-09-15.md` §4.4, §6
 * commit 6): a second round landing appends a new one rather than rebuilding
 * every round already drawn. The first round's own answer is marked with a
 * property set directly on the live DOM node, so finding it still there after
 * the second answer lands is evidence the node itself survived.
 */
app.scene("a second message lands without rebuilding the first round's answer", async () => {
  const id = await freshConversation(WRITE_BASE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });

  await app.page.type(".composebox", "give me a rendered answer");
  await app.page.click(".composesend");
  await app.page.waitFor(`document.querySelector(".turnanswer") !== null`, { timeout: 20000 });

  await app.page.evaluate<void>(`document.querySelector(".turnanswer").__e2eFirstRoundAnswer = true`);

  await app.page.type(".composebox", "what about that todo");
  await app.page.click(".composesend");
  await app.page.waitFor(
    `[...document.querySelectorAll(".turn-model .turntext")].some((t) => t.textContent.includes("still open"))`,
    { timeout: 20000 },
  );

  const still = await app.page.evaluate<{ count: number; marked: boolean }>(
    `(() => {
       const answers = document.querySelectorAll(".turnanswer");
       return { count: answers.length, marked: answers[0]?.__e2eFirstRoundAnswer === true };
     })()`,
  );
  assert.equal(still.count, 2, "two rounds, two answers");
  assert.equal(still.marked, true, "the FIRST is the SAME node — a redraw would have lost the marker");
});

/**
 * THE BOX EMPTIES WHEN A QUESTION LEAVES, NOT WHEN THE BOX ANSWERS IT (WEB-SPEC §3.6). The first `say` is held at
 * the network, so the next question is typed while the first send has not settled; let through, the first is answered
 * and read again, and the next question is still in the box, where Send finds it. Before, the box was emptied once
 * the send settled, wiping what had been typed since: the flake both scenes above it timed out on.
 */
app.scene("the next question typed before a send settles stays in the box, and both are sent", async () => {
  const id = await freshConversation(WRITE_BASE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  const box = `document.querySelector(".composebox").value`;
  const release = await app.page.holdRequests([`${id}/say`]);
  await app.page.type(".composebox", "give me a rendered answer");
  await app.page.click(".composesend");
  await app.page.waitFor(`${box} === ""`, { timeout: 4000 });
  await app.page.type(".composebox", "what about that todo");
  await release();
  await app.page.waitFor(`document.querySelector(".turnanswer") !== null`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<string>(box), "what about that todo", "the first send settled and left the next question");
  await app.page.click(".composesend");
  await app.page.waitFor(
    `[...document.querySelectorAll(".turn-model .turntext")].some((t) => t.textContent.includes("still open"))`,
    { timeout: 20000 },
  );
  assert.deepEqual(
    await app.page.evaluate<string[]>(`[...document.querySelectorAll(".turn-person .turntext")].map((t) => t.textContent ?? "")`),
    ["give me a rendered answer", "what about that todo"],
    "both were sent, in the order typed",
  );
});

/**
 * THE SECRETARY AS A LAYER, ON A LAPTOP: at 62rem and up the panel stands
 * beside the todo list, showing the whole conversation when nothing is
 * selected, and a writer sends from it.
 */
app.scene("the panel is present on the todo list at 62rem and shows the whole conversation", async () => {
  await app.resize(DESKTOP);
  await app.page.navigate(`${WRITE_BASE}/#/`);
  await app.page.waitFor(`document.querySelector(".secretarypanel") !== null`, { timeout: 15000 });
  // THE HEADER IS DRAWN FROM THE SOURCE'S FIRST WINDOW (§9.3's fallback needs
  // to know whether it fired), not at mount — so this waits for TEXT, not
  // merely for the element to exist.
  await app.page.waitFor(
    `(document.querySelector(".secretarypanel .frameheader")?.textContent ?? "") !== ""`,
    { timeout: 15000 },
  );
  const header = await app.page.evaluate<string>(
    `document.querySelector(".secretarypanel .frameheader").textContent`,
  );
  assert.equal(/About /.test(header), false, "nothing is selected, so the panel is the whole conversation");
  assert.match(header, /Whole conversation/);
  // Open it if nothing in it has opened it yet: an unused panel is its toggle alone.
  if ((await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "false") {
    await app.page.click(".panelcollapse");
  }
  // THE PANEL SCROLLS ITSELF, NEVER THE PAGE UNDER IT (2026-09-16, the owner:
  // "Todos page is landing me at the bottom not top"): the conversation's
  // first draw jumps to its newest turn, and in a frame that jump has to
  // land in the frame's own scroll box. The list behind it stays at the top.
  await app.page.waitFor(`document.querySelector(".secretarypanel .turns") !== null`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<number>(`document.scrollingElement.scrollTop`),
    0,
    "mounting the panel must not scroll the todo list",
  );
  // THE COMPOSER IS PINNED, OUTSIDE THE SCROLLING BOX, SO IT IS ALWAYS ON
  // SCREEN (§9.3, §9.4 commit 15) — the same symptom as the sheet's, one
  // frame over: a composer inside `.panelscroll` would need scrolling down
  // past the transcript before "tell it something" was reachable at all.
  await app.page.waitFor(`document.querySelector(".secretarypanel .composebox") !== null`, { timeout: 15000 });
  const composeRect = await app.page.evaluate<{ top: number; bottom: number }>(
    `(() => { const r = document.querySelector(".secretarypanel .composebox").getBoundingClientRect(); return { top: r.top, bottom: r.bottom }; })()`,
  );
  assert.ok(composeRect.top >= 0 && composeRect.bottom <= DESKTOP[1], "the panel's composer is inside the viewport");

  // A WRITER SENDS FROM IT, by Send and by Enter (a bare Enter sends on a wide screen), and stays able to send.
  await sendsFrom(".secretarypanel", "said in the panel by Send", "button");
  await sendsFrom(".secretarypanel", "said in the panel by Enter", "Enter");

  // BELOW 62rem THE PANEL IS OUT OF THE DOCUMENT, not hidden: a hidden panel
  // held the default conversation's turns, and every phone scene that asked
  // the page for `.turnanswer` or `.turn-act` counted them once an earlier
  // file had written there.
  await app.resize(PHONE);
  await app.page.waitFor(`document.querySelector(".secretarypanel") === null`, { timeout: 8000 });
});

/**
 * THE CONVERSATION PAGE IS THE DOCUMENT'S SCROLL BOX, and a sent question is held at its top with a window's room
 * beneath it: at both widths the composer still sits on screen after each send, above the phone's tab bar, and Send is
 * what a press there meets.
 */
app.scene("a writer sends from the conversation page at desktop and phone width, and the composer stays on screen", async () => {
  for (const size of [DESKTOP, PHONE]) {
    await app.resize(size);
    const id = await freshConversation(WRITE_BASE);
    await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
    await app.page.waitFor(`document.querySelector("#view .composebox") !== null`, { timeout: 8000 });
    assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".secretarypanel").length`), 0, "no second secretary beside its own page");
    await sendsFrom("#view", `said on the page by Send at ${size[0]}`, "button");
    // A bare Enter is a new line on a phone; the chord sends at every width.
    await sendsFrom("#view", `said on the page by Enter at ${size[0]}`, size === PHONE ? "Ctrl+Enter" : "Enter");
  }
});

/**
 * One send from a composer under `scope`: the person's turn comes back in that transcript and the box is empty again,
 * and Send is still the element at its own centre, inside the window.
 */
async function sendsFrom(scope: string, said: string, by: "button" | "Enter" | "Ctrl+Enter"): Promise<void> {
  await app.page.type(`${scope} .composebox`, said);
  if (by === "button") await app.page.click(`${scope} .composesend`);
  else await app.page.press("Enter", { ctrl: by === "Ctrl+Enter" });
  await app.page.waitFor(
    `[...document.querySelectorAll(${JSON.stringify(`${scope} .turn-person .turntext`)})].some((t) => t.textContent.includes(${JSON.stringify(said)}))`,
    { timeout: 15000 },
  );
  assert.equal(await app.page.evaluate<string>(`document.querySelector(${JSON.stringify(`${scope} .composebox`)}).value`), "", `${said}: the box is empty once sent`);
  const send = await app.page.evaluate<{ inside: boolean; hit: boolean }>(`(() => {
    const n = document.querySelector(${JSON.stringify(`${scope} .composesend`)});
    const r = n.getBoundingClientRect();
    const at = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    return { inside: r.top >= 0 && r.bottom <= innerHeight, hit: at !== null && n.contains(at) };
  })()`);
  assert.deepEqual(send, { inside: true, hit: true }, `${said}: Send is on screen and nothing covers it`);
}

/**
 * A PHOTO SENT IN THE CHAT IS DRAWN ON ITS TURN
 * (`docs/PHOTOS-PLAN-2026-09-22.md`). The photo is a real JPEG the page draws
 * on a canvas, uploaded through the same gated route the composer uses; the
 * scene then waits for the transcript to draw it at its own size, which
 * proves the store, the say, the wire and the read route in one pass.
 */
app.scene("a photo sent in the chat is drawn on its turn", async () => {
  await app.page.navigate(`${WRITE_BASE}/#/`);
  const conversation = await app.page.evaluate<string>(`(async () => {
    const post = ${SIGNED_POST};
    const opened = await (await post("/api/secretary/conversations", JSON.stringify({ title: "A photo" }))).json();
    const canvas = document.createElement("canvas");
    canvas.width = 8;
    canvas.height = 8;
    const context = canvas.getContext("2d");
    context.fillStyle = "red";
    context.fillRect(0, 0, 8, 8);
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/jpeg"));
    const photo = btoa(String.fromCharCode(...new Uint8Array(await blob.arrayBuffer())));
    const stored = await (await post("/api/secretary/attachments", JSON.stringify({ file: photo }))).json();
    await post("/api/secretary/conversations/" + opened.id + "/say", JSON.stringify({ say: "what is in this photo", attachments: [{ id: stored.file.id }] }));
    return opened.id;
  })()`);
  await app.page.evaluate(`location.hash = "#/secretary/${conversation}"`);
  await app.page.waitFor(
    `[...document.querySelectorAll(".turnphoto")].some((img) => img.complete && img.naturalWidth === 8)`,
    { timeout: 20000 },
  );
});

/** Page script that hands the composer's input a one-page PDF saying `text`, as a picker would. */
const picking = (name: string, text: string): string => `(() => {
  const content = "BT /F1 24 Tf 20 100 Td (${text}) Tj ET";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /MediaBox [0 0 200 200] /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    "<< /Length " + content.length + " >>\\nstream\\n" + content + "\\nendstream",
  ];
  let pdf = "%PDF-1.4\\n";
  const offsets = objects.map((object, i) => {
    const at = pdf.length;
    pdf += (i + 1) + " 0 obj\\n" + object + "\\nendobj\\n";
    return at;
  });
  const xref = pdf.length;
  pdf += "xref\\n0 " + (objects.length + 1) + "\\n0000000000 65535 f \\n";
  for (const at of offsets) pdf += String(at).padStart(10, "0") + " 00000 n \\n";
  pdf += "trailer\\n<< /Size " + (objects.length + 1) + " /Root 1 0 R >>\\nstartxref\\n" + xref + "\\n%%EOF";
  const picked = new DataTransfer();
  picked.items.add(new File([pdf], "${name}", { type: "application/pdf" }));
  const input = document.querySelector(".compose input[type=file]");
  input.files = picked.files;
  input.dispatchEvent(new Event("change"));
})()`;

/** Pictures for the eye, where the gallery files its own. */
const SHOTS = process.env["SUZATARY_SHOTS"] ?? join(tmpdir(), "suzatary-shots");
mkdirSync(SHOTS, { recursive: true });
const photographed = (name: string): Promise<void> => app.page.screenshot(join(SHOTS, `${name}.png`));

/**
 * A PDF PICKED IN THE COMPOSER IS SENT AND NAMED ON ITS TURN. The file goes in
 * through the composer's own input, as the phone's picker hands it over, so the
 * upload, the thumbnail, Send and the transcript's chip are one pass; the box
 * reads the PDF's text with pdftotext before it keeps it.
 */
app.scene("a PDF picked in the composer is sent and named on its turn", async () => {
  await app.page.navigate(`${WRITE_BASE}/#/`);
  const conversation = await app.page.evaluate<string>(`(async () => {
    const opened = await (await ${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({ title: "A PDF" }))).json();
    return opened.id;
  })()`);
  // A full load, so a sheet an earlier scene left open is not over the page.
  await app.page.navigate(`${WRITE_BASE}/?fresh=${conversation}#/secretary/${conversation}`);
  await app.page.waitFor(`document.querySelector(".compose input[type=file]") !== null`, { timeout: 20000 });
  await app.page.evaluate(picking("lease.pdf", "Lease for unit 4"));
  await app.page.waitFor(`document.querySelector(".composethumb .filechip") !== null`, { timeout: 20000 });
  await photographed("chat-pdf-picked");
  await app.page.evaluate(`document.querySelector(".composesend").click()`);
  await app.page.waitFor(
    `[...document.querySelectorAll(".turnphotos .filename")].some((name) => name.textContent === "lease.pdf")`,
    { timeout: 20000 },
  );
  await photographed("chat-pdf-sent");
});

/**
 * A PDF NAMING AN SSN IS KEPT PRIVATE ONLY ON THE OWNER'S SAY. The box holds it back and
 * the composer says why; "Keep it private" puts it in the (fixture's) Drive, the chip is
 * locked, and the sent turn names it without a link, since the box has nothing to show.
 */
app.scene("a PDF naming an SSN waits for the owner, then goes to the Drive", async () => {
  await app.page.navigate(`${WRITE_BASE}/#/`);
  const conversation = await app.page.evaluate<string>(`(async () => {
    const opened = await (await ${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({ title: "Taxes" }))).json();
    return opened.id;
  })()`);
  await app.page.navigate(`${WRITE_BASE}/?fresh=${conversation}#/secretary/${conversation}`);
  await app.page.waitFor(`document.querySelector(".compose input[type=file]") !== null`, { timeout: 20000 });
  await app.page.evaluate(picking("w2.pdf", "SSN 123-45-6789 wages 70000"));
  await app.page.waitFor(`document.querySelector(".composeheld") !== null`, { timeout: 20000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".composeheld").textContent`),
    /w2\.pdf: its text names an identity number.*\/my-files\/Suzatary\/Private/,
  );
  await photographed("chat-private-asked");
  await app.page.evaluate(`document.querySelector(".composeheldkeep").click()`);
  await app.page.waitFor(
    `document.querySelector(".composeheld") === null && document.querySelector(".composethumb .filechip") !== null`,
    { timeout: 20000 },
  );
  await photographed("chat-private-kept");
  await app.page.evaluate(`document.querySelector(".composesend").click()`);
  await app.page.waitFor(
    `[...document.querySelectorAll(".turnphotos .filechip")].some((chip) => chip.title.startsWith("Kept private in your Drive at /my-files/Suzatary/Private/"))`,
    { timeout: 20000 },
  );
  await photographed("chat-private-sent");
});

/**
 * THE ONE "NEEDS YOU" PATH, END TO END (`needsyou.ts`, `notify::needs_you`).
 * With the app open, the secretary asks to read a page; nothing web-specific
 * draws the popup, because every request for his input is an open decision.
 * Open lands on the Inbox with that one row marked, and its card shows the
 * exact URL.
 */
app.scene("a new request pops up in an open app and Open lands on its row", async () => {
  await app.page.navigate(`${WRITE_BASE}/#/`);
  await app.page.waitFor(`document.querySelector(".tree .row, .todospane .empty:not([hidden])") !== null`, { timeout: 15000 });
  await app.page.evaluate(`(async () => {
    const post = ${SIGNED_POST};
    const opened = await (await post("/api/secretary/conversations", JSON.stringify({ title: "The deposit page" }))).json();
    await post("/api/secretary/conversations/" + opened.id + "/say", JSON.stringify({ say: "look up the deposit page" }));
    return opened.id;
  })()`);
  await app.page.waitFor(`document.querySelector(".needsyou") !== null`, { timeout: 30000 });
  await app.page.click(".needsyouopen");
  await app.page.waitFor(`location.hash.startsWith("#/inbox?open=")`, { timeout: 15000 });
  await app.page.waitFor(`document.querySelector(".opened") !== null`, { timeout: 15000 });
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".opened").textContent`),
    /https:\/\/www\.gov\.uk\/tenancy-deposit-protection/,
    "the opened row is the request, with its exact URL",
  );
  // ALWAYS ALLOW: the second answer, confirmed in words, reads the page and
  // writes the rule the card offered.
  await app.page.click(".opened .act-always");
  await app.page.click(".opened .act-always.confirming");
  await app.page.waitFor(
    `(document.querySelector("#toast")?.textContent ?? "").includes("allowed: any page on gov.uk or its subdomains")`,
    { timeout: 15000 },
  );
});

/**
 * A TOPIC LEFT UNNAMED IS NAMED AFTER ITS FIRST ANSWER, AND RENAMED IN PLACE
 * (`docs/CHAT-PLAN-2026-09-25.md` §4.1): "New topic" on the list opens one
 * with the compose box focused, the title job (the fixture script's `Title
 * this conversation` line) names it once the answer lands, and Rename is an
 * append the heading and the list both show.
 */
app.scene("a blank new topic is named after its first answer, and renamed in place", async () => {
  await app.resize(PHONE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
  await app.page.waitFor(`document.querySelector(".act-newtopic") !== null`, { timeout: 15000 });
  await app.page.click(".act-newtopic");
  await app.page.waitFor(`/^#\\/secretary\\/[0-9-]+\\?focus=1$/.test(location.hash)`, { timeout: 15000 });
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "New topic"`, { timeout: 15000 });
  await app.page.waitFor(`document.activeElement?.classList.contains("composebox") === true`, { timeout: 15000 });
  await app.page.type(".composebox", "when is the zephyr filing due");
  await app.page.click(".composesend");
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "A named topic"`, { timeout: 30000 });
  const id = await app.page.evaluate<string>(`location.hash.replace(/^#\\/secretary\\//, "").replace(/\\?.*$/, "")`);

  await app.page.click(".act-rename");
  await app.page.waitFor(`document.activeElement?.classList.contains("renamefield") === true`);
  await app.page.evaluate(`document.querySelector(".renamefield").value = "The zephyr filing"`);
  await app.page.press("Enter");
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "The zephyr filing"`, { timeout: 15000 });

  // The stream names each round's conversation, so the new name stands over its round there too.
  await app.page.evaluate(`location.hash = "#/secretary"`);
  const round = `[...document.querySelectorAll(".streamround")].find((r) => r.querySelector(".streammeta a")?.getAttribute("href")?.startsWith("#/secretary/${id}?"))`;
  await app.page.waitFor(`${round}?.querySelector(".streammeta a").textContent === "The zephyr filing"`, { timeout: 15000 });
  assert.ok(
    (await app.page.evaluate<string>(`${round}.querySelector(".answerblock, .turntext:not(.streamasked)").textContent`)).includes("Nothing has changed since the last briefing."),
    "the round's answer stands under its question",
  );
});

/**
 * A TOPIC THE BOX REFUSES IS SAID, AND LEAVES THE ROUND: the post is answered with the box's 403 and its sentence, the
 * owner is told in those words, and the refused write never shows as a chip or goes again.
 */
app.scene("a topic write the box refuses is said in words and leaves no chip", async () => {
  await app.resize(PHONE);
  await topicRounds(app.page);
  await app.page.navigate(`${WRITE_BASE}/#/secretary?topics=garden%2Cinvoices`);
  const shown = `[...document.querySelectorAll(".streamrounds .streamround")].map((r) => [...r.querySelectorAll(".roundtopic")].map((c) => c.textContent).join(" "))`;
  await app.page.waitFor(`${shown}.join("|") === "garden invoices"`, { timeout: 15000 });
  const trips = `fetch("/api/stores/topics/objects").then((r) => r.json()).then((b) => b.objects.filter((o) => o.literal.includes("'trip'")).length)`;
  const before = await app.page.evaluate<number>(trips);
  await app.page.refuseRequests(["/api/stores/topics/objects"], "this device is not trusted to write topics");
  await app.page.click(".streamround .roundtopicadd");
  await app.page.waitFor(`document.querySelector(".topicsheet .topicoffers") !== null`, { timeout: 8000 });
  await app.page.evaluate(`[...document.querySelectorAll(".topicsheet .topicoffers .chipbtn")].find((b) => b.textContent === "trip").click()`);
  await app.page.waitFor(`document.getElementById("toast")?.textContent.includes("the box refused this topic: this device is not trusted to write topics") === true`, { timeout: 15000 });
  await app.page.waitFor(`${shown}.join("|") === "garden invoices"`, { timeout: 8000 });
  await app.page.resumeRequests();
  assert.equal(await app.page.evaluate<number>(trips), before, "the box holds no new write");
});

/**
 * THE TOPIC BAR (docs/TOPICS-PLAN-2026-10-21.md §6): two chips tapped show the rounds that carry both, the filter is
 * the address, so Back moves it, and a topic added to a round by tap is checked by the type before it is written and
 * reaches the box's topic store.
 */
app.scene("two topics chosen show the rounds holding both, and a topic added by tap lands on the round", async () => {
  await app.resize(PHONE);
  await app.trust();
  await topicRounds(app.page);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
  const chip = (topic: string): string => `.topicbar .topicchip[data-topic="${topic}"] .topiclink`;
  const shown = `[...document.querySelectorAll(".streamrounds .streamround")].map((r) => [...r.querySelectorAll(".roundtopic")].map((c) => c.textContent).join(" "))`;
  await app.page.waitFor(`document.querySelector('${chip("invoices")}') !== null`, { timeout: 15000 });

  await app.page.click(chip("garden"));
  await app.page.waitFor(`location.hash === "#/secretary?topics=garden"`, { timeout: 8000 });
  await app.page.waitFor(`${shown}.length === 2`, { timeout: 8000 });
  await app.page.click(chip("invoices"));
  await app.page.waitFor(`location.hash === "#/secretary?topics=garden%2Cinvoices"`, { timeout: 8000 });
  await app.page.waitFor(`${shown}.length === 1`, { timeout: 8000 });
  assert.deepEqual(await app.page.evaluate<string[]>(shown), ["garden invoices"], "only the round holding both");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".topicbar .topicchip.on").length`), 2);

  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/secretary?topics=garden" && ${shown}.length === 2`, { timeout: 8000 });
  await app.page.evaluate(`history.forward()`);
  await app.page.waitFor(`${shown}.length === 1`, { timeout: 8000 });

  await app.page.click(".streamround .roundtopicadd");
  await app.page.waitFor(`document.activeElement?.classList.contains("topicfield") === true`, { timeout: 8000 });
  await app.page.type(".topicsheet .topicfield", "Trip");
  await app.page.waitFor(`document.querySelector(".topicsheet .topicsaid.problem")?.textContent.includes("lowercase") === true`, { timeout: 8000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".topicsheet .act-topicwrite").disabled`), true, "nothing the type refuses is written");
  await app.page.evaluate(`(() => { const f = document.querySelector(".topicsheet .topicfield"); f.value = "trip"; f.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  await app.page.waitFor(`document.querySelector(".topicsheet .act-topicwrite")?.disabled === false`, { timeout: 8000 });
  await app.page.click(".topicsheet .act-topicwrite");
  await app.page.waitFor(`document.querySelector(".topicsheet") === null && ${shown}.join("|") === "garden invoices trip"`, { timeout: 15000 });
  await app.page.waitFor(
    `fetch("/api/stores/topics/objects").then((r) => r.json()).then((b) => b.objects.filter((o) => o.literal.includes("'trip'")).length === 2)`,
    { timeout: 15000 },
  );
});

/**
 * THE SEARCH ASKS THE BOX AND A HIT OPENS ITS CONVERSATION AT THE TURN
 * (`docs/CHAT-PLAN-2026-09-25.md` §4.2) — over what the scene above said, so
 * it runs after it.
 */
app.scene("search finds what was said and a hit opens its conversation", async () => {
  await app.resize(PHONE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
  await app.page.waitFor(`document.querySelector(".chatsearch") !== null`, { timeout: 15000 });
  await app.page.type(".chatsearch", "zephyr");
  await app.page.waitFor(`document.querySelector(".chathit") !== null`, { timeout: 15000 });
  const excerpt = await app.page.evaluate<string>(`document.querySelector(".chathit .chatlast").textContent`);
  assert.equal(excerpt, "You: when is the zephyr filing due");
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".streamround").length`),
    0,
    "a search replaces the stream while it is on",
  );
  await app.page.click(".chathit");
  await app.page.waitFor(`/^#\\/secretary\\/[0-9-]+\\?at=0$/.test(location.hash)`, { timeout: 15000 });
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "The zephyr filing"`, { timeout: 15000 });
  await app.page.waitFor(
    `[...document.querySelectorAll(".turntext")].some((t) => t.textContent.includes("when is the zephyr filing due"))`,
    { timeout: 15000 },
  );
});

/**
 * AT 62rem AND UP THE LIST IS A COLUMN BESIDE THE CONVERSATION, the one on
 * screen marked; below it there is no column (`docs/CHAT-PLAN-2026-09-25.md`
 * §4.2).
 */
app.scene("at 62rem the list stands beside the conversation, and not on a phone", async () => {
  await app.resize(DESKTOP);
  await app.page.navigate(`${WRITE_BASE}/#/secretary/default`);
  await app.page.waitFor(`document.querySelector(".chatsaside .chatrow.current") !== null`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".chatsaside .chatrow.current .chattitle").textContent`),
    "Secretary",
  );
  assert.ok(
    await app.page.evaluate<boolean>(`document.querySelector(".chatsaside").getBoundingClientRect().width > 0`),
    "the column is drawn",
  );
  await app.resize(PHONE);
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "Secretary"`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<number>(`document.querySelector(".chatsaside")?.getBoundingClientRect().width ?? 0`),
    0,
    "no column on a phone",
  );
});

/**
 * A REMEMBERED FACT IS IN PLAIN SIGHT, AND FORGETTING ERASES IT
 * (`docs/CHAT-PLAN-2026-09-25.md` §3.6, §3.7): the round draws a receipt
 * outside its fold, the memory page lists the fact under its kind with a link
 * to the round that said it, and Forget takes it off the page and the route.
 */
app.scene("a remembered fact shows as a receipt and on the memory page, and Forget erases it", async () => {
  const fact = `${owner.name} takes calls after ten in the morning`;
  await app.resize(PHONE);
  await converse(WRITE_BASE, "remember that I take calls after ten");
  await app.page.waitFor(`document.querySelector(".receipt .receipttext")?.textContent === "${fact}"`, { timeout: 20000 });
  assert.equal(
    await app.page.evaluate<boolean>(`document.querySelector(".receipt").closest(".roundfold") === null`),
    true,
    "the receipt is never folded into the trace",
  );
  const round = await app.page.evaluate<string>("location.hash");
  await app.page.click(".receipt a");
  await app.page.waitFor(`location.hash === "#/secretary/memory"`, { timeout: 15000 });
  await app.page.waitFor(
    `document.querySelector('.memorygroup[data-kind="preference"] .memoryfacttext')?.textContent === "${fact}"`,
    { timeout: 15000 },
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".memoryfact .memorysaid").getAttribute("href")`),
    /^#\/secretary\/[0-9-]+\?at=\d+$/,
    "the fact links to the round that said it",
  );
  await app.page.click(".memoryfact .act.forget");
  await app.page.waitFor(`document.querySelector(".memoryfact") === null`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<boolean>(
      `fetch("/api/secretary/memory").then((r) => r.text()).then((t) => t.includes("calls after ten"))`,
    ),
    false,
    "the route no longer answers it",
  );
  await app.page.navigate(`${WRITE_BASE}/${round}`);
  await app.page.waitFor(`document.querySelector(".receipt")?.dataset.state === "forgotten"`, { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<boolean>(
      `document.querySelector(".receipt .receipttext") === null && !document.querySelector(".receipt").textContent.includes("calls after ten")`,
    ),
    true,
    "the receipt says the fact was remembered and forgotten, never its words",
  );
});

/** Whether the first match is drawn at all: a hidden place hides its buttons. */
const shown = (selector: string): string =>
  `(document.querySelector(${JSON.stringify(selector)})?.offsetParent ?? null) !== null`;

/** Every turn's text by who said it, within the transcript or a part of it. */
const said = (who: string, within = ".turns"): string =>
  `[...document.querySelectorAll(${JSON.stringify(`${within} .turn-${who} .turntext`)})].map((t) => t.textContent).join("\\n")`;

/**
 * STOP (`docs/CHAT-PLAN-2026-09-25.md` §4.3): it sits where the answer lands
 * while a round runs, the round ends in the act "stopped the answer", and
 * what the model was saying never lands. The script's "take your time" line
 * answers once the round's halt is pulled (`capability::pulled`), so the stop
 * always comes before the answer; the stateless arm the fixture takes drops
 * that reply when it returns. A stop is not a write, so
 * the question can be asked again straight after.
 */
app.scene("Stop ends the running answer, and its reply never lands", async () => {
  await app.resize(PHONE);
  await converse(WRITE_BASE, "take your time");
  await app.page.waitFor(shown(".stopanswer"), { timeout: 15000 });
  await app.page.click(".stopanswer");
  await app.page.waitFor(`${said("act")}.includes("stopped the answer")`, { timeout: 20000 });
  await app.page.waitFor(`!${shown(".stopanswer")}`, { timeout: 15000 });
  await app.page.waitFor(shown(".againbutton"), { timeout: 15000 });
  assert.equal(
    await app.page.evaluate<boolean>(`${said("model")}.includes("stopped before it landed")`),
    false,
    "the stopped round's reply was dropped, never appended",
  );
  assert.equal(
    await app.page.evaluate<number>(
      `[...document.querySelectorAll(".turns .turn-model .turntext")].filter((t) => (t.textContent ?? "").trim() === "").length`,
    ),
    0,
    "the act closes the question: no empty answer is drawn after it",
  );
});

/**
 * REGENERATE AND EDIT, THE LAST ROUND ONLY (§4.4, §4.11). Regenerate sets the
 * answer aside, drawn collapsed as "Earlier attempt", and the question is
 * answered again; ↑ in the empty composer brings the question back, and
 * sending it reworded sets the old question aside with its answers.
 */
app.scene("Regenerate answers again beside an earlier attempt, and ↑ edits the question", async () => {
  await converse(WRITE_BASE, "ask me again");
  await app.page.waitFor(shown(".againbutton"), { timeout: 20000 });
  await app.page.clickText(".againbutton", "Regenerate");
  await app.page.waitFor(`document.querySelector(".earlierattempt") !== null`, { timeout: 20000 });
  await app.page.waitFor(shown(".againbutton"), { timeout: 20000 });
  assert.match(
    await app.page.evaluate<string>(said("model", ".earlierattempt")),
    /Nothing has changed/,
    "the first answer is kept, set aside",
  );
  // The regenerated answer is drawn after the earlier attempt is set aside, so the count is waited for.
  const current = `[...document.querySelectorAll(".turns .turn-model")].filter((t) => t.closest(".earlierattempt") === null && t.textContent.trim() !== "").length`;
  await app.page.waitFor(`${current} === 1`, { timeout: 20000 }).catch(() => undefined);
  assert.equal(await app.page.evaluate<number>(current), 1, "one current answer, the regenerated one");

  await app.page.click(".composebox");
  await app.page.press("ArrowUp");
  await app.page.waitFor(`document.querySelector(".composebox").value === "ask me again"`, { timeout: 5000 });
  assert.ok(await app.page.evaluate<boolean>(shown(".composeediting")), "the composer says it is editing");
  await app.page.evaluate(`(() => {
    const box = document.querySelector(".composebox");
    box.value = "";
    box.dispatchEvent(new Event("input"));
    return true;
  })()`);
  await app.page.type(".composebox", "ask me once more");
  await app.page.click(".composesend");
  await app.page.waitFor(`${said("person", ".earlierattempt")}.includes("ask me again")`, { timeout: 20000 });
  await app.page.waitFor(shown(".againbutton"), { timeout: 20000 });
  assert.equal(
    await app.page.evaluate<number>(
      `[...document.querySelectorAll(".turns .turn-person")].filter((t) => t.closest(".earlierattempt") === null && t.textContent.includes("ask me once more")).length`,
    ),
    1,
    "the reworded question is the current one",
  );
  assert.equal(await app.page.evaluate<boolean>(shown(".composeediting")), false, "and the composer is asking again");
});
