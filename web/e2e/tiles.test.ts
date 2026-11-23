/**
 * THE TILES AND THE WINDOW (docs/TILEDLAYOUT-PLAN-2026-11-22.md §13). The arrangement asks the room the tiles stand in
 * (the window less the body's gutters and the open panel); the chrome asks the window. Two tiles stand side by side
 * only where the chrome is the wide one, at every width and text size, and no tile is drawn under its least width. The
 * host draws what the desk answers and nothing else: the same drawing changes nothing on the page, a tile kept across
 * a move keeps its element, its focus and where its page stood on the screen, and a write in flight when its tile goes
 * still lands.
 *
 * `SUZATARY_SHOTS` (or a temp directory) receives the pictures of the last scene: the list, a todo beside it and Back,
 * at each width the review names.
 */

import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { BASE, PHONE, SETTLED, WRITE_BASE, addTodo, doneFromList, reveal, row, suite } from "./harness";
import cast from "../test/cast";
import { launch } from "./cdp";
import tokens from "../src/view/tokens";

const { todos } = cast.world;

const app = await suite("tiles");

const OUT = process.env["SUZATARY_SHOTS"] ?? join(tmpdir(), "suzatary-shots");
mkdirSync(OUT, { recursive: true });

const listed = `document.querySelectorAll(".tree .row").length > 0`;
const todoDrawn = `document.querySelector(".tile .todoview .todobody") !== null`;
const beside = `document.querySelector("#view.tiled .tile .todospane") !== null && ${todoDrawn}`;

// What the window shows, read in one go: the chrome's rule, the room, the tiles' boxes, and every bar's edge.
type Box = { top: number; bottom: number };
type Seen = {
  wide: boolean;
  tiled: boolean;
  root: number;
  room: number;
  tiles: { cls: string; x: number; y: number; w: number; bottom: number }[];
  header: number;
  // The bar at the window's foot (a phone's chrome), and the add sheet's handle docked above it.
  foot: Box | null;
  handle: Box | null;
  panel: number | null;
  scrolls: boolean;
  sideways: boolean;
  inner: { w: number; h: number };
};
const SEEN = `(() => {
  const r = (e) => e.getBoundingClientRect();
  const fixed = (e) => e !== null && !e.hidden && getComputedStyle(e).position === "fixed" && r(e).height > 0;
  const bar = document.querySelector(".tabbar");
  const picker = document.querySelector(".addsheet");
  const panel = document.querySelector(".secretarypanel");
  return {
    wide: matchMedia("(min-width: ${tokens.measure.wide / 16}rem)").matches,
    tiled: document.querySelector("#view").classList.contains("tiled"),
    root: parseFloat(getComputedStyle(document.documentElement).fontSize),
    room: r(document.querySelector("#view")).width,
    tiles: [...document.querySelectorAll("#view > .tile")].map((t) => ({ cls: t.querySelector(".todospane") ? "list" : t.querySelector(".todoview") ? "todo" : "other", x: r(t).x, y: r(t).y, w: r(t).width, bottom: r(t).bottom })),
    header: r(document.querySelector("header")).bottom,
    foot: fixed(bar) ? { top: r(bar).top, bottom: r(bar).bottom } : null,
    handle: fixed(picker) && !picker.classList.contains("open") ? { top: r(picker.querySelector(".pickerhandle")).top, bottom: r(picker.querySelector(".pickerhandle")).bottom } : null,
    panel: panel !== null && r(panel).width > 0 ? r(panel).x : null,
    scrolls: document.documentElement.scrollHeight > innerHeight,
    sideways: document.documentElement.scrollWidth > document.documentElement.clientWidth,
    inner: { w: innerWidth, h: innerHeight },
  };
})()`;

// The room in pixels at the 16px root, which `arrange` is given.
const units = (s: Seen): number => (s.room * 16) / s.root;
const TWO = tokens.measure.list + tokens.measure.divider + tokens.measure.column;

