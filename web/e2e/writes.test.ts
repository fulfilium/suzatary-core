/**
 * The two things the app writes, end to end: an act on a card, and a new todo.
 *
 * `app/cli/tests/acting.rs` already pins the routes and every refusal. What
 * only a browser can check is the half in front of them — that the screen does
 * not wait for the wire, that the entry on the card is the one the folds see,
 * and that the chain really moved. So each scene asserts in both directions:
 * the DOM says the act happened, and `/api/fold` — fetched by the page, through
 * the same gate — agrees.
 *
 * This file drives `E2E_BASE_WRITE`, a fixture server with a chain of its own
 * (`scripts/web-e2e.sh`), because these scenes MOVE the tip and every other
 * file reads a chain that must not move under it.
 */

import assert from "node:assert/strict";

import { WRITE_BASE, addTodo, doneFromList, everyTodo, fold, openInPlace, openTodo, row, suite, untilReplica, pickCategory } from "./harness";
import cast from "../test/cast";

const { todos, owner } = cast.world;

const app = await suite("writes");

const stateOf = async (todo: string): Promise<string | undefined> =>
  (await fold(app.page)).find((entry) => entry.todo === todo)?.state;

/** The default conversation's newest window, read as the tab reads it: the lines' skeleton piece by piece and their
 * content by name (each literal held, each husk taken in its line's place), the inbox's objects, through the module. */
const DEFAULT = `(async () => {
  const core = await import("/b/dev/prodrome.js");
  await core.default({ module_or_path: "/b/dev/prodrome_bg.wasm" });
  const json = async (path) => (await fetch(path)).json();
  const lines = new core.Lines();
  for (;;) {
    const piece = await json("/api/stores/lines/skeleton?since=" + lines.since());
    lines.piece(JSON.stringify(piece), JSON.stringify(piece.keys), 0);
    if (piece.rest === 0) break;
  }
  lines.land().free();
  const lacking = JSON.parse(lines.lacking(JSON.stringify({ type: "Whole" }))).sort((a, b) => (a.hash < b.hash ? -1 : 1));
  for (let at = 0; at < lacking.length; ) {
    const ask = [];
    let bytes = 0;
    for (const node of lacking.slice(at)) {
      if (ask.length > 0 && (ask.length === 64 || bytes + node.size > 1 << 20)) break;
      ask.push(node.hash);
      bytes += node.size;
    }
    at += ask.length;
    const answer = await json("/api/stores/lines/content?hashes=" + ask.join(","));
    const literals = answer.objects.filter((o) => o.held === "literal").map((o) => ({ hash: o.hash, text: o.literal }));
    const husks = answer.objects.filter((o) => o.held === "husk").map((o) => ({ hash: o.hash, husk: o.husk }));
    lines.hold(JSON.stringify(literals), "[]");
    lines.take_husks(JSON.stringify(husks)).free();
  }
  const inbox = new core.Proposals();
  inbox.piece(JSON.stringify(await json("/api/stores/inbox/objects")), 0);
  inbox.land().free();
  try {
    return JSON.parse(lines.conversation("default", inbox));
  } finally {
    lines.free();
    inbox.free();
  }
})()`;

