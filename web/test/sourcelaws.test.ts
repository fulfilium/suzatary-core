/**
 * THE SOURCE'S LAWS: a source is exactly the fold
 * of its events, listeners see every committed state once and in order, and
 * an event that changes nothing commits nothing.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { forAll, type Rng } from "./gen";
import sources from "../src/algebra/source";
import lo from "../src/algebra/lo";

/** A tiny page: a counter with a label, and a load that can fail. */
type Value = { readonly count: number; readonly label: string };
type Event =
  | { readonly kind: "arrived"; readonly label: string }
  | { readonly kind: "failed"; readonly reason: string }
  | { readonly kind: "tick" }
  | { readonly kind: "same" };

function step(state: lo.T<Value>, event: Event): lo.T<Value> {
  switch (event.kind) {
    case "arrived":
      return lo.val({ count: 0, label: event.label });
    case "failed":
      return lo.error(event.reason);
    case "tick":
      return state.type === "Val"
        ? lo.val({ ...state.v, count: state.v.count + 1 })
        : state;
    case "same":
      return state;
  }
}

function event(r: Rng): Event {
  switch (r.int(4)) {
    case 0:
      return { kind: "arrived", label: r.string("abc", 3) };
    case 1:
      return { kind: "failed", reason: r.string("xyz", 4) };
    case 2:
      return { kind: "tick" };
    default:
      return { kind: "same" };
  }
}

test("law: a source is the fold of its events", () => {
  forAll(300, (r) => {
    const events = r.array(r.int(30), () => event(r));
    const source = sources.create<lo.T<Value>, Event>(lo.loading, step);
    for (const e of events) source.dispatch(e);
    assert.deepEqual(source.state(), events.reduce(step, lo.loading as lo.T<Value>));
  });
});

test("law: a listener sees every committed state once, in order, and none after unsubscribing", () => {
  forAll(200, (r) => {
    const events = r.array(r.int(30), () => event(r));
    const source = sources.create<lo.T<Value>, Event>(lo.loading, step);
    const seen: lo.T<Value>[] = [];
    const stop = source.subscribe((s) => seen.push(s));
    const expected: lo.T<Value>[] = [];
    let state: lo.T<Value> = lo.loading;
    for (const e of events) {
      const next = step(state, e);
      if (!Object.is(next, state)) expected.push(next);
      state = next;
      source.dispatch(e);
    }
    assert.deepEqual(seen, expected);
    stop();
    source.dispatch({ kind: "failed", reason: "after" });
    assert.equal(seen.length, expected.length);
  });
});

test("law: an event that changes nothing wakes nobody", () => {
  const source = sources.create<lo.T<Value>, Event>(lo.loading, step);
  let woken = 0;
  source.subscribe(() => {
    woken += 1;
  });
  source.dispatch({ kind: "same" });
  source.dispatch({ kind: "tick" }); // a tick before load is the same state
  assert.equal(woken, 0);
  source.dispatch({ kind: "arrived", label: "a" });
  assert.equal(woken, 1);
  assert.deepEqual(lo.withDefault(source.state(), null), { count: 0, label: "a" });
});
