/**
 * THE TILES AND THE WINDOW (docs/TILEDLAYOUT-PLAN-2026-11-22.md §13). The arrangement asks the room the tiles stand in
 * (the window less the body's gutters and the open panel); the chrome asks the window. Two tiles stand side by side
 * only where the chrome is the wide one, at every width and text size, and no tile is drawn under its least width. The
 * host draws what the desk answers and nothing else: the same drawing changes nothing on the page, a tile kept across
 * a move keeps its element, its focus and where its page stood on the screen, and a write in flight when its tile goes
 * still lands.
 *
 * `SUZATARY_SHOTS` (or a temp directory) receives the pictures of the last scene: the list, a todo beside it and Back,
 * at 390, 988, 1,100 (with the panel open), 1,440 and 1,920 px; and the pictures of the scenes after it.
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
    tiles: [...document.querySelectorAll("#view > .tile")].map((t) => ({ cls: t.querySelector(".todospane, .inboxpage") ? "list" : t.querySelector(".todoview") ? "todo" : "other", x: r(t).x, y: r(t).y, w: r(t).width, bottom: r(t).bottom })),
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
  assert.equal(s.tiled, units(s) >= TWO, `${where}: a room of ${units(s)} px draws ${s.tiles.length} tile(s)`);
  stand(s, where);
};

// The part of the law that holds for any tiles (a page read as a column, `other`, among them): each at least its least
// width, under the header, above every bar and the window's foot, clear of the panel; the page never scrolls sideways,
// nor at all while tiles stand together.
const stand = (s: Seen, where: string): void => {
  const least = { list: tokens.measure.list, todo: tokens.measure.column, other: tokens.measure.column };
  assert.equal(s.sideways, false, `${where}: the page scrolls sideways`);
  assert.equal(s.tiled, s.tiles.length > 1, `${where}: tiled exactly when more than one tile stands`);
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

app.scene("on a phone the add sheet's handle is docked over the bar, and its scrim, the window's, covers the window", async () => {
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

// What stands where at each window width, with the panel closed and (where the wide chrome has one) open, and how wide,
// printed as a table: a todo opened from the list, which obeys the law, then a message opened from that todo.
app.scene("at 390 to 1,920 px, with the panel closed and open, the tiles obey the law; their widths are printed", async () => {
  const rows: string[] = [];
  for (const w of [390, 768, 900, 988, 1024, 1100, 1280, 1440, 1920]) {
    for (const open of w >= tokens.measure.wide ? [false, true] : [false]) {
      await app.resize([w, 900]);
      await app.page.navigate(`${BASE}/#/todo/${todos.agents.id}`);
      await app.page.waitFor(`${todoDrawn} && document.querySelector(".tile .todoview .sourcelink") !== null`, { timeout: 20000 });
      if (w >= tokens.measure.wide) await panelOpen(open);
      await app.page.evaluate(SETTLED);
      const s = await seen();
      agree(s, `${w} px, panel ${open ? "open" : "closed"}`);
      const widths = (t: Seen["tiles"]): string => t.map((x) => `${x.cls === "other" ? "message" : x.cls} ${Math.round(x.w)}`).join(", ");
      await app.page.click(".tile .todoview .sourcelink");
      await app.page.waitFor(`location.hash.startsWith("#/mail/") && document.querySelectorAll("#view > .tile:not(:has(.todospane)):not(:has(.todoview))").length === 1`, { timeout: 20000 });
      await app.page.evaluate(SETTLED);
      const message = await seen();
      stand(message, `${w} px, panel ${open ? "open" : "closed"}, the message opened`);
      rows.push(`| ${w} | ${open ? "open" : "closed"} | ${widths(s.tiles)} | ${widths(message.tiles)} |`);
      if (w >= tokens.measure.wide) await panelOpen(false);
    }
  }
  console.log(["TABLE", ...rows].join("\n"));
  await app.resize(PHONE);
});

// Pictures for the eye: the list, a todo beside it (or over it), and Back.
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

// The tiles on screen, in the room's order: what each draws, and its box.
type Shown = { what: string; x: number; w: number; scroll: number }[];
const SHOWN = `[...document.querySelectorAll("#view > .tile")].map((t) => {
  const r = t.getBoundingClientRect();
  const what = t.querySelector(".todospane") ? "list" : t.querySelector(".todoview") ? "todo" : t.querySelector(".mailpage, .messagepage, .mailview, .mailbody, .mailfacts, .mailid") ? "message" : "other";
  return { what, x: r.x, w: r.width, scroll: t.scrollTop };
})`;
const shown = (): Promise<Shown> => app.page.evaluate<Shown>(SHOWN);
const whats = async (): Promise<string> => (await shown()).map((t) => t.what).join(" ");
const source = `document.querySelector(".tile .todoview .sourcelink")`;

// Opens the agent's todo, whose source is a message, and marks its tile's element to be found again.
const openAgents = async (w: number): Promise<void> => {
  await app.resize([w, 900]);
  await app.page.navigate(`${BASE}/#/todo/${todos.agents.id}`);
  await app.page.waitFor(`${source} !== null`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  await app.page.evaluate(`window.__todoTile = ${source}.closest(".tile")`);
};

app.scene("at a desk's width, a message opened from a todo stands beside it, and Back closes it and leaves the todo where it was", async () => {
  for (const w of [1440, 1920]) {
    await openAgents(w);
    assert.equal(await whats(), "list todo", `${w} px: the todo beside the list`);
    const before = await shown();
    // The click brings the link into view, as a person scrolls to it: the todo's scroll is read once it opened.
    await app.page.click(".tile .todoview .sourcelink");
    await app.page.waitFor(`document.querySelectorAll("#view > .tile").length === 3`, { timeout: 20000 });
    await app.page.evaluate(SETTLED);
    const opened = await shown();
    assert.deepEqual(opened.map((t) => t.what).slice(0, 2), ["list", "todo"], `${w} px: the list and the todo stay`);
    assert.ok((opened[2]?.x ?? 0) > (opened[1]?.x ?? 0), `${w} px: the message stands after the todo`);
    assert.equal(await app.page.evaluate<boolean>(`window.__todoTile.isConnected && window.__todoTile.parentElement.id === "view"`), true, "the todo's tile is the same element");
    assert.ok((await app.page.evaluate<string>(`location.hash`)).startsWith("#/mail/e2e-message-hash~"), "the address names the message, and the tiles it stands among");
    await app.page.screenshot(join(OUT, `open-${w}-beside.png`));
    await app.page.evaluate(`history.back()`);
    await app.page.waitFor(`location.hash === "#/todo/${todos.agents.id}" && document.querySelectorAll("#view > .tile").length === 2`, { timeout: 20000 });
    await app.page.evaluate(SETTLED);
    const back = await shown();
    assert.equal(back.map((t) => t.what).join(" "), "list todo");
    assert.equal(await app.page.evaluate<boolean>(`window.__todoTile.isConnected`), true, "Back keeps the todo's tile");
    assert.equal(back[1]?.scroll, opened[1]?.scroll, "the todo is scrolled where it was");
    assert.equal(back[1]?.w, before[1]?.w, "the todo is as wide as it was");
    await app.page.screenshot(join(OUT, `open-${w}-back.png`));
  }
  await app.resize(PHONE);
});

app.scene("at a phone's width and under a phone's chrome, a message opened from a todo is drawn over it, and Back is the todo", async () => {
  for (const w of [412, 960]) {
    await openAgents(w);
    const todo = w === 412 ? "todo" : "list todo";
    assert.equal(await whats(), todo, `${w} px`);
    await app.page.click(".tile .todoview .sourcelink");
    await app.page.waitFor(`location.hash.startsWith("#/mail/e2e-message-hash") && document.querySelector(".tile .todoview") === null`, { timeout: 20000 });
    await app.page.evaluate(SETTLED);
    assert.equal((await shown()).length, 1, `${w} px: the message alone`);
    assert.deepEqual(
      await app.page.evaluate<string[]>(`[...document.querySelectorAll(".tilelines .crumbs > *")].map((c) => c.textContent.trim())`),
      ["Todos", "Todo"],
      `${w} px: the crumbs name what is behind`,
    );
    await app.page.screenshot(join(OUT, `open-${w}-over.png`));
    // The crumb just before is Back.
    await app.page.click(".tilelines .crumbs > button.back");
    await app.page.waitFor(`location.hash === "#/todo/${todos.agents.id}" && ${source} !== null`, { timeout: 20000 });
    assert.equal(await whats(), todo, `${w} px: Back is the todo`);
    await app.page.screenshot(join(OUT, `open-${w}-back.png`));
  }
  await app.resize(PHONE);
});

// The divider between the list and the todo, read from the page.
type Divider = { x: number; y: number; w: number; h: number; now: number; max: number; list: number; todo: number };
const DIVIDER = `(() => {
  const d = document.querySelector(".tileline");
  const r = d.getBoundingClientRect();
  const tiles = [...document.querySelectorAll("#view > .tile")].map((t) => t.getBoundingClientRect().width);
  return { x: r.x, y: r.y, w: r.width, h: r.height, now: Number(d.getAttribute("aria-valuenow")), max: Number(d.getAttribute("aria-valuemax")), list: tiles[0], todo: tiles[1] };
})()`;
const divider = (): Promise<Divider> => app.page.evaluate<Divider>(DIVIDER);

app.scene("a divider dragged with a mouse and with a finger snaps to its stops, never under a least width, and the keyboard steps it", async () => {
  await app.resize([1440, 900]);
  await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`);
  await app.page.waitFor(beside, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  const first = await divider();
  assert.ok(first.max >= 2, `the divider has ${first.max + 1} stops`);
  const least = { list: tokens.measure.list, todo: tokens.measure.column };
  const held = (d: Divider, how: string): void => {
    assert.ok(d.list + 0.5 >= least.list && d.todo + 0.5 >= least.todo, `${how}: the list is ${d.list} px and the todo ${d.todo} px`);
  };
  const middle = (d: Divider): [number, number] => [d.x + d.w / 2, d.y + d.h / 2];
  for (const by of ["Mouse", "Finger"] as const) {
    // As far right as the window goes: the divider stops where the todo keeps its least.
    let d = await divider();
    const [x, y] = middle(d);
    await app.page.drag([[x, y], [x + 60, y], [x + 200, y], [1430, y]], by);
    const last = `(() => { const d = document.querySelector(".tileline"); return d.getAttribute("aria-valuenow") === d.getAttribute("aria-valuemax"); })()`;
    await app.page.waitFor(last, { timeout: 5000 }).catch(async () => assert.fail(`${by}: the last stop, from ${JSON.stringify(d)} to ${JSON.stringify(await divider())}`));
    const right = await divider();
    held(right, `${by} to the right`);
    // A stop is felt: the divider stands where a stop is, never between.
    await app.page.drag([[...middle(right)], [middle(right)[0] - 37, y]], by);
    await app.page.evaluate(SETTLED);
    d = await divider();
    held(d, `${by} a little to the left`);
    // As far left as the window goes: the list keeps its least.
    await app.page.drag([[...middle(d)], [middle(d)[0] - 200, y], [5, y]], by);
    await app.page.waitFor(`document.querySelector(".tileline").getAttribute("aria-valuenow") === "0"`, { timeout: 5000 }).catch(async () => assert.fail(`${by}: the first stop, from ${JSON.stringify(d)} to ${JSON.stringify(await divider())}`));
    const left = await divider();
    held(left, `${by} to the left`);
    await app.page.screenshot(join(OUT, `divider-1440-${by.toLowerCase()}.png`));
  }
  // A tap steps it to the next stop, on its handle: 8 px beside the gap, at its middle, the pointer meets the handle, not
  // the tile; a tap in the gap away from the handle steps nothing.
  const tapped = await divider();
  const nearby = [tapped.x + tapped.w + 8, tapped.y + tapped.h / 2] as const;
  assert.equal(await app.page.evaluate<boolean>(`document.elementFromPoint(${nearby[0]}, ${nearby[1]})?.classList.contains("tileline") === true`), true, "8 px beside the gap is the handle");
  await app.page.drag([[middle(tapped)[0], tapped.y + 60]], "Mouse");
  await app.page.evaluate(SETTLED);
  assert.equal((await divider()).now, tapped.now, "a tap away from the handle steps nothing");
  await app.page.drag([nearby], "Finger");
  await app.page.waitFor(`Number(document.querySelector(".tileline").getAttribute("aria-valuenow")) === ${tapped.now + 1}`);
  assert.match(await app.page.evaluate<string>(`document.querySelector(".tileline").getAttribute("aria-valuetext")`), /^The left page takes /);
  // The keyboard: the arrows step it, Home and End take it to the ends.
  await app.page.evaluate(`document.querySelector(".tileline").focus()`);
  await app.page.press("Home");
  await app.page.waitFor(`document.querySelector(".tileline").getAttribute("aria-valuenow") === "0"`);
  await app.page.press("ArrowRight");
  await app.page.waitFor(`document.querySelector(".tileline").getAttribute("aria-valuenow") === "1"`);
  await app.page.press("End");
  await app.page.waitFor(`document.querySelector(".tileline").getAttribute("aria-valuenow") === "${first.max}"`);
  held(await divider(), "End");
  // Under the phone's chrome a divider stands between the list and the todo too; at 1,920 px its stops are wider apart.
  for (const w of [960, 1920]) {
    await app.resize([w, 900]);
    // The add sheet slides to its dock under the phone's chrome: the picture waits until it rests.
    await app.page.waitFor(`document.getAnimations().length === 0`);
    held(await divider(), `${w} px`);
    await app.page.screenshot(join(OUT, `divider-${w}.png`));
  }
  await app.resize([1440, 900]);
  await app.page.evaluate(SETTLED);
  assert.equal(await app.page.evaluate<boolean>(`document.activeElement === document.querySelector(".tileline")`), true, "the divider keeps the keyboard");
  // A resize outlives the address: another todo opened beside the list, in the same room (no talk about it opens the
  // secretary's panel), stands at the same stop.
  const kept = await divider();
  await app.page.evaluate(`location.hash = "#/todo/${todos.high.id}"`);
  await app.page.waitFor(`location.hash === "#/todo/${todos.high.id}" && document.querySelector(".tile .todoview")?.textContent.includes("${todos.high.id}") === true`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  const next = await divider();
  assert.equal(next.list + next.todo, kept.list + kept.todo, "the same room");
  assert.deepEqual([next.now, next.list], [kept.now, kept.list], "the stop is kept for the next todo");
  assert.equal(await app.page.evaluate<string>(`location.hash`), `#/todo/${todos.high.id}`, "the address carries no share");
  await app.page.press("Home");
  await app.resize(PHONE);
});

app.scene("with the add sheet up, its field takes a real tap at 412 and 960 px, and its scrim stands beside it, over the window", async () => {
  for (const [w, hash] of [[960, `#/todo/${todos.flat.id}`], [412, "#/"]] as const) {
    await app.resize([w, 900]);
    await app.page.navigate(`${WRITE_BASE}/${hash}`);
    await app.page.waitFor(w === 960 ? beside : listed, { timeout: 20000 });
    await app.page.click(".addsheet .pickerhandle");
    await app.page.waitFor(`document.querySelector(".addsheet.open") !== null && document.getAnimations().length === 0`);
    // The scrim and its sheet share one parent, so one stacking context orders them on every engine: the sheet above.
    const scrim = await app.page.evaluate<{ sibling: boolean; x: number; w: number; h: number }>(`(() => {
      const s = document.querySelector(".scrim");
      const r = s.getBoundingClientRect();
      return { sibling: s.parentElement === document.querySelector(".addsheet").parentElement, x: r.x, w: r.width, h: r.height };
    })()`);
    assert.equal(scrim.sibling, true, `${w} px: the scrim is its sheet's sibling`);
    assert.deepEqual([scrim.x, scrim.w, scrim.h], [0, w, 900], `${w} px: the scrim covers the window`);
    // A real press at the field's middle, hit-tested: nothing covers it.
    await app.page.evaluate(`document.activeElement instanceof HTMLElement && document.activeElement.blur()`);
    await app.page.click(".addsheet .draftbody");
    assert.equal(await app.page.evaluate<boolean>(`document.activeElement?.classList.contains("draftbody") === true`), true, `${w} px: the tap reached the field`);
    await app.page.screenshot(join(OUT, `layers-${w}-addsheet.png`));
    await app.page.evaluate(`document.querySelector(".scrim").click()`);
    await app.page.waitFor(`document.querySelector(".addsheet.open") === null`);
  }
  await app.resize(PHONE);
});

app.scene("on a tile narrower than the window, the topic sheet stands above the tiles, at the window's middle", async () => {
  // The topic sheet over the stream, whose tile is narrower than the window by the body's gutters.
  await app.resize([1440, 900]);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
  await app.page.waitFor(`document.querySelector(".streamround .roundtopicadd") !== null`, { timeout: 20000 });
  await app.page.click(".streamround .roundtopicadd");
  await app.page.waitFor(`document.querySelector(".topicsheet") !== null`, { timeout: 8000 });
  await app.page.evaluate(SETTLED);
  const sheet = await app.page.evaluate<{ inTile: boolean; inLayer: boolean; middle: number; tile: number; inner: number }>(`(() => {
    const s = document.querySelector(".topicsheet");
    const r = s.getBoundingClientRect();
    return { inTile: s.closest(".tile") !== null, inLayer: s.closest("#layers") !== null, middle: r.x + r.width / 2, tile: document.querySelector("#view > .tile").getBoundingClientRect().width, inner: innerWidth };
  })()`);
  assert.deepEqual({ inTile: sheet.inTile, inLayer: sheet.inLayer }, { inTile: false, inLayer: true }, "the topic sheet is the window's");
  assert.ok(sheet.tile < sheet.inner, `the stream's tile (${sheet.tile} px) is narrower than the window`);
  assert.ok(Math.abs(sheet.middle - sheet.inner / 2) <= 1, `the sheet stands at the window's middle (${sheet.middle} of ${sheet.inner})`);
  assert.equal(await app.page.evaluate<boolean>(`document.activeElement?.classList.contains("topicfield") === true`), true, "its field takes the keyboard");
  await app.page.screenshot(join(OUT, "layers-1440-topicsheet.png"));
  await app.resize([1920, 900]);
  await app.page.evaluate(SETTLED);
  assert.ok(Math.abs(await app.page.evaluate<number>(`(() => { const r = document.querySelector(".topicsheet").getBoundingClientRect(); return r.x + r.width / 2 - innerWidth / 2; })()`)) <= 1, "at 1,920 px too");
  await app.page.screenshot(join(OUT, "layers-1920-topicsheet.png"));
  await app.page.click(".topicsheet .act:not(.act-topicwrite)");
  await app.page.waitFor(`document.querySelector(".topicsheet") === null`, { timeout: 8000 });
  await app.resize(PHONE);
});

app.scene("at 960 by 900, a conversation opened from the inbox stands beside it with its composer at the tile's foot", async () => {
  await app.resize([960, 900]);
  await app.page.navigate(`${WRITE_BASE}/#/inbox`);
  await app.page.waitFor(`document.querySelector("#view > .tile .inboxpage") !== null`, { timeout: 20000 });
  // The door a card's Reply takes: an open from the inbox's tile.
  await app.page.evaluate(`document.querySelector("#view > .tile .inboxpage").dispatchEvent(new CustomEvent("suzatary:open", { bubbles: true, cancelable: true, detail: "#/secretary/default" }))`);
  await app.page.waitFor(`document.querySelectorAll("#view > .tile").length === 2 && document.querySelector(".tile .conversation .compose") !== null`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  // The composer sticks to its tile's foot: no bar stands over a tile beside another, so it keeps no room for one.
  const [bottom, compose, tile] = await app.page.evaluate<[string, number, number]>(`(() => { const c = document.querySelector(".tile .conversation > .pagebody > .compose"); return [getComputedStyle(c).bottom, c.getBoundingClientRect().bottom, c.closest(".tile").getBoundingClientRect().bottom]; })()`);
  assert.equal(bottom, "0px", "the composer keeps no room for a bar");
  // Beside the inbox, its own column of conversations is not drawn: the inbox is the list beside it.
  assert.equal(await app.page.evaluate<string>(`getComputedStyle(document.querySelector(".tile .chatsaside")).display`), "none", "no column squeezes the conversation");
  assert.ok(compose <= tile + 0.5, `the composer ends at ${compose}, inside its tile (to ${tile})`);
  stand(await seen(), "960 px, the inbox and a conversation");
  await app.page.screenshot(join(OUT, "compose-960.png"));
  await app.resize(PHONE);
});

app.scene("on a phone, after an open in place the crumb before is a link to its page, not Back to the page replaced", async () => {
  await openAgents(412);
  await app.page.click(".tile .todoview .sourcelink");
  // The message's page is drawn in its tile: a tile is in the document a task before its page is.
  await app.page.waitFor(`location.hash.startsWith("#/mail/e2e-message-hash") && document.querySelector(".tile .todoview") === null && (document.querySelector("#view > .tile")?.firstElementChild ?? null) !== null`, { timeout: 20000 });
  // Another message, opened from the first: one kind, so it takes the first's place.
  await app.page.evaluate(`document.querySelector("#view > .tile").firstElementChild.dispatchEvent(new CustomEvent("suzatary:open", { bubbles: true, cancelable: true, detail: "#/mail/e2e-other-hash" }))`);
  await app.page.waitFor(`location.hash.startsWith("#/mail/e2e-other-hash")`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  const crumb = await app.page.evaluate<{ tag: string; href: string | null; text: string }>(`(() => { const c = [...document.querySelectorAll(".tilelines .crumbs > *")].pop(); return { tag: c.tagName, href: c.getAttribute("href"), text: c.textContent.trim() }; })()`);
  assert.deepEqual(crumb, { tag: "A", href: `#/todo/${todos.agents.id}`, text: "Todo" }, "the crumb goes where it says");
  await app.page.click(".tilelines .crumbs > a:last-child");
  await app.page.waitFor(`location.hash === "#/todo/${todos.agents.id}" && ${source} !== null`, { timeout: 20000 });
  await app.resize(PHONE);
});
