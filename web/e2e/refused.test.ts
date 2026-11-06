/**
 * A LOGIN THAT MAY NOT WRITE.
 *
 * The box names no actor for this login (`GET /api/me`), so the app offers no
 * act controls at all — a button that promised a write the box will refuse is
 * worse than an absent one, because the reader only finds out after tapping.
 *
 * `E2E_BASE` is gated to a login this deployment knows only as a VIEWER, so
 * the refusal is the box's own and nothing here had to invent an error to
 * catch. It is also why the read-only files can share this server: its chain
 * cannot move.
 */

import assert from "node:assert/strict";

import { BASE, fold, openInPlace, openTodo, shownTodos, suite } from "./harness";
import cast from "../test/cast";

const { todos } = cast.world;

const app = await suite("refused");

app.scene("a read-only login gets no act controls at all", async () => {
  await app.open(BASE);
  const before = await shownTodos(app.page);
  assert.ok(before.includes(todos.high.id), "the list still reads fine without a writer");

  // The box named no actor for this login.
  const who = await app.page.evaluate<string>(
    `fetch("/api/me").then((r) => r.json()).then((b) => JSON.stringify(b))`,
  );
  assert.match(who, /"actor":null/, `a viewer has no actor, /api/me said ${who}`);

  // So no line opened in place carries a Done, on the list…
  await openInPlace(app.page, todos.high.id);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".rowopen .act-done, .rowopen .addsub").length`),
    0,
    "a login that cannot seal is not offered a button that seals",
  );
  // …and the todo view offers none either, where every verb normally lives: no act, and no empty row of them under
  // the header. `.todoid` is drawn from the todo's row, so the page stands when it is there.
  await openTodo(app.page, todos.high.id);
  await app.page.waitFor(`document.querySelector(".todoview .todoid") !== null`, { timeout: 4000 });
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".todoview .act").length`), 0);
  assert.equal(
    await app.page.evaluate<boolean>(`document.querySelector(".todoview .actrow") === null`),
    true,
    "a row with nothing in it is not drawn: no empty box under the header",
  );

  assert.equal(
    (await fold(app.page)).find((entry) => entry.todo === todos.high.id)?.state,
    "open",
    "and nothing reached the chain",
  );
});
