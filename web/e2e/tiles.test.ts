/**
 * The arrangement asks the room the tiles stand in (the window less the body's gutters) and the chrome asks the window:
 * tiles stand side by side exactly where the room holds their least widths, at every width and text size. A tile kept
 * across a move keeps its element, its focus and where its page stood, and a write in flight when its tile goes still
 * lands; `SUZATARY_SHOTS` (or a temp directory) receives the pictures of the last scenes.
 */

import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { BASE, PHONE, SETTLED, WRITE_BASE, addTodo, askAtTheTerminal, doneFromList, openInPlace, openTodo, reveal, row, suite } from "./harness";
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
  scrolls: boolean;
  sideways: boolean;
  inner: { w: number; h: number };
  door: boolean;
};
const SEEN = `(() => {
  const r = (e) => e.getBoundingClientRect();
  const fixed = (e) => e !== null && !e.hidden && getComputedStyle(e).position === "fixed" && r(e).height > 0;
  const bar = document.querySelector(".tabbar");
  const picker = document.querySelector(".addsheet");
  return {
    wide: matchMedia("(min-width: ${tokens.measure.wide / 16}rem)").matches,
    tiled: document.querySelector("#view").classList.contains("tiled"),
    root: parseFloat(getComputedStyle(document.documentElement).fontSize),
    room: r(document.querySelector("#view")).width,
    tiles: [...document.querySelectorAll("#view > .tile")].map((t) => ({ cls: t.querySelector(".todospane, .inboxpage") ? "list" : t.querySelector(".todoview") ? "todo" : t.querySelector(".panelbody") ? "aside" : "other", x: r(t).x, y: r(t).y, w: r(t).width, bottom: r(t).bottom })),
    header: r(document.querySelector("header")).bottom,
    foot: fixed(bar) ? { top: r(bar).top, bottom: r(bar).bottom } : null,
    handle: fixed(picker) && !picker.classList.contains("open") ? { top: r(picker.querySelector(".pickerhandle")).top, bottom: r(picker.querySelector(".pickerhandle")).bottom } : null,
    scrolls: document.documentElement.scrollHeight > innerHeight,
    sideways: document.documentElement.scrollWidth > document.documentElement.clientWidth,
    inner: { w: innerWidth, h: innerHeight },
    door: !document.querySelector("#beside").hidden && r(document.querySelector("#beside")).width > 0,
  };
})()`;

// The room in pixels at the 16px root, which `arrange` is given.
const units = (s: Seen): number => (s.room * 16) / s.root;
const { list, divider: gap, column, beside: aside } = tokens.measure;
const TWO = list + gap + column;

// How many tiles a todo opened from the list draws from each room width on, the secretary hidden and shown.
const TILES: { readonly [K in "hidden" | "shown"]: readonly (readonly [number, number])[] } = {
  hidden: [[0, 1], [list + gap + column, 2]],
  shown: [[0, 1], [list + gap + column, 2], [list + gap + column + gap + aside, 3]],
};
const tilesFor = (w: number, open: boolean): number => (TILES[open ? "shown" : "hidden"].filter(([from]) => w >= from).pop() ?? [0, 1])[1];

// The law: tiles stand exactly where the room holds their least widths and dividers, none under a bar in either chrome.
// The header's door is hidden exactly where the secretary is drawn, on a todo at every width and on the list under the
// wide chrome.
const agree = (s: Seen, where: string, open = false): void => {
  assert.equal(s.tiles.length, tilesFor(units(s), open), `${where}: a room of ${units(s)} px draws ${s.tiles.length} tile(s)`);
  const drawn = s.tiles.some((t) => t.cls === "aside");
  assert.ok(!(s.door && drawn), `${where}: a door to a secretary drawn`);
  if (s.wide || s.tiles.some((t) => t.cls === "todo")) assert.equal(s.door, !drawn, `${where}: the door is hidden exactly where the secretary is drawn`);
  stand(s, where);
};

// The part of the law that holds for any tiles, a page read as a column (`other`) among them.
const stand = (s: Seen, where: string): void => {
  const least = { list, todo: column, aside, other: column };
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
  }
  assert.equal(s.scrolls, false, `${where}: the page scrolls; its tiles should`);
};

const seen = (): Promise<Seen> => app.page.evaluate<Seen>(SEEN);

// Whether the room at a window of `w` px (16 px text, the secretary hidden, no scrollbar) holds two tiles.
const twoAt = (w: number): boolean => w - 2 * tokens.measure.gutter >= TWO;

// The one door that shows the secretary, the header's, at every width.
const door = `"#beside"`;
const drawnAside = `document.querySelector("#view > .tile .panelbody") !== null`;
const offered = `!(${drawnAside}) && document.querySelector("#beside:not([hidden])") !== null`;
const pressDoor = async (page = app.page): Promise<void> => page.click(await page.evaluate<string>(door));
// This tab's presses let go, so a scene of the secretary's starts from no say of another.
let loads = 0;
const fresh = async (hash: string, size: readonly [number, number]): Promise<void> => {
  await app.resize(size);
  await app.page.evaluate(`sessionStorage.clear()`);
  loads += 1;
  await app.page.navigate(`${WRITE_BASE}/?scene=${loads}${hash}`);
};
const panelOpen = async (open: boolean): Promise<void> => {
  if (open) {
    await app.page.waitFor(`!(${drawnAside}) && document.querySelector(${door}) !== null`);
    await app.page.click(await app.page.evaluate<string>(door));
    await app.page.waitFor(`(document.querySelector("#view > .tile .panelbody .frameheader")?.textContent ?? "") !== ""`);
  } else {
    await app.page.click("#view > .tile .hidesecretary");
    await app.page.waitFor(`!(${drawnAside})`);
  }
  await app.page.evaluate(SETTLED);
};

