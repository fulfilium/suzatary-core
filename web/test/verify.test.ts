/**
 * The browser's verification, against objects in the store's own print: the two literals below are made up in the
 * exact bytes `events/objects/` holds, with the filenames they would be stored under, so code that hashes them to their
 * own names checks what the store's own load checks. The store is a DAG, so the walk starts from every head and follows
 * every parent; a chain, one head and one parent each, is its simplest case.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import type { Link } from "../src/wire/generated/wire";
import { found, sha256Hex, summarise, verifyChain } from "../src/prodrome/verify";

// 5f24da3f…py — the genesis (no parents).
const GENESIS_HASH = "5f24da3fa50a9dbe144a4aadce04f8833e49292f95f78f35ceef89486596a9cb";
const GENESIS_LITERAL =
  "Sealed(prev='', event=Created(todo='harrowsigneddeed', at=datetime(2025, 10, 2, 0, 0, 0), actor='backfill', text='', note='backfilled from daily/2025-10-02.typ@abb11c0b0e'))";

// 619b8e52…py — the object whose parent is the genesis.
const SECOND_HASH = "619b8e5207059a60707e9b36bd502356d25fe5c985acae6686a8e61b22fd39d5";
const SECOND_LITERAL =
  "Sealed(prev='5f24da3fa50a9dbe144a4aadce04f8833e49292f95f78f35ceef89486596a9cb', event=Created(todo='x7k2q9wf', at=datetime(2026, 6, 20, 0, 0, 0), actor='backfill', text='', note='backfilled from daily/2026-06-20.typ@00c07fbeda'))";

const genesis: Link = {
  hash: GENESIS_HASH,
  parents: [],
  holds: "event",
  kind: "Created",
  todo: "harrowsigneddeed",
  actor: "backfill",
  at: "2025-10-02T00:00:00",
  note: "",
  reverses: { kind: "nothing" },
  literal: GENESIS_LITERAL,
};

const second: Link = {
  hash: SECOND_HASH,
  parents: [GENESIS_HASH],
  holds: "event",
  kind: "Created",
  todo: "x7k2q9wf",
  actor: "backfill",
  at: "2026-06-20T00:00:00",
  note: "",
  reverses: { kind: "nothing" },
  literal: SECOND_LITERAL,
};

test("a literal hashes to the name the store filed it under", async () => {
  assert.equal(await sha256Hex(GENESIS_LITERAL), GENESIS_HASH);
  assert.equal(await sha256Hex(SECOND_LITERAL), SECOND_HASH);
});

test("sha256 is over UTF-8 bytes with no trailing newline", async () => {
  // seal_hash is sha256(print_literal(obj).encode()); the store writes exactly
  // those bytes. A stray newline would be a different hash entirely.
  assert.notEqual(await sha256Hex(GENESIS_LITERAL + "\n"), GENESIS_HASH);
  assert.equal(await sha256Hex(""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
});

test("an intact two-object chain verifies clean", async () => {
  const verdict = await verifyChain([genesis, second], [SECOND_HASH]);
  assert.deepEqual(found.problems(verdict.found), []);
  assert.equal(verdict.found.type, "Reached");
  assert.equal(verdict.walked, 2);
  assert.equal(verdict.verified, 2);
  assert.deepEqual(verdict.genesis, [GENESIS_HASH]);
  // Walk order is heads first.
  assert.deepEqual(
    verdict.objects.map((object) => object.hash),
    [SECOND_HASH, GENESIS_HASH],
  );
  assert.match(summarise(verdict), /2 objects verified by this browser/);
});

test("a tampered literal is reported on ITS object, not on the chain", async () => {
  const tampered: Link = { ...second, literal: SECOND_LITERAL.replace("x7k2q9wf", "x7k2q9wg") };
  const verdict = await verifyChain([genesis, tampered], [SECOND_HASH]);
  assert.equal(verdict.found.type, "Unreached");
  const bad = verdict.objects.find((object) => object.hash === SECOND_HASH);
  const good = verdict.objects.find((object) => object.hash === GENESIS_HASH);
  assert.ok(bad !== undefined && good !== undefined);
  assert.equal(bad?.hashOk, false);
  assert.equal(good?.hashOk, true);
  assert.equal(bad?.condition.type, "Flawed");
  assert.deepEqual(good?.condition, { type: "Clean" });
  assert.equal(verdict.verified, 1);
  // The walk still reaches genesis: a bad object is a bad object, not a break.
  assert.deepEqual(verdict.genesis, [GENESIS_HASH]);
  assert.equal(found.problems(verdict.found).length, 1);
  assert.match(found.problems(verdict.found)[0] ?? "", new RegExp(`^object ${SECOND_HASH} does not hash`));
});

test("a parent that names nothing breaks the walk at the object that names it", async () => {
  const orphaned: Link = { ...second, parents: ["0".repeat(64)] };
  const verdict = await verifyChain([genesis, orphaned], [SECOND_HASH]);
  assert.equal(verdict.found.type, "Unreached");
  // Every object still hashes to its own name — the break is structural.
  assert.equal(verdict.verified, 2);
  assert.equal(verdict.walked, 1);
  assert.deepEqual(verdict.genesis, []);
  assert.match(found.problems(verdict.found).join(" | "), /is named as a parent but was not sent/);
  const unreached = verdict.objects.find((object) => object.hash === GENESIS_HASH);
  assert.deepEqual(unreached?.walk, { type: "Unreached" });
  assert.match(unreached?.condition.type === "Flawed" ? unreached.condition.problem : "", /not reachable from any head/);
});

test("an object the walk never reaches is reported as unreachable", async () => {
  const stray: Link = { ...genesis, hash: SECOND_HASH };
  const verdict = await verifyChain([genesis, stray], [GENESIS_HASH]);
  assert.equal(verdict.found.type, "Unreached");
  assert.equal(verdict.walked, 1);
  assert.match(found.problems(verdict.found).join(" | "), /is not reachable from any head/);
});

test("a cycle in the parents is reported rather than looped over", async () => {
  const a: Link = { ...genesis, hash: "a".repeat(64), parents: ["b".repeat(64)] };
  const b: Link = { ...genesis, hash: "b".repeat(64), parents: ["a".repeat(64)] };
  const verdict = await verifyChain([a, b], ["a".repeat(64)]);
  assert.equal(verdict.found.type, "Unreached");
  assert.match(found.problems(verdict.found).join(" | "), /parents form a cycle/);
});

test("a tip the server never sent is a problem, not a crash", async () => {
  const verdict = await verifyChain([genesis], ["f".repeat(64)]);
  assert.equal(verdict.found.type, "Unreached");
  assert.equal(verdict.walked, 0);
  assert.match(found.problems(verdict.found).join(" | "), /was not sent/);
});

// Two branches over one genesis and the merge that joins them. The hashes are made up: what is checked here is the
// shape of the walk, and the hashing is pinned above on the store's own bytes.

const dagLink = (hash: string, parents: string[]): Link => ({
  hash,
  parents,
  literal: parents.length === 0 ? `Sealed(prev='', made-up ${hash})` : `made-up ${hash}`,
  ...(parents.length > 1
    ? { holds: "envelope", kind: "Woven" }
    : { holds: "event", kind: "Created", todo: "t", actor: "ada", at: "2026-09-01T00:00:00", note: "", reverses: { kind: "nothing" } }),
});

const ROOT = "1".repeat(64);
const MINE = "2".repeat(64);
const YOURS = "3".repeat(64);
const MERGE = "4".repeat(64);

const dag = [
  dagLink(ROOT, []),
  dagLink(MINE, [ROOT]),
  dagLink(YOURS, [ROOT]),
  dagLink(MERGE, [MINE, YOURS]),
];

/** Every object's `literal` here is invented, so the hash check is the wrong
 * question; these tests ask about reachability and report on the walk. */
