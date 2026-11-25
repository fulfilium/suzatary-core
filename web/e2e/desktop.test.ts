/**
 * The wide layout at desktop size, where the phone's scenes do not look: More drops a sheet whose links a press
 * reaches, the secretary's toggle keeps one place, and a hidden secretary stands in no tile. A list drawn alone scrolls
 * the document; tiles drawn together each scroll themselves, and the page under them fits the window.
 */

import assert from "node:assert/strict";

import { BASE, DESKTOP, MIDDLE, suite, WIDE, WRITE_BASE } from "./harness";
import cast from "../test/cast";

const { todos } = cast.world;

const app = await suite("desktop", DESKTOP);

const more = `[...document.querySelectorAll(".tabbar .tab")].find((t) => t.textContent.trim() === "More")`;

type Box = { x: number; y: number; width: number; height: number };
const box = (selector: string): Promise<Box> =>
  app.page.evaluate<Box>(
    `(() => { const r = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; })()`,
  );

app.scene("More opens a sheet whose links are visible and take a press", async () => {
  await app.open(BASE);
  await app.page.clickNode(more);
  await app.page.waitFor(`document.querySelector(".moresheet") !== null && !document.querySelector(".moresheet").hidden`);
  assert.equal(await app.page.evaluate<string>(`${more}.getAttribute("aria-expanded")`), "true", "More says it is open");
  const links = await app.page.evaluate<{ text: string; shown: boolean }[]>(`[...document.querySelectorAll(".moresheet a")].map((a) => {
    const r = a.getBoundingClientRect();
    const at = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    return { text: a.textContent.trim(), shown: r.height > 0 && r.bottom <= innerHeight && at !== null && a.contains(at) };
  })`);
  assert.ok(links.length > 0, "the sheet holds links");
  assert.deepEqual(links.filter((l) => !l.shown).map((l) => l.text), [], "every link is drawn where a press reaches it");
  await app.page.clickText(".moresheet a", "Mail");
  await app.page.waitFor(`location.hash.startsWith("#/mail")`);
  assert.equal(await app.page.evaluate<string | null>(`${more}?.getAttribute("aria-expanded") ?? null`), "false", "the sheet closes on the place it opened");
});

// First in the file that touches the secretary, so the person has said nothing yet.
app.scene("at 1440 px the header holds still across pages, the todo read is the widest column, and an unused secretary is hidden", async () => {
  await app.resize(WIDE);
  const brands: Box[] = [];
  for (const [hash, ready] of [
    ["#/", `document.querySelectorAll(".tree .row").length > 0`],
    ["#/inbox", `document.querySelector("#view h1") !== null`],
    ["#/secretary/default", `document.querySelector("#view .pagetitle") !== null`],
    ["#/about", `document.querySelector(".aboutversion") !== null`],
  ] as const) {
    await app.page.navigate(`${BASE}/${hash}`);
    await app.page.waitFor(ready);
    brands.push(await box("header .brand"));
  }
  assert.deepEqual(brands.slice(1), brands.slice(0, -1), "the brand is where it was on every page");
  await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`);
  await app.page.waitFor(`document.querySelector("#view.tiled .todoview .todobody") !== null`);
  // Nothing in the conversation is about this todo, so the header offers its door and no tile holds the secretary.
  await app.page.waitFor(`document.querySelector("#beside:not([hidden])") !== null`);
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll("#view .panelbody").length`), 0, "a hidden secretary stands in no tile");
  const list = await box(".tile:has(.todospane)");
  const pane = await box(".tile:has(.todoview)");
  assert.ok(pane.width > list.width, `the todo (${pane.width} px) is wider than the list (${list.width} px)`);
  assert.equal(list.x, brands[0]?.x, "the list starts under the brand");
  await app.resize(DESKTOP);
});

// The list no longer fits beside the todo and the secretary, so it stands behind them rather than squeeze either.
app.scene("at 1100 px with the secretary open, the todo stands beside it under the list's crumb, and hiding it brings the list back", async () => {
  await app.resize(MIDDLE);
  await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`);
  await app.page.waitFor(`document.querySelector("#view.tiled .todoview .todobody") !== null`);
  await app.page.waitFor(`document.querySelector("#beside:not([hidden])") !== null`);
  await app.page.click("#beside");
  await app.page.waitFor(`(document.querySelector("#view.tiled .tile .panelbody .frameheader")?.textContent ?? "") !== "" && document.querySelector(".tilelines .crumbs .back") !== null && document.querySelector("#view .todospane") === null`);
  const pane = await box(".tile:has(.todoview)");
  assert.ok(pane.x + pane.width <= (await box(".tile:has(.panelbody)")).x + 0.5, "the todo stands before the secretary");
  await app.page.click("#view > .tile .hidesecretary");
  await app.page.waitFor(`document.querySelector("#view.tiled .tile .todospane") !== null && document.querySelector("#view .panelbody") === null`);
  // The door is back, a real button where the secretary is not drawn.
  await app.page.waitFor(`!document.querySelector("#beside").hidden && document.querySelector("#beside").getBoundingClientRect().width > 0`);
  await app.resize(DESKTOP);
});

app.scene("the secretary's doors keep their places, a hidden secretary stands in no tile, and tiles drawn together fit the window", async () => {
  // A page of many things: its door is the header's under the wide chrome.
  await app.open(WRITE_BASE, "#/");
  await app.page.waitFor(`document.querySelector("#beside:not([hidden])") !== null`);
  const door = await box("#beside");
  await app.page.click("#beside");
  await app.page.waitFor(`document.querySelector("#view > .tile .panelbody") !== null && document.querySelector("#beside").hidden`);
  assert.ok((await box(".tile:has(.panelbody)")).width > 0, "an open secretary has its tile");
  assert.equal(await app.page.evaluate<string | null>(`document.querySelector("#view > .tile .panelbody").getAttribute("aria-label")`), "Secretary", "a named region");
  const hide = await box("#view > .tile .hidesecretary");
  await app.page.click("#view > .tile .hidesecretary");
  await app.page.waitFor(`!document.querySelector("#beside").hidden && document.querySelector("#view .panelbody") === null`);
  assert.deepEqual(await box("#beside"), door, "the door is where it was");
  await app.page.click("#beside");
  await app.page.waitFor(`document.querySelector("#view > .tile .panelbody") !== null`);
  assert.deepEqual(await box("#view > .tile .hidesecretary"), hide, "and its Hide where it was");
  await app.page.click("#view > .tile .hidesecretary");
  await app.page.waitFor(`document.querySelector("#view .panelbody") === null`);
  // A page about one thing: its door is the header's too, where it was.
  await app.page.evaluate(`location.hash = "#/todo/${todos.flat.id}"`);
  await app.page.waitFor(`document.querySelector("#view .todoview") !== null && !document.querySelector("#beside").hidden`);
  assert.deepEqual(await box("#beside"), door, "the door on a todo is where it is on the list");
  const fits = `document.querySelector("#view").classList.contains("tiled") && document.documentElement.scrollHeight <= innerHeight`;
  await app.page.waitFor(fits);
  await app.page.click("#beside");
  await app.page.waitFor(`document.querySelector("#view > .tile .panelbody") !== null`);
  await app.page.evaluate(`new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())))`);
  await app.page.waitFor(fits);
  await app.page.click("#view > .tile .hidesecretary");
  await app.page.waitFor(`document.querySelector("#view .panelbody") === null`);
});