app.scene("from 840 to 1010 px, with the secretary open and hidden, at the browser's text size and at 20 px, no tile is under a bar", async () => {
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
    await panelOpen(true);
    for (let w = 880; w <= 1010; w += 1) {
      await app.resize([px(w), 800]);
      agree(await seen(), `${px(w)} px at ${text} px text, the secretary open`, true);
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

app.scene("at 1100 px with the secretary open, the todo stands beside it, and at 1440 px the list, the todo and the secretary stand at their least or more", async () => {
  await app.resize([1440, 800]);
  await app.open(WRITE_BASE, `#/todo/${todos.flat.id}`);
  for (const w of [1100, 1440]) {
    await app.resize([w, 800]);
    await panelOpen(true);
    await app.page.waitFor(`${todoDrawn} && document.querySelectorAll("#view > .tile").length === ${tilesFor(w - 2 * tokens.measure.gutter, true)}`, { timeout: 20000 });
    const s = await seen();
    agree(s, `${w} px, the secretary open`, true);
    assert.deepEqual(s.tiles.map((t) => t.cls).slice(-2), ["todo", "aside"], `${w} px: the secretary stands after the todo`);
    await panelOpen(false);
  }
  await app.resize([1100, 800]);
  await app.page.waitFor(beside, { timeout: 20000 });
  agree(await seen(), "1100 px, the secretary hidden");
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
  // A click that focuses nothing (a button in Safari, a tap): what anchors the list is what the person sees, not the keyboard.
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

// Printed as a table: a todo opened from the list, then a message opened from that todo.
app.scene("at 390 to 1,920 px, with the secretary hidden and open, the tiles obey the law; their widths are printed", async () => {
  const rows: string[] = [];
  for (const w of [390, 768, 900, 988, 1024, 1100, 1280, 1440, 1920]) {
    // Where the wide header holds the toggle; narrower, the scene of 840 to 1,010 px reads a press made wider.
    const fits = w >= tokens.measure.wide;
    for (const open of fits ? [false, true] : [false]) {
      await app.resize([w, 900]);
      await app.page.navigate(`${BASE}/#/todo/${todos.agents.id}`);
      await app.page.waitFor(`${todoDrawn} && document.querySelector(".tile .todoview .sourcelink") !== null`, { timeout: 20000 });
      if (open) await panelOpen(true);
      await app.page.evaluate(SETTLED);
      const s = await seen();
      agree(s, `${w} px, the secretary ${open ? "open" : "hidden"}`, open);
      const widths = (t: Seen["tiles"]): string => t.map((x) => `${x.cls === "other" ? "message" : x.cls} ${Math.round(x.w)}`).join(", ");
      await app.page.click(".tile .todoview .sourcelink");
      await app.page.waitFor(`location.hash.startsWith("#/mail/") && document.querySelectorAll("#view > .tile:not(:has(.todospane)):not(:has(.todoview))").length === 1`, { timeout: 20000 });
      await app.page.evaluate(SETTLED);
      const message = await seen();
      stand(message, `${w} px, the secretary ${open ? "open" : "hidden"}, the message opened`);
      rows.push(`| ${w} | ${open ? "open" : "hidden"} | ${widths(s.tiles)} | ${widths(message.tiles)} |`);
      // Hidden again on the todo, where it was pressed open.
      if (open) {
        await app.page.evaluate(`history.back()`);
        await app.page.waitFor(`location.hash === "#/todo/${todos.agents.id}" && ${todoDrawn}`, { timeout: 20000 });
        await panelOpen(false);
      }
    }
  }
  console.log(["TABLE", ...rows].join("\n"));
  await app.resize(PHONE);
});

// Pictures for the eye: the list, a todo beside it (or over it), and Back.
app.scene("pictures of the list, a todo and Back at 390, 988, 1100 with the secretary open, 1440 and 1920 px", async () => {
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
    if (panel) await panelOpen(true);
    await app.page.evaluate(SETTLED);
    await app.page.screenshot(join(OUT, `tiles-${w}-list.png`));
    await reveal(app.page, todos.urgent.id);
    await app.page.evaluate(`location.hash = "#/todo/${todos.urgent.id}"`);
    await app.page.waitFor(todoDrawn, { timeout: 20000 });
    // The secretary's press is kept per page, so it is opened on the todo too.
    if (panel) await panelOpen(true);
    await app.page.evaluate(SETTLED);
    agree(await seen(), `${w} px`, panel);
    await app.page.screenshot(join(OUT, `tiles-${w}-todo.png`));
    // Hidden again on the todo, so no later scene meets this tab's press there.
    if (panel) await panelOpen(false);
    await app.page.evaluate(`history.back()`);
    await app.page.waitFor(`location.hash === "#/" && document.querySelector(".todoview") === null && ${listed}`);
    await app.page.evaluate(SETTLED);
    await app.page.screenshot(join(OUT, `tiles-${w}-back.png`));
    if (panel) await panelOpen(false);
  }
  await app.resize(PHONE);
});

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
    await app.page.click(".tilelines .crumbs > button.back");
    await app.page.waitFor(`location.hash === "#/todo/${todos.agents.id}" && ${source} !== null`, { timeout: 20000 });
    assert.equal(await whats(), todo, `${w} px: Back is the todo`);
    await app.page.screenshot(join(OUT, `open-${w}-back.png`));
  }
  await app.resize(PHONE);
});

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
  // 8 px beside the gap, at its middle, the pointer meets the handle, not the tile.
  const tapped = await divider();
  const nearby = [tapped.x + tapped.w + 8, tapped.y + tapped.h / 2] as const;
  assert.equal(await app.page.evaluate<boolean>(`document.elementFromPoint(${nearby[0]}, ${nearby[1]})?.classList.contains("tileline") === true`), true, "8 px beside the gap is the handle");
  await app.page.drag([[middle(tapped)[0], tapped.y + 60]], "Mouse");
  await app.page.evaluate(SETTLED);
  assert.equal((await divider()).now, tapped.now, "a tap away from the handle steps nothing");
  await app.page.drag([nearby], "Finger");
  await app.page.waitFor(`Number(document.querySelector(".tileline").getAttribute("aria-valuenow")) === ${tapped.now + 1}`);
  assert.match(await app.page.evaluate<string>(`document.querySelector(".tileline").getAttribute("aria-valuetext")`), /^The left page takes /);
  await app.page.evaluate(`document.querySelector(".tileline").focus()`);
  await app.page.press("Home");
  await app.page.waitFor(`document.querySelector(".tileline").getAttribute("aria-valuenow") === "0"`);
  await app.page.press("ArrowRight");
  await app.page.waitFor(`document.querySelector(".tileline").getAttribute("aria-valuenow") === "1"`);
  await app.page.press("End");
  await app.page.waitFor(`document.querySelector(".tileline").getAttribute("aria-valuenow") === "${first.max}"`);
  held(await divider(), "End");
  // Under the phone's chrome a divider stands between the list and the todo too.
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
  // A resize outlives the address: another todo in the same room (no talk about it opens the secretary) keeps the stop.
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
  // The conversation draws no list of conversations of its own: that list is a page, opened as a tile.
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".tile .conversation .chatrow") === null`), true, "no column squeezes the conversation");
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

// The pages the tab runs, drawn and kept behind: the host's own count, beside the elements in the document.
const running = `Number(document.getElementById("view").dataset.running)`;

app.scene("on a phone, a half-typed note survives its todo going behind a message and coming back, and the list behind both is kept", async () => {
  await app.resize([412, 900]);
  await app.open(WRITE_BASE, "#/");
  await app.page.waitFor(listed, { timeout: 20000 });
  await app.page.evaluate(`window.__list = document.querySelector(".todospane").closest(".tile")`);
  await openTodo(app.page, todos.agents.id);
  await app.page.waitFor(`${source} !== null && !window.__list.isConnected && ${running} === ${2}`, { timeout: 20000 });
  await app.page.evaluate(`window.__todo = document.querySelector(".todoview").closest(".tile")`);
  const field = `.tile .todoview input.noteline`;
  await app.page.waitFor(`document.querySelector("${field}") !== null`);
  await app.page.type(field, "half a thought");
  await app.page.click(`.tile .todoview .sourcelink`);
  await app.page.waitFor(`location.hash.startsWith("#/mail/") && !window.__todo.isConnected && ${running} === ${3}`, { timeout: 20000 });
  await app.page.screenshot(join(OUT, "kept-412-behind.png"));
  // The crumb is Back to the todo: a real press.
  await app.page.clickText(".crumbs .back", "Todo");
  await app.page.waitFor(`location.hash === "#/todo/${todos.agents.id}" && window.__todo.isConnected && ${running} === ${2}`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<boolean>(`window.__todo.querySelector("input.noteline") === document.querySelector("${field}")`), true, "the note is the field it was");
  assert.equal(await app.page.evaluate<string>(`document.querySelector("${field}").value`), "half a thought", "what was typed is there");
  await app.page.screenshot(join(OUT, "kept-412-back.png"));
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/" && window.__list.isConnected`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<number>(running), 1, "the todo Back left is let go");
});

