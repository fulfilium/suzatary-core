/**
 * THE GALLERY: every destination, both widths, both themes, photographed.
 *
 * CLAUDE.md's rule stands — the browser tests are the UI's evidence and
 * screenshots are for the eye — so this file is not a substitute for an
 * assertion anywhere. What it is for is the other half of that sentence: a
 * design pass is judged by a person looking at it, and "open the app on your
 * phone" is a slow way to hand someone every screen in `SHOTS`.
 *
 * It still ASSERTS, because a photograph of a broken page is worse than no
 * photograph: each scene waits for the page's own landmark and fails if the
 * router put "No such page" there instead.
 *
 * A hash this build does not route is SKIPPED BY NAME rather than shot as a
 * 404, and that is not politeness — it is what lets the same file take the
 * BEFORE picture. Run against a checkout from before a destination existed, it
 * photographs everything else and says in one line which page it could not
 * find, so two galleries taken a day apart are comparable.
 *
 * The pictures go to `$SUZATARY_SHOTS` — outside the repo, like every other
 * screenshot this suite takes. A run with the variable unset still works and
 * still asserts; it just leaves the pictures in a temp directory nobody reads.
 */

import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { BASE, doneFromList, MIDDLE, openFinder, openInPlace, PHONE, pickCategory, PROPOSED_BASE, releaseModel, row, SIGNED_POST, suite, topicRounds, WIDE, WRITE_BASE } from "./harness";
import type { Page } from "./cdp";
import cast from "../test/cast";

const { todos } = cast.world;

const OUT = process.env["SUZATARY_SHOTS"] ?? join(tmpdir(), "suzatary-shots");
mkdirSync(OUT, { recursive: true });

/** The secretary's stream, drawn from a reading of the stores. */
const STREAM_READY = `document.querySelector(".column.secretarystream:not(.pagewaiting) .streamrounds") !== null`;

/** The post a row's Done makes, held while its picture is taken; each run marks a todo of its own done. */
let sendingRun = 0;
let releaseSending: () => Promise<void> = async () => {};

/** One screen worth a picture: where it lives, and what proves it drew. */
interface Shot {
  readonly name: string;
  readonly hash: string;
  /** True once the page is drawn — not merely mounted. */
  readonly ready: string;
  /** Which deployment to photograph it on. `BASE` unless the picture is about
   * a state `BASE` deliberately does not hold — the Inbox is the one: it is
   * empty there, and a gallery of an empty queue shows nothing of the queue. */
  readonly base?: string;
  /** Phone only: the sheet is photographed where a thumb opens it. */
  readonly phoneOnly?: boolean;
  /** Something to press before the shutter — the sheet is transient furniture
   * and no hash opens it. */
  readonly act?: (page: Page) => Promise<void>;
  /** And something to undo afterwards. Whatever `act` opened is still open
   * when the next shot arrives on the SAME hash: `renderNav` rebuilds the sheet
   * closed on every route change, and navigating to the hash you are already on
   * is not one. */
  readonly undo?: (page: Page) => Promise<void>;
}

/**
 * ⚠️ THE ORDER MATTERS, and only for one reason: `#/project` PINS the second
 * chain and every page after it is then about that chain. The fixture's second
 * chain holds one resolved todo and no open work, so a list shot taken after
 * it would be a photograph of an empty list — a true picture of the wrong
 * question. So the chain-pinning destination goes last, and `more` (which
 * opens on `#/`) pins the first chain back on its way past.
 */
