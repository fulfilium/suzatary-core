/**
 * The route is a register and the browser's history is its log: back after
 * going somewhere is the place as it was left, drawn again from the readings
 * and the ui registers alone, and a deep link is a route printed.
 */

import assert from "node:assert/strict";

import { BASE, DESKTOP, PHONE, openInPlace, reveal, row, suite } from "./harness";
import cast from "../test/cast";

const { todos } = cast.world;

const app = await suite("history");

// The view as drawn, the window's scroll and the keyed node that has the keyboard: what back must give back.
const VIEW = `({
  html: new XMLSerializer().serializeToString(document.querySelector("#view")),
  scroll: Math.round(window.scrollY),
  focus: document.activeElement?.closest("[data-key]")?.getAttribute("data-key") ?? "",
  hash: location.hash,
})`;
type View = { html: string; scroll: number; focus: string; hash: string };

const listed = `document.querySelectorAll(".tree .row").length > 0`;

app.scene("back after going to a todo is the list as it was left, and forward is the todo again", async () => {
  await app.page.viewport(PHONE[0], 520);
  // The Inbox badge lands on its own time, so the list is read as it was left only once it has; what it counts is the
  // cards the Inbox draws and its lines with no card, counted there first.
  await app.page.navigate(`${BASE}/#/inbox`);
  await app.page.waitFor(`document.querySelector('.inboxpage .proposals[data-box="Answered"]') !== null`);
  const waits = await app.page.evaluate<number>(`document.querySelectorAll(".inboxpage .batch, .inboxpage .proposal, .inboxpage .unplaced .problem").length`);
  const badge = waits === 0 ? "nothing waits on you" : `${waits} ${waits === 1 ? "waits" : "wait"} on you`;
  await app.page.navigate(`${BASE}/#/`);
  // The deployment's categories are a reading too: the add sheet draws them once the box has said who reads. So is how
  // far setup is, which this deployment has not finished: the first screen says so once the box has answered.
  await app.page.waitFor(`${listed} && document.querySelector("#addsheet .chiprow button") !== null && document.querySelector(".inboxlink")?.textContent === ${JSON.stringify(badge)} && document.querySelector(".setupunfinished") !== null`);
  await openInPlace(app.page, todos.urgent.id);
  await reveal(app.page, todos.agents.id);
  await app.page.evaluate(`document.querySelector('${row(todos.agents.id)} .rowmain').focus({ preventScroll: true })`);
  await app.page.evaluate(`window.scrollTo(0, 120)`);
  await app.page.waitFor(`Math.round(window.scrollY) === 120`);
  const before = await app.page.evaluate<View>(VIEW);
  assert.notEqual(before.focus, "", "a row has the keyboard");

  // A link followed as a script follows one: neither the focus nor the scroll moves before the place is left.
  await app.page.evaluate(`document.querySelector('.rowopen[data-todo="${todos.urgent.id}"] a.capsule.primary').click()`);
  await app.page.waitFor(`location.hash === "#/todo/${todos.urgent.id}" && document.querySelector(".todoview") !== null`, { timeout: 20000 });
  await app.page.evaluate(`history.back()`);
  // The rows draw from the fold at once and the reading lands after them; the list compared is the one it lands on.
  await app.page.waitFor(`location.hash === "#/" && ${listed} && document.querySelector(".tree").getAttribute("data-at") !== ""`);
  const restored = `document.activeElement?.closest("[data-key]")?.getAttribute("data-key") === ${JSON.stringify(before.focus)} && Math.round(window.scrollY) === ${before.scroll}`;
  await app.page.waitFor(restored, { timeout: 8000 }).catch(async (e: unknown) => {
    const now = await app.page.evaluate<View>(VIEW);
    throw new Error(`${String(e)}; now focus ${now.focus} scroll ${now.scroll}, was ${before.focus} ${before.scroll}`);
  });
  const after = await app.page.evaluate<View>(VIEW);
  const at = [...after.html].findIndex((c, i) => c !== before.html[i]);
  assert.equal(after.html, before.html, `the same readings and registers draw the same list; first differs at ${at}: ${before.html.slice(at - 200, at + 200)} | now ${after.html.slice(at - 200, at + 300)}`);
  assert.deepEqual(after, before, "scrolled and focused where it was");

  await app.page.evaluate(`history.forward()`);
  await app.page.waitFor(`location.hash === "#/todo/${todos.urgent.id}" && document.querySelector(".todoview") !== null`, { timeout: 20000 });
  await app.page.viewport(PHONE[0], PHONE[1]);
});

app.scene("a deep link is a route printed, and its back is the fallback in its place", async () => {
  await app.page.navigate("about:blank");
  await app.page.navigate(`${BASE}/#/todo/${todos.urgent.id}`);
  await app.page.waitFor(`document.querySelector(".todoview .back") !== null`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<string>(`location.hash`), `#/todo/${todos.urgent.id}`);
  const length = await app.page.evaluate<number>(`history.length`);
  await app.page.click(".todoview .back");
  await app.page.waitFor(`location.hash === "#/" && ${listed}`);
  assert.equal(await app.page.evaluate<number>(`history.length`), length, "nothing lay behind the link, so back wrote no entry");
});

app.scene("a hash typed by hand is a place in the log, and a page that only forwards is never one", async () => {
  await app.page.navigate(`${BASE}/#/about`);
  await app.page.waitFor(`document.querySelector("#view .pagetitle") !== null`);
  await app.page.evaluate(`location.hash = "#/secretary?ref=todo:life/${todos.urgent.id}"`);
  await app.page.waitFor(`location.hash.startsWith("#/secretary/default?ref=")`, { timeout: 15000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/about"`);
});

app.scene("on a desktop, back from a selected todo unselects it and keeps the list mounted", async () => {
  await app.page.viewport(DESKTOP[0], DESKTOP[1]);
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(listed);
  await app.page.evaluate(`document.querySelector(".tree").dataset.mark = "kept"`);
  await reveal(app.page, todos.urgent.id);
  await app.page.click(`${row(todos.urgent.id)} .rowmain`);
  await app.page.waitFor(`location.hash === "#/todo/${todos.urgent.id}" && document.querySelector(".detailpane .todoview") !== null`, { timeout: 20000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/" && document.querySelector(".detailpane .panehint") !== null`);
  assert.equal(await app.page.evaluate<string | undefined>(`document.querySelector(".tree").dataset.mark`), "kept");
  await app.page.viewport(PHONE[0], PHONE[1]);
});