// THE LAW: no tile is ever under a bar, in either chrome. Two tiles stand exactly where the room holds a list, a
// divider and a column; each at least its least width; under the header, above the bar at the foot and the add
// sheet's handle, clear of the panel, inside the window; the page does not scroll while they stand, and never sideways.
const agree = (s: Seen, where: string): void => {
  const least = { list: tokens.measure.list, todo: tokens.measure.column, other: 0 };
  assert.equal(s.sideways, false, `${where}: the page scrolls sideways`);
  assert.equal(s.tiled, s.tiles.length > 1, `${where}: tiled exactly when more than one tile stands`);
  assert.equal(s.tiled, units(s) >= TWO, `${where}: a room of ${units(s)} px draws ${s.tiles.length} tile(s)`);
  if (!s.tiled) return;
  for (const t of s.tiles) {
    const at = `${where}: the ${t.cls} (${t.y} to ${t.bottom})`;
    assert.ok(t.w + 0.5 >= (least[t.cls as keyof typeof least] * s.root) / 16, `${where}: the ${t.cls} is ${t.w} px, under its least`);
    assert.ok(t.y + 0.5 >= s.header, `${at} is under the header (to ${s.header})`);
    assert.ok(t.bottom <= s.inner.h + 0.5, `${at} is past the window's foot`);
    if (s.foot !== null) assert.ok(t.bottom <= s.foot.top + 0.5, `${at} is under the bar (from ${s.foot.top})`);
    if (s.handle !== null) assert.ok(t.bottom <= s.handle.top + 0.5, `${at} is under the add sheet's handle (from ${s.handle.top})`);
    if (s.panel !== null) assert.ok(t.x + t.w <= s.panel + 0.5, `${where}: the ${t.cls} is under the panel`);
  }
  assert.equal(s.scrolls, false, `${where}: the page scrolls; its tiles should`);
};

const seen = (): Promise<Seen> => app.page.evaluate<Seen>(SEEN);

// Whether the room at a window of `w` px (16 px text, no panel, no scrollbar) holds two tiles.
const twoAt = (w: number): boolean => w - 2 * tokens.measure.gutter >= TWO;

const panelOpen = async (open: boolean): Promise<void> => {
  await app.page.waitFor(`document.querySelector(".secretarypanel .panelcollapse") !== null`);
  const now = (await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "true";
  if (now !== open) await app.page.click(".panelcollapse");
  await app.page.waitFor(`document.querySelector(".panelcollapse").getAttribute("aria-expanded") === "${open}"`);
  await app.page.evaluate(SETTLED);
};

app.scene("from 840 to 1010 px, with the panel and without, at the browser's text size and at 20 px, no tile is under a bar", async () => {
  for (const text of [16, 20]) {
    await app.page.textSize(text);
    const px = (w: number): number => Math.round((w * text) / 16);
    await app.resize([px(1010), 800]);
    await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`);
    await app.page.waitFor(todoDrawn, { timeout: 20000 });
    for (let w = 840; w <= 1010; w += 1) {
      await app.resize([px(w), 800]);
      const s = await seen();
      assert.equal(s.root, text, "the browser's text size is the root's");
      agree(s, `${px(w)} px at ${text} px text`);
    }
    // The panel is the wide chrome's: from the edge, open, the room is what it leaves.
    await panelOpen(true);
    for (let w = 992; w <= 1010; w += 1) {
      await app.resize([px(w), 800]);
      agree(await seen(), `${px(w)} px at ${text} px text, the panel open`);
    }
    await panelOpen(false);
  }
  await app.page.textSize(16);
  await app.resize(PHONE);
});

app.scene("with classic scrollbars, the tiles a window draws do not depend on whether the document scrolls", async () => {
  // A browser of its own: the suite's hides scrollbars, as headless browsers do.
  const browser = await launch("Shown");
  try {
    const page = browser.page;
    for (const w of [866, 880, 1000]) {
      await page.viewport(w, 700);
      await page.navigate(`${BASE}/#/`);
      await page.waitFor(listed, { timeout: 20000 });
      const bar = await page.evaluate<number>(`innerWidth - document.documentElement.clientWidth`);
      assert.ok(bar > 0, `${w} px: the browser draws a scrollbar beside the list`);
      // The list scrolls the document; a todo beside it does not, and a short one opened alone does not either.
      await page.evaluate(`window.scrollTo(0, 300)`);
      await page.evaluate(`location.hash = "#/todo/${todos.flat.id}"`);
      await page.waitFor(todoDrawn, { timeout: 20000 });
      await page.evaluate(SETTLED);
      const s = await page.evaluate<Seen>(SEEN);
      agree({ ...s, room: s.room + (s.scrolls ? bar : 0) }, `${w} px with scrollbars`);
      assert.equal(s.tiled, twoAt(w), `${w} px with scrollbars: the tiles follow the window, not the scrollbar`);
      // Drawn again for the same window, the same tiles: nothing flips once the scrollbar comes or goes.
      await page.evaluate(`window.dispatchEvent(new Event("resize"))`);
      await page.evaluate(SETTLED);
      await page.evaluate(SETTLED);
      assert.equal(await page.evaluate<boolean>(`document.querySelector("#view").classList.contains("tiled")`), s.tiled, `${w} px: the tiles flipped`);
    }
  } finally {
    await browser.close();
  }
});

