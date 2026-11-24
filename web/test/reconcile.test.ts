/**
 * The keyed list's plan: applying the ops produces exactly the new order, over
 * hand-picked and random shapes. Minimality is checked separately: a plan that
 * moved every node would be correct and still throw away the scroll position.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { reconcile } from "../src/algebra/reconcile";

/** `card.ts`'s DOM work, as array surgery: remove, then insert before a key. */
function apply(oldKeys: readonly string[], newKeys: readonly string[]): { after: string[]; moved: number } {
  const list = [...oldKeys];
  let moved = 0;
  for (const op of reconcile(oldKeys, newKeys)) {
    const at = list.indexOf(op.key);
    if (op.op === "remove") {
      assert.notEqual(at, -1, `remove of a key that is not there: ${op.key}`);
      list.splice(at, 1);
      continue;
    }
    if (op.op === "move") {
      assert.notEqual(at, -1, `move of a key that is not there: ${op.key}`);
      list.splice(at, 1);
      moved += 1;
    } else {
      assert.equal(at, -1, `insert of a key already there: ${op.key}`);
    }
    const anchor = op.before === null ? list.length : list.indexOf(op.before);
    assert.notEqual(anchor, -1, `before names a key that is not placed yet: ${String(op.before)}`);
    list.splice(anchor, 0, op.key);
  }
  return { after: list, moved };
}

function drives(oldKeys: readonly string[], newKeys: readonly string[]): number {
  const { after, moved } = apply(oldKeys, newKeys);
  assert.deepEqual(after, [...newKeys]);
  return moved;
}

test("examples: each plan lands on the new order, moving no more nodes than it must", () => {
  const cases: [string, readonly string[], readonly string[], number, ReturnType<typeof reconcile>?][] = [
    ["an unchanged list is no work at all", ["a", "b", "c"], ["a", "b", "c"], 0, []],
    ["empty to empty", [], [], 0],
    ["empty to two", [], ["a", "b"], 0],
    ["two to empty", ["a", "b"], [], 0],
    // The fold that this exists for: a card climbs from the bottom to the top.
    ["one todo becoming urgent moves ONE node, not the list", ["a", "b", "c", "d", "e"], ["e", "a", "b", "c", "d"], 1],
    ["a completed todo leaving the open list is one removal", ["a", "b", "c"], ["a", "c"], 0, [{ op: "remove", key: "b" }]],
    ["a fresh add joins without disturbing what is already ordered", ["a", "b"], ["a", "new", "b"], 0, [{ op: "insert", key: "new", before: "b" }]],
    ["appending names no anchor", ["a"], ["a", "b"], 0, [{ op: "insert", key: "b", before: null }]],
    ["a reversal is the worst case and costs n-1 moves, never n", ["a", "b", "c", "d"], ["d", "c", "b", "a"], 3],
    ["insert, remove and move together still land on the new order", ["a", "b", "c", "d"], ["d", "x", "b"], 1],
  ];
  for (const [name, before, after, moves, plan] of cases) {
    assert.equal(drives(before, after), moves, name);
    if (plan !== undefined) assert.deepEqual(reconcile(before, after), plan, name);
  }
});

test("random folds: applying the plan always yields the new order", () => {
  // Seeded, so a failing case is reproducible.
  let seed = 0x5eed;
  const next = (): number => {
    seed ^= seed << 13;
    seed ^= seed >>> 17;
    seed ^= seed << 5;
    return Math.abs(seed);
  };
  const pool = "abcdefghijklmnop".split("");
  for (let round = 0; round < 400; round += 1) {
    const pick = (): string[] => {
      const kept = pool.filter(() => next() % 3 !== 0);
      for (let i = kept.length - 1; i > 0; i -= 1) {
        const j = next() % (i + 1);
        [kept[i], kept[j]] = [kept[j] as string, kept[i] as string];
      }
      return kept;
    };
    const before = pick();
    const after = pick();
    assert.deepEqual(apply(before, after).after, after);
  }
});
