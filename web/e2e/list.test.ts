/**
 * The list, and the todo page's body.
 *
 * This is the app's front door and the one screen that has to be right: the
 * open todos as one tree of lines, grouped by category, ASCENDING in value at
 * every level because low means urgent. A trusted body is typst's HTML and an
 * unconfirmed one is its source. What the viewer folds or filters is kept on
 * the device across a reload.
 *
 * The ordering and every group's value are checked against `/api/tree`
 * FETCHED BY THE PAGE rather than against numbers written down here: the page
 * builds its tree from the store with the core in the tab, and the claim is
 * that it builds the SAME tree the box does.
 */

import assert from "node:assert/strict";

import { BASE, PHONE, WRITE_BASE, openInPlace, row, shownTodos, suite, openFinder } from "./harness";
import type { Worth } from "../src/wire/generated/wire";
import cast from "../test/cast";
import fulfillment from "../src/todo/fulfillment";

const { todos } = cast.world;

const app = await suite("list");

interface Node {
  kind: "group" | "todo";
  label?: string;
  todo?: string;
  worth: Worth;
  children: Node[];
}

/** A worth's number, or null for `∅` and for a term that does not link: the page prints the same. */
const numberOf = (worth: Worth): number | null => (typeof worth === "object" && "value" in worth ? worth.value : null);

/** `/api/tree` at the instant the page priced its own, fetched by the page. */
async function served(by: string): Promise<Node> {
  await app.page.waitFor(`document.querySelector(".tree")?.getAttribute("data-at") !== ""`, { timeout: 30000 });
  const answer = await app.page.evaluate<string>(
    `fetch("/api/tree?by=${by}&t=" + document.querySelector(".tree").getAttribute("data-at")).then((r) => r.text())`,
  );
  return (JSON.parse(answer) as { root: Node }).root;
}

const todosIn = (n: Node): string[] => [...(n.todo === undefined ? [] : [n.todo]), ...n.children.flatMap(todosIn)];

app.scene("the page's groups are the box's tree: the same groups, order and values, in all three groupings", async () => {
  await app.open(BASE);
  for (const by of ["category", "urgency", "flat"]) {
    await openFinder(app.page);
    await app.page.click(`.pill[data-by="${by}"]`);
    await app.page.waitFor(`document.querySelector('.pill[data-by="${by}"]').getAttribute("aria-pressed") === "true"`);
    const root = await served(by);
    const cards = by === "flat" ? [{ ...root, label: "all" }] : root.children;
    const shown = await app.page.evaluate<[string, number | null][]>(
      `[...document.querySelectorAll(".tree section.group")].map((s) => [s.getAttribute("data-group"), JSON.parse(s.getAttribute("data-worth"))])`,
    );
    assert.equal(shown.length, cards.length, `${by}: one card per group`);
    cards.forEach((card, i) => {
      const [key, value] = shown[i] ?? ["", NaN];
      const boxed = numberOf(card.worth);
      assert.ok(key.endsWith(by === "flat" ? "all" : (card.label ?? "")), `${by}: card ${i} is ${key}, the box says ${card.label}`);
      // The page draws the box's number as the screen observes it, the whole percent.
      assert.equal(value, boxed === null ? null : fulfillment.whole(boxed), `${by}: ${key} is worth ${value} here and ${boxed} on the box`);
    });
    // A group nobody has folded is open exactly when it has a number short of Fine, and then its lines show in the box's order.
    assert.deepEqual(
      await shownTodos(app.page),
      cards.flatMap((card) => {
        const boxed = numberOf(card.worth);
        const value = boxed === null ? null : fulfillment.whole(boxed);
        return value !== null && value < 0.7 ? todosIn(card) : [];
      }),
      `${by}: the lines, in the box's order`,
    );
  }
  await openFinder(app.page);
  await app.page.click(`.pill[data-by="category"]`);
});

