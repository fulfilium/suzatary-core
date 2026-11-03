/**
 * MEMORY, ABOUT, THE CALENDAR AND THE BRIEFINGS AS RUST PROGRAMS (`app/pages`), drawn from the box's answers in a
 * real browser. Mail, a message and a thread are `mail.test.ts`'s.
 *
 * The laws of each page (the view a pure function of its model, a patch applied equal to a fresh
 * draw) are `app/pages/tests/laws.rs`. What only a browser and a box can show is here: the module
 * loads, every runtime effect a page asks for is performed by `web/src/pages/program.ts`, the
 * painter draws what the program answers, and a click on a node the program drew comes back to
 * it as its message.
 */

import assert from "node:assert/strict";

import { BASE, suite } from "./harness";

const app = await suite("programs");

/** The page at `hash` drawn past its placeholder under `title`: a program mounts once its module is here, so the page
 * before it stands until then, and only the title it draws says it has. */
async function drawn(hash: string, title: string): Promise<void> {
  await app.page.navigate(`${BASE}/${hash}`);
  await app.page.waitFor(
    `document.querySelector("#view .pagetitle")?.textContent === ${JSON.stringify(title)} && document.querySelector("#view .pagewaiting") === null`,
    { timeout: 15000 },
  );
}

app.scene("each page draws what the box answers", async () => {
  await app.open(BASE);
  await drawn("#/about", "About");
  assert.match(await app.page.evaluate<string>(`document.querySelector(".aboutversion").textContent`), /^Suzatary /);
  await drawn("#/briefings", "Briefings");
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".briefingspage .pagetitle.destination") !== null`), true);
  await drawn("#/secretary/memory", "Memory");
  assert.match(await app.page.evaluate<string>(`document.querySelector(".memorypage .legend").textContent`), /facts, background for every conversation\./);
  await drawn("#/calendar", "Calendar");
});

app.scene("a day with no briefing is an answer, not a failure", async () => {
  await app.open(BASE);
  await drawn("#/briefings/2001-01-01", "2001-01-01");
  const said = await app.page.evaluate<string>(`document.querySelector(".briefingpage .pagebody").textContent`);
  assert.equal(said, "No briefing was written for this day.");
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".briefingpage .problem") === null`), true);
});

app.scene("memory goes back to the secretary", async () => {
  await app.open(BASE);
  await drawn("#/secretary/memory", "Memory");
  // The click reaches the program as its Back, and its effect moves the address.
  await app.page.evaluate(`document.querySelector(".memorypage .back").click()`);
  await app.page.waitFor(`!location.hash.startsWith("#/secretary/memory")`);
});