app.scene("at 1100 px with the panel open, the todo stands alone, and at 1440 px both stand at their least or more", async () => {
  await app.resize([1440, 800]);
  await app.open(WRITE_BASE, `#/todo/${todos.flat.id}`);
  for (const [w, two] of [
    [1100, false],
    [1440, true],
  ] as const) {
    await app.resize([w, 800]);
    await app.page.waitFor(`document.querySelector(".secretarypanel .panelcollapse") !== null`);
    if ((await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "false") await app.page.click(".panelcollapse");
    await app.page.waitFor(`document.querySelector(".panelcollapse").getAttribute("aria-expanded") === "true"`);
    await app.page.evaluate(SETTLED);
    await app.page.waitFor(two ? beside : `!document.querySelector("#view").classList.contains("tiled") && ${todoDrawn}`, { timeout: 20000 });
    const s = await seen();
    agree(s, `${w} px, panel open`);
    const panel = await app.page.evaluate<number>(`document.querySelector(".secretarypanel").getBoundingClientRect().x`);
    for (const t of s.tiles) assert.ok(t.x + t.w <= panel + 0.5, `${w} px: a tile stands under the panel`);
    await app.page.click(".panelcollapse");
    await app.page.waitFor(`document.querySelector(".panelcollapse").getAttribute("aria-expanded") === "false"`);
  }
  // Closed again, the list comes back beside the todo at 1100 px.
  await app.resize([1100, 800]);
  await app.page.waitFor(beside, { timeout: 20000 });
  agree(await seen(), "1100 px, panel closed");
  await app.resize(PHONE);
});

app.scene("a resize across the edge with a todo open remounts nothing that stays: a half-typed note survives", async () => {
  await app.resize([1100, 800]);
  await app.open(WRITE_BASE, `#/todo/${todos.flat.id}`);
  await app.page.waitFor(beside, { timeout: 20000 });
  const field = `.todoview input.noteline`;
  await app.page.waitFor(`document.querySelector("${field}") !== null`);
  await app.page.evaluate(`document.querySelector(".todoview").dataset.mark = "kept"`);
  await app.page.type(field, "half a thought");
  for (const w of [840, 1100, 700, 900, 1440]) {
    await app.resize([w, 800]);
    await app.page.waitFor(twoAt(w) ? beside : `!document.querySelector("#view").classList.contains("tiled") && ${todoDrawn}`, { timeout: 20000 });
    agree(await seen(), `${w} px`);
    assert.equal(await app.page.evaluate<string | undefined>(`document.querySelector(".todoview").dataset.mark`), "kept", `${w} px: the todo was mounted again`);
    assert.equal(await app.page.evaluate<string>(`document.querySelector("${field}").value`), "half a thought");
    assert.equal(await app.page.evaluate<boolean>(`document.activeElement === document.querySelector("${field}")`), true, `${w} px: the note lost the keyboard`);
  }
  await app.resize(PHONE);
});

app.scene("the same drawing changes nothing on the page, and a resize that keeps the tiles writes only their places", async () => {
  await app.resize([1440, 900]);
  await app.open(WRITE_BASE, `#/todo/${todos.flat.id}`);
  await app.page.waitFor(beside, { timeout: 20000 });
  await app.page.evaluate(`document.querySelector(".todoview input.noteline").focus()`);
  const watch = `(() => { window.__muts = []; new MutationObserver((ms) => window.__muts.push(...ms.map((m) => [m.type, m.target.className ?? "", m.attributeName ?? ""]))).observe(document.querySelector("#view"), { subtree: true, childList: true, attributes: true }); })()`;
  const taken = `new Promise((done) => setTimeout(() => done(window.__muts.filter(([, cls]) => cls === "tile" || cls === "tilelines" || cls === "tileline" || cls === "crumbs" || cls.includes("tiled"))), 50))`;
  await app.page.evaluate(watch);
  await app.page.evaluate(`window.dispatchEvent(new Event("resize"))`);
  await app.page.evaluate(SETTLED);
  assert.deepEqual(await app.page.evaluate<string[][]>(taken), [], "an arrangement drawn again wrote to the page");
  await app.page.evaluate(`window.__muts = []`);
  await app.resize([1400, 900]);
  const wrote = await app.page.evaluate<string[][]>(taken);
  assert.ok(wrote.length > 0, "the tiles were placed again");
  assert.deepEqual(wrote.filter(([type, , attr]) => type !== "attributes" || attr !== "style"), [], "a resize that keeps the tiles writes only their styles");
  assert.equal(await app.page.evaluate<boolean>(`document.activeElement === document.querySelector(".todoview input.noteline")`), true, "the note keeps the keyboard");
  await app.resize(PHONE);
});

app.scene("on a phone, a crumb row with the keyboard keeps it while the window's height moves, and a short window takes no tile away", async () => {
  await app.resize(PHONE);
  await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`);
  await app.page.waitFor(`document.querySelector(".tilelines .crumbs .back") !== null && ${todoDrawn}`, { timeout: 20000 });
  await app.page.evaluate(`document.querySelector(".tilelines .crumbs .back").focus()`);
  for (const h of [500, 300, PHONE[1]]) {
    await app.resize([PHONE[0], h]);
    assert.equal(await app.page.evaluate<boolean>(`document.activeElement === document.querySelector(".tilelines .crumbs .back")`), true, `${h} px tall: the Back crumb lost the keyboard`);
  }
  // A desk's window docked short (developer tools, a tablet's keyboard): the list stays beside the todo.
  await app.resize([1440, 900]);
  await app.page.waitFor(beside, { timeout: 20000 });
  await app.page.evaluate(`document.querySelector(".todospane").dataset.mark = "kept"`);
  for (const h of [360, 200]) {
    await app.resize([1440, h]);
    const s = await seen();
    assert.deepEqual(s.tiles.map((t) => t.cls), ["list", "todo"], `${h} px tall: a tile was taken away`);
    assert.equal(await app.page.evaluate<string | undefined>(`document.querySelector(".todospane").dataset.mark`), "kept");
  }
  await app.resize(PHONE);
});

app.scene("at 1440 px the row clicked stays under the pointer when its todo opens, and Back puts the list where it was", async () => {
  await app.resize([1440, 420]);
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(listed);
  await reveal(app.page, todos.urgent.id);
  const rows = await app.page.evaluate<number>(`document.querySelectorAll(".tree .row").length`);
  const pick = await app.page.evaluate<string>(`[...document.querySelectorAll(".tree .row")].at(-2).dataset.todo`);
  assert.ok(rows > 4, "the list is longer than the window");
  await app.page.evaluate(`document.querySelector('${row("PICK")}'.replace("PICK", ${JSON.stringify(pick)})).scrollIntoView({ block: "center" })`);
  await app.page.evaluate(SETTLED);
  const at = `Math.round(document.querySelector('.tree .row[data-todo="${pick}"]').getBoundingClientRect().top)`;
  const before = await app.page.evaluate<number>(at);
  const scrolled = await app.page.evaluate<number>(`Math.round(window.scrollY)`);
  assert.ok(scrolled > 0, "the document scrolled to the row");
  await app.page.click(`.tree .row[data-todo="${pick}"] .rowmain`);
  await app.page.waitFor(`location.hash === "#/todo/${pick}" && ${beside}`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<number>(at), before, "the clicked row moved when its todo opened beside it");
  // A click that focuses nothing (a button in Safari, a tap), then a click inside the todo: what anchors the list is
  // what the person sees, not what has the keyboard.
  await app.page.evaluate(`document.activeElement?.blur()`);
  await app.page.click(".tile .todoview .todobody");
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".todospane").contains(document.activeElement)`), false, "the list has the keyboard");
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/" && !document.querySelector("#view").classList.contains("tiled") && document.querySelector(".todoview") === null`);
  await app.page.evaluate(SETTLED);
  assert.equal(await app.page.evaluate<number>(at), before, "Back did not put the list where it was");
  assert.equal(await app.page.evaluate<number>(`Math.round(window.scrollY)`), scrolled);
  await app.resize(PHONE);
});

