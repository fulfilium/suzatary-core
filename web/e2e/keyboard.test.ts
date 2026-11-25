/**
 * `web/test/keys.test.ts` pins what a key means; this file checks the meaning is wired in a browser: `j` reaches the
 * list's cursor, `/` lands the caret in the search field, and Ctrl-K opens the palette. The cursor is asserted through
 * `document.activeElement`, because focus is what the app made the cursor (`app/pages/src/todos.rs`), so a test that
 * read a private variable would pass while the ring was elsewhere.
 */

import assert from "node:assert/strict";

import { BASE, suite, WRITE_BASE } from "./harness";
import cast from "../test/cast";

const { todos, mail, counsel } = cast.world;
/** One word that finds the agent's todo AND the first seeded message. */
const query = counsel.name.toLowerCase();

const app = await suite("keyboard");

const cursor = (): Promise<string | null> =>
  app.page.evaluate<string | null>(`document.activeElement?.getAttribute("data-key") ?? null`);

/** The page focuses the cursor once it has drawn the move, so a move is waited for, never read at once. */
const arrives = (key: string): Promise<void> =>
  app.page.waitFor(`document.activeElement?.getAttribute("data-key") === ${JSON.stringify(key)}`);

app.scene("j and k move the cursor down and up the list, group heads and lines alike", async () => {
  await app.open(BASE);
  await app.page.press("j");
  // From nowhere, the first group's head.
  await arrives("c:work");
  await app.page.press("j");
  await arrives(`c:work/t:${todos.urgent.id}`);
  await app.page.press("k");
  await arrives("c:work");
  await app.page.press("k");
  assert.equal(await cursor(), "c:work", "k at the top stays at the top rather than wrapping");
});

// `a` opens the secretary from Todos, and only a writer's page draws a compose box to land in, so its case is
// pressed on the writer's server.
app.scene("slash puts the caret in the search field, and the field then owns the keyboard", async () => {
  for (const [base, key] of [[BASE, "j"], [WRITE_BASE, "a"]] as const) {
    await app.open(base);
    const before = await app.page.evaluate<string>(`window.location.hash`);
    await app.page.press("/");
    assert.equal(await app.page.evaluate<string | null>(`document.activeElement?.className ?? null`), "search");
    // The negative property, live: a key in a field is its letter, not a move or a verb.
    await app.page.press(key);
    assert.equal(await app.page.evaluate<string>(`document.querySelector(".search").value`), key);
    assert.equal(await app.page.evaluate<string>(`window.location.hash`), before, key);
    await app.page.press("Escape");
    // Cleared, because the search is part of the remembered filter and the scenes below share this profile.
    await app.page.evaluate(`(() => {
      const field = document.querySelector(".search");
      field.value = "";
      field.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
    await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`);
  }
});

app.scene("Ctrl-K opens the palette and typing filters it", async () => {
  await app.open(BASE);
  await app.page.press("k", { ctrl: true });
  await app.page.waitFor(`document.querySelector(".overlaybox .palette") !== null`);
  assert.equal(
    await app.page.evaluate<string | null>(`document.activeElement?.className ?? null`),
    "draftfield palettefield",
    "the field is the point of the palette; it takes the keyboard immediately",
  );
  const before = await app.page.evaluate<number>(`document.querySelectorAll(".palrow").length`);
  assert.ok(before > 1, `the palette should offer the destinations and the todos, it offered ${before}`);

  await app.page.type(".palettefield", query);
  await app.page.waitFor(`document.querySelectorAll(".palrow").length === 1`);
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector(".palrow .palhint")?.textContent ?? null`),
    todos.agents.id,
  );

  await app.page.press("Enter");
  await app.page.waitFor(`window.location.hash === "#/todo/${todos.agents.id}"`);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".overlay").length`),
    0,
    "going somewhere closes the palette",
  );
});

// The fixture's own notmuch database holds two synthetic messages, the first about the same deed as the agent's todo,
// so one query matches a local row and a mail row together.
app.scene("the mail group appears below the local rows without disturbing them", async () => {
  await app.open(BASE);
  await app.page.press("k", { ctrl: true });
  await app.page.waitFor(`document.querySelector(".overlaybox .palette") !== null`);
  await app.page.type(".palettefield", query);
  await app.page.waitFor(`document.querySelectorAll(".palrow").length === 1`);
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector(".palrow .palhint")?.textContent ?? null`),
    todos.agents.id,
    "the local row is on screen at once, before any async answer could have arrived",
  );

  // The debounce (250ms) plus the round trip to the fixture's own database.
  await app.page.waitFor(
    `[...document.querySelectorAll(".palgroup")].some((p) => p.textContent === "Mail")`,
    { timeout: 8000 },
  );
  assert.equal(
    await app.page.evaluate<string | null>(
      `[...document.querySelectorAll(".palrow")]
        .find((r) => r.querySelector(".palhint")?.textContent !== "${todos.agents.id}")
        ?.querySelector(".pallabel")?.textContent ?? null`,
    ),
    mail[0].subject,
  );
  assert.equal(
    await app.page.evaluate<string | null>(
      `[...document.querySelectorAll(".palrow")]
        .find((r) => r.querySelector(".palhint")?.textContent !== "${todos.agents.id}")
        ?.querySelector(".palhint")?.textContent ?? null`,
    ),
    counsel.email,
  );
  // The group is appended below the local row, not instead of it.
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector(".palrow .palhint")?.textContent ?? null`),
    todos.agents.id,
  );
  await app.page.press("Escape");
});

// The second seeded subject matches nothing local, so its one row proves a mail row is a real destination.
app.scene("a seeded mail result opens on Enter, and its page draws the message", async () => {
  await app.open(BASE);
  await app.page.press("k", { ctrl: true });
  await app.page.waitFor(`document.querySelector(".overlaybox .palette") !== null`);
  await app.page.type(".palettefield", "ferret census");
  await app.page.waitFor(
    `[...document.querySelectorAll(".palgroup")].some((p) => p.textContent === "Mail")`,
    { timeout: 8000 },
  );
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".palrow").length`),
    1,
    "the seeded subject matches nothing else, local or mail",
  );
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector(".palrow .pallabel")?.textContent ?? null`),
    mail[1].subject,
  );
  await app.page.press("Enter");
  await app.page.waitFor(`window.location.hash.startsWith("#/mail/")`);
  assert.equal(
    await app.page.evaluate<number>(`document.querySelectorAll(".overlay").length`),
    0,
    "opening the message closes the palette",
  );
  await app.page.waitFor(`document.body.textContent.includes(${JSON.stringify(mail[1].subject)})`);
});
