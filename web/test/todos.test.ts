/**
 * The list, as a function.
 *
 * The home page's whole behaviour — which todos, in what order, under which
 * heading — is `todos.ts`, and these are the properties it must not lose. The
 * first one is the system's: **LOW MEANS URGENT**, ascending. A regression that
 * flipped it would look like a working app showing exactly the wrong todo
 * first, which is why it is pinned here as well as on the box, where
 * `site::urgency` and `suzatary todos open` order the same way.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import type { Authored, Entry } from "../src/wire/generated/wire";
import {
  ANY,
  DEFAULT_FILTER,
  byUrgency,
  is,
  isDecision,
  listing,
  matches,
  parseFilter,
  printFilter,
  signature,
  total,
} from "../src/todo/todos";
import cast from "./cast";

const { counsel } = cast.world;

/** A fixed instant, so a boundary test does not depend on when it ran. */
const NOW = "2026-09-13T00:00:00.000Z";

function authored(over: Partial<Authored> = {}): Authored {
  return {
    rich: { body: null, detail: null, rationale: null, waiting_on: null, keys: { body: null, detail: null, rationale: null, waiting_on: null } },
    block: { kind: "todo", name: "todo" },
    created: "2026-08-30",
    body: `Reply to the ${counsel.name} thread`,
    text: `Reply to the ${counsel.name} thread`,
    detail: "",
    category: "study",
    waiting_on: "",
    rationale: [],
    spec: null,
    actor: "ada",
    at: "2026-09-01T09:15:00",
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
    stream: [],
    ...over,
  };
}

// --- the ordering ------------------------------------------------------------

test("the order: ascending fulfillment, the lowest number the most urgent; ties on the id; an unpriced todo last", () => {
  const cases: [string, Entry[], string[]][] = [
    ["the lowest number is the most urgent", [entry({ todo: "b", value: 0.9 }), entry({ todo: "a", value: 0.2 })], ["a", "b"]],
    ["equal values break on the id, so a redraw never shuffles", [entry({ todo: "b", value: 0.5 }), entry({ todo: "a", value: 0.5 })], ["a", "b"]],
    ["no price is not a price of zero", [entry({ todo: "z", value: null }), entry({ todo: "a", value: 0.05 })], ["a", "z"]],
  ];
  for (const [why, rows, order] of cases) assert.deepEqual(rows.sort(byUrgency).map((each) => each.todo), order, why);
});

// --- the listing -------------------------------------------------------------

test("unpriced todos are shown under their own heading, never hidden", () => {
  const list = listing(
    [entry({ todo: "b", value: 0.8 }), entry({ todo: "n", value: null }), entry({ todo: "a", value: 0.1 })],
    DEFAULT_FILTER,
    NOW,
  );
  assert.deepEqual(list.priced.map((each) => each.todo), ["a", "b"]);
  assert.deepEqual(list.unpriced.map((each) => each.todo), ["n"]);
  assert.equal(total(list), 3);
});

// --- the remembered filter ---------------------------------------------------

test("the filter round-trips through the string localStorage holds", () => {
  const filter = {
    state: "all",
    category: is("work"),
    kind: ANY,
    query: "deed",
    attention: true,
    waiting: true,
    dated: true,
  } as const;
  assert.deepEqual(parseFilter(printFilter(filter)), filter);
  assert.deepEqual(parseFilter(printFilter(DEFAULT_FILTER)), DEFAULT_FILTER);
});

test("anything else in localStorage degrades to the default", () => {
  for (const stored of [null, undefined, "", "{", "[]", '"open"', "3", '{"state":"sideways"}', "{}"]) {
    assert.deepEqual(parseFilter(stored), DEFAULT_FILTER);
  }
});

test("a partial stored filter keeps the fields it does carry", () => {
  assert.deepEqual(parseFilter('{"state":"all","query":"work"}'), {
    ...DEFAULT_FILTER,
    state: "all",
    query: "work",
  });
});

// --- the card's signature -----------------------------------------------------

test("the signature changes when a card's own fields change", () => {
  const open = entry({ value: 0.2 });
  assert.equal(signature(open), signature(entry({ value: 0.2 })));
  assert.notEqual(signature(open), signature(entry({ value: 0.3 })));
  assert.notEqual(signature(open), signature(entry({ state: "completed" })));
  assert.notEqual(signature(open), signature(entry({ unconfirmed: true })));
  assert.notEqual(signature(open), signature(entry({ conflicts: { state: ["a", "b"] } })));
  assert.notEqual(
    signature(open),
    signature(entry({ authored: authored({ body: "something else" }) })),
  );
});

test("the signature ignores the stream: an append no card shows is not a redraw", () => {
  assert.equal(signature(entry({ stream: [] })), signature(entry({ stream: ["a", "b"] })));
});

/**
 * A DECISION IS NOT A TODO. The
 * document has separated them since the kind existed; this pins that the
 * list does too, because a decision priced at 55% sitting among the open
 * todos is exactly what shipped to the box on 19 September.
 */
test("the listing leaves decisions to the Inbox", () => {
  const rows = [
    entry({ todo: "harrowreply", value: 0.42 }),
    entry({ todo: "settlebatchdb12e3b7", value: 0.55, authored: authored({ block: { kind: "decision" } }) }),
    entry({ todo: "bookflight", value: 0.61 }),
  ];
  const shown = listing(rows, DEFAULT_FILTER, "2026-09-19T17:00:00");
  assert.deepEqual(
    shown.priced.map((row) => row.todo),
    ["harrowreply", "bookflight"],
  );
  assert.equal(isDecision(rows[1]!), true);
  assert.equal(isDecision(rows[0]!), false);
});

// --- the fixture the browser scenes read --------------------------------------

/** The box's fold of the six-todo fixture chain (`web_vectors.rs`), at the instant it was folded. */
const fixture = JSON.parse(readFileSync("app/core/tests/web/fold.json", "utf8")) as { at: string; entries: Entry[] };

test("on the fixture, each filter narrows the open list to its own todos, most urgent first", () => {
  const { todos } = cast.world;
  const shown = (filter: typeof DEFAULT_FILTER): string[] =>
    fixture.entries.filter((e) => matches(e, filter, fixture.at)).sort(byUrgency).map((e) => e.todo);
  const open = shown(DEFAULT_FILTER);
  assert.equal(open[0], todos.urgent.id, "the deadline two days out sorts first: low means urgent");
  assert.ok(!open.includes(todos.resolved.id), "the resolved todo is not on the open list");
  for (const [filter, only] of [
    [{ ...DEFAULT_FILTER, attention: true }, [todos.agents.id]],
    [{ ...DEFAULT_FILTER, dated: true }, [todos.urgent.id]],
  ] as const) {
    assert.deepEqual(shown(filter), only, JSON.stringify(filter));
  }
});
