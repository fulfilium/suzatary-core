/**
 * `chat/aside.ts`, the owner's rule for the secretary beside a page, as the desk's `Dock`: it stands by itself only on a
 * page about one thing when the conversation holds talk about it, and the talk is asked only there; a press, show or
 * hide, holds for that page alone, and a page of many things is its destination. None beside the secretary's own
 * pages, nor beside a page of many things under a phone's chrome; the secretary itself is held as it stands.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import aside from "../src/chat/aside";
import { parseRoute, type Route } from "../src/shell/route";
import { forAll } from "./gen";

const at = (hash: string): Route => parseRoute(hash);
const NONE: aside.Presses = {};
const MANY = ["#/", "#/inbox", "#/settings", "#/mail", "#/calendar"];
const talking = (said: { readonly [about: string]: aside.Heard }) => (about: string): aside.Heard => said[about] ?? "Unheard";
const of = (hash: string, presses: aside.Presses, talk = talking({}), chrome: aside.Chrome = "Wide"): aside.Said => aside.of(at(hash), chrome, "life", presses, talk);
const shown = (hash: string, presses: aside.Presses, chrome: aside.Chrome = "Wide"): aside.Presses | null => {
  const d = of(hash, presses, talking({}), chrome).door;
  return d.type === "None" ? null : d.shows;
};

test("it stands by itself on a page about one thing with talk, never on a page of many unpressed, never beside its own pages", () => {
  const talk = talking({ "todo:life/file-permit": "Talking", "todo:life/book-flight": "Quiet", "": "Talking" });
  const permit = { "todo:life/file-permit": "open" } as const;
  for (const chrome of ["Wide", "Phone"] as const) {
    assert.deepEqual(of("#/todo/file-permit", NONE, talk, chrome), { dock: { type: "Held", about: "todo:life/file-permit" }, door: { type: "Header", shows: permit }, asks: "todo:life/file-permit" });
    assert.deepEqual(of("#/todo/book-flight", NONE, talk, chrome).dock, { type: "Absent" });
    assert.deepEqual(of("#/todo/unread", NONE, talk, chrome).dock, { type: "Absent" }, "talk not yet read shows nothing");
    for (const hash of ["#/secretary", "#/secretary/default", "#/conversations"]) assert.deepEqual(of(hash, NONE, talk, chrome), { dock: { type: "Absent" }, door: { type: "None" }, asks: null }, hash);
    assert.deepEqual(of("#/aside/todo%3Alife%2Fx", NONE, talk, chrome), { dock: { type: "Held", about: "todo:life/x" }, door: { type: "None" }, asks: null });
  }
  for (const hash of MANY) assert.deepEqual(of(hash, NONE, talk).door.type, "Header", hash);
  for (const hash of MANY) assert.deepEqual(of(hash, NONE, talk).dock, { type: "Absent" }, hash);
});

test("under a phone's chrome a page of many things has no secretary beside it and no door, whatever the presses", () => {
  for (const hash of MANY) {
    const open = shown(hash, NONE) ?? NONE;
    assert.deepEqual(of(hash, open).dock, { type: "Held", about: "" }, `${hash}, wide`);
    assert.deepEqual(of(hash, open, talking({}), "Phone"), { dock: { type: "Absent" }, door: { type: "None" }, asks: null }, `${hash}, a phone`);
  }
});

test("a press decides alone: the talk is asked only on a page about one thing that no press decided", () => {
  const closed = aside.hidden("todo:life/file-permit", at("#/todo/file-permit"), "life", NONE);
  const talk = talking({ "todo:life/file-permit": "Talking" });
  const permit = of("#/todo/file-permit", closed, talk);
  assert.deepEqual([permit.dock, permit.asks], [{ type: "Absent" }, null]);
  const open = shown("#/todo/book-flight", NONE) ?? NONE;
  const flight = of("#/todo/book-flight", open);
  assert.deepEqual([flight.dock, flight.asks], [{ type: "Held", about: "todo:life/book-flight" }, null]);
});

test("a hide is the press of the page the secretary is about: the whole conversation's is the page of many it stood beside", () => {
  const open = shown("#/", NONE) ?? NONE;
  assert.deepEqual(aside.hidden("", at("#/"), "life", open), { todos: "closed" });
  // The secretary about a todo, typed and standing beside the list, leaves the list's own press as it was.
  assert.deepEqual(aside.hidden("todo:life/x", at("#/"), "life", open), { todos: "open", "todo:life/x": "closed" });
  assert.deepEqual(aside.went("todo:life/x", NONE), { "todo:life/x": "open" }, "gone to, it is asked for beside its page");
  assert.equal(aside.went("", NONE), NONE);
});

test("law: a press holds on its page and nowhere else", () => {
  const pages = ["#/", "#/inbox", "#/calendar", "#/todo/file-permit", "#/todo/book-flight", "#/mail/m1%40example.com"];
  const press = (p: string, presses: aside.Presses, how: aside.Press): aside.Presses => {
    if (how === "open") return shown(p, presses) ?? presses;
    const about = of(p, NONE).asks ?? "";
    return aside.hidden(about, at(p), "life", presses);
  };
  forAll(300, (r) => {
    const talk = talking({ "todo:life/file-permit": r.pick(["Quiet", "Talking"] as const) });
    const p = r.pick(pages);
    let presses: aside.Presses = NONE;
    for (const q of pages.filter(() => r.bool())) presses = press(q, presses, r.pick(["open", "closed"] as const));
    const how = r.pick(["open", "closed"] as const);
    const after = press(p, presses, how);
    assert.equal(of(p, after, talk).dock.type, how === "open" ? "Held" : "Absent");
    const say = (q: string, ps: aside.Presses): readonly unknown[] => [of(q, ps, talk).dock, of(q, ps, talk).asks];
    for (const q of pages) if (!sameKey(q, p)) assert.deepEqual(say(q, after), say(q, presses));
  });
});

// Two pages a press reaches as one: the same reference, or many things under one destination.
const sameKey = (a: string, b: string): boolean =>
  JSON.stringify(shown(a, NONE)) === JSON.stringify(shown(b, NONE));

test("a page that names itself is asked about by that name: a message's row hash, whatever id its address used", () => {
  const talk = talking({ "mail:fullhash": "Talking", "mail:shortid": "Quiet" });
  const said = aside.of(at("#/mail/shortid"), "Wide", "life", NONE, talk, "mail:fullhash");
  assert.deepEqual(said, { dock: { type: "Held", about: "mail:fullhash" }, door: { type: "Header", shows: { "mail:fullhash": "open" } }, asks: "mail:fullhash" });
  assert.equal(aside.of(at("#/mail/shortid"), "Wide", "life", NONE, talk).asks, "mail:shortid", "unnamed, the route's id");
});