app.scene("on a phone the add sheet's handle is docked over the bar and its scrim covers the window, inside a tile", async () => {
  await app.resize(PHONE);
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(listed);
  // Docked against the window's foot, not the foot of the tile it stands in: the document's scroll does not move it.
  const handle = `Math.round(document.querySelector(".addsheet .pickerhandle").getBoundingClientRect().bottom)`;
  // From a desk's width the sheet slides to its dock (a transition): read it once it rests.
  await app.page.waitFor(`document.getAnimations().length === 0`);
  const top = await app.page.evaluate<number>(handle);
  await app.page.evaluate(`window.scrollTo(0, 400)`);
  await app.page.evaluate(SETTLED);
  assert.ok(await app.page.evaluate<number>(`window.scrollY`) > 0, "the list scrolls the document");
  assert.equal(await app.page.evaluate<number>(handle), top, "the handle moved with the document");
  assert.ok(top > PHONE[1] - 120 && top <= PHONE[1], `the handle ends at ${top}, not at the window's foot`);
  await app.page.click(".addsheet .pickerhandle");
  await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  const scrim = await app.page.evaluate<{ x: number; y: number; w: number; h: number }>(`(() => { const r = document.querySelector(".scrim").getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; })()`);
  assert.deepEqual(scrim, { x: 0, y: 0, w: PHONE[0], h: PHONE[1] }, "the scrim covers the window");
  await app.page.evaluate(`document.querySelector(".scrim").click()`);
  await app.page.waitFor(`document.querySelector(".addsheet.open") === null`);
});

