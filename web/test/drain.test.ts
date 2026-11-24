/**
 * A PAGE UNDRAWN (`web/src/pages/drain.ts`). Hidden, it keeps running: it hears its readings and writes, what it asks
 * of the screen waits until it is drawn again, and a move of the address is dropped. Left with a write in flight, the
 * write still lands: the box's print answers after the page is gone, the seal it asks for is still performed, its
 * answer still said, and only then is the program freed. Nothing else a gone page asks is performed.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import drain from "../src/pages/drain";

const run = (events: readonly drain.Event[]): drain.State => events.reduce(drain.step, drain.DRAWN);

test("a two-phase write whose page is left between the phases is sealed, said, then freed", () => {
  // Printed (Post) asked; the page left; the print answers and the program asks the seal (Act); the seal answers.
  const left = run(["Wrote", "Left"]);
  assert.equal(left.type, "Leaving");
  for (const k of ["Act", "Post", "Toast", "Offer"] as const) assert.equal(drain.verdict(left, k), "Performs", k);
  for (const k of ["Go", "Replace", "Focus", "Scroll", "Keys", "Hold", "Watch", "Timer", "Get"] as const) assert.equal(drain.verdict(left, k), "Drops", k);
  // The program steps on the print's answer before it is counted answered, so the seal it asks is owed first.
  const sealing = run(["Wrote", "Left", "Wrote", "Answered"]);
  assert.equal(sealing.type, "Leaving");
  assert.equal(run(["Wrote", "Left", "Wrote", "Answered", "Answered"]).type, "Gone");
});

test("an Undo offered by a gone page keeps it until the offer expires, and an Undo taken is a write of its own", () => {
  assert.equal(run(["Wrote", "Left", "Offered", "Answered"]).type, "Leaving");
  assert.equal(run(["Wrote", "Left", "Offered", "Answered", "Expired"]).type, "Gone");
  assert.equal(run(["Wrote", "Left", "Offered", "Answered", "Wrote", "Expired"]).type, "Leaving");
});

test("a page left with nothing owed is gone at once, and a gone page performs nothing", () => {
  assert.equal(run(["Left"]).type, "Gone");
  assert.equal(run(["Wrote", "Answered", "Left"]).type, "Gone");
  assert.equal(drain.verdict(run(["Left"]), "Toast"), "Drops");
  assert.equal(drain.verdict(drain.DRAWN, "Go"), "Performs");
});

test("a hidden page reads and writes, its focus, scroll and keys wait for it to be drawn, and its moves of the address drop", () => {
  const hidden = run(["Hid"]);
  assert.equal(hidden.type, "Hidden");
  for (const k of ["Hold", "Watch", "Core", "Get", "Act", "Post", "Toast", "Offer", "Timer"] as const) assert.equal(drain.verdict(hidden, k), "Performs", k);
  for (const k of ["Focus", "Scroll", "Keys"] as const) assert.equal(drain.verdict(hidden, k), "Waits", k);
  for (const k of ["Go", "Replace", "Leave", "Back"] as const) assert.equal(drain.verdict(hidden, k), "Drops", k);
  // A write made while hidden is still owed when it is drawn again, and when it is let go.
  assert.deepEqual(run(["Hid", "Wrote", "Showed"]), { type: "Drawn", owed: { writes: 1, offers: 0 } });
  assert.equal(run(["Hid", "Wrote", "Left"]).type, "Leaving");
  assert.equal(run(["Hid", "Left"]).type, "Gone");
  assert.equal(run(["Wrote", "Left", "Showed"]).type, "Leaving", "a page let go is never drawn again");
});

test("what waits keeps one scroll, one focus and the keys of each ask: the last of each", () => {
  const scroll = { type: "Scroll", to: { type: "End" } } as const
  const focus = { type: "Focus", field: "compose" } as const
  const keys = (ask: string) => ({ type: "Keys" as const, ask, keys: [] })
  assert.equal(drain.slot(scroll), drain.slot({ ...scroll, to: { type: "Top", field: "x" } }))
  assert.equal(drain.slot(focus), drain.slot({ ...focus, field: "rename" }))
  assert.notEqual(drain.slot(keys("a")), drain.slot(keys("b")))
  assert.equal(drain.slot(keys("a")), drain.slot(keys("a")))
  assert.notEqual(drain.slot(scroll), drain.slot(focus))
});