app.scene("Done marks only its line as sending, raises no banner, and the line leaves once the box takes it", async () => {
  await app.open(WRITE_BASE);
  assert.ok((await everyTodo(app.page)).includes(todos.high.id));
  assert.equal(await stateOf(todos.high.id), "open");
  // The replica has to be holding the chain before it can seal onto it.
  await untilReplica(app.page);

  // The post is held at the network, so a card off the list is off because this browser folded its own objects.
  // The Done button arrives with `GET /api/me`'s answer, not with the line.
  await openInPlace(app.page, todos.high.id);
  await app.page.waitFor(`document.querySelector('.rowopen[data-todo="${todos.high.id}"] .act-done') !== null`, {
    timeout: 8000,
  });
  const release = await app.page.holdRequests(["/objects"]);
  await app.page.click(`.rowopen[data-todo="${todos.high.id}"] .act-done`);
  // THE ROW SAYS IT, AND NOTHING ELSE DOES (audit X1): the line stays where it was, quiet, until the box has it.
  await app.page.waitFor(`document.querySelector('${row(todos.high.id)}.pending .queuedmark')?.textContent === "done · sending"`, {
    timeout: 4000,
  });
  assert.equal(await stateOf(todos.high.id), "open", "the chain must not have moved while the post is in flight");
  assert.equal(await app.page.evaluate<boolean>(`document.getElementById("stale").hidden`), true, "no banner for an act on its way");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".tree .row.pending").length`), 1, "only its own row is marked");
  await release();

  // The box's own fold, asked by its route.
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${todos.high.id}" && e.state === "completed"))`,
    { timeout: 15000 },
  );
  const entry = (await fold(app.page)).find((candidate) => candidate.todo === todos.high.id);
  assert.equal(entry?.value, 1, "a resolved todo reads 1.0 from the instant it was resolved");
  await app.page.waitFor(`document.querySelector('${row(todos.high.id)}') === null`, { timeout: 15000 });

  // WHAT THE TAB WROTE IS THE CORE'S CHANGE: named by the chain's genesis and
  // resting on this todo's own writes, never on whichever head the box had.
  const written = await app.page.evaluate<string>(
    `fetch("/api/objects").then((r) => r.json()).then((b) => b.objects.map((o) => o.literal).filter((l) => l.includes("event=Completed(todo='${todos.high.id}'") && l.includes("actor='${owner.actor}'")).at(-1) ?? "")`,
  );
  const shape = /^Change\(genesis='[0-9a-f]{64}', deps=\(([^)]*)\), event=Completed\(/.exec(written);
  assert.ok(shape !== null, `a change, it was ${written}`);
  assert.ok((shape[1] ?? "").length > 0, "it rests on the todo's earlier writes");
});

app.scene("the add sheet puts a todo on the chain", async () => {
  await app.open(WRITE_BASE);
  const before = await everyTodo(app.page);

  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await app.page.waitFor(`document.activeElement?.className.includes("draftbody") === true`);

  await app.page.type(".draftbody", "Renew the lease");
  // A category is required (`draft.ts`), and the chips say what they are.
  await pickCategory(app.page, "home");
  await app.page.click(".addbtn");

  const id = "renewthelease";
  await app.page.waitFor(`document.querySelector('${row(id)}') !== null`, { timeout: 8000 });
  assert.deepEqual(
    (await everyTodo(app.page)).filter((todo) => !before.includes(todo)),
    [id],
    "exactly one line should be new, and its id should be the body's slug",
  );
  // The box's fold holds it once the replica has pushed, which is a round trip
  // after the card appears here: waited for, like the scene above, never read
  // at once.
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "open"))`,
    { timeout: 15000 },
  );
  assert.equal(await stateOf(id), "open", "the fold the server keeps must hold it too");
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector('${row(id)} .rowtitle')?.textContent ?? null`),
    "Renew the lease",
  );
  await app.page.waitFor(`document.querySelector(".addsheet.open") === null`, { timeout: 2000 });
});