app.scene("a write in flight when its tile goes still lands, and says so", async () => {
  await app.open(WRITE_BASE);
  const id = await addTodo(app.page, "Defer me while away", "home");
  const other = await addTodo(app.page, "Read me meanwhile", "home");
  await app.resize([1440, 900]);
  await app.page.navigate(`${WRITE_BASE}/#/todo/${id}`);
  await app.page.waitFor(`${beside} && document.querySelector(".deferbox summary") !== null`, { timeout: 20000 });
  await app.page.click(".deferbox summary");
  const future = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
  await app.page.evaluate(`(() => { const n = document.querySelector(".deferbox input[type=date]"); n.value = "${future}"; n.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  // The box's print of the deferral is held: the todo's tile is closed before it answers.
  const release = await app.page.holdRequests(["/author"]);
  await app.page.click(".deferbox .addbtn");
  await app.page.untilHeld();
  await reveal(app.page, other);
  await app.page.click(`${row(other)} .rowmain`);
  await app.page.waitFor(`location.hash === "#/todo/${other}" && document.querySelector(".tile .todoview") !== null`, { timeout: 20000 });
  await release();
  // The print answers a program whose tile is gone: it seals the deferral, and says so.
  await app.page.waitFor(`document.getElementById("toast")?.textContent?.includes("deferred") === true`, { timeout: 15000 });
  await app.page.waitFor(`fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.find((e) => e.todo === "${id}")?.value === 1)`, { timeout: 15000 });
  await app.resize(PHONE);
});

app.scene("the notice of a write made from a scrolled list is in view, with its Undo", async () => {
  await app.resize(PHONE);
  await app.open(WRITE_BASE);
  const id = await addTodo(app.page, "Notice me from below", "home");
  // Short enough that the list scrolls the document whatever the writer's chain holds by now.
  await app.resize([PHONE[0], 360]);
  await app.page.evaluate(`window.scrollTo(0, document.documentElement.scrollHeight)`);
  await app.page.evaluate(SETTLED);
  assert.ok((await app.page.evaluate<number>(`window.scrollY`)) > 0, "the list scrolls the document");
  await doneFromList(app.page, id);
  await app.page.waitFor(`document.getElementById("toast")?.hidden === false && document.querySelector("#toast .toastact") !== null`, { timeout: 15000 });
  const r = await app.page.evaluate<{ top: number; bottom: number }>(`(() => { const r = document.getElementById("toast").getBoundingClientRect(); return { top: r.top, bottom: r.bottom }; })()`);
  assert.ok(r.top >= 0 && r.bottom <= 360, `the notice stands at ${r.top} to ${r.bottom}, out of the window`);
  assert.ok((await app.page.evaluate<number>(`window.scrollY`)) > 0, "the page stays where it was scrolled");
  await app.resize(PHONE);
});

// The table of the description: at each window width, with the panel closed and (where the wide chrome has one) open,
// which tiles stand and how wide, measured; each obeys the law.
app.scene("at 390 to 1,920 px, with the panel closed and open, the tiles obey the law; their widths are printed", async () => {
  const rows: string[] = [];
  for (const w of [390, 768, 900, 988, 1024, 1100, 1280, 1440, 1920]) {
    await app.resize([w, 900]);
    await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`);
    await app.page.waitFor(todoDrawn, { timeout: 20000 });
    for (const open of w >= tokens.measure.wide ? [false, true] : [false]) {
      if (w >= tokens.measure.wide) await panelOpen(open);
      await app.page.evaluate(SETTLED);
      const s = await seen();
      agree(s, `${w} px, panel ${open ? "open" : "closed"}`);
      rows.push(`| ${w} | ${open ? "open" : "closed"} | ${s.tiles.map((t) => `${t.cls} ${Math.round(t.w)}`).join(", ")} |`);
    }
    if (w >= tokens.measure.wide) await panelOpen(false);
  }
  console.log(["TABLE", ...rows].join("\n"));
  await app.resize(PHONE);
});

