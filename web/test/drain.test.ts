/**
 * A PAGE LEFT WITH A WRITE IN FLIGHT (`web/src/pages/drain.ts`): the write still lands. The box's print answers after
 * the page is gone, the seal it asks for is still performed, its answer still said, and only then is the program
 * freed. Nothing else a gone page asks is performed.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import drain from "../src/pages/drain";

const run = (events: readonly drain.Event[]): drain.State => events.reduce(drain.step, drain.LIVE);

test("a two-phase write whose page is left between the phases is sealed, said, then freed", () => {
  // Printed (Post) asked; the page left; the print answers and the program asks the seal (Act); the seal answers.
  const left = run(["Wrote", "Left"]);
  assert.equal(left.type, "Leaving");
  for (const k of ["Act", "Post", "Toast", "Offer"] as const) assert.equal(drain.performs(left, k), true, k);
  for (const k of ["Go", "Replace", "Focus", "Scroll", "Keys", "Hold", "Watch", "Timer", "Get"] as const) assert.equal(drain.performs(left, k), false, k);
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
  assert.equal(drain.performs(run(["Left"]), "Toast"), false);
  assert.equal(drain.performs(drain.LIVE, "Go"), true);
});