app.scene("a page whose write the box refuses as older is never reloaded under the reader, and the act it sealed lands after a reload", async () => {
  await app.open(WRITE_BASE);
  await untilReplica(app.page);
  await app.page.evaluate(`window.__e2eBeforeUpgrade = true`);

  // The object this act seals goes out as a build from before the contract would send it: the box refuses it with
  // 426. Nothing reloads under the reader (the box serves this page's own build, so nothing is offered), and the
  // build the next load runs sends the act the replica kept.
  await app.page.sendNextAsStale(["/objects"]);
  // The core's worker posts the act: attached, its requests are journaled with the page's.
  await app.page.inWorker("prodromework", "true");
  const journal = await app.page.journal();
  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await app.page.type(".draftbody", "Water the fern");
  await pickCategory(app.page, "home");
  await app.page.click(".addbtn");

  // The box answered the push 426, and the page stood still after it.
  const refused = (): boolean => journal.lines().some((l) => /< 426 \/api\/\S*objects$/.test(l));
  for (let waited = 0; !refused(); waited += 100) {
    if (waited > 20000) assert.fail(`the push was never refused:\n${journal.lines().join("\n")}`);
    await new Promise((ok) => setTimeout(ok, 100));
  }
  await app.page.waitFor(`new Promise((ok) => setTimeout(() => ok(true), 1500))`);
  assert.equal(await app.page.evaluate<boolean>(`window.__e2eBeforeUpgrade === true`), true, "nothing reloaded under the reader");
  await app.page.reload();
  await app.page.waitFor(`window.__e2eBeforeUpgrade === undefined && document.readyState === "complete"`, { timeout: 20000 });
  const id = "waterthefern";
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "open"))`,
    { timeout: 20000 },
  );
  assert.equal(await stateOf(id), "open", "the act sealed before the reload reached the box after it");
  await app.page.resumeRequests();
});

app.scene("a recurring todo offers Did it instead of Done, and stays open once tended", async () => {
  await app.open(WRITE_BASE);
  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await app.page.waitFor(`document.activeElement?.className.includes("draftbody") === true`);

  await app.page.type(".draftbody", "Restock the printer paper");
  await pickCategory(app.page, "home");
  await app.page.clickText(".addsheet .segmented .segment", "After a pass");
  await app.page.waitFor(`document.querySelector(".addsheet .cyclefields")?.hidden === false`, { timeout: 2000 });
  await app.page.evaluate(`(() => {
    const days = document.querySelector(".addsheet .cyclefields .draftdays");
    days.value = "14";
    days.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await app.page.click(".addbtn");

  const id = "restocktheprinterpaper";
  await app.page.waitFor(`document.querySelector('${row(id)}') !== null`, { timeout: 8000 });
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.recurring === true))`,
    { timeout: 15000 },
  );

  await openInPlace(app.page, id);
  await app.page.waitFor(`document.querySelector('.rowopen[data-todo="${id}"] .act-tend') !== null`, { timeout: 8000 });
  assert.equal(
    await app.page.evaluate<boolean>(`document.querySelector('.rowopen[data-todo="${id}"] .act-done') !== null`),
    false,
    "a recurring todo's row offers Did it, never Done",
  );
  await app.page.click(`.rowopen[data-todo="${id}"] .act-tend`);

  // TENDED, NOT COMPLETED: the row stays on the open list — a pass restarts
  // the cycle, it does not resolve the todo — and the chain agrees.
  await app.page.waitFor(
    `fetch("/api/todo/${id}").then((r) => r.json()).then((b) => b.stream.some((link) => link.kind === "Tended"))`,
    { timeout: 15000 },
  );
  assert.notEqual(await stateOf(id), "completed");
  assert.ok(await app.page.evaluate<boolean>(`document.querySelector('${row(id)}') !== null`), "the row stayed open");
});

// --- editandtick (docs/APP.md §4 items 2-3) -----------------------------------
//
// The first two scenes author a fresh todo, whose id no other file's collides with. The subtodo scene takes
// `pack-bags`, the one fixture todo with a checklist: `authoring::update_todo` edits content, which the lifecycle
// does not gate, so `sync.test.ts` completing it first changes nothing here.

app.scene("editing a body changes the line and the chain follows", async () => {
  await app.open(WRITE_BASE);
  const id = await addTodo(app.page, "Edit me please", "home");
  await openTodo(app.page, id);
  await app.page.waitFor(`document.querySelector(".editbox summary") !== null`, { timeout: 4000 });
  await app.page.click(".editbox summary");
  await app.page.waitFor(`document.querySelector(".editbox")?.open === true`, { timeout: 2000 });
  await app.page.waitFor(`document.querySelector(".editbox .draftbody") !== null`, { timeout: 8000 });
  // The field arrives prefilled with what the chain holds; setting `.value`
  // and firing the same event the keyboard would is what a real edit does to
  // it, without racing `Input.insertText` against the field's own selection.
  await app.page.evaluate(`(() => {
    const node = document.querySelector(".editbox .draftbody");
    node.value = "Edit me please, updated";
    node.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await app.page.click(".editbox .addbtn");
  await app.page.waitFor(`document.querySelector(".todolead")?.textContent?.includes("updated") === true`, {
    timeout: 4000,
  });
  // THE CHAIN FOLLOWS: the box's own record, fetched by the page like any
  // other read, carries the new body — not a guess about what `authorUpdate`
  // sent. The page draws the edit from its replica before the push lands, so
  // the box is waited for.
  await app.page.waitFor(
    `fetch("/api/todo/${id}").then((r) => r.json()).then((b) => b.entry.authored.body === "Edit me please, updated")`,
    { timeout: 15000 },
  );
  await app.page.click(".tilelines .crumbs .back");
  await app.page.waitFor(
    `document.querySelector('${row(id)} .rowtitle')?.textContent?.includes("updated") === true`,
    { timeout: 4000 },
  );
});

