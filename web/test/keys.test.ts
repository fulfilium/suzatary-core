/**
 * What a key means, and — more importantly — when it means nothing.
 *
 * The failure this suite exists to prevent is a letter eaten: someone typing a
 * todo body that contains "c" and watching a different todo get completed. So
 * the first properties are the negative ones — while a field has the keyboard,
 * only Escape is claimed, and a modified key is always the browser's.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { SHORTCUTS, keyAction } from "../src/shell/keys";
import type { KeyAction, KeyContext } from "../src/shell/keys";

const IDLE = { typing: false, pending: "", modified: false } as const;

test("typing eats everything but Escape", () => {
  for (const key of ["j", "k", "c", "x", "n", "/", "?", "g", "a", "Enter"]) {
    assert.equal(keyAction(key, { ...IDLE, typing: true }), null, key);
  }
  assert.deepEqual(keyAction("Escape", { ...IDLE, typing: true }), { type: "escape" });
});

test("a modified key belongs to the browser, except the palette's one chord", () => {
  for (const key of ["c", "n", "r", "/", "a", "Escape"]) {
    assert.equal(keyAction(key, { ...IDLE, modified: true }), null, key);
  }
  assert.deepEqual(keyAction("k", { ...IDLE, modified: true }), { type: "palette" });
  // Including out of a field: the palette is how you leave the one you are in.
  assert.deepEqual(keyAction("K", { typing: true, pending: "", modified: true }), { type: "palette" });
});

test("each key means its one action in the context it lands in, and an unbound key is nobody's", () => {
  const secretary: KeyAction = { type: "go", route: { type: "secretary", refs: [], topics: [], focus: true } };
  const armed = { ...IDLE, pending: "g" };
  const cases: [string, KeyContext, KeyAction | null][] = [
    // `a` asks the secretary with nothing on screen to point at, and does nothing on the conversation page, whose
    // compose box it would focus is already there.
    ["a", { ...IDLE, onConversation: true }, null],
    ["a", IDLE, secretary],
    ["a", { ...IDLE, onConversation: false }, secretary],
    ["j", IDLE, { type: "move", by: 1 }],
    ["k", IDLE, { type: "move", by: -1 }],
    ["Enter", IDLE, { type: "open" }],
    ["l", IDLE, { type: "open" }],
    ["h", IDLE, { type: "close" }],
    // c completes and x cancels, in the wire's own vocabulary.
    ["c", IDLE, { type: "act", name: "done" }],
    ["x", IDLE, { type: "act", name: "cancel" }],
    // Undo and redo are a separate kind from `act`; Shift alone is no chord `modified` reserves.
    ["u", IDLE, { type: "undo" }],
    ["U", IDLE, { type: "redo" }],
    ["/", IDLE, { type: "focusSearch" }],
    ["n", IDLE, { type: "add" }],
    ["?", IDLE, { type: "help" }],
    ["Escape", IDLE, { type: "escape" }],
    ["g", IDLE, { type: "arm" }],
    ["t", armed, { type: "go", route: { type: "todos" } }],
    ["i", armed, { type: "go", route: { type: "inbox" } }],
    ["p", armed, { type: "go", route: { type: "project" } }],
    ["m", armed, { type: "go", route: { type: "mail", query: "", account: "" } }],
    ["s", armed, { type: "go", route: { type: "secretary", refs: [], topics: [] } }],
    ["b", armed, { type: "go", route: { type: "briefings" } }],
    ...["q", "F5", "Tab", "ArrowDown", " "].map((key): [string, KeyContext, null] => [key, IDLE, null]),
  ];
  for (const [key, context, action] of cases) assert.deepEqual(keyAction(key, context), action, `${key} in ${JSON.stringify(context)}`);
});

test("an armed prefix swallows the key rather than firing the unprefixed one", () => {
  // `g c` must not complete a todo. It is nothing, and the prefix is spent.
  assert.equal(keyAction("c", { ...IDLE, pending: "g" }), null);
});

test("every key the shortcuts sheet advertises is a key that does something", () => {
  // The sheet is prose beside a table; this is what keeps the two honest.
  const advertised = SHORTCUTS.flatMap((row) => row.keys.split(" / "));
  const chords = advertised.filter((keys) => keys.includes("⌘") || keys.includes("Ctrl"));
  assert.equal(chords.length, 2);
  for (const keys of chords) {
    assert.deepEqual(keyAction(keys.slice(-1).toLowerCase(), { ...IDLE, modified: true }), { type: "palette" });
  }
  const unprefixed = advertised.filter(
    (keys) => !keys.startsWith("g ") && !keys.includes("⌘") && !keys.includes("Ctrl"),
  );
  for (const keys of unprefixed) {
    const key = keys === "Enter" || keys === "Escape" ? keys : keys.trim();
    assert.notEqual(keyAction(key, IDLE), null, `${key} is advertised and does nothing`);
  }
  for (const keys of advertised.filter((each) => each.startsWith("g "))) {
    const key = keys.slice(2);
    assert.notEqual(keyAction(key, { ...IDLE, pending: "g" }), null, `g ${key} is advertised and does nothing`);
  }
});
