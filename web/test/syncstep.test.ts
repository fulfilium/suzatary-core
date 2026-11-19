/**
 * `stepSync`: the replica, which chain it belongs to, the exchange's own phase — idle,
 * pushing, pulling, refused with what — and the fold fetch in flight
 * (`fetching`), as a closed
 * `SyncState` and a pure fold over `SyncEvent`. One example per arm and a
 * random-walk property, the same split `storestep.test.ts` has.
 *
 * `sync.ts` touches IndexedDB and the network only inside its exported
 * functions, never inside `stepSync`, so this file drives the fold with no
 * DOM and no fetch — exactly what let `replica.test.ts` test `replica.ts`
 * the same way.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { NOTHING, reset } from "../src/prodrome/replica";
import type { Replica } from "../src/prodrome/replica";
import { type Over, replicaWith } from "./held";
import { INITIAL_SYNC, type SyncEvent, type SyncState, stepSync } from "../src/prodrome/sync";
import { forAll, SLUG, type Rng } from "./gen";

const name = (seed: string): string => seed.repeat(64).slice(0, 64);

function replica(over: Over = {}): Replica {
  return replicaWith({ objects: [{ hash: name("a"), literal: "Sealed(prev='', event=None)" }], ...over });
}

// --- one example per arm -----------------------------------------------------

test("chainSwitched: drops the replica, keeps the phase", () => {
  const state: SyncState = {
    ...INITIAL_SYNC,
    replica: replica(),
    holding: "life",
    phase: { kind: "pushing", on: "life" },
  };
  const next = stepSync(state, { kind: "chainSwitched", on: "suzatary" });
  assert.equal(next.replica, NOTHING);
  assert.equal(next.holding, "suzatary");
  assert.deepEqual(next.phase, { kind: "pushing", on: "life" }, "the exchange in flight is still about the chain it started on");
});

test("committed: joins the row into what is held, so memory only climbs — the `holding` guard is the wrapper's, not `step`'s", () => {
  const found = replica({ boxHeads: ["t1"] });
  const next = stepSync(INITIAL_SYNC, { kind: "committed", replica: found });
  assert.equal(next.replica, found);
  assert.equal(stepSync(next, { kind: "committed", replica: NOTHING }), next, "a row holding less moves nothing, and wakes nobody");
  const b = { hash: name("b"), literal: `Sealed(prev='${name("a")}', event=None)` };
  const more = stepSync(next, { kind: "committed", replica: replica({ objects: [b] }) });
  assert.deepEqual(more.replica.objects.map((o) => o.hash).sort(), [name("a"), name("b")], "two rows are their union");
  assert.deepEqual(stepSync(more, { kind: "committed", replica: reset(more.replica) }).replica.objects, [], "a reset still wins, by its generation");
});

test("pushing / pulling: the phase and its chain, and repeating one is not news", () => {
  const pushing = stepSync(INITIAL_SYNC, { kind: "pushing", on: "life" });
  assert.deepEqual(pushing.phase, { kind: "pushing", on: "life" });
  assert.equal(stepSync(pushing, { kind: "pushing", on: "life" }), pushing);
  assert.deepEqual(
    stepSync(pushing, { kind: "pushing", on: "suzatary" }).phase,
    { kind: "pushing", on: "suzatary" },
    "another chain's exchange is news",
  );
  const pulling = stepSync(pushing, { kind: "pulling", on: "life" });
  assert.deepEqual(pulling.phase, { kind: "pulling", on: "life" });
});

test("refused: carries the box's own sentence", () => {
  const next = stepSync(INITIAL_SYNC, { kind: "refused", reason: "too large" });
  assert.deepEqual(next.phase, { kind: "refused", reason: "too large" });
});

test("exchangeSettled: back to idle, and settling twice is not news", () => {
  const pushing = stepSync(INITIAL_SYNC, { kind: "pushing", on: "life" });
  const idle = stepSync(pushing, { kind: "exchangeSettled" });
  assert.deepEqual(idle.phase, { kind: "idle" });
  assert.equal(stepSync(idle, { kind: "exchangeSettled" }), idle);
});

test("fetchStarted / fetchSettled: the chain in flight, news only when it moves", () => {
  const started = stepSync(INITIAL_SYNC, { kind: "fetchStarted", on: "life" });
  assert.equal(started.fetching, "life");
  assert.equal(stepSync(started, { kind: "fetchStarted", on: "life" }), started);
  assert.equal(stepSync(started, { kind: "fetchStarted", on: "suzatary" }).fetching, "suzatary");
  const settled = stepSync(started, { kind: "fetchSettled" });
  assert.equal(settled.fetching, null);
  assert.equal(stepSync(INITIAL_SYNC, { kind: "fetchSettled" }), INITIAL_SYNC);
});

// --- property: a random walk of events ---------------------------------------

function randomEvent(r: Rng): SyncEvent {
  switch (r.int(7)) {
    case 0:
      return { kind: "chainSwitched", on: r.pick(["life", "suzatary"]) };
    case 1:
      return { kind: "committed", replica: replica({ boxHeads: [r.string(SLUG, 6)], fetchedAt: 1 + r.int(5) }) };
    case 2:
      return { kind: "pushing", on: r.pick(["life", "suzatary"]) };
    case 3:
      return { kind: "pulling", on: r.pick(["life", "suzatary"]) };
    case 4:
      return { kind: "refused", reason: r.string(SLUG, 8) };
    case 5:
      return { kind: "fetchStarted", on: r.pick(["life", "suzatary"]) };
    default:
      return { kind: "fetchSettled" };
  }
}

test("property: a random walk of events is total", () => {
  forAll(300, (r) => {
    let state: SyncState = INITIAL_SYNC;
    for (let i = 0; i < 40; i += 1) state = stepSync(state, randomEvent(r));
  });
});

test("property: a random walk agrees with `events.reduce(stepSync, INITIAL_SYNC)`", () => {
  forAll(200, (r) => {
    const events = r.array(r.int(20), () => randomEvent(r));
    const folded = events.reduce(stepSync, INITIAL_SYNC);
    let state: SyncState = INITIAL_SYNC;
    for (const event of events) state = stepSync(state, event);
    assert.deepEqual(state, folded);
  });
});