// THE TYPST EDITOR. What is typed is markup: the field shades it with typst's own highlight classes, the preview
// under it is the page the worker compiled in the restricted project, a text that does not compile says where and is
// refused by the box in the same words, and once saved the list draws it.
const typeInto = (selector: string, text: string): Promise<unknown> =>
  app.page.evaluate(`(() => {
    const node = document.querySelector(${JSON.stringify(selector)});
    node.value = ${JSON.stringify(text)};
    node.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);

app.scene("the editor highlights markup, previews it live, refuses what does not compile, and the list draws it", async () => {
  await app.open(WRITE_BASE);
  const id = await addTodo(app.page, "Renew the passport", "home");
  await openTodo(app.page, id);
  await app.page.waitFor(`document.querySelector(".editbox summary") !== null`, { timeout: 4000 });
  await app.page.click(".editbox summary");
  await app.page.waitFor(`document.querySelector(".editbox .draftbody") !== null`, { timeout: 8000 });

  await typeInto(".editbox .draftbody", "Renew the *passport* today");
  await app.page.waitFor(`document.querySelector(".editbox .typedshade .typ-strong")?.textContent?.includes("passport") === true`, {
    timeout: 20000,
  });
  await app.page.waitFor(`document.querySelector(".editbox .typedpreview strong")?.textContent === "passport"`, { timeout: 20000 });

  await typeInto(".editbox .draftbody", "Renew the *passport* ] today");
  await app.page.waitFor(`document.querySelector(".editbox .typedproblems li")?.textContent?.startsWith("1:22: unexpected closing bracket") === true`, {
    timeout: 20000,
  });
  assert.ok(await app.page.evaluate<boolean>(`document.querySelector(".editbox .typedshade .typedmark")?.textContent === "]"`), "the span is marked");
  assert.ok(
    await app.page.evaluate<boolean>(`document.querySelector(".editbox .typedpreview.stale strong")?.textContent === "passport"`),
    "the last page that compiled stays, marked stale, so nothing jumps under the cursor",
  );
  await app.page.click(".editbox .addbtn");
  await app.page.waitFor(`document.querySelector(".editbox .formrefusal")?.textContent?.includes("1:22: unexpected closing bracket") === true`, {
    timeout: 8000,
  });

  await typeInto(".editbox .draftbody", "Renew the *passport* today");
  await app.page.waitFor(`document.querySelector(".editbox .typedproblems li") === null`, { timeout: 20000 });
  await app.page.click(".editbox .addbtn");
  await app.page.waitFor(`document.querySelector(".todoview .todobody strong")?.textContent === "passport"`, { timeout: 8000 });
  // The chain holds the markup as written, once the push the page drew ahead of has landed.
  await app.page.waitFor(
    `fetch("/api/todo/${id}").then((r) => r.json()).then((b) => b.entry.authored.body === "Renew the *passport* today")`,
    { timeout: 15000 },
  );
  // The row's HTML is the box's, carried by its fold: rendered when the box adopted the object, read on the next fetch.
  await app.page.click(".tilelines .crumbs .back");
  await app.page.reload();
  await app.page.waitFor(`document.querySelector('${row(id)} .rowtitle strong')?.textContent === "passport"`, { timeout: 20000 });

  await app.page.navigate(`${WRITE_BASE}/#/typst`);
  await app.page.waitFor(
    `[...document.querySelectorAll(".typstframe .ttitle")].some((t) => t.textContent.includes("passport") && t.textContent.includes("today"))`,
    { timeout: 30000 },
  );
});