app.scene("on a phone the first line starts in the top third, under one control that searches, filters and groups", async () => {
  await app.open(BASE);
  // The finder is page state, shut whenever the list is drawn anew.
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`);
  const top = await app.page.evaluate<number>(`document.querySelector(".tree .row").getBoundingClientRect().top`);
  assert.ok(top < PHONE[1] / 3, `the first line starts at y = ${top} of ${PHONE[1]}`);
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".finder").hidden`), true, "the search and the grouping wait behind the magnifier");
  assert.equal(await app.page.evaluate<string>(`document.querySelector(".findsaid").textContent`), "", "the default filter says nothing");
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".filtersummary, .filterdrawer").length`), 0);

  // The control lives in the title row and holds the search, the filters and the grouping.
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".pagehead .titlerow .findbtn") !== null`), true);
  await openFinder(app.page);
  assert.equal(
    await app.page.evaluate<boolean>(`[".search", ".attentionbtn", ".pill[data-by]"].every((q) => document.querySelector(".finder " + q) !== null)`),
    true,
  );
  // Shut over a narrowed list, it says what narrows it.
  await app.page.clickText(".finder .segmented .segment", "Done");
  await app.page.click(".findbtn");
  await app.page.waitFor(`document.querySelector(".finder").hidden === true`);
  assert.equal(await app.page.evaluate<string>(`document.querySelector(".findsaid").textContent`), "Done");
  await openFinder(app.page);
  await app.page.clickText(".finder .segmented .segment", "Open");
  await app.page.waitFor(`document.querySelector(".findsaid").textContent === ""`);
});

app.scene("a fold is the viewer's and stays on this device", async () => {
  await app.open(BASE);
  await app.page.click(`.tree section.group[data-group="c:home"] .grouphead`);
  await app.page.waitFor(`document.querySelector('${row(todos.flat.id)}') === null`);
  const peek = await app.page.evaluate<string>(`document.querySelector('section.group[data-group="c:home"] .peek').textContent`);
  assert.match(peek, new RegExp(todos.flat.body), "a folded group keeps its most urgent todo in sight");
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`);
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector('${row(todos.flat.id)}') === null`), true, "the fold came back");
  await app.page.click(`.tree section.group[data-group="c:home"] .grouphead`);
  await app.page.waitFor(`document.querySelector('${row(todos.flat.id)}') !== null`);
});

app.scene("a trusted body is rendered by typst", async () => {
  await app.page.navigate(`${BASE}/#/todo/${todos.urgent.id}`);
  // The body is its source until its block's HTML is held, then that HTML in its place.
  await app.page.waitFor(`document.querySelector(".todobody .rich") !== null`);
  const strong = await app.page.evaluate<string | null>(
    `document.querySelector(".todobody .rich strong")?.textContent ?? null`,
  );
  assert.equal(strong, cast.world.todos.urgent.strong, "the body should arrive as typst's HTML, through web/src/rich.ts");
});