const structural = (problems: readonly string[]): string[] => problems.filter((p) => !/does not hash/.test(p));

test("the walk crosses a merge and reaches genesis through both branches", async () => {
  const verdict = await verifyChain(dag, [MERGE]);
  assert.deepEqual(structural(found.problems(verdict.found)), []);
  assert.equal(verdict.walked, 4);
  assert.deepEqual(verdict.genesis, [ROOT]);
  // Breadth first from the head: the merge, then both branches, then the root.
  assert.deepEqual(
    verdict.objects.map((object) => object.walk),
    [0, 1, 1, 2].map((depth) => ({ type: "Reached", depth })),
  );
  assert.deepEqual(verdict.objects.map((object) => object.hash), [MERGE, MINE, YOURS, ROOT]);
});

test("two heads are both walked, and neither is an orphan", async () => {
  const verdict = await verifyChain([dagLink(ROOT, []), dagLink(MINE, [ROOT]), dagLink(YOURS, [ROOT])], [MINE, YOURS]);
  assert.deepEqual(structural(found.problems(verdict.found)), []);
  assert.equal(verdict.walked, 3);
  assert.deepEqual(verdict.tips, [MINE, YOURS]);
  assert.deepEqual(verdict.genesis, [ROOT]);
});

test("one head walked while another is not sent leaves the branch unreachable", async () => {
  const verdict = await verifyChain([dagLink(ROOT, []), dagLink(MINE, [ROOT])], [MINE, YOURS]);
  assert.equal(verdict.found.type, "Unreached");
  assert.match(structural(found.problems(verdict.found)).join(" | "), new RegExp(`the tip ${YOURS} was not sent`));
});

test("two genesis objects are both reported — two replicas, two beginnings", async () => {
  const OTHER = "5".repeat(64);
  const verdict = await verifyChain(
    [dagLink(ROOT, []), dagLink(OTHER, []), dagLink(MERGE, [ROOT, OTHER])],
    [MERGE],
  );
  assert.deepEqual(structural(found.problems(verdict.found)), []);
  assert.equal(verdict.genesis.length, 2);
});

test("a change that supersedes nothing names no parents and is no genesis", async () => {
  const CHANGE = "6".repeat(64);
  const change: Link = { ...dagLink(CHANGE, []), literal: `Change(genesis='${ROOT}', deps=(), made-up)` };
  const verdict = await verifyChain([dagLink(ROOT, []), change, dagLink(MERGE, [ROOT, CHANGE])], [MERGE]);
  assert.deepEqual(structural(found.problems(verdict.found)), []);
  assert.deepEqual(verdict.genesis, [ROOT]);
});
