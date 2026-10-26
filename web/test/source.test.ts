/**
 * `conversation/source.ts` — the closed state sum and its pure `step`
 * (`docs/WEBALGEBRA-PLAN-2026-09-16.md` §3), plus the effectful shell driven
 * with a fake `fetchWindow`/`ask` and a fake stream that never opens a
 * socket, the same "inject the effectful half" `store.test.ts` uses on
 * `createStore`.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ConversationPayload, Frame, TurnRow } from "../src/wire/generated/wire";
import turns from "../src/chat/turns"
import type asks from "../src/prodrome/asks";
import { answering, asking, createSource, step } from "../src/chat/source";
import type { SourceDeps, SourceEvent, SourceState } from "../src/chat/source";
import type stream from "../src/algebra/stream";
import { forAll } from "./gen";
import grouped from "../src/chat/grouped";
import time from "../src/app/time";

function turn(index: number, over: Partial<turns.Question> = {}): turns.Question {
  return {
    index,
    who: "person",
    text: "hi",
    references: [],
    attachments: [],
    queued: false,
    at: "2026-09-16T10:00:00",
    attempt: "current",
    gone: null,
    ...over,
  };
}

// A window as the core's worker answers one, grouped into rounds.
function payload(over: Partial<ConversationPayload> = {}): grouped.Window {
  return grouped.of({
    id: "default",
    title: "Secretary",
    model: "claude",
    opened: "2026-09-16",
    turns: [],
    proposals: [],
    unplaced: [],
    state: "idle",
    first: 0,
    total: 0,
    again: null,
    ...over,
  }, time.UTC);
}

const LOADING: SourceState = { kind: "loading", id: "default", about: undefined };

function windowEvent(over: Partial<Extract<SourceEvent, { kind: "window" }>> = {}): SourceEvent {
  return { kind: "window", mode: "newest", payload: payload(), fallback: false, at: 0, ...over };
}

// --- step: the transition table ---------------------------------------------

test("a window's unplaced proposal lines are kept, each with why, and the next window replaces them", () => {
  const unplaced = [{ index: 0, why: "notHeld" as const }];
  const held = step(LOADING, windowEvent({ payload: payload({ unplaced }) }));
  assert.ok(held.kind === "loaded");
  assert.deepEqual(held.unplaced, unplaced);
  const placed = step(held, windowEvent({ payload: payload() }));
  assert.ok(placed.kind === "loaded");
  assert.deepEqual(placed.unplaced, []);
});

test("loading + window -> loaded, with the payload's own fields", () => {
  const next = step(LOADING, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1 }) }));
  assert.equal(next.kind, "loaded");
  assert.ok(next.kind === "loaded");
  assert.equal(next.turns.length, 1);
  assert.equal(next.title, "Secretary");
  assert.equal(next.problem, null);
  assert.equal(next.fallback, false);
});

test("loading + failed -> failed, never loaded", () => {
  const next = step(LOADING, { kind: "failed", reason: "network gone" });
  assert.deepEqual(next, { kind: "failed", id: "default", about: undefined, reason: "network gone" });
});

test("failed + window -> loaded (a later fetch recovers)", () => {
  const failed = step(LOADING, { kind: "failed", reason: "x" });
  const next = step(failed, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1 }) }));
  assert.equal(next.kind, "loaded");
});

test("loaded + failed -> STAYS loaded, with `problem` set and the turns untouched", () => {
  const loaded = step(LOADING, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1 }) }));
  const next = step(loaded, { kind: "failed", reason: "network gone" });
  assert.equal(next.kind, "loaded");
  assert.ok(next.kind === "loaded");
  assert.equal(next.problem, "network gone");
  assert.equal(next.turns.length, 1);
});

test("a `window` after a `problem` clears it", () => {
  const loaded = step(LOADING, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1 }) }));
  const withProblem = step(loaded, { kind: "failed", reason: "x" });
  const recovered = step(withProblem, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1 }) }));
  assert.equal(recovered.kind, "loaded");
  assert.ok(recovered.kind === "loaded");
  assert.equal(recovered.problem, null);
});

test("a `frame` before anything has loaded is dropped", () => {
  const frame: Frame = { kind: "working", verb: "search" };
  assert.deepEqual(step(LOADING, { kind: "frame", frame, at: 0 }), LOADING);
  const failed = step(LOADING, { kind: "failed", reason: "x" });
  assert.deepEqual(step(failed, { kind: "frame", frame, at: 0 }), failed);
});

test("frames step the answer's place: working, then text landing, then the turn", () => {
  let state = step(LOADING, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1, state: "thinking" }), at: 5 }));
  assert.deepEqual(state.kind === "loaded" ? state.answer : null, { type: "Thinking", since: 5 });
  state = step(state, { kind: "frame", frame: { kind: "working", verb: "search" }, at: 9 });
  assert.deepEqual(state.kind === "loaded" ? state.answer : null, { type: "Working", verb: "search", checked: 1, since: 5 });
  state = step(state, { kind: "frame", frame: { kind: "delta", text: "Hello" }, at: 10 });
  state = step(state, { kind: "frame", frame: { kind: "delta", text: ", world" }, at: 11 });
  assert.deepEqual(state.kind === "loaded" ? state.answer : null, { type: "Streaming", blocks: [], tail: "Hello, world" });
  state = step(state, { kind: "frame", frame: { kind: "turn" }, at: 12 });
  assert.deepEqual(state.kind === "loaded" ? state.answer : null, { type: "Done" });
  assert.equal(state.kind === "loaded" ? state.turns.length : -1, 1, "the refetch is the shell's job");
});

test("a `state` frame updates the conversation state, and an idle one ends the answer", () => {
  let state = step(LOADING, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1 }) }));
  state = step(state, { kind: "frame", frame: { kind: "state", state: "thinking" }, at: 3 });
  assert.equal(state.kind === "loaded" ? state.conversationState : "x", "thinking");
  assert.deepEqual(state.kind === "loaded" ? state.answer : null, { type: "Thinking", since: 3 });
  state = step(state, { kind: "frame", frame: { kind: "state", state: "idle" }, at: 4 });
  assert.deepEqual(state.kind === "loaded" ? state.answer : null, { type: "Done" });
});

test("a window is what the page holds: the worker answered every turn it asked for, older or newest", () => {
  const first = step(LOADING, windowEvent({ payload: payload({ turns: [turn(2), turn(3)], first: 2, total: 4 }) }));
  const older = step(first, windowEvent({ mode: "older", payload: payload({ turns: [0, 1, 2, 3].map((i) => turn(i)), first: 0, total: 4 }) }));
  assert.deepEqual(older.kind === "loaded" ? older.turns.map((t) => t.index) : [], [0, 1, 2, 3]);
  assert.equal(older.kind === "loaded" ? older.first : -1, 0);
});

test("the page asks for every turn it holds, and an older window for those before them too", () => {
  assert.deepEqual(asking(LOADING, "newest", undefined), {});
  const held = step(LOADING, windowEvent({ payload: payload({ turns: [70, 71, 72].map((i) => turn(i)), first: 70, total: 73 }) }));
  assert.deepEqual(asking(held, "newest", undefined), { limit: 60, from: 70 });
  assert.deepEqual(asking(held, "older", undefined), { limit: 63, from: 10 });
  assert.deepEqual(asking(held, "older", "todo:life/x"), { about: "todo:life/x", limit: 63 }, "a window about a reference by count");
});

test("`fallback` is monotone: once true, a later window without it stays true", () => {
  const fell = step(LOADING, windowEvent({ payload: payload({ turns: [turn(0)], first: 0, total: 1 }), fallback: true }));
  assert.equal(fell.kind === "loaded" ? fell.fallback : false, true);
  const later = step(fell, windowEvent({ payload: payload({ turns: [turn(1)], first: 1, total: 2 }), fallback: false }));
  assert.equal(later.kind === "loaded" ? later.fallback : false, true);
});

// --- step: a property, over a random walk of events -------------------------

/** A tiny seeded PRNG (mulberry32) — deterministic across runs, so a failure
 * is reproducible without a shared `gen.ts` (that module is
 * `docs/WEBALGEBRA-PLAN-2026-09-16.md` §5 commit 1's, not this commit's). */