app.scene("an unconfirmed body is shown as source, not rendered", async () => {
  await app.page.navigate(`${BASE}/#/todo/${todos.agents.id}`);
  await app.page.waitFor(`document.querySelector(".todobody") !== null`);
  const text = await app.page.evaluate<string>(`document.querySelector(".todobody.assource").textContent`);
  assert.match(text, new RegExp(`\\*${todos.agents.strong}\\*`), "rendered means confirmed — the asterisks must still be there");
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".todobody strong").length`),
    0,
    "an untrusted actor's markup must not become elements",
  );
  const note = await app.page.evaluate<string>(
    `[...document.querySelectorAll(".todoview .legend")].map((e) => e.textContent).join(" ")`,
  );
  assert.match(note, /until you confirm it/);
});

app.scene("the filter is remembered, and comes back parsed rather than trusted", async () => {
  await app.open(BASE);
  await openFinder(app.page);
  await app.page.click(".attentionbtn");
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length === 1`);
  await app.page.reload();
  await app.page.waitFor(`document.querySelector(".attentionbtn")?.getAttribute("aria-pressed") === "true"`);
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length === 1`);
  assert.deepEqual(await shownTodos(app.page), [todos.agents.id]);

  // A hand-edited value degrades to the default rather than breaking the page
  // (`register.stored`, then the list's `Filter::of` in Rust), which is the property the remembered filter rests on.
  await app.page.evaluate(`localStorage.setItem("suzatary.ui.kept/todos.filter", "{not json")`);
  await app.page.reload();
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 1`);
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector(".attentionbtn").getAttribute("aria-pressed")`),
    "false",
  );
});

/**
 * A SUBTODO FROM THE LIST (docs/TODOTREE-PLAN-2026-09-25.md step 5): the add
 * sheet opens under the line, the box authors the child and the parent's
 * revision, this replica seals both, and the child is drawn under its parent.
 */
app.scene("Add a subtodo puts the new todo under its parent", async () => {
  await app.open(WRITE_BASE);
  await openInPlace(app.page, todos.checklist.id);
  await app.page.waitFor(`document.querySelector('.rowopen[data-todo="${todos.checklist.id}"] .addsub') !== null`, { timeout: 8000 });
  await app.page.click(`.rowopen[data-todo="${todos.checklist.id}"] .addsub`);
  await app.page.waitFor(`document.querySelector(".addsheet.open .subof")?.hidden === false`);
  assert.match(await app.page.evaluate<string>(`document.querySelector(".subof").textContent`), new RegExp(`A subtodo of ${todos.checklist.body}`));
  await app.page.waitFor(`document.activeElement?.className.includes("draftbody") === true`);
  await app.page.type(".draftbody", "Buy a travel adapter");
  await app.page.click(".addbtn");
  const child = "buyatraveladapter";
  await app.page.waitFor(`document.querySelector('${row(todos.checklist.id)} .rowcount')?.textContent === "1"`, { timeout: 10000 });
  // The parent is still Fine, so its subtodos start folded, like any Fine fold nobody has touched.
  const fold = `document.querySelector('${row(todos.checklist.id)} .rowfold')`;
  if ((await app.page.evaluate<string>(`${fold}.getAttribute("aria-expanded")`)) === "false") {
    await app.page.click(`${row(todos.checklist.id)} .rowfold`);
  }
  await app.page.waitFor(`document.querySelector('.tree .row[data-key$="/t:${todos.checklist.id}/t:${child}"]') !== null`, { timeout: 10000 });
  const shown = await shownTodos(app.page);
  assert.equal(shown.indexOf(child), shown.indexOf(todos.checklist.id) + 1, "the child's line is right under its parent");
});

/**
 * THE THIRTY DAYS' DATES ARE SHORT AND NEVER TOUCH (docs/DESIGN-DECISIONS-2026-09-25.md
 * §F): on a phone every date the line shows is the locale's short day, and no
 * two labels' boxes overlap.
 */
app.scene("at phone width the thirty days' dates are short and no two overlap", async () => {
  await app.resize(PHONE);
  await app.page.navigate(`${BASE}/#/todo/${todos.urgent.id}`);
  await app.page.waitFor(`document.querySelector(".curvebox .curvedates") !== null`, { timeout: 15000 });
  const labels = await app.page.evaluate<[string, number, number][]>(
    `[...document.querySelectorAll(".curvedates .curvedate")]
       .filter((l) => getComputedStyle(l).display !== "none")
       .map((l) => { const r = l.getBoundingClientRect(); return [l.textContent, r.left, r.right]; })`,
  );
  assert.ok(labels.length >= 3, `the start, now and the end at least, got ${JSON.stringify(labels)}`);
  for (const [said] of labels) assert.equal(/\d{4}/.test(said), false, `${said} is short`);
  const sorted = [...labels].sort((a, b) => a[1] - b[1]);
  sorted.slice(1).forEach(([said, left], i) => {
    const [before, , right] = sorted[i] ?? ["", 0, 0];
    assert.ok(right <= left, `${before} ends at ${right}, ${said} starts at ${left}`);
  });
});
