/**
 * The router's parser, over the spellings other writers compose: the APK's widget, links and share sheet, and the
 * box's notifications. Totality and the round trip are `laws.test.ts`'s.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { hrefOf, parseRoute } from "../src/shell/route";

test("the empty hash, #/ and #/todos are all home", () => {
  for (const hash of ["", "#", "#/", "#/todos"]) {
    assert.deepEqual(parseRoute(hash), { type: "todos" });
  }
});

test("#/ does not swallow the other pages", () => {
  assert.deepEqual(parseRoute("#/about"), { type: "about" });
  assert.deepEqual(parseRoute("#/inbox"), { type: "inbox" });
});

test("a todo id is decoded", () => {
  assert.deepEqual(parseRoute("#/todo/permitrenewal"), { type: "todo", id: "permitrenewal" });
  assert.deepEqual(parseRoute(`#/todo/${encodeURIComponent("a b/c")}`), { type: "todo", id: "a b/c" });
});

test("briefings: the list and one day, and a bare prefix names the list", () => {
  assert.deepEqual(parseRoute("#/briefings"), { type: "briefings" });
  assert.deepEqual(parseRoute("#/briefings/2026-09-01"), { type: "briefing", date: "2026-09-01" });
  assert.equal(hrefOf({ type: "briefings" }), "#/briefings");
  assert.equal(hrefOf({ type: "briefing", date: "2026-09-01" }), "#/briefings/2026-09-01");
});

test("the secretary: the list and one conversation", () => {
  assert.deepEqual(parseRoute("#/secretary"), { type: "secretary", refs: [], topics: [] });
  assert.deepEqual(parseRoute("#/secretary/"), { type: "secretary", refs: [], topics: [] });
  assert.deepEqual(parseRoute("#/secretary/2026-09-09-1"), {
    type: "conversation",
    id: "2026-09-09-1",
    refs: [],
  });
  assert.equal(
    hrefOf({ type: "conversation", id: "2026-09-09-1", refs: [] }),
    "#/secretary/2026-09-09-1",
  );
});

/**
 * Android's share sheet hands the app two strings and `MainActivity` spells them into this fragment, the only protocol
 * between the two halves; a shared URL holds `?`, `&`, `#`, `=` and `/`, each structure in a hash with a query.
 */
test("a shared text and title ride the hash, and round-trip", () => {
  assert.deepEqual(parseRoute("#/add"), { type: "add", text: "", title: "" });
  assert.deepEqual(parseRoute("#/add?text=Renew%20the%20lease"), {
    type: "add",
    text: "Renew the lease",
    title: "",
  });
  assert.deepEqual(
    parseRoute("#/add?text=https%3A%2F%2Fexample.test%2Fa%3Fb%3D1%26c%3D2%23top&title=A%20title"),
    { type: "add", text: "https://example.test/a?b=1&c=2#top", title: "A title" },
  );
  // A title with no text is still a share — a note app that names a heading.
  assert.deepEqual(parseRoute("#/add?title=Just%20a%20name"), {
    type: "add",
    text: "",
    title: "Just a name",
  });
  // Malformed, like every other route here: absent rather than an exception.
  assert.deepEqual(parseRoute("#/add?text=%E0%A4%A"), { type: "add", text: "", title: "" });
  for (const route of [
    { type: "add", text: "", title: "" } as const,
    { type: "add", text: "https://example.test/a?b=1&c=2#top", title: "A title" } as const,
    { type: "add", text: "a + b = c, 100%", title: "" } as const,
  ]) {
    assert.deepEqual(parseRoute(hrefOf(route)), route);
  }
});
