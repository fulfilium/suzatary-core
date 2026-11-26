/**
 * The gallery: every destination at each width and theme, photographed for the eye, never a substitute for a scene's
 * assertion; it asserts only that each page drew its own landmark rather than "No such page". A hash this build does not
 * route is skipped by name rather than shot as a 404, so the same file takes the before picture. The pictures go to
 * `$SUZATARY_SHOTS`, or else a temp directory.
 */

import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { BASE, doneFromList, MIDDLE, openFinder, openInPlace, PHONE, pickCategory, PROPOSED_BASE, releaseModel, row, suite, switchChain, topicRounds, TYPST_MODULE, WIDE, WRITE_BASE, opened } from "./harness";
import type { Page } from "./cdp";
import cast from "../test/cast";

const { todos } = cast.world;

const OUT = process.env["SUZATARY_SHOTS"] ?? join(tmpdir(), "suzatary-shots");
mkdirSync(OUT, { recursive: true });

const STREAM_READY = `document.querySelector(".column.secretarystream:not(.pagewaiting) .streamrounds") !== null`;

/** The post a row's Done makes, held while its picture is taken; each run marks a todo of its own done. */
let sendingRun = 0;
let releaseSending: () => Promise<void> = async () => {};

interface Shot {
  readonly name: string;
  readonly hash: string;
  /** True once the page is drawn — not merely mounted. */
  readonly ready: string;
  /** `BASE` unless the picture is about a state `BASE` deliberately does not hold, as the Inbox's queue. */
  readonly base?: string;
  /** Phone only: the sheet is photographed where a thumb opens it. */
  readonly phoneOnly?: boolean;
  /** Something to press before the shutter: no hash opens a sheet. */
  readonly act?: (page: Page) => Promise<void>;
  /** Whatever `act` opened is still open when the next shot arrives on the same hash, which is no route change. */
  readonly undo?: (page: Page) => Promise<void>;
}

