// A program's listeners (`web/src/pages/subscribed.ts`), with fake sources: the listeners alive are the slots patched
// in, a slot started again lets its earlier listener go, and a program let go keeps none.

import { test } from "node:test";
import assert from "node:assert/strict";

import drain from "../src/pages/drain";
import subscribed from "../src/pages/subscribed";
import type { SubPatch } from "../src/wire/generated/pages";
import { forAll } from "./gen";
import type { Rng } from "./gen";

const send = (): void => {};

// A source that records its start and its stop, and the listeners alive: one a source that was started and not stopped.
const fake = () => {
  const alive = new Set<string>();
  let starts = 0;
  const t = subscribed.of<number, number, never>((slot, source) => {
    const id = `${slot}:${source}:${starts++}`;
    alive.add(id);
    return () => void alive.delete(id);
  }, drain.DRAWN);
  return { t, alive: () => [...alive].sort(), starts: () => starts };
};

const events = (es: readonly drain.Event[]): drain.State => es.reduce(drain.step, drain.DRAWN);

const patch = (r: Rng): SubPatch<number, number> => {
  const slot = r.int(4);
  return r.bool() ? { type: "Start", slot, source: r.int(3) } : { type: "Stop", slot };
};

test("law: the listeners alive are the slots patched in, one each", () =>
  forAll(300, r => {
    const host = fake();
    const want = new Map<string, number>();
    for (const p of r.array(r.int(30), () => patch(r))) {
      host.t.apply(p, send);
      if (p.type === "Start") want.set(JSON.stringify(p.slot), p.source);
      else want.delete(JSON.stringify(p.slot));
      assert.deepEqual([...host.t.slots()].sort(), [...want.keys()].sort());
      assert.equal(host.alive().length, want.size, "each slot has one listener, and one a patch replaced or stopped is gone");
    }
    host.t.follow(events(["Left"]));
    assert.deepEqual(host.alive(), []);
    assert.deepEqual(host.t.slots(), []);
  }));

test("a Start on a held slot stops the earlier listener first", () => {
  const host = fake();
  host.t.apply({ type: "Start", slot: 1, source: 7 }, send);
  host.t.apply({ type: "Start", slot: 1, source: 8 }, send);
  assert.deepEqual(host.alive(), ["1:8:1"]);
  assert.equal(host.starts(), 2);
});

test("law: a Leaving program keeps no subscription, whatever is patched in after", () =>
  forAll(300, r => {
    const host = fake();
    host.t.apply({ type: "Start", slot: 0, source: 0 }, send);
    host.t.follow(events(["Wrote", "Left"]));
    assert.equal(host.alive().length, 0);
    for (const p of r.array(r.int(10), () => patch(r))) host.t.apply(p, send);
    assert.deepEqual(host.alive(), []);
    assert.deepEqual(host.t.slots(), []);
  }));

test("a program drawn or hidden keeps its subscriptions", () => {
  const host = fake();
  host.t.apply({ type: "Start", slot: 0, source: 0 }, send);
  host.t.follow(events(["Hid"]));
  host.t.follow(events(["Hid", "Showed"]));
  assert.equal(host.alive().length, 1);
});
