/**
 * A NEW TOPIC FROM THE SECRETARY PAGE, AT PHONE WIDTH: New topic opens a draft the box holds nothing of, the name
 * typed over the composer and the first message open the conversation in one send, and the answer reaches the page.
 * A draft left with nothing sent leaves no conversation behind, so no empty "New topic" stands on the box. A rename
 * closes on the box's answer, though the device is still taking its lines, as it is after an update moved their form.
 *
 * Its own file, so the conversations the box holds are only the ones its scene made.
 */

import assert from "node:assert/strict";

import { atTheTerminal, PHONE, SIGNED_POST, suite, WRITE_BASE } from "./harness";

const app = await suite("topic");

/** The dated conversations the writer's box holds, by `secretary list`. */
const dated = (): string[] => atTheTerminal(["secretary", "list"]).split("\n").filter((l) => /^\d{4}-\d{2}-\d{2}-\d+ /.test(l));

app.scene("a new topic is named, asked and answered, and a draft left unsent writes nothing", async () => {
  await app.resize(PHONE);
  await app.page.navigate(`${WRITE_BASE}/#/secretary`);
  await app.page.waitFor(`document.querySelector(".act-newtopic") !== null`, { timeout: 15000 });
  const before = dated();

  await app.page.click(".act-newtopic");
  await app.page.waitFor(`location.hash === "#/secretary/new"`, { timeout: 8000 });
  await app.page.waitFor(`document.activeElement?.classList.contains("composebox") === true`, { timeout: 8000 });
  await app.page.evaluate(`history.back()`);
  await app.page.waitFor(`location.hash === "#/secretary"`, { timeout: 8000 });
  assert.deepEqual(dated(), before, "a draft left unsent opens no conversation");

  await app.page.click(".act-newtopic");
  await app.page.waitFor(`document.querySelector(".topicname") !== null`, { timeout: 8000 });
  await app.page.type(".topicname", "The zephyr filing");
  await app.page.type(".composebox", "when is the zephyr filing due");
  await app.page.click(".composesend");
  await app.page.waitFor(`/^#\\/secretary\\/\\d{4}-\\d{2}-\\d{2}-\\d+$/.test(location.hash)`, { timeout: 15000 });
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "The zephyr filing"`, { timeout: 15000 });
  await app.page.waitFor(
    `[...document.querySelectorAll(".turn-person .turntext")].some((t) => t.textContent.includes("when is the zephyr filing due"))`,
    { timeout: 15000 },
  );
  await app.page.waitFor(`(document.querySelector("#view")?.textContent ?? "").includes("Nothing has changed since the last briefing.")`, { timeout: 30000 });

  const after = dated();
  assert.equal(after.length, before.length + 1, "one send opened one conversation");
  assert.match(after.find((l) => !before.includes(l)) ?? "", /The zephyr filing {2}2 turn\(s\)/, "it holds its name, the question and the answer");
});

app.scene("a rename closes on the box's answer while the lines are still being taken", async () => {
  await app.resize(PHONE);
  await app.page.attachWorkers();
  await app.page.navigate(`${WRITE_BASE}/#/secretary/default`);
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  const id = await app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "The filing"})).then((r) => r.json()).then((j) => j.id)`,
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
    assert.match(atTheTerminal(["secretary", "list"]), new RegExp(`^${id} {2}The zephyr filing `, "m"), "the box holds the new name");
  } finally {
    await release();
  }
  await app.page.waitFor(`document.querySelector(".pagetitle")?.textContent === "The zephyr filing"`, { timeout: 15000 });
});
