/**
 * The slug guard: a chain name is a path segment and the remembered selection is input from outside the program, so a
 * value that is no slug by `suzatary_base::chains::ChainName`'s rule opens the default chain, never `/api/../fold`.
 */

import { bare } from "../src/shell/route";
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
  // Under node there is no `window`, so the stored value reads as absent, as in a private window.
  assert.equal(remembered(), DEFAULT_CHAIN);
  assert.equal(app.now().chain, DEFAULT_CHAIN);
  assert.equal(app.apiBase(app.now()), `/api/${DEFAULT_CHAIN}`);
});

test("a pin moves the chain only to another slug, and a refusal leaves it where it was", () => {
  const at = app.initial(bare({ type: "inbox" }), DEFAULT_CHAIN);
  assert.equal(app.step(at, { type: "Pinned", chain: DEFAULT_CHAIN }), at, "the chain it is already on has not moved");
  assert.equal(app.step(at, { type: "Pinned", chain: ".." }), at, "a traversal is ignored");
  const other = app.step(at, { type: "Pinned", chain: "ops" });
  assert.equal(other.chain, "ops");
  assert.equal(app.apiBase(other), "/api/ops");
  // And a refusal from the second chain does not knock it back to the first.
  assert.equal(app.step(other, { type: "Pinned", chain: "../life" }), other);
});
