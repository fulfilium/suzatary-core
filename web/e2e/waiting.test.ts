/**
 * WHAT A PERSON SEES WHILE THE APP WAITS
 * (`docs/AUDIT-EXPERIENCE-2026-10-13.md` X5, X8, X14, X17, X21).
 *
 * Each scene holds what the page waits for and watches what the page draws: a
 * placeholder and never a sentence, turns above the composer, one field above
 * the conversations, and a column that clips nothing. Every wait is on the
 * page's own drawing.
 */

import assert from "node:assert/strict";

import { PHONE, SIGNED_POST, suite, WRITE_BASE } from "./harness";

const app = await suite("waiting");

const WIDE: readonly [number, number] = [1440, 900];

/** The words the page shows a reader, its controls' labels included, outside the app's own chrome. */
const words = `document.getElementById("view").innerText`;

/** The nodes on screen: laid out, not see-through, and not inside a closed disclosure. */
const visible = (selector: string): string =>
  `[...document.querySelectorAll(${JSON.stringify(selector)})].filter((n) => n.checkVisibility({ opacityProperty: true, visibilityProperty: true }))`;

/** Every placeholder of a visible field whose text is wider than the room the field gives it. */
const cutPlaceholders = `(async () => {
  await document.fonts.ready;
  const ctx = document.createElement("canvas").getContext("2d");
  return [...document.querySelectorAll("#view input[placeholder], #view textarea[placeholder]")]
    .filter((i) => i.checkVisibility())
    .filter((i) => {
      const s = getComputedStyle(i);
      ctx.font = s.fontStyle + " " + s.fontWeight + " " + s.fontSize + " " + s.fontFamily;
      return ctx.measureText(i.placeholder).width > i.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight) + 0.5;
    })
    .map((i) => i.placeholder);
})()`;

async function freshConversation(): Promise<string> {
  await app.open(WRITE_BASE);
  return app.page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Waiting"})).then((r) => r.json()).then((j) => j.id)`,
  );
}

/** Hold what the Mail page asks the box, open it, and wait for the placeholder to show; the answer lets the box through. */
async function mailWaiting(): Promise<() => Promise<void>> {
  const release = await app.page.holdRequests(["/api/mail/"]);
  await app.page.evaluate(`location.hash = "#/mail"`);
  await app.page.waitFor(`document.querySelector(".mailpage .pagewait .placeholder.ph-rows") !== null`, { timeout: 8000 });
  await app.page.waitFor(`getComputedStyle(document.querySelector(".mailpage .placeholder")).visibility === "visible"`, { timeout: 8000 });
  return async () => {
    await release();
    await app.page.waitFor(`document.querySelector(".mailpage .maillist") !== null && document.querySelector(".mailpage .placeholder") === null`, { timeout: 15000 });
  };
}

app.scene("a page on its way shows its placeholder after 300 ms with no words, still with reduced motion, in both themes", async () => {
  await app.resize(PHONE);
  await app.open(WRITE_BASE);
  const done = await mailWaiting();
  try {
    assert.equal(
      await app.page.evaluate<string>(`getComputedStyle(document.querySelector(".mailpage .placeholder")).animationDelay`),
      "0.3s",
      "nothing shows for the first 300 ms",
    );
    assert.ok(!/load|reading|asking|fetching/i.test(await app.page.evaluate<string>(words)), "no sentence says it is loading");
    assert.notEqual(await app.page.evaluate<string>(`getComputedStyle(document.querySelector(".mailpage .loadfill")).animationName`), "none", "the mark moves");
    for (const theme of ["light", "dark"] as const) {
      await app.page.readerMedia(theme, "reduce");
      await app.page.waitFor(`matchMedia("(prefers-color-scheme: ${theme})").matches && matchMedia("(prefers-reduced-motion: reduce)").matches`);
      const seen = await app.page.evaluate<{ fill: string; bar: string; ground: string }>(`(() => {
        const s = (q) => getComputedStyle(document.querySelector(q));
        return { fill: s(".mailpage .loadfill").animationName, bar: s(".mailpage .phbar").backgroundColor, ground: getComputedStyle(document.body).backgroundColor };
      })()`);
      assert.equal(seen.fill, "none", `${theme}: with reduced motion the mark holds still`);
      assert.notEqual(seen.bar, seen.ground, `${theme}: the bars stand out from the ground`);
    }
  } finally {
    await done();
    await app.page.colorScheme(null);
  }
});

