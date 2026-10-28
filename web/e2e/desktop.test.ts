/**
 * THE WIDE LAYOUT, AT DESKTOP SIZE: what the owner's browser at a desk sees, where the phone's tests do not look.
 *
 * More drops a sheet whose links a press reaches. The secretary panel's toggle keeps one place, open or closed, and
 * a closed panel takes no width. The Todos page fits the window, so only its panes scroll.
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

/**
 * ONE GRID FOR EVERY PAGE (the 2026-10-13 audit, X4): the brand and the words stand in the same place on every page,
 * a todo beside the list is the wider of the two, and a secretary panel with nothing about the page in it is closed to
 * its toggle until the person opens it. First in the file that touches the panel, so the person has said nothing yet.
 */
app.scene("at 1440 px the header holds still across pages, the todo read is the widest column, and an unused panel is closed", async () => {
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
  await app.page.waitFor(`document.querySelector(".detailpane .todoview .todobody") !== null`);
  await app.page.waitFor(`document.querySelector(".secretarypanel .frameheader")?.textContent.includes("Nothing about") === true`);
  assert.equal(await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`), "false", "nothing in it is about this todo");
  assert.equal((await box(".secretarypanel")).width, 0);
  const list = await box(".listpane");
  const pane = await box(".detailpane");
  assert.ok(pane.width > list.width, `the todo (${pane.width} px) is wider than the list (${list.width} px)`);
  assert.equal(list.x, brands[0]?.x, "the list starts under the brand");
  await app.resize(DESKTOP);
});

/**
 * AT A MIDDLE WIDTH WITH THE PANEL OPEN the todo is still the wider column: the list gives way first, down to 16rem,
 * so the thing being read keeps its room.
 */
app.scene("at 1100 px with the panel open, the todo is wider than the list", async () => {
  await app.resize(MIDDLE);
  await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`);
  await app.page.waitFor(`document.querySelector(".detailpane .todoview .todobody") !== null`);
  await app.page.waitFor(`document.querySelector(".secretarypanel .panelcollapse")?.textContent !== ""`);
  if ((await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "false") {
    await app.page.click(".panelcollapse");
  }
  await app.page.waitFor(`document.querySelector(".panelcollapse").getAttribute("aria-expanded") === "true"`);
  const list = await box(".listpane");
  const pane = await box(".detailpane");
  assert.ok(pane.width > list.width, `the todo (${pane.width} px) is wider than the list (${list.width} px)`);
  await app.page.click(".panelcollapse");
  await app.page.waitFor(`document.querySelector(".panelcollapse").getAttribute("aria-expanded") === "false"`);
  await app.resize(DESKTOP);
});

app.scene("the panel's toggle keeps its place, a closed panel takes no width, and the Todos page fits the window", async () => {
  await app.open(WRITE_BASE);
  await app.page.waitFor(`document.querySelector(".secretarypanel .panelcollapse") !== null`);
  if ((await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "false") {
    await app.page.click(".panelcollapse");
  }
  const open = await box(".panelcollapse");
  assert.ok((await box(".secretarypanel")).width > 0, "an open panel has its column");
  await app.page.click(".panelcollapse");
  await app.page.waitFor(`document.querySelector(".panelcollapse").getAttribute("aria-expanded") === "false"`);
  assert.deepEqual(await box(".panelcollapse"), open, "the toggle is where it was");
  assert.equal((await box(".secretarypanel")).width, 0, "a closed panel takes no width");
  assert.equal(await app.page.evaluate<boolean>(`document.activeElement === document.querySelector(".panelcollapse")`), true, "the toggle keeps the focus");
  // The panel's column closes before the page's height settles, so wait for the height itself.
  const fits = `document.documentElement.scrollHeight <= innerHeight`;
  await app.page.waitFor(fits);
  assert.equal(await app.page.evaluate<boolean>(fits), true, "closed, the page does not scroll; its panes do");
  await app.page.click(".panelcollapse");
  await app.page.waitFor(`document.querySelector(".panelcollapse").getAttribute("aria-expanded") === "true"`);
  assert.deepEqual(await box(".panelcollapse"), open, "and back where it was");
  assert.equal(await app.page.evaluate<boolean>(fits), true, "open, the page does not scroll either");
});
