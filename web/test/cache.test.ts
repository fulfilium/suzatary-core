/**
 * The cached fold is SLIM (the two fields that make a fold megabytes never reach `localStorage`, whose quota exception
 * is swallowed) and PARSED: an older build's, half-written or hand-edited cache is `null` and costs one fetch.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import type { Authored, Entry } from "../src/wire/generated/wire";
import { MAX_CHARS, fits, parseFold, printFold, slim } from "../src/app/cache";
import cast from "./cast";

const body = `reply to ${cast.world.counsel.name}`;

function authored(over: Partial<Authored> = {}): Authored {
  return {
    rich: { body: "<p>body</p>", detail: "<p>a screen and a half</p>", rationale: null, waiting_on: null, keys: { body: null, detail: null, rationale: null, waiting_on: null } },
    block: { kind: "todo", name: "task" },
    created: "2026-08-01",
    body,
    text: body,
    detail: "several paragraphs of detail",
    category: "study",
    waiting_on: "",
    rationale: ["because"],
    spec: null,
    actor: "ada",
    at: "2026-08-01T09:00:00",
    source: null,
    subtodos: [],
    ...over,
  };
}

function entry(over: Partial<Entry> = {}): Entry {
  return {
    todo: "harrowreply",
    state: "open",
    states: ["open"],
    at: "",
    claimed: "",
    unconfirmed: false,
    recurring: false,
    value: 0.42,
    conflicts: {},
    authored: authored(),
    candidates: [],
    stream: ["hash1", "hash2"],
    ...over,
  };
}

test("slimming drops exactly what no card draws", () => {
  const small = slim(entry());
  assert.deepEqual(small.stream, []);
  assert.equal(small.authored?.detail, "");
  assert.equal(small.authored?.rich.detail, null);
  // …and keeps everything that a card does.
  assert.equal(small.authored?.body, body);
  assert.equal(small.authored?.rich.body, "<p>body</p>");
  assert.equal(small.value, 0.42);
  assert.equal(small.todo, "harrowreply");
});

test("slimming a todo with no Authored record is not a crash", () => {
  assert.equal(slim(entry({ authored: null })).authored, null);
});

test("a fold round-trips, slimmed", () => {
  const printed = printFold({ at: "2026-09-04T11:00:00", fetchedAt: 1234, entries: [entry()] });
  const back = parseFold(printed);
  assert.notEqual(back, null);
  assert.equal(back?.at, "2026-09-04T11:00:00");
  assert.equal(back?.fetchedAt, 1234);
  assert.equal(back?.entries[0]?.authored?.detail, "");
  assert.deepEqual(back?.entries[0]?.stream, []);
});

test("the detail never reaches the stored string at all", () => {
  const printed = printFold({ at: "t", fetchedAt: 0, entries: [entry()] });
  assert.equal(printed.includes("several paragraphs of detail"), false);
  assert.equal(printed.includes("a screen and a half"), false);
  assert.equal(printed.includes("hash1"), false);
});

test("anything else in localStorage is nothing, not a broken page", () => {
  for (const stored of [null, undefined, "", "{", "[]", "3", '"a fold"', "{}", '{"at":"t"}']) {
    assert.equal(parseFold(stored), null, String(stored));
  }
});

test("a cache whose entries are the wrong shape is refused whole", () => {
  // An older build's Entry is not this build's, and half a fold is worse than none.
  assert.equal(parseFold('{"at":"t","tip":"","fetchedAt":1,"entries":[{"todo":"a"}]}'), null);
});

test("a fold too big for the box is not kept", () => {
  assert.equal(fits("x".repeat(MAX_CHARS)), true);
  assert.equal(fits("x".repeat(MAX_CHARS + 1)), false);
});