const SHOTS: readonly Shot[] = [
  { name: "list", hash: "#/", ready: `document.querySelectorAll(".tree .row").length > 0` },
  {
    name: "todo",
    hash: `#/todo/${todos.urgent.id}`,
    ready: `document.querySelector(".todoview .todobody") !== null`,
  },
  // The list's one control opened: the search, the filters and the grouping under the title.
  {
    name: "list-finder",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    act: async (page: Page): Promise<void> => openFinder(page),
    undo: async (page: Page): Promise<void> => {
      await page.click(".findbtn");
      await page.waitFor(`document.querySelector(".finder").hidden === true`);
    },
  },
  // A line opened in place: its acts under its title, the title not printed twice.
  {
    name: "list-opened",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    phoneOnly: true,
    act: async (page: Page): Promise<void> => openInPlace(page, todos.urgent.id),
    undo: async (page: Page): Promise<void> => {
      await page.click(`.tree .row[data-todo="${todos.urgent.id}"] .rowmain`);
      await page.waitFor(`document.querySelector(".rowopen") === null`);
    },
  },
  // The add sheet refusing a todo with no category: the chips say so where they are.
  {
    name: "add-sheet-refused",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await page.evaluate(`localStorage.removeItem("suzatary.ui.todos.category/")`);
      await page.click(".addsheet .pickerhandle");
      await page.waitFor(`document.querySelector(".addsheet.open") !== null`);
      await page.waitFor(`document.activeElement?.className.includes("draftbody") === true`);
      if ((await page.evaluate<string>(`document.querySelector(".addsheet .draftbody").value`)) === "") await page.type(".draftbody", "Renew the library card");
      await page.evaluate(`[...document.querySelectorAll(".addsheet .chipbtn[aria-pressed=true]")].forEach((b) => b.click())`);
      await page.click(".addbtn");
      await page.waitFor(`[...document.querySelectorAll(".addsheet .formrefusal")].some((p) => !p.hidden)`);
    },
    undo: async (page: Page): Promise<void> => {
      await page.click(".addsheet .pickerhandle");
      await page.waitFor(`document.querySelector(".addsheet.open") === null`);
    },
  },
  {
    name: "inbox",
    hash: "#/inbox",
    ready: `document.querySelector("#view h1") !== null`,
    // The one picture taken somewhere else: `BASE`'s Inbox is empty on purpose
    // (`destinations.test.ts` is the claim about that), and a queue is worth
    // photographing with something in it.
    base: PROPOSED_BASE,
  },
  // THE INBOX AS THE OWNER MEETS IT (audit X2, X9, X10): the work first, every card closed, the box's own setup
  // cards one closed row after it; then that row opened in place, and one card opened whole.
  {
    name: "inbox-write",
    hash: "#/inbox",
    ready: `document.querySelector('.inboxpage .proposals[data-box="Answered"] .proposal') !== null`,
    base: WRITE_BASE,
  },
  {
    name: "inbox-setup-open",
    hash: "#/inbox",
    ready: `document.querySelector('.inboxpage .proposals[data-box="Answered"] .setupgroup') !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await page.click(".inboxpage .setupgroup > summary");
      await page.waitFor(`document.querySelector(".inboxpage .setupgroup").open === true`);
    },
    undo: async (page: Page): Promise<void> => {
      await page.click(".inboxpage .setupgroup > summary");
      await page.waitFor(`document.querySelector(".inboxpage .setupgroup").open === false`);
    },
  },
  {
    name: "inbox-card-open",
    hash: "#/inbox",
    ready: `document.querySelector('.inboxpage .proposals[data-box="Answered"] > .proposal .proposaldetails') !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await page.click(".inboxpage .proposals > .proposal .proposaldetails > summary");
      await page.waitFor(`document.querySelector(".inboxpage .proposals > .proposal .proposaldetails").open === true`);
    },
    undo: async (page: Page): Promise<void> => {
      await page.click(".inboxpage .proposals > .proposal .proposaldetails > summary");
      await page.waitFor(`document.querySelector(".inboxpage .proposals > .proposal .proposaldetails").open === false`);
    },
  },
  // DONE, ON ITS WAY (audit X1): the row says so, quietly, and nothing else on the page does.
  {
    name: "todos-sending",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      // One word per scene (two themes at each of three widths), so no run adds a todo another already added.
      const word = ["one", "two", "three", "four", "five", "six"][sendingRun] ?? `more${sendingRun}`;
      sendingRun += 1;
      const id = `sendingdemo${word}`;
      await page.click(".addsheet .pickerhandle");
      await page.waitFor(`document.querySelector(".addsheet.open") !== null`);
      await page.waitFor(`document.activeElement?.className.includes("draftbody") === true`);
      await page.type(".draftbody", `Sending demo ${word}`);
      await pickCategory(page, "home");
      await page.click(".addbtn");
      await page.waitFor(`document.getElementById("toast")?.textContent === "added ${id}"`, { timeout: 8000 });
      releaseSending = await page.holdRequests(["/objects"]);
      // A phone opens the line in place; a wide window selects it into the pane beside the list.
      if (await page.evaluate<boolean>(`window.innerWidth < 992`)) await doneFromList(page, id);
      else {
        await page.click(`${row(id)} .rowmain`);
        await page.waitFor(`document.querySelector(".actrow .act-done") !== null`, { timeout: 8000 });
        await page.click(".actrow .act-done");
      }
      await page.waitFor(`document.querySelector('${row(id)} .queuedmark')?.textContent === "done · sending"`, { timeout: 8000 });
    },
    undo: async (page: Page): Promise<void> => {
      await releaseSending();
      await page.waitFor(`document.querySelector(".tree .row.pending") === null`, { timeout: 15000 });
    },
  },
  {
    // DEFER, AS A CURVE (`docs/APP.md` §4 item 4, `deferascurve`): the
    // picture the design rests on — a line flat at 100% until the date and
    // then exactly the term it deferred, which is what a deferral IS rather
    // than a snoozed flag off to the side. `WRITE_BASE` because there is
    // nothing to photograph on a chain that cannot move; the todo and the
    // defer are authored HERE rather than baked into the fixture, so the
    // picture is proof the write path draws what it claims to.
    name: "deferred-graph",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      const id = "deferralgraphdemo";
      // The fold, not the screen: a deferred todo is fully fulfilled and its group may be folded.
      const exists = await page.evaluate<boolean>(
        `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}"))`,
      );
      if (!exists) {
        await page.click(".addsheet .pickerhandle");
        await page.waitFor(`document.querySelector(".addsheet.open") !== null`);
        await page.waitFor(`document.activeElement?.className.includes("draftbody") === true`);
        await page.type(".draftbody", "Deferral graph demo");
        await pickCategory(page, "home");
        await page.click(".addbtn");
        await page.waitFor(`document.querySelector('.tree .row[data-todo="${id}"]') !== null`, { timeout: 8000 });
        // THE ROW IS THE REPLICA'S, drawn before the push: the todo page below
        // asks the BOX, so it waits for the toast `addsheet.ts` raises once
        // the exchange is done, or it can open on a 404 that never redraws.
        await page.waitFor(`document.getElementById("toast")?.textContent === "added ${id}"`, { timeout: 8000 });
      }
      await page.evaluate(`location.hash = "#/todo/${id}"`);
      await page.waitFor(`window.location.hash === "#/todo/${id}"`);
      await page.waitFor(`document.querySelector(".deferbox summary") !== null`, { timeout: 4000 });
      await page.click(".deferbox summary");
      await page.waitFor(`document.querySelector(".deferbox")?.open === true`, { timeout: 2000 });
      // A defer to an already-deferred date replaces the outer deferral
      // (`authoring::defer`'s own law), so re-running this shot against a
      // chain that already carries the demo todo is harmless.
      const future = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
      await page.evaluate(`(() => {
        const node = document.querySelector(".deferbox input[type=date]");
        node.value = "${future}";
        node.dispatchEvent(new Event("input", { bubbles: true }));
        return true;
      })()`);
      await page.click(".deferbox .addbtn");
      await page.waitFor(`document.getElementById("toast")?.textContent?.includes("deferred") === true`, {
        timeout: 4000,
      });
      await page.waitFor(`document.querySelector(".curvebox .curveplot") !== null`, { timeout: 8000 });
    },
  },
  // Drawn once the fetch has landed either way: two windows on a box with
  // transcripts, the empty note on one without (CI), the problem line if the
  // route refused. The fixture's `repo` is a temp dir, so no secretspec and no
  // endpoint — the ceiling is simply absent here, which is a state the page
  // has to draw honestly anyway.
  { name: "about", hash: "#/about", ready: `document.querySelector(".aboutversion") !== null` },
  // What is connected, by group, as a writer sees it: Ask, the lines, a card on the line it is about, Change folded.
  {
    name: "settings",
    hash: "#/settings",
    ready: `document.querySelector(".settingsline") !== null && document.querySelector(".settingshere h2") !== null`,
    base: WRITE_BASE,
  },
  {
    name: "usage",
    hash: "#/usage",
    ready: `document.querySelector(".usagebody .usagewindow, .usagebody .empty, .usagepage .problem") !== null`,
  },
  // The stream (`docs/TOPICS-PLAN-2026-10-21.md` §6) on the deployment that
  // holds conversations, and the default conversation itself.
  {
    name: "secretary",
    hash: "#/secretary",
    ready: STREAM_READY,
    base: WRITE_BASE,
  },
  {
    name: "conversation",
    hash: "#/secretary/default",
    ready: `document.querySelector("#view .pagetitle") !== null`,
  },
  /**
   * THE MEMORY (`docs/CHAT-PLAN-2026-09-25.md` §3.6) with something in it:
   * the fixture script's `remember my accountant and my answers` line has the
   * secretary remember two facts, once, and the picture is the page they land on.
   */
  {
    name: "memory",
    hash: "#/secretary/memory",
    ready: `document.querySelector(".memorypage .memorygroup, .memorypage .empty") !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      if ((await page.evaluate<number>(`document.querySelectorAll(".memorypage .memorygroup").length`)) > 1) return;
      const id = await page.evaluate<string>(
        `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Gallery memory"})).then((r) => r.json()).then((j) => j.id)`,
      );
      await page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
      await page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
      await page.type(".composebox", "remember my accountant and my answers");
      await page.click(".composesend");
      await page.waitFor(`document.querySelectorAll(".receipt").length === 2`, { timeout: 20000 });
      await page.navigate(`${WRITE_BASE}/#/secretary/memory`);
      await page.waitFor(`document.querySelectorAll(".memorypage .memorygroup").length > 1`, { timeout: 15000 });
    },
  },
  /**
   * A CONVERSATION WITH A REAL EXCHANGE ON IT
   * (`docs/ONECHAT-PLAN-2026-09-13.md` §6 commit 6) — NOT MID-ANSWER, and that
   * limit is measured rather than assumed: `say`'s own response draws the
   * turn it just appended as `queued` before the worker has answered it
   * (`serve::secretary::say`'s ordering), which is a deterministic fact about the
   * WIRE (`web/e2e/secretary.test.ts` checks exactly that) — but the fixture
   * model (`Scripted`) has no artificial delay, so the round and the live
   * stream's redraw both finish, in practice, before a screenshot's own
   * `waitFor` can ever catch the DOM in between. A gallery shot photographs
   * what a `waitFor` can hold still for, so this settles for the turn and its
   * answer both landed rather than reaching for a moment this fixture cannot
   * hold open.
   */
  {
    name: "secretary-answering",
    hash: "#/secretary/default",
    ready: `document.querySelector("#view .pagetitle") !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      const id = await page.evaluate<string>(
        `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Gallery shot"})).then((r) => r.json()).then((j) => j.id)`,
      );
      await page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
      await page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
      // A HEADING AND A PANEL (`docs/APP.md`, "typstanswers"), so the picture
      // shows what a rendered answer actually looks like rather than a plain
      // paragraph that happens to compile too.
      await page.type(".composebox", "give me a rendered answer");
      await page.click(".composesend");
      await page.waitFor(`document.querySelectorAll(".turn-person").length > 0`, { timeout: 8000 });
      await page.waitFor(`document.querySelector(".answerblock.rich") !== null`, { timeout: 8000 });
    },
  },
  // WHAT A PAGE SHOWS WHILE IT WAITS (the audit's X5): a page and a conversation, each held at the box until the
  // placeholder is on screen, then let through.
  {
    name: "page-waiting",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      // A query this tab has not asked before, so the page waits on the box.
      held = await page.holdRequests(["/api/mail/"]);
      await page.evaluate(`location.hash = "#/mail?q=" + encodeURIComponent("gallery " + Date.now())`);
      await page.waitFor(`getComputedStyle(document.querySelector(".mailpage .placeholder") ?? document.body).visibility === "visible" && document.querySelector(".mailpage .placeholder") !== null`, { timeout: 8000 });
    },
    undo: release(`document.querySelector(".mailpage .maillist") !== null`),
  },
  {
    name: "conversation-waiting",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      const id = await page.evaluate<string>(
        `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Gallery waiting"})).then((r) => r.json()).then((j) => j.id)`,
      );
      held = await page.holdRequests(["/api/"]);
      await page.evaluate(`location.hash = "#/secretary/${id}"`);
      // The page's own transcript, never a panel's. The new conversation can also reach the page through the event
      // stream opened before the hold, which no hold stops, and then it is drawn at once with no placeholder: the
      // wait ends on whichever of the two the page draws.
      await page.waitFor(
        `(() => {
           const p = document.querySelector("#view .transcriptcontent .placeholder");
           return p !== null ? getComputedStyle(p).visibility === "visible" : document.querySelector("#view .composebox") !== null;
         })()`,
        { timeout: 8000 },
      );
    },
    undo: release(`document.querySelector("#view .transcriptcontent .placeholder") === null`),
  },
  // A store's sync (WEB-SPEC §3.20): the lines forgotten and their content held at the box, so the line under the
  // header stands with its neutral pie while the secretary's page waits.
  {
    name: "syncing",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await page.evaluate(`new Promise((done, fail) => {
        const r = indexedDB.open("suzatary")
        r.onerror = () => fail(String(r.error))
        r.onsuccess = () => {
          const tx = r.result.transaction(["chain", "content"], "readwrite")
          tx.objectStore("chain").delete(IDBKeyRange.bound("objects:store/lines", "objects:store/lines\\uffff"))
          tx.objectStore("content").clear()
          tx.oncomplete = () => (r.result.close(), done(true))
          tx.onerror = () => fail(String(tx.error))
        }
      })`);
      held = await page.holdRequests(["/api/stores/lines/content"]);
      await page.navigate(`${WRITE_BASE}/?fresh=${Date.now()}#/secretary/default`);
      await page.waitFor(`document.querySelector("#syncing .syncline svg.pie") !== null`, { timeout: 15000 });
    },
    undo: release(`document.getElementById("syncing").hidden`),
  },
  // The Typst list before the tab's Typst is here (X21): a fresh document, so Typst is fetched again and held.
  {
    name: "typst-waiting",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      held = await page.holdRequests(["typst_bg.wasm"]);
      await page.navigate(`${WRITE_BASE}/?fresh=${Date.now()}#/typst`);
      await page.waitFor(`getComputedStyle(document.querySelector(".typstframe .placeholder") ?? document.body).visibility === "visible" && document.querySelector(".typstframe .placeholder") !== null`, { timeout: 15000 });
    },
    undo: release(`document.querySelector(".typstframe .placeholder") === null`),
  },
  // A slow answer on its way (X8): "Thinking · N s" beside Stop, the script answering once the shot is taken.
  {
    name: "conversation-thinking",
    hash: "#/secretary/default",
    ready: `document.querySelector("#view .pagetitle") !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await ask(page, "take your time");
      await page.waitFor(`/^Thinking · \\d+ s$/.test(document.querySelector(".answerplace .answerstate")?.textContent ?? "")`, { timeout: 8000 });
    },
    undo: async (page: Page): Promise<void> => {
      releaseModel();
      await page.waitFor(`document.querySelector(".answerplace .answerstate") === null`, { timeout: 20000 });
    },
  },
  // An answer at rest, then its menu open (X14).
  {
    name: "answer-menu",
    hash: "#/secretary/default",
    ready: `document.querySelector("#view .pagetitle") !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await ask(page, "propose the harrow reply");
      // The round's own answer once nothing runs: a model turn among its calls is folded away as the round ends.
      const answer = ".round > .turn-model";
      await page.waitFor(`document.querySelector("${answer} .turnmorebutton") !== null && document.querySelector(".answerplace .answerstate") === null`, { timeout: 20000 });
      await page.click(`${answer} .turnmorebutton`);
      await page.waitFor(`document.querySelector("${answer} .turnmore").open`);
    },
  },
  // The topic bar with two topics chosen, over the one round that carries both.
  {
    name: "secretary-topics",
    hash: "#/secretary",
    ready: STREAM_READY,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await topicRounds(page);
      await page.navigate(`${WRITE_BASE}/#/secretary?topics=garden%2Cinvoices`);
      await page.waitFor(`document.querySelectorAll(".topicbar .topicchip.on").length === 2 && document.querySelectorAll(".streamround").length === 1`, { timeout: 15000 });
    },
  },
  // One topic chosen: its rounds, each with its topics as chips beside its words.
  {
    name: "secretary-round-topics",
    hash: "#/secretary?topics=garden",
    ready: `${STREAM_READY} && document.querySelectorAll(".streamround .roundtopic").length >= 3`,
    base: WRITE_BASE,
  },
  // The rename sheet on a chosen topic, a name in use typed into it: the merge it would make, said before it is made.
  {
    name: "secretary-rename",
    hash: "#/secretary?topics=garden",
    ready: `${STREAM_READY} && document.querySelector(".topicbar .topicrename") !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      await page.click(".topicbar .topicrename");
      await page.waitFor(`document.activeElement?.classList.contains("topicfield") === true`, { timeout: 8000 });
      await page.evaluate(`(() => { const f = document.querySelector(".topicsheet .topicfield"); f.value = "trip"; f.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await page.waitFor(`document.querySelector(".topicsheet .topicsaid")?.textContent.includes("one topic") === true`, { timeout: 8000 });
    },
    undo: async (page: Page): Promise<void> => {
      await page.click(".topicsheet .act:not(.act-topicwrite)");
      await page.waitFor(`document.querySelector(".topicsheet") === null`, { timeout: 8000 });
    },
  },
  {
    name: "project",
    hash: "#/project",
    ready: `document.querySelector("#view .pagetitle")?.textContent === "Project"`,
  },
  /**
   * THE SHEET'S OTHER STATE (chain item `chainswitcherlies`,
   * `docs/AUDIT-UX-2026-09-12.md` finding 5): the switcher is missing here,
   * not merely disabled, because Project already answers "which chain" for
   * itself. Stays right after `project` — still that chain, and `more` below
   * pins the first one back on its way past, same as it always did.
   */
  {
    name: "moreproject",
    hash: "#/project",
    ready: `document.querySelector("#view .pagetitle")?.textContent === "Project"`,
    phoneOnly: true,
    act: async (page: Page): Promise<void> => {
      await page.evaluate(
        `[...document.querySelectorAll(".tabbar .tab")].find((tab) => tab.textContent?.includes("More"))?.click()`,
      );
      await page.waitFor(`document.querySelector(".moresheet")?.hidden === false`);
    },
    undo: async (page: Page): Promise<void> => {
      await page.evaluate(`document.body.click()`);
      await page.waitFor(`document.querySelector(".moresheet")?.hidden === true`);
    },
  },
  {
    name: "more",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    act: async (page: Page): Promise<void> => {
      await page.evaluate(
        `[...document.querySelectorAll(".tabbar .tab")].find((tab) => tab.textContent?.includes("More"))?.click()`,
      );
      await page.waitFor(`document.querySelector(".moresheet")?.hidden === false`);
    },
    undo: async (page: Page): Promise<void> => {
      await page.evaluate(`document.body.click()`);
      await page.waitFor(`document.querySelector(".moresheet")?.hidden === true`);
    },
  },
];

