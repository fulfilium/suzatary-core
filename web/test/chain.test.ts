/**
 * THE SLUG GUARD, which is a PARSE and not a formality.
 *
 * `chain.ts`'s own header says what it is for: a chain name is a PATH SEGMENT
 * (`/api/<chain>/fold`), and the selection is remembered in `localStorage`,
 * which is an input from outside the program like any other. A hand-edited or
 * stale entry must not become `/api/../fold` — so the value that comes back is
 * parsed against the same rule `suzatary_base::chains::ChainName` keeps, and a
 * value that is not a slug opens the default chain instead.
 *
 * `theme.ts`'s equivalent three-line guard has had a test since it shipped;
 * this one is the older and the more load-bearing of the two, and had none.
 *
 * What is pinned here is the guard and the BOUNDARY, not the storage: what a
 * browser does with `localStorage` needs a browser, and `kv.ts` already
 * degrades to "no stored value" under node — which is why the default is
 * observable here at all.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { DEFAULT_CHAIN, isChainName, remembered } from "../src/app/chain";
import app from "../src/app/app";

test("the guard admits a slug of up to 32 and refuses traversal, separators, case, length and non-strings", () => {
  const cases: [unknown, boolean][] = [
    ["life", true],
    ["suzatary", true],
    ["a", true],
    ["a-b-2", true],
    ["a".repeat(32), true],
    // The one this guard exists for: `/api/../fold` is another deployment's
    // question asked of this box.
    ["..", false],
    ["../life", false],
    ["life/../mail", false],
    ["life mail", false],
    ["life;rm", false],
    ["%2e%2e", false],
    ["A", false],
    ["Life", false],
    ["", false],
    ["a".repeat(33), false],
    // A slug starts with a LETTER, so a leading digit or dash is not one.
    ["1life", false],
    ["-life", false],
    [null, false],
    [undefined, false],
    [0, false],
    [1, false],
    [{}, false],
    [[], false],
    [["life"], false],
    [true, false],
  ];
  for (const [value, ok] of cases) assert.equal(isChainName(value), ok, JSON.stringify(value));
});

test("a store this browser cannot read opens on the first chain", () => {
  // Under node there is no `window`, so the stored value reads as absent — the
  // same answer a private window, cleared site data or a browser that blocks
  // storage gives. The app opens on `life` either way.
  assert.equal(remembered(), DEFAULT_CHAIN);
  assert.equal(app.now().chain, DEFAULT_CHAIN);
  assert.equal(app.apiBase(app.now()), `/api/${DEFAULT_CHAIN}`);
});

test("a pin moves the chain only to another slug, and a refusal leaves it where it was", () => {
  const at = app.initial({ type: "inbox" }, DEFAULT_CHAIN);
  assert.equal(app.step(at, { type: "Pinned", chain: DEFAULT_CHAIN }), at, "the chain it is already on has not moved");
  assert.equal(app.step(at, { type: "Pinned", chain: ".." }), at, "a traversal is ignored");
  const other = app.step(at, { type: "Pinned", chain: "ops" });
  assert.equal(other.chain, "ops");
  assert.equal(app.apiBase(other), "/api/ops");
  // And a refusal from the second chain does not knock it back to the first.
  assert.equal(app.step(other, { type: "Pinned", chain: "../life" }), other);
});