app.scene("a conversation on its way shows turns above the composer, at the bottom, never an empty page", async () => {
  await app.resize(PHONE);
  const id = await freshConversation();
  // Whatever the conversation is read through, it asks the box first; held, the page stays on its way.
  const release = await app.page.holdRequests(["/api/"]);
  try {
    await app.page.evaluate(`location.hash = "#/secretary/${id}"`);
    await app.page.waitFor(`document.querySelector(".transcriptcontent .placeholder.ph-turns") !== null && document.querySelector(".composebox") !== null`, { timeout: 8000 });
    await app.page.waitFor(`getComputedStyle(document.querySelector(".transcriptcontent .placeholder")).visibility === "visible"`, { timeout: 8000 });
    const at = await app.page.evaluate<{ placeholder: number; composer: number; height: number }>(`({
      placeholder: document.querySelector(".transcriptcontent .placeholder").getBoundingClientRect().bottom,
      composer: document.querySelector(".compose").getBoundingClientRect().top,
      height: innerHeight,
    })`);
    assert.ok(at.placeholder <= at.composer + 1, "the turns stand above the composer");
    assert.ok(at.composer > at.height / 2, "the composer is at the bottom, where a thumb is");
  } finally {
    await release();
  }
  await app.page.waitFor(`document.querySelector(".transcriptcontent .placeholder") === null`, { timeout: 15000 });
});

app.scene("the conversations have one field above them, and no placeholder is cut, at 390 and 1440 px", async () => {
  for (const size of [PHONE, WIDE]) {
    await app.resize(size);
    await app.open(WRITE_BASE);
    await app.page.evaluate(`location.hash = "#/secretary"`);
    await app.page.waitFor(`document.querySelector(".column.secretarystream:not(.pagewaiting) .streamrounds") !== null`, { timeout: 15000 });
    assert.equal(await app.page.evaluate<number>(`${visible(".chats .chathead input")}.length`), 1, `${size[0]} px: the search, and no name field`);
    assert.equal(await app.page.evaluate<number>(`${visible(".chats .chathead .act-newtopic")}.length`), 1, `${size[0]} px`);
    assert.deepEqual(await app.page.evaluate<string[]>(cutPlaceholders), [], `${size[0]} px`);
    await app.page.evaluate(`location.hash = "#/secretary/default"`);
    await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
    if (size === WIDE) await app.page.waitFor(`document.querySelector(".chatsaside .chathead input") !== null`, { timeout: 8000 });
    assert.deepEqual(await app.page.evaluate<string[]>(cutPlaceholders), [], `${size[0]} px`);
  }
});

app.scene("a page waiting on a search keeps its whole column: nothing is clipped, the search field inside it", async () => {
  await app.resize(PHONE);
  await app.open(WRITE_BASE);
  await app.page.evaluate(`location.hash = "#/secretary"`);
  await app.page.waitFor(`document.querySelector(".column.secretarystream:not(.pagewaiting) .streamrounds") !== null`, { timeout: 15000 });
  await app.page.type(".chats .chatsearch", "deed");
  // A search waits at least its debounce, a state the column says.
  await app.page.waitFor(`document.querySelector(".column.chats.pagewaiting") !== null`, { timeout: 8000 });
  const seen = await app.page.evaluate<{ overflow: string; clipped: boolean; inside: boolean }>(`(() => {
    const column = document.querySelector(".column.chats");
    const field = column.querySelector(".chatsearch").getBoundingClientRect();
    const box = column.getBoundingClientRect();
    return {
      overflow: getComputedStyle(column).overflow,
      clipped: column.scrollHeight > column.clientHeight + 1,
      inside: field.top >= box.top - 1 && field.bottom <= box.bottom + 1,
    };
  })()`);
  assert.equal(seen.overflow, "visible");
  assert.equal(seen.clipped, false, "the column shows all it holds");
  assert.equal(seen.inside, true, "the search field stands inside the column");
  await app.page.waitFor(`document.querySelector(".column.chats.pagewaiting") === null`, { timeout: 15000 });
});