// Pictures for the eye: the list, a todo beside it (or over it), and Back, at each width the review names.
app.scene("pictures of the list, a todo and Back at 390, 988, 1100 with the panel open, 1440 and 1920 px", async () => {
  await app.open(BASE);
  for (const [w, panel] of [
    [390, false],
    [988, false],
    [1100, true],
    [1440, false],
    [1920, false],
  ] as const) {
    await app.resize([w, 900]);
    await app.page.navigate(`${BASE}/#/`);
    await app.page.waitFor(listed);
    if (panel) {
      await app.page.waitFor(`document.querySelector(".secretarypanel .panelcollapse") !== null`);
      if ((await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "false") await app.page.click(".panelcollapse");
    }
    await app.page.evaluate(SETTLED);
    await app.page.screenshot(join(OUT, `tiles-${w}-list.png`));
    await reveal(app.page, todos.urgent.id);
    await app.page.evaluate(`location.hash = "#/todo/${todos.urgent.id}"`);
    await app.page.waitFor(todoDrawn, { timeout: 20000 });
    // The panel's press is kept per page, so it is opened on the todo too.
    if (panel && (await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "false") await app.page.click(".panelcollapse");
    await app.page.evaluate(SETTLED);
    agree(await seen(), `${w} px`);
    await app.page.screenshot(join(OUT, `tiles-${w}-todo.png`));
    await app.page.evaluate(`history.back()`);
    await app.page.waitFor(`location.hash === "#/" && document.querySelector(".todoview") === null && ${listed}`);
    await app.page.evaluate(SETTLED);
    await app.page.screenshot(join(OUT, `tiles-${w}-back.png`));
    if (panel && (await app.page.evaluate<string>(`document.querySelector(".panelcollapse").getAttribute("aria-expanded")`)) === "true") await app.page.click(".panelcollapse");
  }
  await app.resize(PHONE);
});
