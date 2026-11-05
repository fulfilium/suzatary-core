/**
 * THE THREE VECTORS THE RUST WRITES, AND WHAT THE PURE MODULES SAY BACK
 * (`docs/WEBALGEBRA-PLAN-2026-09-16.md` §5 commit 6; `docs/WEB-SPEC.md` §4) —
 * the `about-fixture.json` pattern (`app/core/src/secretary/about.rs`,
 * `rounds.test.ts`) generalised from one payload to three.
 *
 * `app/core/tests/web_vectors.rs` builds a `FoldPayload`, a `ChainPayload` and
 * a `ConversationPayload` over the six-todo fixture chain at a fixed instant,
 * and beside each one writes the ANSWER the Rust gives to the question this
 * file asks — never regenerated here, only READ: a disagreement with a
 * vector is a finding, not staleness (`docs/WEBALGEBRA-PLAN-2026-09-16.md`
 * §1 item 5). Every payload is passed through its runtime guard first
 * (`isFoldPayload` etc.) exactly as a page would, and then folded with the
 * PURE module a page actually uses — `todos.ts` for the fold, `verify.ts`
 * (the fallback verifier; `wasm.test.ts` covers the wasm one) for the chain,
 * `rounds.ts` for the conversation.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import d from "../src/wire/decoder";
import wire from "../src/wire/generated/payload";
import { DEFAULT_FILTER, listing, total } from "../src/todo/todos";
import { found, verifyChain } from "../src/prodrome/verify";
import { rounds } from "../src/chat/rounds";

function vector<T>(file: string, decoder: d.Decoder<T>): T {
  const decoded = decoder(JSON.parse(readFileSync(`app/core/tests/web/${file}`, "utf8")));
  if (decoded.type !== "Val") throw new Error(`${file} does not decode: ${decoded.type === "Error" ? decoded.e : ""}`);
  return decoded.v;
}

function answer<T>(file: string): T {
  return JSON.parse(readFileSync(`app/core/tests/web/${file}`, "utf8")) as T;
}

// --- the fold: the open list's own order -------------------------------------

interface FoldAnswer {
  readonly open_ascending: readonly string[];
  readonly count: number;
}

test("the fold: the open list's ascending order and count match the Rust's", () => {
  const payload = vector("fold.json", wire.FoldPayload);
  const wants = answer<FoldAnswer>("fold.answer.json");

  const list = listing(payload.entries, DEFAULT_FILTER, payload.at);
  assert.deepEqual(
    list.priced.map((entry) => entry.todo),
    wants.open_ascending,
    "todos.ts's byUrgency order",
  );
  assert.equal(total(list), wants.count, "every open todo in the fixture is priced, so nothing is unpriced");
});

// --- the chain: verify's verdict, and what a walk can and cannot say ---------

interface ChainAnswer {
  readonly verify: readonly string[];
  readonly order: readonly string[];
}

test("the chain: the fallback verifier's verdict matches the Rust's own verify", async () => {
  const payload = vector("chain.json", wire.ChainPayload);
  const wants = answer<ChainAnswer>("chain.answer.json");

  const verdict = await verifyChain(payload.links, payload.heads);
  assert.deepEqual(found.problems(verdict.found), wants.verify, "the same objects, the same fsck findings");
  assert.equal(verdict.found.type, wants.verify.length === 0 ? "Reached" : "Unreached");

  // THE WALK REACHES EVERYTHING THE LINEARISATION ORDERS — checkable: both
  // are a full covering walk of the same DAG, so the SET of hashes agrees
  // even though the order does not.
  const walked = new Set(verdict.objects.map((object) => object.hash));
  assert.equal(walked.size, wants.order.length, "the walk visits every object the chain holds");
  for (const hash of wants.order) assert.ok(walked.has(hash), `${hash} was walked`);

  // SKIPPED, BY NAME: the exact SEQUENCE of `wants.order` (parents before
  // children, `wire::ChainPayload`'s own promise) against `verdict.objects`'
  // own order. `verifyChain` walks heads-first, for verification, which is a
  // DIFFERENT valid topological order over a DAG with six independent
  // components (one per todo) — there is no pure TS module that recomputes
  // the STORE's own linearisation, only the wire's promise that `links`
  // already carries it. Inventing an order-sensitive comparison here would
  // be asserting something no browser code actually computes.
});

// --- the conversation: round boundaries, and whose proposal is whose --------

interface ConversationAnswer {
  readonly rounds: ReadonlyArray<readonly [number, number]>;
  readonly proposal_rounds: readonly number[];
}

test("the conversation: round spans and proposal placement match the Rust's about::round_spans", () => {
  const payload = vector("conversation.json", wire.ConversationPayload);
  const wants = answer<ConversationAnswer>("conversation.answer.json");

  const grouped = rounds(payload.turns, payload.proposals);
  assert.deepEqual(
    grouped.map((round) => [round.startIndex, round.endIndex]),
    wants.rounds,
  );

  const proposalRounds = payload.proposals.map((proposal) =>
    grouped.findIndex((round) => round.proposals.some((p) => p.index === proposal.index)),
  );
  assert.deepEqual(proposalRounds, wants.proposal_rounds);
});

// --- the proposals: a card at every stage (whether it is open is the Rust card's, `app/pages/tests/vectors.rs`) --

test("law: every open proposal's card offers an answer, runs its own act or asks for his check, and no closed one does", () => {
  const rows = vector("proposals.json", d.arr(wire.ProposalRow));
  const wants = answer<{ readonly open: boolean }[]>("proposals.answer.json");
  assert.equal(rows.length, wants.length);
  assert.ok(rows.some((row) => row.state === "unknown") && rows.some((row) => row.state === "checked"));
  rows.forEach((row) => {
    assert.equal(
      row.card.answers.length > 0,
      row.state === "standing" || row.state === "failed",
      `${row.state}: a yes only where one is safe`,
    );
    if (row.state === "failed") assert.equal(row.card.answers[0]?.label, "Try again");
  });
});
