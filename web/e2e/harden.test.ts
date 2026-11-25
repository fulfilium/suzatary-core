/**
 * `rich.ts`'s `harden()` rewrites every anchor typst emitted: a `todo:<id>` becomes an in-app hash, and everything else
 * is outbound and leaves with `rel="noopener noreferrer"` and `target="_blank"`. ⚠️ `harden` needs a DOM, so this scene
 * is its only test; it reads one rendered body carrying both kinds of anchor (`pack-bags`' detail), because the claim
 * is that the two are told apart. Nothing here writes, so it shares the read-only server.
 */

import assert from "node:assert/strict";

import { BASE, DESKTOP, suite } from "./harness";
import cast from "../test/cast";

const { todos } = cast.world;

const app = await suite("harden");

/** Read off the live DOM, not the payload: `harden` runs on nodes after parsing. */
type Anchor = { href: string; rel: string; target: string; cls: string; text: string; beside: string };
const anchorBy = async (text: string): Promise<Anchor | null> =>
  app.page.evaluate<Anchor | null>(`(() => {
    const anchor = [...document.querySelectorAll(".todoview .rich a")].find(
      (candidate) => candidate.textContent?.includes(${JSON.stringify(text)}),
    );
    if (anchor === undefined) return null;
    return {
      href: anchor.getAttribute("href") ?? "",
      rel: anchor.getAttribute("rel") ?? "",
      target: anchor.getAttribute("target") ?? "",
      cls: anchor.getAttribute("class") ?? "",
      text: anchor.textContent ?? "",
      beside: anchor.nextSibling?.nodeType === 3 ? anchor.nextSibling.textContent ?? "" : "",
    };
  })()`);

const openPackBags = async (): Promise<void> => {
  await app.open(BASE, `#/todo/${todos.checklist.id}`);
  // A cold typst cache serves source, which holds no anchors, so the wait is for the rendered body.
  await app.page.waitFor(`document.querySelectorAll(".todoview .rich a").length >= 2`, { timeout: 20000 });
};

app.scene("a todo: link becomes a hash into this app, and an outbound one leaves with noopener noreferrer", async () => {
  await app.resize(DESKTOP);
  await openPackBags();
  const inward = await anchorBy("the flight");
  assert.ok(inward !== null, "the rendered detail should carry the todo: anchor");
  assert.equal(inward.href, `#/todo/${todos.flat.id}`, "a todo: link must be rewritten to the in-app hash");
  // A new tab for a link into this app would leave two copies of the tab and a Back button that does nothing.
  assert.equal(inward.target, "", "an in-app link must not open a new tab");
  assert.ok(inward.cls.split(/\s+/).includes("todolink"), "the rewritten anchor is marked, so the stylesheet can say so");

  const outward = await anchorBy("example.invalid");
  assert.ok(outward !== null, "the rendered detail should carry the outbound anchor");
  assert.equal(outward.href, "https://example.invalid/baggage", "an outbound href is left exactly as typst wrote it");
  assert.equal(outward.text, "example.invalid", "an outbound anchor reads as the host it leads to");
  // typst sets the label's apostrophe as a typographic one.
  assert.equal(outward.beside, " — the airline\u2019s page", "the writer's label stands beside it, as plain text");
  // Without `noopener` the opened page holds `window.opener` and can navigate this one; `noreferrer` because the strings came out of mail.
  assert.equal(outward.rel, "noopener noreferrer");
  assert.equal(outward.target, "_blank");
  assert.ok(!outward.cls.split(/\s+/).includes("todolink"), "an outbound link is not an in-app one");
});