app.scene("ticking a subtodo moves the chip", async () => {
  await app.page.navigate(`${WRITE_BASE}/#/todo/${todos.checklist.id}`);
  await app.page.waitFor(`document.querySelector(".checklist") !== null`, { timeout: 4000 });
  const chipTextOf = `[...document.querySelectorAll(".todoview .cardmeta .chip")].map((e) => e.textContent).find((t) => /done$/.test(t ?? "")) ?? ""`;
  const before = await app.page.evaluate<string>(chipTextOf);
  assert.match(before, /of 2 done/);
  const links = `fetch("/api/life/chain").then((r) => r.json()).then((c) => c.links.filter((l) => l.todo === "${todos.checklist.id}").length)`;
  const linksBefore = await app.page.evaluate<number>(links);
  await app.page.click('.checklist input[type="checkbox"]:not(:checked)');
  await app.page.waitFor(`(${chipTextOf}) === "2 of 2 done"`, { timeout: 4000 });
  // The chain follows once the tick's push lands, which the chip, drawn from the replica, does not wait for.
  await app.page.waitFor(
    `fetch("/api/todo/${todos.checklist.id}").then((r) => r.json()).then((b) => b.entry.authored.subtodos.every((s) => s.done))`,
    { timeout: 15000 },
  );
  // Drawing the page again runs none of its handlers: the tick is one write, and it stays ticked.
  await new Promise((r) => setTimeout(r, 1500));
  assert.equal((await app.page.evaluate<number>(links)) - linksBefore, 1, "one tick is one write to its todo");
  assert.ok(await app.page.evaluate<boolean>(`[...document.querySelectorAll('.checklist input[type="checkbox"]')].every((c) => c.checked)`), "it stays ticked");
});

// --- Defer, as a curve (docs/APP.md §4 item 4, project chain `deferascurve`) --
//
// Both scenes author a fresh todo, as `editandtick`'s do.

app.scene("deferring a todo takes it to the bottom of the list, fully fulfilled until the day", async () => {
  await app.open(WRITE_BASE);
  const id = await addTodo(app.page, "Defer me until later", "home");
  const before = await everyTodo(app.page);
  const indexBefore = before.indexOf(id);
  assert.ok(indexBefore >= 0);

  await openTodo(app.page, id);
  await app.page.waitFor(`document.querySelector(".deferbox summary") !== null`, { timeout: 4000 });
  await app.page.click(".deferbox summary");
  await app.page.waitFor(`document.querySelector(".deferbox")?.open === true`, { timeout: 2000 });
  const future = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
  await app.page.evaluate(`(() => {
    const node = document.querySelector(".deferbox input[type=date]");
    node.value = "${future}";
    node.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await app.page.click(".deferbox .addbtn");
  await app.page.waitFor(`document.getElementById("toast")?.textContent?.includes("deferred") === true`, {
    timeout: 4000,
  });

  // THE CHAIN FOLLOWS: a deferral to the future reads 1.0 today, exactly like
  // a resolved todo does — "nothing owed before D" IS 100% fulfilled now.
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.find((e) => e.todo === "${id}")?.value === 1)`,
    { timeout: 8000 },
  );
  await app.page.click(".tilelines .crumbs .back");
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 4000 });
  const after = await everyTodo(app.page);
  const indexAfter = after.indexOf(id);
  assert.ok(indexAfter >= 0);
  assert.ok(
    indexAfter > indexBefore,
    `a fully-fulfilled deferred todo sorts toward the bottom (was ${indexBefore}, now ${indexAfter})`,
  );
});

