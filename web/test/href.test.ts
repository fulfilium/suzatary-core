/**
 * `view/href.ts` — what an anchor typst emitted may lead to, read as the browser reads it.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { readFileSync } from "node:fs";

import href from "../src/view/href";

import { forAll } from "./gen";

const BASE = "https://box.example.test/#/todo/life/x";

/** Every C0 control, the space, and the whitespace a browser's URL parser strips or skips. */
const STRAY = [...Array.from({ length: 0x20 }, (_, c) => String.fromCharCode(c)), " ", "\u00a0", "\u2028", "\ufeff"];

const EXECUTABLE = ["javascript:alert(1)", "vbscript:msgbox(1)", "data:text/html,<p>x"];

/** Never a link the browser would follow into a scheme that runs: inert, or outbound to one that navigates. */
const runsNothing = (raw: string): void => {
  const h = href.of(raw, BASE);
  if (h.type === "Outbound") assert.ok(href.OUTBOUND.has(new URL(h.href, BASE).protocol), JSON.stringify(raw));
  else assert.notEqual(h.type, "Todo", JSON.stringify(raw));
};

/** `text` with `stray` at `at`, a case of each letter flipped as the seed says. */
const salted = (text: string, at: number, stray: string, flips: boolean[]): string =>
  [...text.slice(0, at), stray, ...text.slice(at)].map((ch, i) => (flips[i] === true ? ch.toUpperCase() : ch)).join("");

test("law: an executable scheme never leads anywhere that runs, whatever control or space sits inside it", () => {
  for (const url of EXECUTABLE) {
    const scheme = url.indexOf(":") + 1;
    for (const stray of STRAY) {
      for (let at = 0; at <= scheme; at += 1) {
        runsNothing(salted(url, at, stray, []));
      }
    }
  }
});

test("law: any number of strays, in any case, still never lead anywhere that runs", () => {
  forAll(500, (r) => {
    let raw = r.pick(EXECUTABLE);
    for (let n = r.int(4); n >= 0; n -= 1) raw = salted(raw, r.int(raw.indexOf(":") + 2), r.pick(STRAY), r.array(raw.length + 1, () => r.bool()));
    runsNothing(raw);
  });
});

test("an outbound link of a scheme that navigates is kept as written, and shown by where it leads", () => {
  const cases: readonly (readonly [string, string])[] = [
    ["https://example.invalid/baggage", "example.invalid"],
    ["http://example.invalid", "example.invalid"],
    ["mailto:a@example.invalid", "a@example.invalid"],
    ["tel:+1-555-555-0100", "+1-555-555-0100"],
  ];
  for (const [raw, shown] of cases) {
    const h = href.of(raw, BASE);
    assert.ok(h.type === "Outbound" && h.href === raw && h.shown === shown, JSON.stringify(h));
  }
});

test("law: an outbound anchor shows the host it leads to, and its label only beside it", () => {
  forAll(300, (r) => {
    const host = r.pick(["evil.example", "bank.example.evil.example", "xn--bnk-sna.example"]);
    const posing = r.pick(["", "bank.example@", "bank.example:pw@", "mail:ab12@"]);
    const rest = r.pick(["", "/", "/?q=the%20owner%27s%20balance", "/#mail:ab12"]);
    const label = r.pick(["mail:ab12", "bank.example", "Research Scientist @ Example Lab", ""]);
    const h = href.of(`https://${posing}${host}${rest}`, BASE);
    assert.ok(h.type === "Outbound" && h.shown === host, JSON.stringify(h));
    if (h.type === "Outbound") assert.equal(href.beside(h, label), label);
  });
});

// The box's vectors: every stored link still renders, and the tab draws each outbound one by its host.
const vectors = JSON.parse(readFileSync("app/core/tests/web/closed-world.json", "utf8")) as { text: string; admitted: boolean }[]

test("every link in the box's vectors renders, and an outbound one is drawn by its host with its label beside", () => {
  let outbound = 0
  for (const v of vectors) {
    const link = /link\("([^"]*)"\)(?:\[([^\]#]*)\])?/.exec(v.text)
    const h = link === null ? undefined : href.of(link[1] ?? "", BASE)
    if (link === null || h === undefined || h.type !== "Outbound") continue
    outbound += 1
    assert.ok(v.admitted, `${v.text} renders`)
    const written = new URL(link[1] ?? "", BASE)
    assert.equal(h.shown, written.protocol.startsWith("http") ? written.host : written.pathname, v.text)
    // With no label, typst labels the link with its address, which is drawn as the host alone.
    const label = link[2] ?? h.address
    assert.equal(href.beside(h, label), link[2] === undefined ? "" : label.trim(), v.text)
  }
  assert.ok(outbound >= 5, "the vectors hold labeled and bare outbound links")
})

test("a todo: link names its todo, a bare one and any other scheme name nothing", () => {
  assert.deepEqual(href.of("todo:life/flight", BASE), { type: "Todo", id: "life/flight" });
  for (const raw of ["todo:", "ftp://example.invalid/x", "blob:https://box.example.test/x", "file:///etc/passwd", "http://[x"]) {
    assert.deepEqual(href.of(raw, BASE), { type: "Inert" }, raw);
  }
});
