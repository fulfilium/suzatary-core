/**
 * THE LAWS, AS PROPERTIES — each `test` here is a law, its name the law's
 * statement. Random inputs from `gen.ts`, many seeds, the failing seed in the
 * message.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { hrefOf, parseRoute } from "../src/shell/route";
import { isWide, screenFor } from "../src/shell/layout";
import { reconcile } from "../src/algebra/reconcile";
import { forAll, keys, reshuffled, route, width } from "./gen";

// --- The address ---------------------------------------------------------------

test("law: a route survives print and parse (hrefOf then parseRoute)", () => {
  forAll(400, (r) => {
    const original = route(r);
    const back = parseRoute(hrefOf(original));
    assert.deepEqual(back, original, `${hrefOf(original)}`);
  });
});

test("law: parsing is total — any hash yields a route, and an unknown one names itself", () => {
  forAll(300, (r) => {
    const hash = `#/${r.string("abcdefghijklmnopqrstuvwxyz/?=&%-", r.int(24))}`;
    const parsed = parseRoute(hash);
    assert.ok(typeof parsed.type === "string");
    if (parsed.type === "unknown") assert.equal(parsed.hash, hash);
  });
});

// --- The screen -----------------------------------------------------------------

test("law: screenFor is total, and a todo is a pane at 62rem and a page below it", () => {
  forAll(400, (r) => {
    const at = route(r);
    const px = width(r);
    const screen = screenFor(at, isWide(px));
    if (at.type === "todo") {
      if (isWide(px)) {
        assert.equal(screen.type, "todos", `${hrefOf(at)} at ${px}px`);
        if (screen.type === "todos") assert.equal(screen.selected, at.id);
      } else {
        assert.equal(screen.type, "todo", `${hrefOf(at)} at ${px}px`);
      }
    }
    if (screen.type === "todos" && !isWide(px)) {
      assert.equal(screen.selected, "", `${hrefOf(at)} at ${px}px: a narrow list selects nothing`);
    }
  });
});

test("law: the same route at the same width always resolves to the same screen", () => {
  forAll(200, (r) => {
    const at = route(r);
    const px = width(r);
    assert.deepEqual(screenFor(at, isWide(px)), screenFor(at, isWide(px)));
  });
});

// --- The reconciler ------------------------------------------------------------

/** Apply a plan to a key list the way `dom` would: removals, then inserts and
 * moves before their anchor. */
function apply(oldKeys: readonly string[], plan: readonly ReturnType<typeof reconcile>[number][]): string[] {
  const out = [...oldKeys];
  for (const op of plan) {
    if (op.op === "remove") {
      out.splice(out.indexOf(op.key), 1);
      continue;
    }
    if (op.op === "move") out.splice(out.indexOf(op.key), 1);
    const at = op.before === null ? out.length : out.indexOf(op.before);
    out.splice(at, 0, op.key);
  }
  return out;
}

test("law: reconciling a list with itself is no work at all", () => {
  forAll(300, (r) => {
    const same = keys(r);
    assert.deepEqual(reconcile(same, same), []);
  });
});

test("law: applying the plan yields exactly the new order, from any old order", () => {
  forAll(500, (r) => {
    const before = keys(r);
    const after = reshuffled(r, before);
    assert.deepEqual(apply(before, reconcile(before, after)), after, `${before} -> ${after}`);
  });
});

test("law: a plan never names a key twice, and never moves what it inserts", () => {
  forAll(300, (r) => {
    const before = keys(r);
    const after = reshuffled(r, before);
    const plan = reconcile(before, after);
    const named = plan.map((op) => `${op.op === "remove" ? "-" : "+"}${op.key}`);
    assert.equal(new Set(named).size, named.length, `${plan.map((op) => op.op + ":" + op.key)}`);
    for (const op of plan) {
      if (op.op === "insert") assert.equal(before.includes(op.key), false);
      if (op.op === "move" || op.op === "remove") assert.equal(before.includes(op.key), true);
    }
  });
});
