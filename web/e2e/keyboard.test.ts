/**
 * The keyboard, in a browser rather than as a function.
 *
 * `web/test/keys.test.ts` already pins what a key MEANS — including the
 * negative property that matters most, that a focused field yields nothing but
 * Escape — and `web/test/row.test.ts` what l and h do where the cursor stands.
 * What they cannot check is that the meaning is WIRED: that `j` reaches the
 * list's cursor, that the cursor is focus so a screen reader follows it, that
 * `/` lands the caret in the search field, and that ⌘K is the one chord this
 * app claims from the browser.
 *
 * The cursor is asserted through `document.activeElement`, because that is
 * what the app decided the cursor IS (`app/pages/src/todos.rs`: focus is the mark, so
 * Tab and `j` cannot disagree). A test that read a private variable would pass
 * while the ring was somewhere else.
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

app.scene("j and k move the cursor down and up the list, group heads and lines alike", async () => {
  await app.open(BASE);
  await app.page.press("j");
  assert.equal(await cursor(), "c:work", "j from nowhere should take the first group's head");
  await app.page.press("j");
  assert.equal(await cursor(), `c:work/t:${todos.urgent.id}`);
  await app.page.press("k");
  assert.equal(await cursor(), "c:work");
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
    // AND PUT IT BACK: the search is part of the remembered filter, and the
    // scenes below share this browser profile.
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

/**
 * SEARCH EVERYTHING (project chain `searcheverything`, docs/APP.md §4 item 6):
 * the palette's mail source, in a real browser.
 *
 * SINCE `fixturemailisolation` THE FIXTURE HAS ITS OWN MAIL. `suzatary-dev
 * fixture-chain` (`app/cli/src/commands/fixture.rs::mail_fixture`) builds a
 * notmuch database in the fixture's own temp directory and seeds it with two
 * synthetic messages, so `/api/mail/search` here answers from THAT database
 * and never this box's — the content is known ahead of time, so this scene
 * asserts on it directly rather than only on shape. `MAIL_ONE_SUBJECT`
 * (`cast.world.mail[0]`, from counsel) is deliberately about the same deed
 * the agent's todo is, so one query
 * matches a local row and a mail row together, exactly as it would on a real
 * corpus.
 */
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
  // The mail row is the SEEDED message, by name — not just "some label".
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
  // The local row is UNTOUCHED — the group is appended below it, not instead.
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector(".palrow .palhint")?.textContent ?? null`),
    todos.agents.id,
  );
  await app.page.press("Escape");
});

/** `MAIL_TWO_SUBJECT` matches nothing local, so this is one row, unambiguous
 * to open: the whole point is proving a mail row is a REAL destination, not
 * only a label the palette can draw, and that the message page draws what the
 * box answers for it. */
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