/** What a waiting shot holds at the box, let through by its `undo` once the picture is taken. */
let held: () => Promise<void> = async () => {};
function release(drawn: string): (page: Page) => Promise<void> {
  return async (page: Page): Promise<void> => {
    await held();
    held = async () => {};
    await page.waitFor(drawn, { timeout: 15000 });
  };
}

/** A fresh conversation, asked `said`. */
async function ask(page: Page, said: string): Promise<void> {
  const id = await page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Gallery"})).then((r) => r.json()).then((j) => j.id)`,
  );
  await page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  await page.type(".composebox", said);
  await page.click(".composesend");
}

/**
 * WAIT FOR THE PIXELS, not only for the DOM.
 *
 * A `waitFor` answers as soon as an element EXISTS, and chromium composites on
 * its own schedule — a scroller or a layer under a sheet can still be an
 * unpainted grey rectangle at that instant, which is a photograph of the
 * browser's plumbing rather than of this app. Two animation frames plus a beat
 * is the cheap fix, and this file is the one place in the suite where what the
 * screen LOOKS like is the deliverable.
 */
const painted = async (page: Page): Promise<void> => {
  await page.evaluate(
    `new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 150))))`,
  );
};

/** What the router puts on screen when it knows nothing about a hash. A build
 * without the Inbox lands here, and a picture of that is not a deliverable. */