/**
 * ⚠️ The order matters: `otherchain` picks the second chain, whose fixture holds no open work, so it goes last and its
 * `undo` picks the first chain back.
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
      await page.evaluate(`localStorage.removeItem("suzatary.ui.kept/todos.category")`);
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
    // `BASE`'s Inbox is empty on purpose (`destinations.test.ts`), and a queue is worth photographing with something in it.
    base: PROPOSED_BASE,
  },
  // The Inbox as the owner meets it, the box's own setup cards one closed row after the work; then that row and a card opened.
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
  // Done, on its way: the row says so, quietly, and nothing else on the page does.
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
    // A deferral as a curve, flat at 100% until the date and then the term it deferred, authored here so the picture
    // proves the write path draws it.
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
        // The row is the replica's, drawn before the push, and the todo page asks the box: without the toast it can open on a 404.
        await page.waitFor(`document.getElementById("toast")?.textContent === "added ${id}"`, { timeout: 8000 });
      }
      await page.evaluate(`location.hash = "#/todo/${id}"`);
      await page.waitFor(`window.location.hash === "#/todo/${id}"`);
      await page.waitFor(`document.querySelector(".deferbox summary") !== null`, { timeout: 4000 });
      await page.click(".deferbox summary");
      await page.waitFor(`document.querySelector(".deferbox")?.open === true`, { timeout: 2000 });
      // A defer to an already-deferred date replaces the outer deferral (`authoring::defer`'s law), so a rerun is harmless.
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
  // The fixture's `repo` is a temp dir, so no secretspec and no endpoint: the ceiling is absent here.
  { name: "about", hash: "#/about", ready: `document.querySelector(".aboutversion") !== null` },
  { name: "briefings", hash: "#/briefings", ready: `document.querySelector("#view .pagetitle")?.textContent === "Briefings"` },
  // What is connected, by group, as a writer sees it: Ask, the lines, a card on the line it is about, Change folded.
  {
    name: "settings",
    hash: "#/settings",
    ready: `document.querySelector(".settingsline") !== null && document.querySelector(".settingshere h2") !== null`,
    base: WRITE_BASE,
  },
  // The stream on the deployment that holds conversations, and the default conversation itself.
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
  /** The fixture script's `remember my accountant and my answers` line has the secretary remember two facts, once. */
  {
    name: "memory",
    hash: "#/secretary/memory",
    ready: `document.querySelector(".memorypage .memorygroup, .memorypage .empty") !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      if ((await page.evaluate<number>(`document.querySelectorAll(".memorypage .memorygroup").length`)) > 1) return;
      const id = await page.evaluate<string>(
        opened("Gallery memory"),
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
  /** Not mid-answer: the fixture model (`Scripted`) has no delay, so no `waitFor` can catch the queued turn before its answer. */
  {
    name: "secretary-answering",
    hash: "#/secretary/default",
    ready: `document.querySelector("#view .pagetitle") !== null`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      const id = await page.evaluate<string>(
        opened("Gallery shot"),
      );
      await page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
      await page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
      // A heading and a panel, so the picture shows what a rendered answer looks like.
      await page.type(".composebox", "give me a rendered answer");
      await page.click(".composesend");
      await page.waitFor(`document.querySelectorAll(".turn-person").length > 0`, { timeout: 8000 });
      await page.waitFor(`document.querySelector(".answerblock.rich") !== null`, { timeout: 8000 });
    },
  },
  // What a page shows while it waits: a page and a conversation, each held at the box until the placeholder is on screen.
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
        opened("Gallery waiting"),
      );
      held = await page.holdRequests(["/api/"]);
      await page.evaluate(`location.hash = "#/secretary/${id}"`);
      // The event stream opened before the hold can draw the conversation at once with no placeholder, so either ends the wait.
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
  // A store's sync: the lines forgotten and their content held at the box, so the line under the
  // header stands with its neutral pie while the secretary's page waits.
  {
    name: "syncing",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      // Cleared from a page that runs no app, so no tab's core writes its snapshot back as it is hidden.
      await page.navigate(`${WRITE_BASE}/api/about`);
      await page.evaluate(`new Promise((done, fail) => {
        const r = indexedDB.open("suzatary")
        r.onerror = () => fail(String(r.error))
        r.onsuccess = () => {
          try {
            const tx = r.result.transaction(["replica", "content"], "readwrite")
            tx.objectStore("replica").delete(IDBKeyRange.bound("lines/", "lines/\\uffff"))
            tx.objectStore("content").clear()
            tx.oncomplete = () => (r.result.close(), done(true))
            tx.onerror = () => fail(String(tx.error))
          } catch (e) {
            fail(String(e))
          }
        }
      })`);
      held = await page.holdRequests(["/api/stores/lines/content"]);
      await page.navigate(`${WRITE_BASE}/?fresh=${Date.now()}#/secretary/default`);
      await page.waitFor(`document.querySelector("#syncing .syncline svg.pie") !== null`, { timeout: 15000 });
    },
    undo: release(`document.getElementById("syncing").hidden`),
  },
  // The Typst list before the tab's Typst is here: a fresh document, so Typst is fetched again and held.
  {
    name: "typst-waiting",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    base: WRITE_BASE,
    act: async (page: Page): Promise<void> => {
      held = await page.holdRequests([TYPST_MODULE]);
      await page.navigate(`${WRITE_BASE}/?fresh=${Date.now()}#/typst`);
      await page.waitFor(`getComputedStyle(document.querySelector(".typstframe .placeholder") ?? document.body).visibility === "visible" && document.querySelector(".typstframe .placeholder") !== null`, { timeout: 15000 });
    },
    undo: release(`document.querySelector(".typstframe .placeholder") === null`),
  },
  // A slow answer on its way: "Thinking · N s" beside Stop, the script answering once the shot is taken.
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
  // An answer at rest, then its menu open.
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
  /** The second chain's list, reached by the chain picker in More. */
  {
    name: "otherchain",
    hash: "#/",
    ready: `document.querySelectorAll(".tree .row").length > 0`,
    act: async (page: Page): Promise<void> => {
      await switchChain(page, "suzatary");
      await page.waitFor(`document.querySelector(".todospane .empty:not([hidden])") !== null`);
    },
    undo: async (page: Page): Promise<void> => {
      await switchChain(page, "life");
      await page.waitFor(`document.querySelectorAll(".tree .row").length > 0`);
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

async function ask(page: Page, said: string): Promise<void> {
  const id = await page.evaluate<string>(
    opened("Gallery"),
  );
  await page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  await page.type(".composebox", said);
  await page.click(".composesend");
}

/** A `waitFor` answers once an element exists and chromium composites on its own schedule, so this waits for the pixels. */
const painted = async (page: Page): Promise<void> => {
  await page.evaluate(
    `new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 150))))`,
  );
};

/** What the router puts on screen when it knows nothing about a hash. */
const LOST = `document.querySelector("#view .pagetitle")?.textContent === "No such page"`;

/** What a page that could not load says: the loader's own line when no build ran, or a page's "Could not load". */
const FAILED = `(document.documentElement.dataset["started"] === "refused" || [...document.querySelectorAll(".problem")].some((p) => (p.textContent ?? "").startsWith("Could not load")))`;

/** What a load the box answered can fail on: the controlling worker and its build, each registration, the build served now. */
const BROWSER_STATE = `(async () => {
  const strip = (w) => (w === null ? null : w.state + " " + w.scriptURL.replace(location.origin, ""));
  const registrations = await navigator.serviceWorker.getRegistrations();
  return {
    at: location.href,
    started: document.documentElement.dataset["started"] ?? null,
    said: [...document.querySelectorAll(".problem")].map((p) => p.textContent),
    controller: navigator.serviceWorker.controller === null ? null : navigator.serviceWorker.controller.scriptURL.replace(location.origin, ""),
    registrations: registrations.map((r) => ({ scope: r.scope.replace(location.origin, ""), installing: strip(r.installing), waiting: strip(r.waiting), active: strip(r.active) })),
    served: await fetch("/b/current", { cache: "no-store" }).then((r) => r.text(), (e) => "unreadable: " + String(e)),
  };
})()`;

/**
 * ⚠️ Asked only once the mount has settled: `navigate` returns when the location is right and the app's routing is a
 * promise after that, so a bare read would answer about the previous page.
 */
const routed = async (page: Page, ready: string): Promise<boolean> => {
  await page.waitFor(`(${ready}) || (${LOST})`, { timeout: 20000 });
  return !(await page.evaluate<boolean>(LOST));
};

const app = await suite("shots");
await app.journal();
// A page that says it could not load fails the wait it is in at once, with the state of the browser.
app.page.endWaitsWhen({
  when: FAILED,
  error: async () => {
    const state = await app.page.evaluate<unknown>(BROWSER_STATE).catch((e: unknown) => `unreadable: ${String(e)}`);
    return new Error(`the page could not load\n  browser state ${JSON.stringify(state)}`);
  },
});

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
          // Not built yet on this branch: said once, and the rest of the gallery goes on.
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
      assert.ok(taken > 0, "the gallery is empty — nothing this suite asked for was routed");
      assert.equal(await app.page.evaluate<boolean>(LOST), false);
    });
  }
}