function mulberry32(seed: number): () => number {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function randomEvent(rand: () => number): SourceEvent {
  const pick = rand();
  if (pick < 0.5) {
    const count = Math.floor(rand() * 3);
    const first = Math.floor(rand() * 5);
    return {
      kind: "window",
      mode: rand() < 0.5 ? "newest" : "older",
      payload: payload({
        turns: Array.from({ length: count }, (_, i) => turn(first + i)),
        first,
        total: first + count,
      }),
      fallback: rand() < 0.2,
      at: 0,
    };
  }
  if (pick < 0.85) {
    const frameKinds = ["state", "delta", "turn", "working"] as const;
    const kind = frameKinds[Math.floor(rand() * frameKinds.length)]!;
    const frame: Frame =
      kind === "state"
        ? { kind: "state", state: (["idle", "queued", "thinking", "answering"] as const)[Math.floor(rand() * 4)]! }
        : kind === "delta"
          ? { kind: "delta", text: "x" }
          : kind === "turn"
            ? { kind: "turn" }
            : { kind: "working", verb: "search" };
    return { kind: "frame", frame, at: 0 };
  }
  return { kind: "failed", reason: "boom" };
}

test("property: no illegal state is reachable over a random walk of events", () => {
  const rand = mulberry32(1234567);
  for (let trial = 0; trial < 50; trial += 1) {
    let state: SourceState = { kind: "loading", id: "default", about: undefined };
    let sawLoaded = false;
    for (let step_ = 0; step_ < 60; step_ += 1) {
      const event = randomEvent(rand);
      const before = state;
      state = step(state, event);
      // `Failed` IS REACHABLE ONLY FROM `Loading` — once a window has ever
      // landed, this source stays `Loaded` for the rest of its life: never
      // `loading` again, and a `failed` event only sets `problem`.
      if (sawLoaded) {
        assert.notEqual(state.kind, "loading");
        assert.notEqual(state.kind, "failed");
      }
      if (state.kind === "loaded") sawLoaded = true;
      // MONOTONE FALLBACK: a `Loaded` state's `fallback` never goes from true
      // to false across one step.
      if (before.kind === "loaded" && state.kind === "loaded" && before.fallback) {
        assert.equal(state.fallback, true);
      }
      // `id`/`about` NEVER CHANGE — this source is for one conversation,
      // filtered one way, for its whole life.
      assert.equal(state.id, "default");
      assert.equal(state.about, undefined);
    }
  }
});

// --- readings that land out of order ------------------------------------------

// Measured in the `secretary` e2e scene under load: `say`'s answer is built
// before the worker is nudged, and lands after the round's frames and after
// the refetch its `turn` frame asked for.
test("law: say's answer landing after the round it started is past, and moves nothing", () => {
  const asked = turn(0, { queued: true });
  const answered: TurnRow[] = [turn(0), { index: 1, who: "model", text: "hi", blocks: [], at: "2026-09-16T10:00:00", attempt: "current", gone: null }];
  const events: SourceEvent[] = [
    windowEvent({ payload: payload() }),
    { kind: "frame", frame: { kind: "state", state: "thinking" }, at: 1 },
    { kind: "frame", frame: { kind: "turn" }, at: 2 },
    { kind: "frame", frame: { kind: "state", state: "idle" }, at: 2 },
    windowEvent({ payload: payload({ turns: answered, total: 2, again: 0 }), at: 3 }),
    windowEvent({ payload: payload({ turns: [asked], total: 1, state: "queued" }), at: 4 }),
  ];
  const last = events.reduce(step, LOADING);
  assert.ok(last.kind === "loaded");
  assert.deepEqual(last.answer, { type: "Done" });
  assert.equal(last.conversationState, "idle");
  assert.deepEqual(last.turns, answered);
  assert.equal(last.again, 0);
});

test("law: newest readings commute: whatever order they land in, the one holding the most turns is what is on hand", () => {
  forAll(200, (r) => {
    const readings = r.array(1 + r.int(5), () => r.int(8))
      .filter((n, i, ns) => ns.indexOf(n) === i)
      .map((total) =>
        windowEvent({
          payload: payload({
            turns: Array.from({ length: total }, (_, i) => turn(i, { queued: i === total - 1 && r.bool() })),
            total,
            state: r.pick(["idle", "queued", "thinking", "answering"] as const),
          }),
        }),
      );
    const order = readings.map((e) => ({ e, k: r.int(1000) })).sort((a, b) => a.k - b.k).map(({ e }) => e);
    const newest = readings.reduce((a, b) => (a.kind === "window" && b.kind === "window" && b.payload.total > a.payload.total ? b : a));
    const landed = order.reduce(step, LOADING);
    const alone = step(LOADING, newest);
    assert.ok(landed.kind === "loaded" && alone.kind === "loaded");
    assert.deepEqual(
      { turns: landed.turns, total: landed.total, state: landed.conversationState, answer: landed.answer.type },
      { turns: alone.turns, total: alone.total, state: alone.conversationState, answer: alone.answer.type },
    );
  });
});

// --- the effectful shell -----------------------------------------------------

function inertStream(closed: () => void = () => {}): stream.Socket {
  return {
    addEventListener: () => {},
    readyState: 0,
    close: closed,
  };
}

/** The shell's effects, each window read whole: no test here holds a conversation that is not on the device. */
function deps(
  over: Partial<Omit<SourceDeps, "fetchWindow">> & { fetchWindow?: (id: string, window: Parameters<SourceDeps["fetchWindow"]>[1]) => Promise<grouped.Window> } = {},
): SourceDeps {
  const { fetchWindow = async () => payload(), ...rest } = over;
  return {
    fetchWindow: (id, window) => fetchWindow(id, window).then((p) => ({ type: "Read", payload: p })),
    ask: async () => {},
    openStream: () => inertStream(),
    wake: () => () => {},
    pull: async () => {},
    watch: () => () => {},
    ...rest,
  };
}

test("law: readings land in any order, and what is on hand is never one asked before what was drawn", async () => {
  // The cards ride another store than the turns, so the readings here hold the same turns and differ only in what
  // the later ones saw; the title stands for it.
  const cases: { n: number; order: number[]; failing: Set<number> }[] = [];
  forAll(100, (r) => {
    const n = 2 + r.int(5);
    const order = Array.from({ length: n }, (_, i) => ({ i, k: r.int(1000) })).sort((a, b) => a.k - b.k).map(({ i }) => i);
    cases.push({ n, order, failing: new Set(order.filter(() => r.bool(0.2))) });
  });
  for (const { n, order, failing } of cases) {
    const pending: { ok: (p: grouped.Window) => void; no: (e: Error) => void }[] = [];
    const source = createSource(
      "default",
      undefined,
      deps({ fetchWindow: () => new Promise((ok, no) => void pending.push({ ok, no })) }),
    );
    const reads = Array.from({ length: n }, () => source.refresh());
    await new Promise((done) => setTimeout(done, 0));
    assert.equal(pending.length, n, "every refresh asked");
    // The latest-asked reading landed so far, and the one on hand: a reading asked after every one landed before it is
    // drawn (a failure keeps what is on hand and says so), and any other is dropped whole.
    let latest = -1;
    let shown = -1;
    for (const i of order) {
      const p = pending[i];
      assert.ok(p !== undefined);
      const before = source.state();
      if (failing.has(i)) p.no(new Error(`reading ${i} failed`));
      else p.ok(payload({ title: `reading ${i}`, turns: [turn(0)], total: 1 }));
      await new Promise((done) => setTimeout(done, 0));
      const after = source.state();
      if (i < latest) {
        assert.deepEqual(after, before, `reading ${i}, landing after reading ${latest}, moved nothing`);
        continue;
      }
      latest = i;
      if (!failing.has(i)) shown = i;
      const now = after.kind === "loaded" ? Number(after.title.replace("reading ", "")) : -1;
      assert.equal(now, shown, `reading ${i} landed last of those asked so far`);
    }
    await Promise.all(reads);
    source.destroy();
  }
});

test("refresh loads the newest window", async () => {
  const source = createSource(
    "default",
    undefined,
    deps({ fetchWindow: async () => payload({ turns: [turn(0), turn(1)], first: 0, total: 2 }) }),
  );
  await source.refresh();
  const state = source.state();
  assert.equal(state.kind, "loaded");
  assert.equal(state.kind === "loaded" ? state.turns.length : -1, 2);
  assert.equal(state.kind === "loaded" ? state.title : "", "Secretary");
});

test("earlier does nothing once `first` is already 0", async () => {
  let calls = 0;
  const source = createSource(
    "default",
    undefined,
    deps({
      fetchWindow: async () => {
        calls += 1;
        return payload({ turns: [turn(0)], first: 0, total: 1 });
      },
    }),
  );
  await source.refresh();
  calls = 0;
  await source.earlier();
  assert.equal(calls, 0);
});

test("say reads the window again once the box took the turn, never its answer", async () => {
  const source = createSource(
    "default",
    undefined,
    deps({
      fetchWindow: async () => payload({ turns: [turn(0, { queued: true })], first: 0, total: 1 }),
    }),
  );
  await source.say("hello", []);
  const state = source.state();
  assert.equal(state.kind === "loaded" && state.turns[0] !== undefined ? turns.queued(state.turns[0]) : undefined, true);
});

test("stop asks the box once and reads the window again", async () => {
  const asked: asks.Ask[] = [];
  const source = createSource(
    "default",
    undefined,
    deps({
      ask: async (a) => {
        asked.push(a);
      },
      fetchWindow: async () => payload({ turns: [turn(0)], first: 0, total: 1, state: "thinking" }),
    }),
  );
  await source.refresh();
  await source.stop();
  assert.deepEqual(asked, [{ type: "Round", kind: "Stop", conversation: "default", after: 1 }]);
  assert.equal(answering(source.state()), "running");
});

test("the empty-slice fallback: `about` yields no turns, so the source refetches the whole conversation and flags it", async () => {
  const seen: (string | undefined)[] = [];
  const source = createSource(
    "default",
    { kind: "todo", chain: "life", id: "x" },
    deps({
      fetchWindow: async (_id, window) => {
        seen.push(window.about);
        return window.about === undefined
          ? payload({ turns: [turn(0)], first: 0, total: 1 })
          : payload({ turns: [], first: 0, total: 0 });
      },
    }),
  );
  await source.refresh();
  assert.deepEqual(seen, ["todo:life/x", undefined]);
  const state = source.state();
  assert.equal(state.kind === "loaded" ? state.fallback : false, true);
  assert.equal(state.kind === "loaded" ? state.turns.length : -1, 1);
});

test("once fallen back, a later fetch does not ask with `about` again", async () => {
  const seen: (string | undefined)[] = [];
  const source = createSource(
    "default",
    { kind: "todo", chain: "life", id: "x" },
    deps({
      fetchWindow: async (_id, window) => {
        seen.push(window.about);
        return window.about === undefined
          ? payload({ turns: [turn(0)], first: 0, total: 1 })
          : payload({ turns: [], first: 0, total: 0 });
      },
    }),
  );
  await source.refresh();
  await source.refresh();
  assert.deepEqual(seen, ["todo:life/x", undefined, undefined]);
});

test("`about` that DOES find rounds never triggers the fallback", async () => {
  const source = createSource(
    "default",
    { kind: "todo", chain: "life", id: "x" },
    deps({
      fetchWindow: async (_id, window) =>
        window.about === "todo:life/x"
          ? payload({ turns: [turn(0)], first: 0, total: 1 })
          : payload({ turns: [], first: 0, total: 0 }),
    }),
  );
  await source.refresh();
  const state = source.state();
  assert.equal(state.kind === "loaded" ? state.fallback : true, false);
});

test("a failed refresh sets `problem` (or `Failed`, before the first load) and a later refresh recovers", async () => {
  let fail = true;
  const source = createSource(
    "default",
    undefined,
    deps({
      fetchWindow: async () => {
        if (fail) {
          fail = false;
          throw new Error("network gone");
        }
        return payload({ turns: [turn(0)], first: 0, total: 1 });
      },
    }),
  );
  await source.refresh();
  assert.equal(source.state().kind, "failed");
  await source.refresh();
  const state = source.state();
  assert.equal(state.kind, "loaded");
  assert.equal(state.kind === "loaded" ? state.problem : "x", null);
  assert.equal(state.kind === "loaded" ? state.turns.length : -1, 1);
});

test("subscribers hear a `window`/`newest` event on refresh", async () => {
  const events: string[] = [];
  const source = createSource(
    "default",
    undefined,
    deps({ fetchWindow: async () => payload({ turns: [turn(0)], first: 0, total: 1 }) }),
  );
  source.subscribe((_state, event) => {
    events.push(event.kind === "window" ? `window:${event.mode}` : event.kind);
  });
  await source.refresh();
  assert.deepEqual(events, ["window:newest"]);
});

test("destroy closes the stream it opened", () => {
  let closed = 0;
  const source = createSource("default", undefined, deps({ openStream: () => inertStream(() => (closed += 1)) }));
  source.start();
  source.destroy();
  assert.equal(closed, 1);
});
