/**
 * A new topic from the secretary page at phone width: the name typed over the composer and the first message open the
 * conversation in one send, a draft left with nothing sent leaves no conversation on the box, and a rename closes on
 * the box's answer though the device is still taking its lines. Its own file, so the conversations the box holds are
 * only the ones its scenes made.
 */

import assert from "node:assert/strict";

import { atTheTerminal, PHONE, suite, WRITE_BASE, opened } from "./harness";

const app = await suite("topic");

/** The conversations a tab opened on the writer's box, by `secretary list`: each named by its opening line's id. */
const topics = (): string[] => atTheTerminal(["secretary", "list"]).split("\n").filter((l) => /^[0-9a-f]{32} /.test(l));

app.scene("a new topic is named, asked and answered, and a draft left unsent writes nothing", async () => {
  await app.resize(PHONE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
  await app.page.waitFor(`document.querySelector(".act-newtopic") !== null`, { timeout: 15000 });
  const before = topics();

  await app.page.click(".act-newtopic");
  await app.page.waitFor(`location.hash === "#/secretary/new"`, { timeout: 8000 });
  await app.page.waitFor(`document.activeElement?.classList.contains("composebox") === true`, { timeout: 8000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/secretary"`, { timeout: 8000 });
  assert.deepEqual(topics(), before, "a draft left unsent opens no conversation");

  await app.page.click(".act-newtopic");
  // The draft's own name field, shown: a field left from another page cannot take focus, so the words would go elsewhere.
  await app.page.waitFor(
    `location.hash === "#/secretary/new" && document.activeElement?.classList.contains("composebox") === true && (document.querySelector(".topicname")?.getClientRects().length ?? 0) > 0`,
    { timeout: 8000 },
  );
  await app.page.type(".topicname", "The zephyr filing");
  await app.page.waitFor(`document.querySelector(".topicname")?.value === "The zephyr filing"`, { timeout: 8000 });
  await app.page.type(".composebox", "when is the zephyr filing due");
  await app.page.click(".composesend");
  await app.page.waitFor(`/^#\\/secretary\\/[0-9a-f]{32}$/.test(location.hash)`, { timeout: 15000 });
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "The zephyr filing"`, { timeout: 15000 });
  await app.page.waitFor(
    `[...document.querySelectorAll(".turn-person .turntext")].some((t) => t.textContent.includes("when is the zephyr filing due"))`,
    { timeout: 15000 },
  );
  await app.page.waitFor(`(document.querySelector("#view")?.textContent ?? "").includes("Nothing has changed since the last briefing.")`, { timeout: 30000 });

  const after = topics();
  assert.equal(after.length, before.length + 1, "one send opened one conversation");
  assert.match(after.find((l) => !before.includes(l)) ?? "", /The zephyr filing {2}2 turn\(s\)/, "it holds its name, the question and the answer");
});

app.scene("a rename is drawn at once while the lines are held back, and the box holds it once they go", async () => {
  await app.resize(PHONE);
  await app.page.attachWorkers();
  await app.page.navigate(`${WRITE_BASE}/#/secretary/default`);
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  const id = await app.page.evaluate<string>(
    opened("The filing"),
  );
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "The filing"`, { timeout: 15000 });

  const release = await app.page.holdRequests(["/api/stores/lines/"]);
  try {
    await app.page.click(".act-rename");
    await app.page.waitFor(`document.activeElement?.classList.contains("renamefield") === true`, { timeout: 8000 });
    await app.page.evaluate(`document.querySelector(".renamefield").value = "The zephyr filing"`);
    await app.page.press("Enter");
    await app.page.waitFor(`document.querySelector(".renamefield") === null && document.querySelector(".pagetitle")?.textContent === "The zephyr filing"`, { timeout: 8000 });
  } finally {
    await release();
  }
  // The line is handed over behind the drawing; the box holds it once the held requests go.
  const named = new RegExp(`^${id} {2}The zephyr filing `, "m");
  for (let tries = 0; !named.test(atTheTerminal(["secretary", "list"])); tries += 1) {
    assert.ok(tries < 50, "the box holds the new name");
    await new Promise((done) => setTimeout(done, 200));
  }
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "The zephyr filing"`, { timeout: 15000 });
});
