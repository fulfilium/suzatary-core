/** The transcript regrouped into rounds, by each row's own `index` field. */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import type { ProposalRow, TurnRow } from "../src/wire/generated/wire";
import { rounds } from "../src/chat/rounds";
import { aCard } from "./cards";
import { forAll } from "./gen";

function person(index: number, text: string, at = "2026-09-15T09:00:00"): TurnRow {
  return {
    index,
    who: "person",
    text,
    references: [],
    attachments: [],
    queued: false,
    at,
    attempt: "current",
    gone: null,
  };
}

function act(index: number, text: string, at = "2026-09-15T09:00:00"): TurnRow {
  return {
    index,
    who: "act",
    text,
    references: [],
    at,
    attempt: "current",
    gone: null,
  };
}

function model(index: number, text: string, at = "2026-09-15T09:00:01"): TurnRow {
  return {
    index,
    who: "model",
    text,
    blocks: [],
    at,
    attempt: "current",
    gone: null,
  };
}

function verb(
  index: number,
  name: string,
  text = "answer",
  refused = false,
  at = "2026-09-15T09:00:02",
): TurnRow {
  return {
    index,
    who: "verb",
    verb: name,
    text,
    outcome: refused ? { is: "refused" } : { is: "answered" },
    at,
    attempt: "current",
    gone: null,
  };
}

function proposalAt(index: number, turn: number): ProposalRow {
  const sources = ["mail:abc123abc123"];
  return {
    index,
    act: { kind: "email", account: "a", to: [], cc: [], subject: "s", body: "b", attachments: [], held: { type: "Read", values: [] } },
    card: aCard(),
    why: "because",
    sources,
    at: "2026-09-15T09:00:05",
    state: "standing",
    outcome: "",
    tries: 0,
    turn,
    coding: null,
    announced: null,
    revises: null,
  };
}

const earlier = (row: TurnRow): TurnRow => ({ ...row, attempt: "earlier" });

type Shape = {
  readonly opening?: number | null;
  readonly who?: TurnRow["who"];
  readonly trace?: number;
  readonly answer?: number | null;
  readonly key?: number;
  readonly start?: number;
  readonly end?: number;
  readonly earlier?: readonly number[];
};

test("examples: where a round begins and ends, what it opens with and answers, and what it sets aside", () => {
  const cases: [string, TurnRow[], Shape[]][] = [
    ["a bare exchange is one round: opening, no trace, an answer", [person(0, "hi"), model(1, "hello", "2026-09-15T09:00:10")], [{ opening: 0, trace: 0, answer: 1 }]],
    [
      "an unfinished round ends in a status turn's own trace, with no answer",
      [person(0, "check something"), model(1, "Reading mail.", "2026-09-15T09:00:01"), verb(2, "search", "3 hits")],
      [{ trace: 2, answer: null }],
    ],
    [
      "two rounds are two rounds",
      [person(0, "first"), model(1, "first answer", "2026-09-15T09:00:02"), person(2, "second"), model(3, "second answer", "2026-09-15T09:00:05")],
      [{ opening: 0, answer: 1 }, { opening: 2, answer: 3 }],
    ],
    ["an empty answer yields a round with no answer to draw", [person(0, "anything to add?"), model(1, "")], [{ answer: null, trace: 0 }]],
    [
      "a leading model turn (the morning brief) opens with no person or act",
      [model(0, "Your brief for Monday is ready.", "2026-09-15T10:03:00")],
      [{ opening: null, answer: 0, key: 0 }],
    ],
    [
      "an act can open a round exactly like a person's turn",
      [act(0, "You did: cancelled todo:life/x"), model(1, "Understood.", "2026-09-15T09:00:03")],
      [{ who: "act", answer: 1 }],
    ],
    // A windowed page: `turns[0]` is really turn 40, never 0.
    [
      "a window that starts mid-transcript reads its own absolute indices, not positions",
      [person(40, "what now"), model(41, "Here.", "2026-09-15T09:00:02")],
      [{ opening: 40, answer: 41, start: 40, end: 41 }],
    ],
    // `?about=` can drop whole rounds, so the indices jump from 1 to 10; the gap is not one round spanning it.
    [
      "a gap between two kept rounds (?about=) still groups each one correctly",
      [person(0, "first"), model(1, "first answer", "2026-09-15T09:00:02"), person(10, "much later", "2026-09-16T09:00:00"), model(11, "later answer", "2026-09-16T09:00:02")],
      [{ start: 0, end: 1 }, { start: 10, end: 11 }],
    ],
    [
      "a regenerated round keeps its question, the new answer, and the old one as an earlier attempt",
      [person(0, "q"), earlier(model(1, "old")), model(2, "new")],
      [{ opening: 0, answer: 2, earlier: [1] }],
    ],
    [
      "an edited question and all after it are an earlier attempt of their own, before the new round",
      [earlier(person(0, "q")), earlier(model(1, "a")), person(2, "q, reworded"), model(3, "b")],
      [{ key: 0, opening: null, earlier: [0, 1] }, { key: 2, opening: 2, earlier: [] }],
    ],
    [
      "a stopped round asked again draws the stop inside the earlier attempt, beside its question",
      [person(0, "q"), earlier(act(1, "Ada stopped the answer")), earlier(model(2, "")), model(3, "a")],
      [{ answer: 3, earlier: [1, 2] }],
    ],
  ];
  for (const [name, turns, want] of cases) {
    const grouped = rounds(turns, []);
    assert.equal(grouped.length, want.length, name);
    want.forEach((w, n) => {
      const round = grouped[n]!;
      const got: Record<keyof Shape, unknown> = {
        opening: round.opening?.index ?? null,
        who: round.opening?.turn.who,
        trace: round.trace.length,
        answer: round.answer?.index ?? null,
        key: round.key,
        start: round.startIndex,
        end: round.endIndex,
        earlier: round.earlier.map((p) => p.index),
      };
      for (const k of Object.keys(w) as (keyof Shape)[]) assert.deepEqual(got[k], w[k], `${name}: round ${n} ${k}`);
    });
  }
});