app.scene("a list drawn again after Back takes its keys back: j moves its cursor", async () => {
  await app.resize([412, 900]);
  await app.open(WRITE_BASE, "#/");
  await app.page.waitFor(listed, { timeout: 20000 });
  await app.page.evaluate(`window.__list = document.querySelector(".todospane").closest(".tile")`);
  // A todo opened over the list: the list is kept behind, its keys let go.
  await openTodo(app.page, todos.agents.id);
  await app.page.waitFor(`!window.__list.isConnected && ${running} === ${2}`, { timeout: 20000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/" && window.__list.isConnected && ${running} === 1`, { timeout: 20000 });
  await app.page.evaluate(`document.activeElement?.blur()`);
  await app.page.press("j");
  await app.page.waitFor(`document.activeElement?.closest(".todospane") !== null && document.activeElement.getAttribute("data-key") !== null`, { timeout: 8000 });
});

app.scene("on a phone, a write made while the list is behind its todo lands, and the list shows it when it comes back", async () => {
  await app.resize([412, 900]);
  await app.open(WRITE_BASE, "#/");
  await app.page.waitFor(listed, { timeout: 20000 });
  const id = await addTodo(app.page, "Water the kept fern", "home");
  await app.page.evaluate(`window.__list = document.querySelector(".todospane").closest(".tile")`);
  await openTodo(app.page, id);
  await app.page.waitFor(`document.querySelector(".tile .todoview .act-done") !== null && !window.__list.isConnected && ${running} === ${2}`, { timeout: 20000 });
  await app.page.click(".tile .todoview .act-done");
  // The list hears its store while hidden: the row goes from the element kept behind.
  await app.page.waitFor(`window.__list.querySelector('${row(id)}') === null`, { timeout: 20000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/" && window.__list.isConnected`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${row(id)}') === null`), true, "the done todo is not listed");
});

app.scene("fifty opens and Backs at a desk leave no node, listener, measure or running page behind", async () => {
  await openAgents(1440);
  const go = async (): Promise<void> => {
    await app.page.click(".tile .todoview .sourcelink");
    await app.page.waitFor(`document.querySelectorAll("#view > .tile").length === 3`, { timeout: 20000 });
    await app.page.evaluate(`history.back()`);
    await app.page.waitFor(`location.hash === "#/todo/${todos.agents.id}" && document.querySelectorAll("#view > .tile").length === 2`, { timeout: 20000 });
  };
  // A first round warms the message's code and the readings it holds.
  await go();
  await app.page.evaluate(SETTLED);
  const measures = `performance.getEntriesByType("measure").length`;
  const before = { ...(await app.page.counts()), measures: await app.page.evaluate<number>(measures) };
  for (let i = 0; i < 50; i += 1) await go();
  await app.page.evaluate(SETTLED);
  const after = { ...(await app.page.counts()), measures: await app.page.evaluate<number>(measures) };
  console.log(`fifty opens and Backs: ${JSON.stringify({ before, after })}`);
  assert.equal(await app.page.evaluate<number>(running), 2, "the list and the todo run, and nothing else");
  assert.ok(after.listeners <= before.listeners, `listeners ${before.listeners} -> ${after.listeners}`);
  assert.ok(after.nodes <= before.nodes * 1.05, `nodes ${before.nodes} -> ${after.nodes}`);
  // Each name the tab measures keeps its latest `KEPT` (app/perf.ts), so the count is bounded whatever the rounds.
  assert.ok(after.measures <= 256 * 12, `measures ${before.measures} -> ${after.measures}`);
});

app.scene("every workspace's page and a todo kept behind a message on a phone: nine pages kept, each the element left", async () => {
  await app.resize([412, 900]);
  // A tab that holds the About page alone.
  await app.open(WRITE_BASE, "#/");
  await app.page.evaluate(`location.hash = "#/about"`);
  await app.page.reload();
  await app.page.waitFor(`document.querySelector("#view .pagetitle")?.textContent === "About" && ${running} === 1`, { timeout: 20000 });
  const alone = await app.page.counts();
  // Each page drawn is held by the scene, so it can find the element again.
  const hold = `((window.__pages ??= []).push(document.querySelector("#view > .tile")), true)`;
  await app.page.evaluate(hold);
  for (const hash of ["#/inbox", "#/secretary", "#/briefings", "#/mail", "#/calendar", "#/settings", "#/"]) {
    await app.page.evaluate(`location.hash = ${JSON.stringify(hash)}`);
    await app.page.waitFor(`location.hash === ${JSON.stringify(hash)} && document.querySelectorAll("#view > .tile").length === 1 && document.querySelector("#view .pagewaiting") === null && !window.__pages.includes(document.querySelector("#view > .tile"))`, { timeout: 20000 });
    await app.page.evaluate(hold);
  }
  await app.page.waitFor(listed, { timeout: 20000 });
  await openTodo(app.page, todos.agents.id);
  await app.page.evaluate(hold);
  await app.page.click(`#view > .tile .todoview .sourcelink`);
  await app.page.waitFor(`location.hash.startsWith("#/mail/")`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  const many = await app.page.counts();
  const runs = await app.page.evaluate<number>(running);
  console.log(`pages running: ${runs}; ${JSON.stringify({ alone, many })}`);
  assert.equal(runs, 10, "the message, and behind it the seven other workspaces' pages, the list and the todo");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll("#view > .tile").length`), 1, "one page drawn on a phone");
  // Back to the About page: the page kept is the element left, drawn again.
  await app.page.evaluate(`location.hash = "#/about"`);
  await app.page.waitFor(`window.__pages[0].isConnected`, { timeout: 20000 });
});

// The secretary is the pinned tile: a press holds for its page in this tab at any width, beside the todo where the room
// holds both and one crumb behind it where it does not, and what was half typed in it survives every move.
const composer = `#view > .tile .panelbody .composebox`;
app.scene("the secretary at 412, 960, 1,280 and 1,440 px, shown, behind and hidden, with Back, keeps a half-typed question", async () => {
  for (const w of [412, 960, 1280, 1440]) {
    await app.resize([1440, 900]);
    await app.page.navigate(`${WRITE_BASE}/#/todo/${todos.flat.id}`);
    await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
    assert.equal(await app.page.evaluate<boolean>(drawnAside), false, "nothing about the todo, so it is not drawn");
    await panelOpen(true);
    await app.page.waitFor(`document.querySelector("${composer}") !== null`, { timeout: 20000 });
    await app.page.evaluate(`window.__aside = document.querySelector("${composer}").closest(".tile")`);
    await app.page.type(composer, "half a question");
    await app.resize([w, 900]);
    const alone = w - 2 * tokens.measure.gutter < column + gap + aside;
    if (alone) {
      await app.page.waitFor(`${todoDrawn} && !window.__aside.isConnected && [...document.querySelectorAll(".tilelines .crumbafter")].some((c) => c.textContent.includes("Secretary"))`, { timeout: 20000 });
      await app.page.screenshot(join(OUT, `secretary-${w}-behind.png`));
      await app.page.clickText(".tilelines .crumbafter", "Secretary");
      await app.page.waitFor(`window.__aside.isConnected && document.querySelector("#view .todoview") === null`, { timeout: 20000 });
      await app.page.screenshot(join(OUT, `secretary-${w}-open.png`));
      assert.equal(await app.page.evaluate<string>(`document.querySelector("${composer}").value`), "half a question", `${w} px: what was typed is there`);
      await app.page.evaluate(`history.back()`);
      await app.page.waitFor(`location.hash === "#/todo/${todos.flat.id}" && ${todoDrawn} && !window.__aside.isConnected`, { timeout: 20000 });
    } else {
      await app.page.waitFor(`${todoDrawn} && window.__aside.isConnected`, { timeout: 20000 });
      await app.page.screenshot(join(OUT, `secretary-${w}-open.png`));
      assert.equal(await app.page.evaluate<string>(`document.querySelector("${composer}").value`), "half a question", `${w} px: what was typed is there`);
    }
    await app.resize([1440, 900]);
    await app.page.waitFor(`window.__aside.isConnected`, { timeout: 20000 });
    assert.equal(await app.page.evaluate<string>(`document.querySelector("${composer}").value`), "half a question", `${w} px, then 1,440: what was typed is there`);
    // Emptied as a person empties it, so the composer's own draft is empty for the next width.
    await app.page.evaluate(`(() => { const c = document.querySelector("${composer}"); c.value = ""; c.dispatchEvent(new Event("input", { bubbles: true })); })()`);
    await panelOpen(false);
    if (w >= tokens.measure.wide) await app.page.screenshot(join(OUT, `secretary-${w}-hidden.png`));
    assert.equal(await app.page.evaluate<string>(`location.hash`), `#/todo/${todos.flat.id}`, `${w} px: the address names the todo alone`);
  }
  await app.resize(PHONE);
});

// A link to another todo opens beside the list, where no press shows the secretary, and shown there it speaks of the other.
app.scene("the secretary beside a todo talks about that todo, and after an open, about the next", async () => {
  await app.resize([1440, 900]);
  await app.open(WRITE_BASE, `#/todo/${todos.flat.id}`);
  await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
  await panelOpen(true);
  const header = `(document.querySelector("#view > .tile .panelbody .frameheader")?.textContent ?? "")`;
  await app.page.waitFor(`${header}.includes("${todos.flat.id}")`, { timeout: 20000 });
  const was = await app.page.evaluate<string>(`location.hash`);
  await reveal(app.page, todos.agents.id);
  await app.page.click(`.tile ${row(todos.agents.id)}`);
  await app.page.waitFor(`location.hash !== ${JSON.stringify(was)} && document.querySelector(".tile .todoview")?.textContent.includes("${todos.agents.id}") === true`, { timeout: 20000 });
  // Another page: its own press, none, and no talk about it, so the secretary is not drawn until it is shown there.
  await app.page.waitFor(`${offered} && !(${drawnAside})`, { timeout: 20000 });
  await panelOpen(true);
  await app.page.waitFor(`${header}.includes("${todos.agents.id}")`, { timeout: 20000 });
  await app.page.screenshot(join(OUT, "secretary-1440-next.png"));
  await panelOpen(false);
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === ${JSON.stringify(was)} && ${header}.includes("${todos.flat.id}")`, { timeout: 20000 });
  await panelOpen(false);
  await app.resize(PHONE);
});

// A hide lets its program go with its leaf, and a show mounts it again from its address, as a page navigated to.
app.scene("fifty shows and hides of the secretary at a desk leave no node or listener behind, and a show draws it at a fourfold throttle", async () => {
  await app.resize([1440, 900]);
  await app.page.navigate(`${WRITE_BASE}/#/todo/${todos.flat.id}`);
  await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
  const round = async (): Promise<void> => {
    await pressDoor();
    await app.page.waitFor(`document.querySelector("${composer}") !== null`, { timeout: 20000 });
    await app.page.click("#view > .tile .hidesecretary");
    await app.page.waitFor(`document.querySelector("#view .panelbody") === null && document.querySelectorAll("#view > .tile").length === 2`, { timeout: 20000 });
  };
  await round();
  await app.page.evaluate(SETTLED);
  const before = await app.page.counts();
  for (let i = 0; i < 50; i += 1) await round();
  await app.page.evaluate(SETTLED);
  const after = await app.page.counts();
  console.log(`fifty shows and hides of the secretary: ${JSON.stringify({ before, after })}`);
  assert.ok(after.listeners <= before.listeners, `listeners ${before.listeners} -> ${after.listeners}`);
  assert.ok(after.nodes <= before.nodes * 1.05, `nodes ${before.nodes} -> ${after.nodes}`);
  await app.page.throttle(4);
  try {
    const shows: number[] = [];
    for (let i = 0; i < 5; i += 1) {
      const t0 = await app.page.evaluate<number>(`performance.now()`);
      await pressDoor();
      await app.page.waitFor(`document.querySelector("${composer}") !== null && document.querySelector("#view > .tile .panelbody .turns, #view > .tile .panelbody .empty") !== null`, { timeout: 20000 });
      shows.push((await app.page.evaluate<number>(`performance.now()`)) - t0);
      await app.page.click("#view > .tile .hidesecretary");
      await app.page.waitFor(`document.querySelector("#view .panelbody") === null`, { timeout: 20000 });
    }
    console.log(`a show of the secretary, CPU x4, press to the conversation drawn: ${shows.map((m) => m.toFixed(0)).join(", ")} ms`);
  } finally {
    await app.page.throttle(1);
  }
  await app.resize(PHONE);
});

// Each frame's length while the divider is dragged across its stops at a fourfold throttle, printed, and none past a long task.
app.scene("a divider dragged with the secretary pinned keeps its frames short at a fourfold CPU throttle", async () => {
  await app.resize([1440, 900]);
  await app.page.navigate(`${WRITE_BASE}/#/todo/${todos.flat.id}`);
  await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
  await panelOpen(true);
  await app.page.waitFor(`document.querySelector("${composer}") !== null && document.querySelectorAll("#view > .tile").length === 3`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  await app.page.throttle(4);
  try {
    await app.page.evaluate(`(() => { window.__frames = []; let last = performance.now(); const tick = (t) => { window.__frames.push(t - last); last = t; if (window.__frames.length < 100000) window.__tick = requestAnimationFrame(tick); }; window.__tick = requestAnimationFrame(tick); })()`);
    const d = await divider();
    const [x, y] = [d.x + d.w / 2, d.y + d.h / 2];
    const path: [number, number][] = Array.from({ length: 40 }, (_, i) => [x + i * 12, y]);
    await app.page.drag([...path, ...path.slice().reverse()], "Mouse");
    await app.page.evaluate(SETTLED);
    const frames = await app.page.evaluate<number[]>(`(cancelAnimationFrame(window.__tick), window.__frames.slice(1))`);
    const sorted = frames.slice().sort((a, b) => a - b);
    const at = (q: number): number => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
    console.log(`divider drag with the secretary pinned, CPU x4: ${frames.length} frames, median ${at(0.5).toFixed(1)} ms, p95 ${at(0.95).toFixed(1)} ms, max ${at(1).toFixed(1)} ms`);
    assert.ok(at(1) < 100, `a frame of ${at(1).toFixed(1)} ms during the drag`);
  } finally {
    await app.page.throttle(1);
  }
  await panelOpen(false);
  await app.resize(PHONE);
});

const ask = async (page: typeof app.page, text: string): Promise<void> => {
  await page.type(composer, text);
  await page.click("#view > .tile .panelbody .composesend");
  await page.waitFor(`[...document.querySelectorAll("#view > .tile .panelbody .turn-person .turntext")].some((t) => t.textContent.includes(${JSON.stringify(text)}))`, { timeout: 20000 });
};

// The talk is a reading: a tab with no press on a todo the conversation talks about draws the secretary beside it by that alone.
app.scene("the secretary stands beside a todo by its talk alone, and stays the same element as the talk moves", async () => {
  await app.resize([1440, 900]);
  await app.open(WRITE_BASE, `#/todo/${todos.checklist.id}`);
  await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
  await panelOpen(true);
  await ask(app.page, "is the checklist done");
  await panelOpen(false);
  // Another tab holds no press and has read the conversation whole, so the talk alone shows the secretary beside the todo.
  const other = await app.tab();
  try {
    await other.viewport(1440, 900);
    await other.navigate(`${WRITE_BASE}/#/secretary/default`);
    await other.waitFor(`document.querySelectorAll("#view .turn-person").length > 0`, { timeout: 20000 });
    await other.evaluate(`location.hash = "#/todo/${todos.checklist.id}"`);
    await other.waitFor(`document.querySelector("${composer}") !== null && document.querySelector("#view > .tile .todoview") !== null`, { timeout: 20000 });
    assert.equal(await other.evaluate<boolean>(`document.querySelector("#beside").hidden`), true, "no door to a secretary drawn");
    assert.equal(await other.evaluate<string>(`location.hash`), `#/todo/${todos.checklist.id}`, "the address names the todo alone");
    await other.evaluate(`window.__aside = document.querySelector("${composer}").closest(".tile")`);
    await ask(other, "and the second item");
    await other.evaluate(SETTLED);
    assert.equal(await other.evaluate<boolean>(`window.__aside.isConnected && window.__aside.contains(document.querySelector("${composer}"))`), true, "the same element");
  } finally {
    await other.close();
  }
  await app.resize(PHONE);
});

// A link followed in the secretary is followed from the page before it, so the todo opens between the list and the secretary.
app.scene("a todo opened from the secretary beside the list stands between them", async () => {
  await app.resize([1440, 900]);
  await app.open(WRITE_BASE, "#/");
  await app.page.waitFor(`${listed} && ${offered}`, { timeout: 20000 });
  await panelOpen(true);
  const link = `#view > .tile .panelbody .turnrefs a[href="#/todo/${todos.checklist.id}"]`;
  await app.page.waitFor(`document.querySelector('${link}') !== null`, { timeout: 20000 });
  await app.page.click(link);
  // A press in this tab closed the secretary beside that todo, so the say lets it go; shown again, it stands at the end.
  await app.page.waitFor(`location.hash === "#/todo/${todos.checklist.id}" && ${todoDrawn} && !(${drawnAside})`, { timeout: 20000 });
  assert.deepEqual((await seen()).tiles.map((t) => t.cls), ["list", "todo"], "the list stays, the todo beside it");
  await panelOpen(true);
  assert.deepEqual((await seen()).tiles.map((t) => t.cls), ["list", "todo", "aside"], "the list, the todo, the secretary");
  await panelOpen(false);
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/"`, { timeout: 20000 });
  await panelOpen(false);
  await app.resize(PHONE);
});

// The secretary costs a person no page of `MOST`.
app.scene("with three pages and the secretary held, a fourth opens beside them, as without it", async () => {
  await app.resize([2400, 900]);
  const three = `#/todo/${todos.agents.id}~|!(|(%2F,2%2Ftodo%2F${todos.flat.id},2*),,,,,,,)`;
  const from = `[...document.querySelectorAll("#view > .tile")].find((t) => t.querySelector(".todoview")?.textContent.includes("${todos.agents.id}"))?.querySelector(".sourcelink")`;
  const opened = async (held: boolean): Promise<string> => {
    await app.page.navigate(`${WRITE_BASE}/#/`);
    await app.page.evaluate(`location.hash = ${JSON.stringify(three)}`);
    await app.page.waitFor(`document.querySelectorAll("#view > .tile .todoview").length === 2 && ${from} != null`, { timeout: 20000 });
    if (held) await panelOpen(true);
    await app.page.evaluate(`${from}.click()`);
    await app.page.waitFor(`location.hash.startsWith("#/mail/") && document.querySelectorAll("#view > .tile .todoview").length === 2`, { timeout: 20000 });
    await app.page.evaluate(SETTLED);
    return app.page.evaluate<string>(`location.hash`);
  };
  assert.equal(await opened(true), await opened(false), "the same pages, the same address");
  await app.resize(PHONE);
});

// On a phone the secretary's own Hide goes back to the todo and closes the crumb, so Back then is the list, never the secretary.
app.scene("on a phone the secretary is shown alone and hidden, and Back and Forward cross both", async () => {
  await app.resize([412, 900]);
  await app.open(WRITE_BASE, "#/");
  const todo = `#/todo/${todos.flat.id}`;
  await app.page.evaluate(`location.hash = "${todo}"`);
  await app.page.waitFor(`location.hash === "${todo}" && ${todoDrawn}`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<boolean>(`!document.querySelector("#beside").hidden && document.querySelector('#view a[href^="#/aside"]') === null`), true, "a phone's door is the header's, the one there is");
  await panelOpen(true);
  await app.page.waitFor(`location.hash.startsWith("#/aside/") && document.querySelector("#view .todoview") === null`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector("#view > .tile .hidesecretary").getBoundingClientRect().height > 0`), true, "its Hide is drawn with it");
  const crumb = `[...document.querySelectorAll(".tilelines .crumbafter")].some((c) => c.textContent.includes("Secretary"))`;
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "${todo}" && ${todoDrawn} && ${crumb}`, { timeout: 20000 });
  await app.page.evaluate(`history.forward()`);
  await app.page.waitFor(`location.hash.startsWith("#/aside/") && ${drawnAside}`, { timeout: 20000 });
  await panelOpen(false);
  await app.page.waitFor(`location.hash === "${todo}" && ${todoDrawn} && !(${crumb})`, { timeout: 20000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/" && ${listed} && !(${drawnAside})`, { timeout: 20000 });
  await app.page.evaluate(`history.forward()`);
  await app.page.waitFor(`location.hash === "${todo}" && ${todoDrawn} && !(${crumb})`, { timeout: 20000 });
  await app.resize([960, 900]);
  const entries = await app.page.evaluate<number>(`history.length`);
  await panelOpen(true);
  await app.page.waitFor(`${drawnAside} && ${todoDrawn} && location.hash === "${todo}"`, { timeout: 20000 });
  await app.page.screenshot(join(OUT, "secretary-960-drawn.png"));
  await panelOpen(false);
  await app.page.waitFor(`location.hash === "${todo}" && ${todoDrawn}`, { timeout: 20000 });
  assert.equal(await app.page.evaluate<number>(`history.length`), entries, "no entry of the history");
  await app.resize(PHONE);
});

app.scene("from 900 to 991 px the secretary drawn beside a todo has its own Hide, and the header no door", async () => {
  await app.resize([960, 900]);
  await app.open(WRITE_BASE, `#/todo/${todos.flat.id}`);
  await app.page.waitFor(todoDrawn, { timeout: 20000 });
  await panelOpen(true);
  for (const w of [900, 920, 960, 991]) {
    await app.resize([w, 900]);
    await app.page.waitFor(`${drawnAside} && ${todoDrawn}`, { timeout: 20000 });
    const seenAt = await app.page.evaluate<{ hide: boolean; header: boolean }>(`({ hide: document.querySelector("#view > .tile .hidesecretary").getBoundingClientRect().width > 0, header: getComputedStyle(document.querySelector("#beside")).display !== "none" && !document.querySelector("#beside").hidden })`);
    assert.deepEqual(seenAt, { hide: true, header: false }, `${w} px`);
  }
  await panelOpen(false);
  assert.equal(await app.page.evaluate<boolean>(`!document.querySelector("#beside").hidden`), true, "hidden, its door is back");
  await app.resize(PHONE);
});

// A show pressed in the same task as the link that opens another todo waits for that move to end, and is that todo's.
// A scene whose move ended before the press fails rather than pressing late.
app.scene("a show pressed during a move is the page's the move draws", async () => {
  await fresh(`#/todo/${todos.urgent.id}`, [1440, 900]);
  await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
  await reveal(app.page, todos.high.id);
  // The door is pressed once the move has begun (its address written) and before it can end.
  await app.page.evaluate(`(() => {
    const door = document.querySelector("#beside");
    const r = document.querySelector('.tile ${row(todos.high.id)}').getBoundingClientRect();
    document.elementFromPoint(r.left + 24, r.top + r.height / 2).dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, clientX: r.left + 24, clientY: r.top + r.height / 2 }));
    let tries = 0;
    window.__late = false;
    const pressed = () => (location.hash === "#/todo/${todos.high.id}" ? door.click() : ++tries > 10000 ? (window.__late = true) : queueMicrotask(pressed));
    pressed();
  })()`);
  assert.equal(await app.page.evaluate<boolean>(`window.__late`), false, "the door was pressed during the move");
  const header = `(document.querySelector("#view > .tile .panelbody .frameheader")?.textContent ?? "")`;
  await app.page.waitFor(`location.hash === "#/todo/${todos.high.id}" && ${header}.includes("${todos.high.id}")`, { timeout: 20000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/todo/${todos.urgent.id}" && ${todoDrawn} && ${offered}`, { timeout: 20000 });
  await app.page.evaluate(`history.forward()`);
  await app.page.waitFor(`location.hash === "#/todo/${todos.high.id}" && ${drawnAside}`, { timeout: 20000 });
  await panelOpen(false);
  await app.resize(PHONE);
});

app.scene("a list at 412 and 960 px has no secretary and no door, and keeps its press for the wide chrome", async () => {
  await fresh("#/", [1440, 900]);
  await app.page.waitFor(`${listed} && ${offered}`, { timeout: 20000 });
  await panelOpen(true);
  for (const w of [412, 960]) {
    await app.resize([w, 900]);
    const crumb = `[...document.querySelectorAll(".tilelines .crumbafter")].some((c) => c.textContent.includes("Secretary"))`;
    await app.page.waitFor(`!(${drawnAside}) && !(${crumb}) && document.querySelector("#beside").hidden`, { timeout: 20000 });
  }
  await app.resize([1440, 900]);
  await app.page.waitFor(drawnAside, { timeout: 20000 });
  await panelOpen(false);
  await app.resize(PHONE);
});

// The door pressed with Enter focuses the secretary's region, from which Tab reaches its composer; its Hide focuses the door.
const region = `document.activeElement?.matches("#view .panelbody") === true`;
const byKeyboard = async (hash: string, size: readonly [number, number], ready: string): Promise<void> => {
  const where = `${hash} at ${size[0]} px`;
  await fresh(hash, size);
  await app.page.waitFor(`${ready} && ${offered}`, { timeout: 20000 });
  await app.page.evaluate(`document.querySelector("#beside").focus()`);
  await app.page.press("Enter");
  await app.page.waitFor(`${drawnAside} && ${region}`, { timeout: 20000 });
  // The list's cursor is the focus, so a key the list took would take the focus from the region.
  const stood = await app.page.evaluate<string>(`location.hash`);
  for (const key of ["Enter", "j"]) {
    await app.page.press(key);
    await app.page.evaluate(SETTLED);
    assert.deepEqual(await app.page.evaluate<[boolean, string]>(`[${region}, location.hash]`), [true, stood], `${where}: ${key} on the region`);
  }
  let tabs = 0;
  while (!(await app.page.evaluate<boolean>(`document.activeElement?.matches("#view .panelbody .composebox") === true`))) {
    assert.ok(++tabs < 80, `${where}: Tab reaches the composer`);
    await app.page.press("Tab");
  }
  await app.page.press("a");
  await app.page.evaluate(`document.querySelector("#view > .tile .hidesecretary").focus()`);
  await app.page.press("Enter");
  await app.page.waitFor(`document.activeElement === document.querySelector("#beside") && !(${drawnAside}) && ${ready}`, { timeout: 20000 });
  await app.page.press("Enter");
  await app.page.waitFor(`${drawnAside} && ${region}`, { timeout: 20000 });
  await app.page.evaluate(`(() => { const c = document.querySelector("#view .panelbody .composebox"); c.value = ""; c.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  await panelOpen(false);
};
app.scene("show, type, hide and show again with the keyboard alone, the focus always where the act left it", async () => {
  await byKeyboard(`#/todo/${todos.flat.id}`, [1440, 900], `${todoDrawn} && ${listed}`);
  await byKeyboard("#/mail/e2e-message-hash", [1440, 900], `document.querySelector("#view .mailmessage") !== null`);
  await byKeyboard(`#/todo/${todos.flat.id}`, [412, 900], todoDrawn);
  await app.resize(PHONE);
});

// The door stands in the header's row as the brand does, so the header is as tall with it as without it, and at 150
// percent text the door's width gives before the header overflows.
app.scene("at 390 and 412 px, and at 150 percent text, the header is as tall with the door as without it, the live mark stays, and the door is tapped across the header", async () => {
  const measured = `(() => {
    const r = (e) => { const b = e.getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; };
    const header = document.querySelector("body > header");
    return { header: r(header), door: r(document.querySelector("#beside")), live: r(document.querySelector("#live")), first: document.querySelector(".tree .row")?.getBoundingClientRect().top ?? null, sideways: document.documentElement.scrollWidth > document.documentElement.clientWidth || document.querySelector("#beside").getBoundingClientRect().right > header.getBoundingClientRect().right + 0.5 };
  })()`;
  type Box = { x: number; y: number; w: number; h: number };
  type Measured = { header: Box; door: Box; live: Box; first: number | null; sideways: boolean };
  const at = (x: number, y: number): string => `(() => { const e = document.elementFromPoint(${x}, ${y}); return e === null ? "none" : e.closest("#beside") !== null ? "door" : e.closest("body > header") !== null ? "header" : "page"; })()`;
  for (const [w, text] of [[390, 16], [412, 16], [390, 24], [1440, 16]] as const) {
    const where = `${w} px at ${text} px text`;
    await app.page.textSize(text);
    await fresh(`#/todo/${todos.flat.id}`, [w, 900]);
    await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
    await app.page.evaluate(SETTLED);
    const with_ = await app.page.evaluate<Measured>(measured);
    assert.ok(with_.door.h > 0, `${where}: the door is drawn`);
    assert.equal(with_.sideways, false, `${where}: the header overflows sideways`);
    // Under a phone's chrome, its tap area is the header's height at the door's middle; just under the header, the page.
    const x = with_.door.x + with_.door.w / 2;
    if (w < 1000) for (const y of [with_.header.y + 1, with_.door.y + with_.door.h / 2, with_.header.y + with_.header.h - 1]) assert.equal(await app.page.evaluate<string>(at(x, y)), "door", `${where}: a tap at ${y} px`);
    if (w < 1000) assert.equal(await app.page.evaluate<string>(at(x, with_.header.y + with_.header.h + 2)), "page", `${where}: a tap just under the header`);
    await app.page.evaluate(`document.querySelector("#beside").hidden = true`);
    await app.page.evaluate(SETTLED);
    const without = await app.page.evaluate<Measured>(measured);
    assert.equal(with_.header.h, without.header.h, `${where}: the header's height`);
    assert.deepEqual(with_.live, without.live, `${where}: the live mark's box`);
    if (w >= 1000) continue;
    // The list, a page of many things, draws no door under a phone's chrome.
    await app.page.evaluate(`location.hash = "#/"`);
    await app.page.waitFor(`${listed} && document.querySelector("#beside").hidden`, { timeout: 20000 });
    await app.page.evaluate(SETTLED);
    const list = await app.page.evaluate<Measured>(measured);
    assert.equal(list.header.h, without.header.h, `${where}: the list's header`);
    assert.deepEqual(list.live, without.live, `${where}: the live mark on the list`);
    await app.page.evaluate(`location.hash = "#/todo/${todos.flat.id}"`);
    await app.page.waitFor(`${todoDrawn} && ${offered}`, { timeout: 20000 });
    await app.page.evaluate(`history.back()`);
    await app.page.waitFor(`location.hash === "#/" && ${listed}`, { timeout: 20000 });
    await app.page.evaluate(SETTLED);
    assert.equal((await app.page.evaluate<Measured>(measured)).first, list.first, `${where}: the list's first line, after a page with the door`);
  }
  await app.page.textSize(16);
  await app.resize(PHONE);
});

// On a phone the secretary alone scrolls the document, and its Hide stays under the app's header, never beneath it.
app.scene("on a phone the secretary's Hide is reachable after the conversation scrolls", async () => {
  await fresh("#/", [412, 900]);
  await app.page.waitFor(listed, { timeout: 20000 });
  await app.page.evaluate(`location.hash = "#/todo/${todos.checklist.id}"`);
  await app.page.waitFor(todoDrawn, { timeout: 20000 });
  await panelOpen(true);
  const reachable = `(() => { const b = document.querySelector("#view > .tile .hidesecretary"); const r = b.getBoundingClientRect(); const at = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return at !== null && b.contains(at); })()`;
  await app.page.waitFor(reachable, { timeout: 20000 });
  await app.page.evaluate(`window.scrollTo(0, document.documentElement.scrollHeight)`);
  await app.page.evaluate(SETTLED);
  assert.equal(await app.page.evaluate<boolean>(reachable), true, "after a scroll to the foot");
  await app.page.evaluate(`window.scrollTo(0, 0)`);
  await app.page.evaluate(SETTLED);
  assert.equal(await app.page.evaluate<boolean>(reachable), true, "at the top");
  await panelOpen(false);
  await app.resize(PHONE);
});

// A hide pressed while a link in the secretary opens another page waits for that move and acts on the page it draws:
// the link's entry stays, so Back returns to the secretary.
app.scene("on a phone a hide pressed with a link followed in the secretary keeps the link's entry", async () => {
  await fresh("#/", [412, 900]);
  await app.page.waitFor(listed, { timeout: 20000 });
  await app.page.evaluate(`location.hash = "#/todo/${todos.flat.id}"`);
  await app.page.waitFor(todoDrawn, { timeout: 20000 });
  await panelOpen(true);
  const link = `#view > .tile .panelbody .turnrefs a[href="#/todo/${todos.checklist.id}"]`;
  await app.page.waitFor(`location.hash.startsWith("#/aside/") && document.querySelector('${link}') !== null`, { timeout: 20000 });
  const was = await app.page.evaluate<string>(`location.hash`);
  // Hide is pressed once the link's move has begun (its address written) and before it can end.
  await app.page.evaluate(`(() => {
    const hide = document.querySelector("#view > .tile .hidesecretary");
    document.querySelector('${link}').click();
    let tries = 0;
    window.__late = false;
    const pressed = () => (location.hash === "#/todo/${todos.checklist.id}" ? hide.click() : ++tries > 10000 ? (window.__late = true) : queueMicrotask(pressed));
    pressed();
  })()`);
  assert.equal(await app.page.evaluate<boolean>(`window.__late`), false, "Hide was pressed during the link's move");
  await app.page.waitFor(`location.hash === "#/todo/${todos.checklist.id}" && ${todoDrawn}`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  assert.equal(await app.page.evaluate<string>(`location.hash`), `#/todo/${todos.checklist.id}`, "the link's entry stays");
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === ${JSON.stringify(was)}`, { timeout: 20000 });
  await panelOpen(false);
  await app.resize(PHONE);
});

// A line opened in place offers the secretary's own page, never the secretary beside a page, whose one door is the header's.
app.scene("the list row's Ask is the secretary's own page about the todo", async () => {
  await fresh("#/", [412, 900]);
  await app.page.waitFor(listed, { timeout: 20000 });
  await openInPlace(app.page, todos.agents.id);
  const capsule = await app.page.evaluate<string | null>(`[...document.querySelectorAll('.rowopen[data-todo="${todos.agents.id}"] a.capsule')].find((a) => a.textContent.trim() === "Ask")?.getAttribute("href") ?? null`);
  assert.ok(capsule?.startsWith("#/secretary?ref=todo") === true && decodeURIComponent(capsule).endsWith(`/${todos.agents.id}`), `the row's Ask: ${String(capsule)}`);
  await app.resize(PHONE);
});

app.scene("a bare secretary address about a message stands in the Mail workspace, and its Hide lands on the message", async () => {
  await fresh("#/aside/mail%3Aabcdef12", [1440, 900]);
  await app.page.waitFor(`${drawnAside} && document.querySelector(".tabbar [aria-current='page']")?.textContent.includes("Mail") === true`, { timeout: 20000 });
  // Its Hide lands on the message it is about, never the Mail list.
  await app.page.click("#view > .tile .hidesecretary");
  await app.page.waitFor(`!(${drawnAside})`, { timeout: 20000 });
  await app.page.evaluate(SETTLED);
  assert.equal(await app.page.evaluate<string>(`location.hash`), "#/mail/abcdef12", "the person lands on the message");
  await app.resize(PHONE);
});

// THE HEADING IS A READING OF THE LINES HELD NOW (`About` in app/pages/src/conversation.rs). Reloaded while its pull of
// the lines is held back, a device that holds a round about something else shows that round under a heading that claims
// nothing; once the pull ends whole it says nothing is about the todo, the question typed meanwhile still in the
// composer. Sent, that question is about the todo (the composer names it), and the heading turns to it with no reload.
app.scene("the secretary beside a todo says nothing is about it only once a pull has ended whole, and follows the talk", async () => {
  await app.resize([1440, 900]);
  await askAtTheTerminal("default", "anything new today");
  const about = `todo:life/${todos.resolved.id}`;
  const header = `(document.querySelector("#view > .tile .panelbody .frameheader")?.textContent ?? "")`;
  const headed = (said: string): string => `${header}.startsWith(${JSON.stringify(said)})`;
  const open = async (): Promise<void> => {
    loads += 1;
    await app.page.navigate(`${WRITE_BASE}/?scene=${loads}#/aside/${encodeURIComponent(about)}`);
  };
  await open();
  await app.page.waitFor(headed(`Nothing about ${about} yet`), { timeout: 20000 });
  const release = await app.page.holdRequests(["/api/stores/lines/skeleton"]);
  await open();
  await app.page.untilHeld();
  await app.page.waitFor(
    `${headed(`Talk about ${about} may not be on this device yet`)} && [...document.querySelectorAll("#view > .tile .panelbody .turn-person .turntext")].some((t) => t.textContent.includes("anything new today"))`,
    { timeout: 20000 },
  );
  await app.page.type(composer, "is the card renewed");
  await release();
  await app.page.waitFor(headed(`Nothing about ${about} yet`), { timeout: 20000 });
  assert.equal(await app.page.evaluate<string>(`document.querySelector("${composer}").value`), "is the card renewed", "the draft stays");
  await app.page.click("#view > .tile .panelbody .composesend");
  await app.page.waitFor(headed(`About ${about}`), { timeout: 20000 });
  await app.resize(PHONE);
});
