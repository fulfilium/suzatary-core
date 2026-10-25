/**
 * The write side, as functions.
 *
 * Three things are pinned here and each of them is a mechanism the app rests
 * on. The REQUEST carries the act header — that is the CSRF half of "a browser
 * tab is a trusted actor", and `gate::check_csrf` refuses without it,
 * so a build that dropped it would produce an app whose every button 403s. The
 * box's ANSWERS to a write are decoded strictly, so an answer this build does
 * not know is no answer rather than an empty list. And CONFIRM is offered
 * exactly where the server says a todo is unconfirmed.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import payload from "../src/wire/generated/payload";
import d from "../src/wire/decoder";
import http from "../src/wire/http";
import type { Entry } from "../src/wire/generated/wire";
import { applicable } from "../src/todo/todos";

// --- the request -------------------------------------------------------------

test("a write carries the act header, the JSON content type and the POST method", () => {
  const request = http.actRequest({ todo: "file83b" });
  assert.equal(request.method, "POST");
  const headers = request.headers as Record<string, string>;
  assert.equal(headers[http.ACT_HEADER], "1");
  assert.equal(headers["content-type"], "application/json");
  assert.equal(request.body, '{"todo":"file83b"}');
});

test("a post's receipt is guarded like every other payload", () => {
  const receipt = { heads: ["ab12"], adopted: ["ab12"], already: [], refused: [], since: "Known", frontier: { digest: "0".repeat(64), count: 1 } };
  assert.equal(d.is(payload.ReceiptPayload)(receipt), true);
  // All the lists are ALWAYS sent, so an answer missing one is a server this
  // build does not know rather than an empty list to be assumed.
  assert.equal(d.is(payload.ReceiptPayload)({ ...receipt, already: undefined }), false);
  assert.equal(d.is(payload.ReceiptPayload)({ ...receipt, heads: "ab12" }), false, "the heads are a set, never one");
  assert.equal(
    d.is(payload.ReceiptPayload)({ ...receipt, refused: [{ hash: "cd", reason: "rests on … which this box does not hold", kind: "final" }] }),
    true,
  );
  // Whether to post it again is the box's to say: a refusal with no kind, or another, is no answer this build reads.
  assert.equal(d.is(payload.ReceiptPayload)({ ...receipt, refused: [{ hash: "cd", reason: "unsigned" }] }), false);
  assert.equal(d.is(payload.ReceiptPayload)({ ...receipt, refused: [{ hash: "cd", reason: "unsigned", kind: "later" }] }), false);
  // A refusal with no reason is a refusal that says nothing, which is the one
  // thing this app must never render.
  assert.equal(d.is(payload.ReceiptPayload)({ ...receipt, refused: [{ hash: "cd" }] }), false);
});

test("the authoring's answer is an event print and the todo it is about", () => {
  assert.equal(
    d.is(payload.AuthoredPayload)({ todo: "renewthelease", event: "Authored(todo='renewthelease', …)", revision: null }),
    true,
  );
  assert.equal(
    d.is(payload.AuthoredPayload)({ todo: "boardingpass", event: "Authored(todo='boardingpass', …)", revision: "Authored(…)" }),
    true,
    "a subtodo carries its parent's revision",
  );
  assert.equal(d.is(payload.AuthoredPayload)({ todo: "renewthelease", revision: null }), false, "an answer with no event authored nothing");
  assert.equal(d.is(payload.AuthoredPayload)({ event: "Authored(…)", revision: null }), false);
  assert.equal(d.is(payload.AuthoredPayload)({ todo: "renewthelease", event: "Authored(…)" }), false, "revision is null, never absent");
});

// --- what applies ------------------------------------------------------------

function entry(over: Partial<Entry> = {}): Entry {
  return {
    todo: "file83b",
    state: "open",
    states: ["open"],
    at: "",
    claimed: "",
    unconfirmed: false,
    recurring: false,
    value: 0.4,
    conflicts: {},
    authored: null,
    candidates: [],
    stream: [],
    ...over,
  };
}

test("confirm is offered exactly when the SERVER says the todo is unconfirmed", () => {
  assert.deepEqual(applicable(entry({ unconfirmed: true })), ["done", "cancel", "confirm"]);
  assert.deepEqual(applicable(entry({ state: "completed", unconfirmed: true })), ["reopen", "confirm"]);
  // The roster lives on the server; this side reads its verdict and no more.
  assert.equal(applicable(entry({ claimed: "completed" })).includes("confirm"), false);
});