app.scene("a deferral to a date not after today is refused in the box's own words", async () => {
  await app.open(WRITE_BASE);
  const id = await addTodo(app.page, "Do not defer me into the past", "home");
  await openTodo(app.page, id);
  await app.page.waitFor(`document.querySelector(".deferbox summary") !== null`, { timeout: 4000 });
  await app.page.click(".deferbox summary");
  await app.page.waitFor(`document.querySelector(".deferbox")?.open === true`, { timeout: 2000 });
  // Set directly, bypassing the picker's own `min` — the point is that the
  // BOX refuses this, in its own words, not that the browser's date input
  // would have stopped a person from typing it.
  await app.page.evaluate(`(() => {
    const node = document.querySelector(".deferbox input[type=date]");
    node.value = "2020-01-01";
    node.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await app.page.click(".deferbox .addbtn");
  await app.page.waitFor(`document.getElementById("toast")?.textContent?.includes("defer to a date after today") === true`, {
    timeout: 4000,
  });
  assert.equal(await stateOf(id), "open");
});

// --- Undo and redo ------------------------------------------------------------
//
// An undo is an append, so the scene checks the box's fold as well as the DOM. `email-harrow` is the one fixture
// todo no other file on the shared `WRITE_BASE` chain touches.

/** Author a todo through the sheet, the way the scene above does, and return
 * its id (the body's slug) once the box's fold holds it, so a scene that asks
 * the box about it asks after the push. A FRESH id is collision-proof by
 * construction — the todo store derives it from text this call chose. */

/** How far behind the box's clock this scene sets the tab's. A phone's clock runs behind the box's by some amount, and an
 * undo is the one act the box stamps, so for that long every reading the tab draws holds the undo's object and does
 * not yet believe it: the window in which a line could leave, made long enough to see every time. */
const BEHIND_MS = 2000;

app.scene("the toast's Undo button reopens what was just completed, and U redoes it", async () => {
  const id = todos.agents.id;
  const behind = await app.page.beforeEveryDocument(`{ const now = Date.now; Date.now = () => now() - ${BEHIND_MS}; }`);
  try {
    await app.open(WRITE_BASE);
    assert.equal(await stateOf(id), "open");
    await untilReplica(app.page);
    const open = await app.page.evaluate<string>(`document.querySelector('${row(id)}').dataset.worth`);

    await doneFromList(app.page, id);
    // The act takes the line off the OPEN list once the box has it, so the toast's own button — not the list's cursor —
    // is what this scene undoes through.
    await app.page.waitFor(
      `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "completed"))`,
      { timeout: 15000 },
    );
    await app.page.waitFor(`document.querySelector('${row(id)}') === null`, { timeout: 15000 });
    await app.page.waitFor(`document.querySelector("#toast .toastact") !== null`, { timeout: 4000 });

    // The line at every frame and every change to the list from the tap on: absent, or drawn at its worth.
    await app.page.evaluate(`(() => {
      const seen = (window.__line = []);
      const look = () => seen.push(document.querySelector('${row(id)}')?.dataset.worth ?? null);
      window.__lineWatch = new MutationObserver(look);
      window.__lineWatch.observe(document.getElementById("view"), { subtree: true, childList: true, attributes: true });
      const frame = () => { look(); if (window.__lineWatch !== null) requestAnimationFrame(frame); };
      requestAnimationFrame(frame);
    })()`);
    await app.page.click("#toast .toastact");
    await app.page.waitFor(
      `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "open"))`,
      { timeout: 15000 },
    );
    // Settled once a reading that believes the undo is drawn: it prices the line open again.
    await app.page.waitFor(`document.querySelector('${row(id)}')?.dataset.worth === ${JSON.stringify(open)}`, { timeout: 15000 });
    const seen = await app.page.evaluate<(string | null)[]>(`(window.__lineWatch.disconnect(), (window.__lineWatch = null), window.__line)`);
    const back = seen.findIndex((worth) => worth !== null);
    assert.ok(back !== -1, "the line came back");
    assert.ok(!seen.slice(back).includes(null), `once back, the line never left the list before the reading that holds the undo: ${JSON.stringify(seen)}`);

    await app.page.waitFor(
      `(() => { const m = document.querySelector('${row(id)} .rowmain'); m?.focus(); return m !== null && document.activeElement === m; })()`,
    );
    await app.page.press("U");
    await app.page.waitFor(
      `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "completed"))`,
      { timeout: 15000 },
    );
  } finally {
    await behind();
  }
});

// --- the one-line field beside an act ----------------------------------

/**
 * `app/cli/tests/acts.rs` pins both halves over HTTP; this checks that the field beside the button is what reaches
 * them, through `core.lifecycle`'s `note` and `POST /api/objects`. On a fresh todo, as `addTodo` makes one.
 */
app.scene("Done with a line lands on the sealed event and the default conversation", async () => {
  await app.open(WRITE_BASE);
  await untilReplica(app.page);
  const id = await addTodo(app.page, "Call the insurer", "work");
  await app.page.navigate(`${WRITE_BASE}/#/todo/${id}`);
  await app.page.waitFor(`document.querySelector(".actrow .act-done") !== null`, { timeout: 8000 });
  await app.page.type(".actrow .noteline", "the meeting moved to October");
  await app.page.click(".actrow .act-done");
  await app.page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "completed"))`,
    { timeout: 15000 },
  );

  const stream = await app.page.evaluate<{ kind: string; note: string }[]>(
    `fetch("/api/todo/${id}").then((r) => r.json()).then((b) => b.stream)`,
  );
  const completed = stream.find((link) => link.kind === "Completed");
  assert.equal(completed?.note, "the meeting moved to October", "the sealed event carries the line");

  // The default conversation hears it once the box adopts the object (§5).
  await app.page.waitFor(
    `${DEFAULT}.then((b) => b.turns.some((t) => t.who === "act" && t.text === "${owner.name} completed it: the meeting moved to October"))`,
    { timeout: 15000 },
  );
});
