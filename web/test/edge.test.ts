/**
 * THE CHROME'S ONE EDGE (docs/TILEDLAYOUT-PLAN-2026-11-22.md §13): the window's chrome asks the window, at one width,
 * `--m-wide`. The tiles ask the room they stand in (`main`, the window less the gutters and the open panel) whether it
 * holds a list, a divider and a column, and `web/e2e/tiles.test.ts` holds that no tile stands under a bar in either
 * chrome.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import tokens from "../src/view/tokens";

const css = readFileSync("web/static/style.css", "utf8");
const m = tokens.measure;

test("every width rule of the window's chrome is at the edge", () => {
  const rules = [...css.matchAll(/@media[^{]*?\((min|max)-width:\s*([\d.]+)(rem|px)\)/g)].map(([, side, n, unit]) => `${side} ${n}${unit}`);
  assert.ok(rules.length > 0);
  const edge = m.wide / 16;
  assert.deepEqual(
    rules.filter((r) => r !== `min ${edge}rem` && r !== `max ${edge - 0.01}rem`),
    [],
    "a width rule not at the edge is a second breakpoint",
  );
});

test("the body's gutters are the token's", () => {
  assert.match(css, /\nbody \{[^}]*\n  padding: 0 var\(--m-gutter\) /);
});
