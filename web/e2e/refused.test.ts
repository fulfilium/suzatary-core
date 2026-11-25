/**
 * A login the box names no actor for (`GET /api/me`) is offered no act controls, because a button promising a write
 * the box will refuse is worse than none. `E2E_BASE` is gated to a viewer, so the refusal is the box's own, and its
 * chain cannot move, which is why the read-only files share this server.
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

  const who = await app.page.evaluate<string>(
    `fetch("/api/me").then((r) => r.json()).then((b) => JSON.stringify(b))`,
  );
  assert.match(who, /"actor":null/, `a viewer has no actor, /api/me said ${who}`);

  await openInPlace(app.page, todos.high.id);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".rowopen .act-done, .rowopen .addsub").length`),
    0,
    "a login that cannot seal is not offered a button that seals",
  );
  // `.todoid` is drawn from the todo's row, so the page stands when it is there.
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
