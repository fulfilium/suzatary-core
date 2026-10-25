/**
 * `conversation/again.ts` — the offer to ask the last round again, and the
 * composer's mode (`docs/CHAT-PLAN-2026-09-25.md` §4.4, §4.11).
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ConversationPayload, ConversationState } from "../src/wire/generated/wire";
import turns from "../src/chat/turns"
import again from "../src/chat/again";
import { step as sourceStep } from "../src/chat/source";
import type { SourceState } from "../src/chat/source";
import { forAll } from "./gen";
import grouped from "../src/chat/grouped";
import time from "../src/app/time";

const turn = (index: number, over: Partial<turns.Question> = {}): turns.Question => ({
  index,
  who: "person",
  text: `q${index}`,
  references: [],
  attachments: [],
  queued: false,
  at: "2026-09-25T10:00:00",
  attempt: "current",
  gone: null,
  ...over,
});

const payload = (over: Partial<ConversationPayload>): grouped.Window =>
  grouped.of({
  id: "default",
  title: "Secretary",
  model: "m",
  opened: "2026-09-25",
  turns: [turn(0), { index: 1, who: "model", text: "a", blocks: [], at: "2026-09-25T10:00:00", attempt: "current", gone: null }],
  proposals: [],
  unplaced: [],
  state: "idle",
  first: 0,
  total: 2,
  again: 0,
  ...over,
}, time.UTC);

const loaded = (over: Partial<ConversationPayload> = {}): SourceState =>
  sourceStep(
    { kind: "loading", id: "default", about: undefined },
    { kind: "window", mode: "newest", payload: payload(over), fallback: false, at: 0 },
  );

test("law: the box's question is offered exactly while nothing runs and the window holds it", () => {
  assert.equal(again.offer({ kind: "loading", id: "default", about: undefined }).type, "None");
  assert.deepEqual(again.offer(loaded()), { type: "Last", question: turn(0) });
  assert.equal(again.offer(loaded({ again: null })).type, "None", "the box offers nothing");
  assert.equal(again.offer(loaded({ again: 7 })).type, "None", "not in this window");
  for (const state of ["queued", "thinking", "answering"] as ConversationState[]) {
    assert.equal(again.offer(loaded({ state })).type, "None", state);
  }
});

const events = (question: turns.Question): again.Event[] => [
  { type: "edit", offer: again.none },
  { type: "edit", offer: { type: "Last", question } },
  { type: "cancel" },
  { type: "sent" },
];

test("law: an edit with nothing offered, and a cancel or send while asking, change nothing — the same object", () => {
  const editing: again.Mode = { type: "Editing", question: turn(2) };
  assert.equal(again.step(again.asking, { type: "edit", offer: again.none }), again.asking);
  assert.equal(again.step(editing, { type: "edit", offer: again.none }), editing);
  assert.equal(again.step(again.asking, { type: "cancel" }), again.asking);
  assert.equal(again.step(again.asking, { type: "sent" }), again.asking);
});

test("law: step reaches only asking, or editing the question last offered", () => {
  forAll(200, (r) => {
    const questions = r.array(1 + r.int(4), () => turn(r.int(50)));
    let mode = again.asking;
    let offered: turns.Question | null = null;
    for (let i = r.int(30); i > 0; i -= 1) {
      const e = r.pick(events(r.pick(questions)));
      mode = again.step(mode, e);
      if (e.type === "edit" && e.offer.type === "Last") offered = e.offer.question;
      if (e.type === "cancel" || e.type === "sent") offered = null;
      if (mode.type === "Asking") assert.equal(offered, null);
      else assert.equal(mode.question, offered);
    }
  });
});