const LOST = `document.querySelector("#view .pagetitle")?.textContent === "No such page"`;

/**
 * Did the router recognise this hash? ⚠️ ASKED ONLY ONCE THE MOUNT HAS
 * SETTLED — `navigate` returns when the LOCATION is right, and the app's own
 * routing is a promise after that, so a bare read here would be answering
 * about the previous page. Waiting for "either this page's landmark or the
 * not-found title" is the condition that is true in both cases and in neither
 * before.
 */
const routed = async (page: Page, ready: string): Promise<boolean> => {
  await page.waitFor(`(${ready}) || (${LOST})`, { timeout: 20000 });
  return !(await page.evaluate<boolean>(LOST));
};

const app = await suite("shots");

// The widths the audit reads a page at: a phone, a small laptop and a wide window.
for (const [size, label] of [
  [PHONE, "phone"],
  [MIDDLE, "middle"],
  [WIDE, "wide"],
] as const) {
  for (const theme of ["light", "dark"] as const) {
    app.scene(`${label}, ${theme}`, async () => {
      await app.resize(size);
      await app.page.colorScheme(theme);
      let taken = 0;
      for (const shot of SHOTS) {
        if (shot.phoneOnly === true && label !== "phone") continue;
        if (shot.base === WRITE_BASE) await app.trust();
        await app.page.navigate(`${shot.base ?? BASE}/${shot.hash}`);
        if (!(await routed(app.page, shot.ready))) {
          // Not built yet on this branch. Say so once, loudly enough to read in
          // the run's output, and go on — the rest of the gallery is still a
          // gallery.
          console.log(`shots: ${shot.hash} is not routed in this build — no ${shot.name} picture`);
          continue;
        }
        const path = join(OUT, `${label}-${theme}-${shot.name}.png`);
        try {
          if (shot.act !== undefined) await shot.act(app.page);
          await painted(app.page);
          await app.page.screenshot(path);
          if (shot.undo !== undefined) await shot.undo(app.page);
        } finally {
          // A shot that failed still lets its held requests through, so the next scene's pages can load.
          await held();
          held = async () => {};
        }
        taken += 1;
      }
      // The scene's own claim, so this file is a test and not a script: it
      // photographed something, and the last page it visited is one this app
      // knows.
      assert.ok(taken > 0, "the gallery is empty — nothing this suite asked for was routed");
      assert.equal(await app.page.evaluate<boolean>(LOST), false);
    });
  }
}
