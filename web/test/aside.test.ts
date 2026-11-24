/**
 * `chat/aside.ts`, the owner's rule for the secretary beside a wide page: it opens by itself only on a page about one
 * thing when the conversation holds talk about it; a page of many things keeps it closed until a press; a press, open
 * or close, holds for that page alone, and a page of many things is its destination.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import aside from "../src/chat/aside";
import app from "../src/app/app";
import { parseAddress } from "../src/shell/route";
import { forAll } from "./gen";

const at = (hash: string): aside.Placement => aside.placement(app.initial(parseAddress(hash), "life"), "Beside");
const page = (hash: string): aside.Page => {
  const p = at(hash);
  assert.equal(p.type, "Beside", hash);
  return p.type === "Beside" ? p.page : { type: "Many", key: "" };
};
const NONE: aside.Presses = {};

test("it opens by itself on a page about one thing with talk, and never on a page of many", () => {
  assert.equal(aside.shown(page("#/todo/file-permit"), NONE, "Talking"), true);
  assert.equal(aside.shown(page("#/todo/file-permit"), NONE, "Quiet"), false);
  for (const hash of ["#/", "#/inbox", "#/settings", "#/mail", "#/calendar"]) {
    assert.equal(aside.shown(page(hash), NONE, "Talking"), false, hash);
  }
});

test("law: a press turns over what is drawn on its page and nowhere else", () => {
  const pages = ["#/", "#/inbox", "#/calendar", "#/todo/file-permit", "#/todo/book-flight", "#/mail/m1%40example.com"].map(page);
  forAll(300, (r) => {
    const p = r.pick(pages);
    const presses: aside.Presses = Object.fromEntries(pages.filter(() => r.bool()).map((q) => [q.key, r.pick(["open", "closed"] as const)]));
    const talk = r.pick(["Quiet", "Talking"] as const);
    const after = aside.pressed(p, presses, talk);
    assert.equal(aside.shown(p, after, talk), !aside.shown(p, presses, talk));
    for (const q of pages) if (q.key !== p.key) assert.equal(aside.shown(q, after, talk), aside.shown(q, presses, talk), q.key);
  });
});
