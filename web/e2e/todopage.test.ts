/**
 * A todo's page on a phone is no wider than the screen: the history shows each `Authored` record's fields as the chain
 * printed them, a spec being a literal hundreds of characters long with no space to break at, so a long value wraps and
 * a literal scrolls in its own box.
 */

import assert from "node:assert/strict";

import { BASE, suite } from "./harness";
import cast from "../test/cast";

const { todos } = cast.world;

const app = await suite("todopage");

async function openPage(): Promise<void> {
  await app.page.navigate(`${BASE}/#/todo/${todos.urgent.id}`);
  await app.page.waitFor(
    `document.querySelector(".todoview .whytree") !== null && document.querySelector(".todoview .history .timeline .diff") !== null`,
    { timeout: 20000 },
  );
}

async function openHistory(): Promise<void> {
  await openPage();
  if (!(await app.page.evaluate<boolean>(`document.querySelector(".todoview .history").open`))) await app.page.click(".todoview .history > summary");
  await app.page.waitFor(`document.querySelector(".todoview .history").open === true`);
}

app.scene("at 390px the todo page is no wider than the screen", async () => {
  await openHistory();
  const [scroll, inner] = await app.page.evaluate<[number, number]>(
    `[document.documentElement.scrollWidth, window.innerWidth]`,
  );
  assert.ok(scroll <= inner, `the document is ${scroll}px wide on a ${inner}px screen`);
  const wider = await app.page.evaluate<string[]>(
    `[...document.querySelectorAll(".todoview *")].filter((e) => e.getBoundingClientRect().right > window.innerWidth + 0.5 && e.closest(".literal") === null).map((e) => e.tagName + "." + e.className)`,
  );
  assert.deepEqual(wider, [], "every box ends inside the screen, a literal's scroll box aside");
  const literal = await app.page.evaluate<boolean>(
    `[...document.querySelectorAll(".timeline .literal")].some((e) => e.scrollWidth > e.clientWidth)`,
  );
  assert.ok(literal, "a spec's literal is shown whole, in a box that scrolls");
});