/** One row of `app/conversations/tests/about-fixture.json`. */
interface FixtureTurn {
  readonly who: TurnRow["who"];
  readonly text: string;
  readonly references: readonly string[];
  readonly at: string;
}

/**
 * THE SAME FIXTURE `suzatary_conversations::secretary::about`'s OWN TEST READS: Rust's `round_spans` answers
 * `[(0,3),(4,5),(6,7)]` over the same eight turns, and `rounds` must partition the identical three spans.
 */
test("the shared fixture groups into the same three spans the Rust side pins", () => {
  const rows = JSON.parse(readFileSync("app/conversations/tests/about-fixture.json", "utf8")) as FixtureTurn[];
  const turns: TurnRow[] = rows.map((row, index) => ({
    index,
    who: row.who,
    verb: row.who === "verb" ? "todo_show" : "",
    text: row.text,
    outcome: { is: "answered" },
    references: [...row.references],
    attachments: [],
    queued: false,
    blocks: [],
    at: row.at,
    attempt: "current",
    gone: null,
  }));
  const grouped = rounds(turns, []);
  const spans = grouped.map((round) => [round.startIndex, round.endIndex]);
  assert.deepEqual(spans, [
    [0, 3],
    [4, 5],
    [6, 7],
  ]);
});

test("law: every turn is drawn once — a current one as opening, trace or answer, an earlier one in earlier", () => {
  forAll(300, (r) => {
    const makers = [person, act, model, (i: number) => verb(i, r.pick(["search", "show", "todo_add"]))];
    const turns = r.array(r.int(24), () => 0).map((_, index) => {
      const row = r.pick(makers)(index, r.pick(["", "text"]));
      return r.bool(0.3) ? earlier(row) : row;
    });
    const grouped = rounds(turns, []);
    const drawn = grouped.flatMap((round) => [
      ...(round.opening === null ? [] : [round.opening]),
      ...round.trace,
      ...(round.answer === null ? [] : [round.answer]),
    ]);
    const kept = grouped.flatMap((round) => round.earlier);
    assert.ok(drawn.every((p) => p.turn.attempt === "current"), "no earlier turn drawn as current");
    assert.ok(kept.every((p) => p.turn.attempt === "earlier"), "no current turn folded away");
    const seen = [...drawn, ...kept].map((p) => p.index).sort((a, b) => a - b);
    const expected = turns
      .filter((t) => !(t.attempt === "current" && t.who === "model" && t.text === "" && endsItsRound(turns, t.index)))
      .map((t) => t.index);
    assert.deepEqual(seen, expected);
  });
});

/** A current model turn nothing current follows before the next opening — the round's answer candidate. */
function endsItsRound(turns: readonly TurnRow[], index: number): boolean {
  const next = turns.slice(index + 1).find((t) => t.attempt === "current" || t.who === "person");
  return next === undefined || next.who === "person" || next.who === "act";
}

test("law: every card is drawn once, in the last round begun by the turn it follows, else the first", () => {
  forAll(300, (r) => {
    const makers = [person, act, model, (i: number) => verb(i, "search")];
    let at = 0;
    const turns = r.array(r.int(16), () => 0).map(() => {
      at += 1 + r.int(3);
      return r.pick(makers)(at, r.pick(["", "text"]));
    });
    const cards = r.array(r.int(6), () => 0).map((_, n) => proposalAt(n, r.int(at + 4)));
    const grouped = rounds(turns, cards);
    if (grouped.length === 0) return assert.equal(turns.length, 0, "no round only with no turn");
    grouped.forEach((round, n) => {
      const next = grouped[n + 1];
      const mine = cards.filter(
        (p) => (n === 0 || round.startIndex <= p.turn - 1) && (next === undefined || next.startIndex > p.turn - 1),
      );
      assert.deepEqual(round.proposals, mine, "the cards begun by this round and not the next, in the order given");
    });
    assert.equal(grouped.flatMap((round) => round.proposals).length, cards.length, "each drawn once");
  });
});
