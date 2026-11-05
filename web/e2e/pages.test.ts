/**
 * EVERY PAGE, AT PHONE WIDTH, DRAWS WHAT THE BOX ANSWERS AND NOTHING FAILS. The scene asks nothing of the data beyond
 * a todo to open, so it runs against the fixture's reader in the suite and against a copy of the owner's data tree in
 * the live half's smoke (`scripts/web-e2e.sh` with `SUZATARY_E2E_LIVE`), where it is the one scene: what the other
 * scenes prove of each page on the fixture, this proves the owner's real readings do not break.
 */

import assert from "node:assert/strict";

import { BASE, PHONE, suite } from "./harness";

const app = await suite("pages", PHONE, "Reads");

/** Every destination a person reaches from the shell's navigation; a todo of the list is added by the scene. */
const PAGES = [
  "#/",
  "#/inbox",
  "#/typst",
  "#/mail",
  "#/chain",
  "#/secretary",
  "#/secretary/memory",
  "#/briefings",
  "#/calendar",
  "#/usage",
  "#/about",
  "#/settings",
] as const;

/** What says a destination has drawn past its placeholder: its title. */
const TITLED = `document.querySelector("#view .pagetitle") !== null && document.querySelector("#view .pagewaiting") === null`;

/** The page at `hash` drawn as `drawn` says, then what it shows that says it failed: a problem drawn in the view, or an
 * exception the page threw while it drew. */
async function failures(hash: string, thrown: () => string[], drawn = TITLED): Promise<string[]> {
  const before = thrown().length;
  await app.page.navigate(`${BASE}/${hash}`);
  try {
    await app.page.waitFor(drawn, { timeout: 60000 });
  } catch {
    return [`${hash}: never drew`];
  }
  const problems = await app.page.evaluate<string[]>(
    `[...document.querySelectorAll("#view .problem")].filter((p) => !p.hidden && p.textContent.trim() !== "").map((p) => p.textContent.trim())`,
  );
  return [...problems, ...thrown().slice(before)].map((said) => `${hash}: ${said}`);
}

app.scene("every page draws at phone width with no problem and no exception", async () => {
  const journal = await app.journal();
  const thrown = (): string[] => journal.lines().filter((line) => / exception /.test(line));
  const failed: string[] = [];
  for (const hash of PAGES) failed.push(...(await failures(hash, thrown)));
  await app.page.navigate(`${BASE}/#/`);
  await app.page.waitFor(`document.querySelectorAll(".tree .row[data-todo]").length > 0`, { timeout: 60000 });
  const todo = await app.page.evaluate<string>(`document.querySelector(".tree .row[data-todo]").dataset.todo`);
  // A todo at phone width is a pushed screen with no title of its own: its reading is what it draws.
  failed.push(...(await failures(`#/todo/${encodeURIComponent(todo)}`, thrown, `document.querySelector(".todoview .whytree") !== null`)));
  assert.deepEqual(failed, []);
});
